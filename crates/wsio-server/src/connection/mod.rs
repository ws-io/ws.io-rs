use std::sync::{
    Arc,
    LazyLock,
    atomic::{
        AtomicU64,
        Ordering,
    },
};

use anyhow::{
    Result,
    bail,
};
use arc_swap::ArcSwap;
use http::{
    HeaderMap,
    Uri,
};
use kikiutils::{
    atomic::enum_cell::AtomicEnumCell,
    types::fx_collections::FxDashSet,
};
use num_enum::{
    IntoPrimitive,
    TryFromPrimitive,
};
use serde::{
    Serialize,
    de::DeserializeOwned,
};
use tokio::{
    spawn,
    sync::{
        Mutex,
        mpsc::{
            Receiver,
            Sender,
            channel,
        },
    },
    task::JoinHandle,
    time::{
        sleep,
        timeout,
    },
};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

#[cfg(feature = "connection-extensions")]
mod extensions;

#[cfg(feature = "connection-extensions")]
use self::extensions::ConnectionExtensions;
use crate::{
    WsIoServer,
    core::{
        channel_capacity_from_websocket_config,
        event::registry::WsIoEventRegistry,
        packet::{
            WsIoPacket,
            WsIoPacketType,
        },
        traits::task::spawner::TaskSpawner,
        types::BoxAsyncUnaryResultHandler,
        utils::task::abort_locked_task,
    },
    namespace::{
        WsIoServerNamespace,
        operators::broadcast::WsIoServerNamespaceBroadcastOperator,
    },
};

// Enums
#[repr(u8)]
#[derive(Debug, Eq, IntoPrimitive, PartialEq, TryFromPrimitive)]
enum ConnectionState {
    Activating,
    AwaitingInit,
    Closed,
    Closing,
    Created,
    Initiating,
    Ready,
}

// Structs
pub struct WsIoServerConnection {
    cancel_token: ArcSwap<CancellationToken>,
    event_registry: WsIoEventRegistry<WsIoServerConnection, WsIoServerConnection>,
    #[cfg(feature = "connection-extensions")]
    extensions: ConnectionExtensions,
    headers: HeaderMap,
    id: u64,
    init_timeout_task: Mutex<Option<JoinHandle<()>>>,
    joined_rooms: FxDashSet<String>,
    message_tx: Sender<Arc<Message>>,
    namespace: Arc<WsIoServerNamespace>,
    on_close_handler: Mutex<Option<BoxAsyncUnaryResultHandler<Self>>>,
    request_uri: Uri,
    state: AtomicEnumCell<ConnectionState>,
}

impl TaskSpawner for WsIoServerConnection {
    #[inline]
    fn cancel_token(&self) -> Arc<CancellationToken> {
        self.cancel_token.load_full()
    }
}

impl WsIoServerConnection {
    #[inline]
    pub(crate) fn new(
        headers: HeaderMap,
        namespace: Arc<WsIoServerNamespace>,
        request_uri: Uri,
    ) -> (Arc<Self>, Receiver<Arc<Message>>) {
        let channel_capacity = channel_capacity_from_websocket_config(&namespace.config.websocket_config);
        let (message_tx, message_rx) = channel(channel_capacity);
        (
            Arc::new(Self {
                cancel_token: ArcSwap::new(Arc::new(CancellationToken::new())),
                event_registry: WsIoEventRegistry::new(),
                #[cfg(feature = "connection-extensions")]
                extensions: ConnectionExtensions::new(),
                headers,
                id: NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed),
                init_timeout_task: Mutex::new(None),
                joined_rooms: FxDashSet::default(),
                message_tx,
                namespace,
                on_close_handler: Mutex::new(None),
                request_uri,
                state: AtomicEnumCell::new(ConnectionState::Created),
            }),
            message_rx,
        )
    }

    // Private methods
    #[inline]
    fn handle_event_packet(self: &Arc<Self>, event: &str, packet_data: Option<Vec<u8>>) -> Result<()> {
        self.event_registry.dispatch_event_packet(
            self.clone(),
            event,
            &self.namespace.config.packet_codec,
            packet_data,
            self,
        );

        Ok(())
    }

    async fn handle_init_packet(self: &Arc<Self>, packet_data: Option<&[u8]>) -> Result<()> {
        // Verify current state; only valid from AwaitingInit → Initiating
        let state = self.state.get();
        match state {
            ConnectionState::AwaitingInit => self.state.try_transition(state, ConnectionState::Initiating)?,
            _ => bail!("Received init packet in invalid state: {state:?}"),
        }

        // Abort init-timeout task
        abort_locked_task(&self.init_timeout_task).await;

        // Invoke init_response_handler with timeout protection if configured
        if let Some(init_response_handler) = &self.namespace.config.init_response_handler {
            timeout(
                self.namespace.config.init_response_handler_timeout,
                init_response_handler(self.clone(), packet_data, &self.namespace.config.packet_codec),
            )
            .await??
        }

        // Activate connection
        self.state
            .try_transition(ConnectionState::Initiating, ConnectionState::Activating)?;

        // Invoke middleware with timeout protection if configured
        if let Some(middleware) = &self.namespace.config.middleware {
            timeout(
                self.namespace.config.middleware_execution_timeout,
                middleware(self.clone()),
            )
            .await??;

            // Ensure connection is still in Activating state
            self.state.ensure(ConnectionState::Activating, |state| {
                format!("Cannot activate connection in invalid state: {state:?}")
            })?;
        }

        // Invoke on_connect_handler with timeout protection if configured
        if let Some(on_connect_handler) = &self.namespace.config.on_connect_handler {
            timeout(
                self.namespace.config.on_connect_handler_timeout,
                on_connect_handler(self.clone()),
            )
            .await??;
        }

        // Transition state to Ready
        self.state
            .try_transition(ConnectionState::Activating, ConnectionState::Ready)?;

        // Insert connection into namespace
        self.namespace.insert_connection(self.clone());

        // Send ready packet
        self.send_packet(&WsIoPacket::new_ready()).await?;

        // Invoke on_ready_handler if configured
        if let Some(on_ready_handler) = self.namespace.config.on_ready_handler.clone() {
            // Run handler asynchronously in a detached task
            self.spawn_task(on_ready_handler(self.clone()));
        }

        Ok(())
    }

    async fn send_packet(&self, packet: &WsIoPacket) -> Result<()> {
        self.send_message(self.namespace.encode_packet_to_message(packet)?)
            .await
    }

    // Protected methods
    pub(crate) async fn cleanup(self: &Arc<Self>) {
        // Set connection state to Closing
        self.state.store(ConnectionState::Closing);

        // Remove connection from namespace
        self.namespace.remove_connection(self.id);

        // Leave all joined rooms
        let joined_rooms = self.joined_rooms.iter().map(|entry| entry.clone()).collect::<Vec<_>>();
        for room_name in &joined_rooms {
            self.namespace.remove_connection_id_from_room(room_name, self.id);
        }

        self.joined_rooms.clear();

        // Abort init-timeout task
        abort_locked_task(&self.init_timeout_task).await;

        // Cancel all ongoing operations via cancel token
        self.cancel_token.load().cancel();

        // Invoke on_close_handler with timeout protection if configured
        if let Some(on_close_handler) = self.on_close_handler.lock().await.take() {
            let _ = timeout(
                self.namespace.config.on_close_handler_timeout,
                on_close_handler(self.clone()),
            )
            .await;
        }

        // Set connection state to Closed
        self.state.store(ConnectionState::Closed);
    }

    #[inline]
    pub(crate) fn close(&self) {
        // Skip if connection is already Closing or Closed, otherwise set connection state to Closing
        match self.state.get() {
            ConnectionState::Closed | ConnectionState::Closing => return,
            _ => self.state.store(ConnectionState::Closing),
        }

        // Send websocket close frame to initiate graceful shutdown
        let _ = self.message_tx.try_send(Arc::new(Message::Close(None)));
    }

    pub(crate) async fn emit_event_message(&self, message: Arc<Message>) -> Result<()> {
        self.state.ensure(ConnectionState::Ready, |state| {
            format!("Cannot emit in invalid state: {state:?}")
        })?;

        self.send_message(message).await
    }

    pub(crate) async fn handle_incoming_packet(self: &Arc<Self>, encoded_packet: &[u8]) -> Result<()> {
        // TODO: lazy load
        let packet = self.namespace.config.packet_codec.decode(encoded_packet)?;
        match packet.r#type {
            WsIoPacketType::Event => {
                if self.is_ready() {
                    if let Some(event) = packet.key.as_deref() {
                        return self.handle_event_packet(event, packet.data);
                    } else {
                        bail!("Event packet missing key");
                    }
                }

                Ok(())
            }
            WsIoPacketType::Init => self.handle_init_packet(packet.data.as_deref()).await,
            _ => Ok(()),
        }
    }

    pub(crate) async fn init(self: &Arc<Self>) -> Result<()> {
        // Verify current state; only valid Created
        self.state.ensure(ConnectionState::Created, |state| {
            format!("Cannot init connection in invalid state: {state:?}")
        })?;

        // Generate init request data if init request handler is configured
        let init_request_data = if let Some(init_request_handler) = &self.namespace.config.init_request_handler {
            timeout(
                self.namespace.config.init_request_handler_timeout,
                init_request_handler(self.clone(), &self.namespace.config.packet_codec),
            )
            .await??
        } else {
            None
        };

        // Transition state to AwaitingInit
        self.state
            .try_transition(ConnectionState::Created, ConnectionState::AwaitingInit)?;

        // Spawn init-response-timeout watchdog to close connection if init not received in time
        let connection = self.clone();
        *self.init_timeout_task.lock().await = Some(spawn(async move {
            sleep(connection.namespace.config.init_response_timeout).await;
            if connection.state.is(ConnectionState::AwaitingInit) {
                connection.close();
            }
        }));

        // Send init packet
        self.send_packet(&WsIoPacket::new_init(init_request_data)).await
    }

    pub(crate) async fn send_message(&self, message: Arc<Message>) -> Result<()> {
        Ok(self.message_tx.send(message).await?)
    }

    // Public methods
    pub async fn disconnect(&self) {
        let _ = self.send_packet(&WsIoPacket::new_disconnect()).await;
        self.close()
    }

    pub async fn emit<D: Serialize>(&self, event: impl AsRef<str>, data: Option<&D>) -> Result<()> {
        self.emit_event_message(
            self.namespace.encode_packet_to_message(&WsIoPacket::new_event(
                event.as_ref(),
                data.map(|data| self.namespace.config.packet_codec.encode_data(data))
                    .transpose()?,
            ))?,
        )
        .await
    }

    #[inline]
    pub fn except(
        self: &Arc<Self>,
        room_names: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> WsIoServerNamespaceBroadcastOperator {
        self.namespace.except(room_names).except_connection_ids(vec![self.id])
    }

    #[cfg(feature = "connection-extensions")]
    #[inline]
    pub fn extensions(&self) -> &ConnectionExtensions {
        &self.extensions
    }

    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    #[inline]
    pub fn id(&self) -> u64 {
        self.id
    }

    #[inline]
    pub fn is_ready(&self) -> bool {
        self.state.is(ConnectionState::Ready)
    }

    #[inline]
    pub fn join(self: &Arc<Self>, room_names: impl IntoIterator<Item = impl AsRef<str>>) {
        for room_name in room_names {
            let room_name = room_name.as_ref();
            self.namespace.add_connection_id_to_room(room_name, self.id);
            self.joined_rooms.insert(room_name.into());
        }
    }

    #[inline]
    pub fn leave(self: &Arc<Self>, room_names: impl IntoIterator<Item = impl AsRef<str>>) {
        for room_name in room_names {
            self.namespace
                .remove_connection_id_from_room(room_name.as_ref(), self.id);

            self.joined_rooms.remove(room_name.as_ref());
        }
    }

    #[inline]
    pub fn namespace(&self) -> Arc<WsIoServerNamespace> {
        self.namespace.clone()
    }

    #[inline]
    pub fn off(&self, event: impl AsRef<str>) {
        self.event_registry.off(event.as_ref());
    }

    #[inline]
    pub fn off_by_handler_id(&self, event: impl AsRef<str>, handler_id: u32) {
        self.event_registry.off_by_handler_id(event.as_ref(), handler_id);
    }

    #[inline]
    pub fn on<H, Fut, D>(&self, event: impl AsRef<str>, handler: H) -> u32
    where
        H: Fn(Arc<WsIoServerConnection>, Arc<D>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
        D: DeserializeOwned + Send + Sync + 'static,
    {
        self.event_registry.on(event.as_ref(), handler)
    }

    pub async fn on_close<H, Fut>(&self, handler: H)
    where
        H: Fn(Arc<WsIoServerConnection>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        *self.on_close_handler.lock().await = Some(Box::new(move |connection| Box::pin(handler(connection))));
    }

    #[inline]
    pub fn request_uri(&self) -> &Uri {
        &self.request_uri
    }

    #[inline]
    pub fn server(&self) -> WsIoServer {
        self.namespace.server()
    }

    #[inline]
    pub fn to(
        self: &Arc<Self>,
        room_names: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> WsIoServerNamespaceBroadcastOperator {
        self.namespace.to(room_names).except_connection_ids(vec![self.id])
    }
}

// Constants/Statics
static NEXT_CONNECTION_ID: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));

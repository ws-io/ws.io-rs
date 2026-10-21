use std::{
    fmt::{
        Debug as FmtDebug,
        Formatter,
        Result as FmtResult,
    },
    mem::take,
    panic::AssertUnwindSafe,
    sync::{
        Arc,
        atomic::{
            AtomicU64,
            Ordering,
        },
    },
};

use anyhow::{
    Result,
    anyhow,
    bail,
};
use bytes::Bytes;
use futures_util::FutureExt;
use http::{
    HeaderMap,
    Uri,
};
use kikiutils::{
    atomic::enum_cell::AtomicEnumCell,
    types::fx_collections::FxHashSet,
};
use num_enum::{
    IntoPrimitive,
    TryFromPrimitive,
};
use parking_lot::Mutex as ParkingLotMutex;
use serde::{
    Serialize,
    de::DeserializeOwned,
};
use tokio::{
    select,
    spawn,
    sync::{
        Mutex,
        mpsc,
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
    cancel_token: CancellationToken,
    event_dispatcher_task: Mutex<Option<JoinHandle<()>>>,
    event_queue_tx: mpsc::Sender<WsIoPacket>,
    event_registry: WsIoEventRegistry<WsIoServerConnection>,
    #[cfg(feature = "connection-extensions")]
    extensions: ConnectionExtensions,
    headers: HeaderMap,
    id: u64,
    init_timeout_task: Mutex<Option<JoinHandle<()>>>,
    joined_rooms: ParkingLotMutex<FxHashSet<String>>,
    message_tx: mpsc::Sender<Arc<Message>>,
    namespace: Arc<WsIoServerNamespace>,
    on_close_handler: Mutex<Option<BoxAsyncUnaryResultHandler<Self>>>,
    request_uri: Uri,
    state: AtomicEnumCell<ConnectionState>,
}

impl FmtDebug for WsIoServerConnection {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let event_dispatcher_task = match self.event_dispatcher_task.try_lock() {
            Ok(task) => {
                if task.is_some() {
                    "<task>"
                } else {
                    "<none>"
                }
            },
            Err(_) => "<locked>",
        };

        let init_timeout_task = match self.init_timeout_task.try_lock() {
            Ok(task) => {
                if task.is_some() {
                    "<task>"
                } else {
                    "<none>"
                }
            },
            Err(_) => "<locked>",
        };

        let on_close_handler = match self.on_close_handler.try_lock() {
            Ok(handler) => {
                if handler.is_some() {
                    "<handler>"
                } else {
                    "<none>"
                }
            },
            Err(_) => "<locked>",
        };

        let mut debug = f.debug_struct("WsIoServerConnection");
        debug
            .field("id", &self.id)
            .field("state", &self.state)
            .field("request_uri", &self.request_uri)
            .field("headers", &self.headers)
            .field("message_tx", &self.message_tx)
            .field("event_queue_tx", &self.event_queue_tx)
            .field("event_dispatcher_task", &event_dispatcher_task)
            .field("cancel_token", &"<cancel_token>")
            .field("namespace", &"<namespace>")
            .field("event_registry", &self.event_registry)
            .field("init_timeout_task", &init_timeout_task)
            .field("on_close_handler", &on_close_handler);

        match self.joined_rooms.try_lock() {
            Some(rooms) => debug.field("joined_rooms_len", &rooms.len()),
            None => debug.field("joined_rooms_len", &"<locked>"),
        };

        #[cfg(feature = "connection-extensions")]
        debug.field("extensions", &self.extensions);

        debug.finish()
    }
}

impl TaskSpawner for WsIoServerConnection {
    #[inline]
    fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }
}

impl WsIoServerConnection {
    #[inline]
    pub(crate) fn new(
        headers: HeaderMap,
        namespace: Arc<WsIoServerNamespace>,
        request_uri: Uri,
    ) -> (Arc<Self>, mpsc::Receiver<Arc<Message>>, mpsc::Receiver<WsIoPacket>) {
        let channel_capacity = namespace.config.channel_capacity;
        let (event_queue_tx, event_queue_rx) = mpsc::channel(channel_capacity);
        let (message_tx, message_rx) = mpsc::channel(channel_capacity);
        let id = NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed);

        #[cfg(feature = "tracing")]
        tracing::debug!(
            connection_id = id,
            namespace = %namespace.path(),
            request_path = request_uri.path(),
            "creating server connection"
        );

        (
            Arc::new(Self {
                cancel_token: CancellationToken::new(),
                event_dispatcher_task: Mutex::new(None),
                event_queue_tx,
                event_registry: WsIoEventRegistry::new(),
                #[cfg(feature = "connection-extensions")]
                extensions: ConnectionExtensions::new(),
                headers,
                id,
                init_timeout_task: Mutex::new(None),
                joined_rooms: ParkingLotMutex::new(FxHashSet::default()),
                message_tx,
                namespace,
                on_close_handler: Mutex::new(None),
                request_uri,
                state: AtomicEnumCell::new(ConnectionState::Created),
            }),
            message_rx,
            event_queue_rx,
        )
    }

    // Private methods
    #[inline]
    async fn handle_event_packet(self: &Arc<Self>, packet: WsIoPacket) -> Result<()> {
        #[cfg(feature = "tracing")]
        tracing::trace!(
            connection_id = self.id,
            event = packet.key.as_deref().unwrap_or_default(),
            has_data = packet.data.is_some(),
            "received client event packet"
        );

        let cancel_token = self.cancel_token();
        select! {
            biased;
            () = cancel_token.cancelled() => Ok(()),
            result = self.event_queue_tx.send(packet) => result.map_err(|_| anyhow!("event dispatcher is closed")),
        }
    }

    async fn handle_init_packet(self: &Arc<Self>, packet_data: Option<&[u8]>) -> Result<()> {
        // Verify current state; only valid from AwaitingInit → Initiating
        let state = self.state.get();
        if state == ConnectionState::AwaitingInit {
            self.state.try_transition(state, ConnectionState::Initiating)?;
        } else {
            #[cfg(feature = "tracing")]
            tracing::debug!(
                connection_id = self.id,
                ?state,
                "received init packet in invalid server connection state"
            );

            bail!("Received init packet in invalid state: {state:?}");
        }

        #[cfg(feature = "tracing")]
        tracing::debug!(connection_id = self.id, "received client init packet");

        // Abort init-timeout task
        abort_locked_task(&self.init_timeout_task).await;

        // Invoke init_response_handler with timeout protection if configured
        if let Some(init_response_handler) = &self.namespace.config.init_response_handler {
            match timeout(
                self.namespace.config.init_response_handler_timeout,
                init_response_handler(Arc::clone(self), packet_data, &self.namespace.config.packet_codec),
            )
            .await
            {
                Ok(result) => result?,
                Err(err) => {
                    #[cfg(feature = "tracing")]
                    tracing::warn!(
                        connection_id = self.id,
                        error = %err,
                        "server init response handler timed out"
                    );

                    return Err(err.into());
                },
            }
        }

        // Activate connection
        self.state
            .try_transition(ConnectionState::Initiating, ConnectionState::Activating)?;

        // Invoke middleware with timeout protection if configured
        if let Some(middleware) = &self.namespace.config.middleware {
            match timeout(
                self.namespace.config.middleware_execution_timeout,
                middleware(Arc::clone(self)),
            )
            .await
            {
                Ok(result) => result?,
                Err(err) => {
                    #[cfg(feature = "tracing")]
                    tracing::warn!(connection_id = self.id, error = %err, "server middleware timed out");
                    return Err(err.into());
                },
            }

            // Ensure connection is still in Activating state
            self.state.ensure(ConnectionState::Activating, |state| {
                format!("Cannot activate connection in invalid state: {state:?}")
            })?;
        }

        // Invoke on_connect_handler with timeout protection if configured
        if let Some(on_connect_handler) = &self.namespace.config.on_connect_handler {
            match timeout(
                self.namespace.config.on_connect_handler_timeout,
                on_connect_handler(Arc::clone(self)),
            )
            .await
            {
                Ok(result) => result?,
                Err(err) => {
                    #[cfg(feature = "tracing")]
                    tracing::warn!(connection_id = self.id, error = %err, "server on-connect handler timed out");
                    return Err(err.into());
                },
            }
        }

        // Transition state to Ready
        self.state
            .try_transition(ConnectionState::Activating, ConnectionState::Ready)?;

        // Insert connection into namespace
        if !self.namespace.insert_connection(self) {
            bail!("Cannot initialize connection while namespace is shutting down");
        }

        #[cfg(feature = "tracing")]
        tracing::debug!(connection_id = self.id, "server connection is ready");

        // Send ready packet
        self.send_packet(&WsIoPacket::new_ready()).await?;

        // Invoke on_ready_handler if configured
        if let Some(on_ready_handler) = self.namespace.config.on_ready_handler.clone() {
            // Run handler asynchronously in a detached task
            self.spawn_task(on_ready_handler(Arc::clone(self)));
        }

        Ok(())
    }

    async fn send_packet(&self, packet: &WsIoPacket) -> Result<()> {
        self.send_message(self.namespace.encode_packet_to_message(packet).await?)
            .await
    }

    // Protected methods
    pub(super) async fn cleanup(self: &Arc<Self>) {
        #[cfg(feature = "tracing")]
        tracing::debug!(connection_id = self.id, "cleaning up server connection");

        // Set connection state to Closing
        self.state.store(ConnectionState::Closing);

        // Revoke work immediately, including when the transport closed independently.
        self.cancel_token.cancel();

        // Stop event dispatch before mutating namespace and room membership.
        let event_dispatcher_task = self.event_dispatcher_task.lock().await.take();
        if let Some(event_dispatcher_task) = event_dispatcher_task {
            event_dispatcher_task.abort();
            let _ = event_dispatcher_task.await;
        }

        // Remove connection from namespace
        self.namespace.remove_connection(self.id);

        // Leave all joined rooms
        // Closing rejects new joins; taking the same lock also waits for accepted joins to finish both index updates.
        let joined_rooms = take(&mut *self.joined_rooms.lock());
        for room_name in &joined_rooms {
            self.namespace.remove_connection_id_from_room(room_name, self.id);
        }

        // Abort init-timeout task
        abort_locked_task(&self.init_timeout_task).await;

        // Invoke on_close_handler with timeout protection if configured
        let on_close_handler = self.on_close_handler.lock().await.take();
        if let Some(on_close_handler) = on_close_handler
            && let Err(_err) = timeout(
                self.namespace.config.on_close_handler_timeout,
                on_close_handler(Arc::clone(self)),
            )
            .await
        {
            #[cfg(feature = "tracing")]
            tracing::warn!(connection_id = self.id, error = %_err, "server close handler timed out");
        }

        // Set connection state to Closed
        self.state.store(ConnectionState::Closed);

        #[cfg(feature = "tracing")]
        tracing::debug!(connection_id = self.id, "server connection closed");
    }

    #[inline]
    pub(super) fn close(&self) {
        // Skip if connection is already Closing or Closed, otherwise set connection state to Closing
        match self.state.get() {
            ConnectionState::Closed | ConnectionState::Closing => return,
            _state @ (ConnectionState::Activating
            | ConnectionState::AwaitingInit
            | ConnectionState::Created
            | ConnectionState::Initiating
            | ConnectionState::Ready) => {
                #[cfg(feature = "tracing")]
                tracing::debug!(connection_id = self.id, state = ?_state, "closing server connection");
                self.state.store(ConnectionState::Closing);
            },
        }

        // Revoke event dispatch and managed tasks before waiting for transport shutdown.
        self.cancel_token.cancel();

        // Send websocket close frame to initiate graceful shutdown
        let _ = self.message_tx.try_send(Arc::new(Message::Close(None)));
    }

    pub(super) async fn emit_event_message(&self, message: Arc<Message>) -> Result<()> {
        self.state.ensure(ConnectionState::Ready, |state| {
            format!("Cannot emit in invalid state: {state:?}")
        })?;

        self.send_message(message).await
    }

    pub(super) async fn handle_incoming_packet(self: &Arc<Self>, encoded_packet: Bytes) -> Result<()> {
        // TODO: lazy load
        let packet = {
            let encoded_packet = self.namespace.config.packet_transformer.decode(encoded_packet).await?;
            match self.namespace.config.packet_codec.decode(&encoded_packet) {
                Ok(packet) => packet,
                Err(err) => {
                    #[cfg(feature = "tracing")]
                    tracing::debug!(connection_id = self.id, error = %err, "failed to decode client packet");
                    return Err(err);
                },
            }
        };

        match &packet.r#type {
            WsIoPacketType::Event => {
                if self.is_ready() {
                    return self.handle_event_packet(packet).await;
                }

                Ok(())
            },
            WsIoPacketType::Init => self.handle_init_packet(packet.data.as_deref()).await,
            _ => Ok(()),
        }
    }

    pub(super) async fn init(self: &Arc<Self>) -> Result<()> {
        // Verify current state; only valid Created
        self.state.ensure(ConnectionState::Created, |state| {
            format!("Cannot init connection in invalid state: {state:?}")
        })?;

        #[cfg(feature = "tracing")]
        tracing::debug!(connection_id = self.id, "initializing server connection");

        // Generate init request data if init request handler is configured
        let init_request_data = if let Some(init_request_handler) = &self.namespace.config.init_request_handler {
            match timeout(
                self.namespace.config.init_request_handler_timeout,
                init_request_handler(Arc::clone(self), &self.namespace.config.packet_codec),
            )
            .await
            {
                Ok(result) => result?,
                Err(err) => {
                    #[cfg(feature = "tracing")]
                    tracing::warn!(
                        connection_id = self.id,
                        error = %err,
                        "server init request handler timed out"
                    );

                    return Err(err.into());
                },
            }
        } else {
            None
        };

        // Transition state to AwaitingInit
        self.state
            .try_transition(ConnectionState::Created, ConnectionState::AwaitingInit)?;

        // Spawn init-response-timeout watchdog to close connection if init not received in time
        let connection = Arc::clone(self);
        *self.init_timeout_task.lock().await = Some(spawn(async move {
            sleep(connection.namespace.config.init_response_timeout).await;
            if connection.state.is(ConnectionState::AwaitingInit) {
                #[cfg(feature = "tracing")]
                tracing::warn!(
                    connection_id = connection.id,
                    "timed out waiting for client init response packet"
                );

                connection.close();
            }
        }));

        // Send init packet
        self.send_packet(&WsIoPacket::new_init(init_request_data)).await
    }

    pub(super) async fn send_message(&self, message: Arc<Message>) -> Result<()> {
        Ok(self.message_tx.send(message).await?)
    }

    pub(super) async fn start_event_dispatcher(self: &Arc<Self>, mut event_queue_rx: mpsc::Receiver<WsIoPacket>) {
        let cancel_token = self.cancel_token();
        let connection = Arc::clone(self);
        *self.event_dispatcher_task.lock().await = Some(spawn(async move {
            let dispatcher = async {
                loop {
                    let event_packet = select! {
                        biased;
                        () = cancel_token.cancelled() => break,
                        event_packet = event_queue_rx.recv() => event_packet,
                    };

                    let Some(event_packet) = event_packet else {
                        break;
                    };

                    if !connection.is_ready() {
                        break;
                    }

                    let Some(event) = event_packet.key else {
                        continue;
                    };

                    if let Err(_err) = connection
                        .event_registry
                        .dispatch_event_packet(
                            Arc::clone(&connection),
                            event,
                            &connection.namespace.config.packet_codec,
                            event_packet.data,
                            &cancel_token,
                        )
                        .await
                    {
                        #[cfg(feature = "tracing")]
                        tracing::warn!(
                            connection_id = connection.id,
                            error = %_err,
                            "server event dispatcher failed; closing connection"
                        );

                        connection.close();
                        break;
                    }
                }
            };

            if AssertUnwindSafe(dispatcher).catch_unwind().await.is_err() {
                #[cfg(feature = "tracing")]
                tracing::error!(
                    connection_id = connection.id,
                    "server event dispatcher panicked; closing connection"
                );

                connection.close();
            }
        }));
    }

    // Public methods

    /// Attempts to queue a protocol Disconnect within `disconnect_timeout`, then requests closure.
    ///
    /// Packet transformation and queue backpressure share this timeout. Transport shutdown has
    /// a separate grace period of the same duration. Peer receipt is not guaranteed.
    pub async fn disconnect(&self) {
        #[cfg(feature = "tracing")]
        tracing::debug!(connection_id = self.id, "disconnecting server connection");

        let disconnect_packet = WsIoPacket::new_disconnect();
        let cancel_token = self.cancel_token();
        select! {
            biased;
            () = cancel_token.cancelled() => {},
            _ = timeout(
                self.namespace.config.disconnect_timeout,
                self.send_packet(&disconnect_packet),
            ) => {},
        }

        self.close();
    }

    /// Encodes and queues an event without waiting for peer receipt.
    ///
    /// The transformed packet must fit the configured outgoing message limit.
    /// Success is not a delivery acknowledgement.
    pub async fn emit<D: Serialize>(&self, event: impl AsRef<str>, data: Option<&D>) -> Result<()> {
        self.emit_event_message(
            self.namespace
                .encode_packet_to_message(&WsIoPacket::new_event(
                    event.as_ref(),
                    data.map(|data| self.namespace.config.packet_codec.encode_data(data))
                        .transpose()?,
                ))
                .await?,
        )
        .await
    }

    #[inline]
    pub fn except(
        self: &Arc<Self>,
        room_names: impl IntoIterator<Item = impl Into<String>>,
    ) -> WsIoServerNamespaceBroadcastOperator {
        self.namespace.except(room_names).except_connection_ids([self.id])
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

    /// Joins rooms unless the connection is closing or closed.
    #[inline]
    pub fn join(self: &Arc<Self>, room_names: impl IntoIterator<Item = impl Into<String>>) {
        for room_name in room_names {
            let room_name = room_name.into();

            #[cfg(feature = "tracing")]
            let logged_room_name = room_name.clone();

            {
                let mut joined_rooms = self.joined_rooms.lock();
                if matches!(self.state.get(), ConnectionState::Closing | ConnectionState::Closed) {
                    return;
                }

                // Keep the lock order consistent with leave: connection rooms, then namespace rooms.
                self.namespace.add_connection_id_to_room(&room_name, self.id);
                joined_rooms.insert(room_name);
            }

            #[cfg(feature = "tracing")]
            tracing::trace!(connection_id = self.id, room = %logged_room_name, "connection joined room");
        }
    }

    #[inline]
    pub fn leave(self: &Arc<Self>, room_names: impl IntoIterator<Item = impl Into<String>>) {
        for room_name in room_names {
            let room_name = room_name.into();
            {
                let mut joined_rooms = self.joined_rooms.lock();
                self.namespace.remove_connection_id_from_room(&room_name, self.id);
                joined_rooms.remove(&room_name);
            }

            #[cfg(feature = "tracing")]
            tracing::trace!(connection_id = self.id, room = %room_name, "connection left room");
        }
    }

    #[inline]
    pub fn namespace(&self) -> Arc<WsIoServerNamespace> {
        Arc::clone(&self.namespace)
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
        room_names: impl IntoIterator<Item = impl Into<String>>,
    ) -> WsIoServerNamespaceBroadcastOperator {
        self.namespace.to(room_names).except_connection_ids([self.id])
    }
}

// Constants/Statics
static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(0);

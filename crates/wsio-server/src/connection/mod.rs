use std::{
    fmt::{
        Debug as FmtDebug,
        Formatter,
        Result as FmtResult,
    },
    panic::AssertUnwindSafe,
    sync::{
        Arc,
        LazyLock,
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
use arc_swap::ArcSwap;
use futures_util::FutureExt;
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
    select,
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
    event_dispatcher_task: Mutex<Option<JoinHandle<()>>>,
    event_queue_tx: Sender<WsIoPacket>,
    event_registry: WsIoEventRegistry<WsIoServerConnection>,
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
            .field("joined_rooms_len", &self.joined_rooms.len())
            .field("message_tx", &self.message_tx)
            .field("event_queue_tx", &self.event_queue_tx)
            .field("event_dispatcher_task", &event_dispatcher_task)
            .field("cancel_token", &"<cancel_token>")
            .field("namespace", &"<namespace>")
            .field("event_registry", &self.event_registry)
            .field("init_timeout_task", &init_timeout_task)
            .field("on_close_handler", &on_close_handler);

        #[cfg(feature = "connection-extensions")]
        debug.field("extensions", &self.extensions);

        debug.finish()
    }
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
    ) -> (Arc<Self>, Receiver<Arc<Message>>, Receiver<WsIoPacket>) {
        let channel_capacity = channel_capacity_from_websocket_config(&namespace.config.websocket_config);
        let (event_queue_tx, event_queue_rx) = channel(channel_capacity);
        let (message_tx, message_rx) = channel(channel_capacity);
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
                cancel_token: ArcSwap::new(Arc::new(CancellationToken::new())),
                event_dispatcher_task: Mutex::new(None),
                event_queue_tx,
                event_registry: WsIoEventRegistry::new(),
                #[cfg(feature = "connection-extensions")]
                extensions: ConnectionExtensions::new(),
                headers,
                id,
                init_timeout_task: Mutex::new(None),
                joined_rooms: FxDashSet::default(),
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
        let Some(_event) = packet.key.as_deref() else {
            bail!("Event packet missing key");
        };

        #[cfg(feature = "tracing")]
        tracing::trace!(
            connection_id = self.id,
            event = _event,
            has_data = packet.data.is_some(),
            "received client event packet"
        );

        let cancel_token = self.cancel_token();
        select! {
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
                init_response_handler(self.clone(), packet_data, &self.namespace.config.packet_codec),
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
                middleware(self.clone()),
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
                on_connect_handler(self.clone()),
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
        self.namespace.insert_connection(self);

        #[cfg(feature = "tracing")]
        tracing::debug!(connection_id = self.id, "server connection is ready");

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
    pub(super) async fn cleanup(self: &Arc<Self>) {
        #[cfg(feature = "tracing")]
        tracing::debug!(connection_id = self.id, "cleaning up server connection");

        // Set connection state to Closing
        self.state.store(ConnectionState::Closing);

        // Stop event dispatch before mutating namespace and room membership.
        let event_dispatcher_task = self.event_dispatcher_task.lock().await.take();
        if let Some(event_dispatcher_task) = event_dispatcher_task {
            event_dispatcher_task.abort();
            let _ = event_dispatcher_task.await;
        }

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
        if let Some(on_close_handler) = self.on_close_handler.lock().await.take()
            && let Err(_err) = timeout(
                self.namespace.config.on_close_handler_timeout,
                on_close_handler(self.clone()),
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
            _state => {
                #[cfg(feature = "tracing")]
                tracing::debug!(connection_id = self.id, state = ?_state, "closing server connection");
                self.state.store(ConnectionState::Closing);
            },
        }

        // Send websocket close frame to initiate graceful shutdown
        let _ = self.message_tx.try_send(Arc::new(Message::Close(None)));
    }

    pub(super) async fn emit_event_message(&self, message: Arc<Message>) -> Result<()> {
        self.state.ensure(ConnectionState::Ready, |state| {
            format!("Cannot emit in invalid state: {state:?}")
        })?;

        self.send_message(message).await
    }

    pub(super) async fn handle_incoming_packet(self: &Arc<Self>, encoded_packet: &[u8]) -> Result<()> {
        // TODO: lazy load
        let packet = match self.namespace.config.packet_codec.decode(encoded_packet) {
            Ok(packet) => packet,
            Err(err) => {
                #[cfg(feature = "tracing")]
                tracing::debug!(connection_id = self.id, error = %err, "failed to decode client packet");
                return Err(err);
            },
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
                init_request_handler(self.clone(), &self.namespace.config.packet_codec),
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
        let connection = self.clone();
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

    pub(super) async fn start_event_dispatcher(self: &Arc<Self>, mut event_queue_rx: Receiver<WsIoPacket>) {
        let cancel_token = self.cancel_token();
        let connection = self.clone();
        *self.event_dispatcher_task.lock().await = Some(spawn(async move {
            let dispatcher = async {
                loop {
                    let event_packet = select! {
                        () = cancel_token.cancelled() => break,
                        event_packet = event_queue_rx.recv() => event_packet,
                    };

                    let Some(event_packet) = event_packet else {
                        break;
                    };

                    let Some(event) = event_packet.key else {
                        continue;
                    };

                    if let Err(_err) = connection
                        .event_registry
                        .dispatch_event_packet(
                            connection.clone(),
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
    pub async fn disconnect(&self) {
        #[cfg(feature = "tracing")]
        tracing::debug!(connection_id = self.id, "disconnecting server connection");
        let _ = self.send_packet(&WsIoPacket::new_disconnect()).await;
        self.close();
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

    #[inline]
    pub fn join(self: &Arc<Self>, room_names: impl IntoIterator<Item = impl Into<String>>) {
        for room_name in room_names {
            let room_name = room_name.into();
            self.namespace.add_connection_id_to_room(&room_name, self.id);

            #[cfg(feature = "tracing")]
            tracing::trace!(connection_id = self.id, room = %room_name, "connection joined room");
            self.joined_rooms.insert(room_name);
        }
    }

    #[inline]
    pub fn leave(self: &Arc<Self>, room_names: impl IntoIterator<Item = impl Into<String>>) {
        for room_name in room_names {
            let room_name = room_name.into();
            self.namespace.remove_connection_id_from_room(&room_name, self.id);

            self.joined_rooms.remove(&room_name);

            #[cfg(feature = "tracing")]
            tracing::trace!(connection_id = self.id, room = %room_name, "connection left room");
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
        room_names: impl IntoIterator<Item = impl Into<String>>,
    ) -> WsIoServerNamespaceBroadcastOperator {
        self.namespace.to(room_names).except_connection_ids([self.id])
    }
}

// Constants/Statics
static NEXT_CONNECTION_ID: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use http::{
        HeaderMap,
        Uri,
    };
    use tokio::{
        sync::mpsc::unbounded_channel,
        time::{
            sleep,
            timeout,
        },
    };

    use super::*;

    fn create_test_connection() -> Arc<WsIoServerConnection> {
        let server = Arc::new(WsIoServer::builder().build());
        let namespace = server.new_namespace_builder("/socket").register().unwrap();
        let (connection, _rx, _event_rx) =
            WsIoServerConnection::new(HeaderMap::new(), namespace, Uri::from_static("http://localhost"));

        connection
    }

    fn create_test_connection_with_event_queue_rx() -> (Arc<WsIoServerConnection>, Receiver<WsIoPacket>) {
        let server = Arc::new(WsIoServer::builder().build());
        let namespace = server.new_namespace_builder("/socket").register().unwrap();
        let (connection, _rx, event_queue_rx) =
            WsIoServerConnection::new(HeaderMap::new(), namespace, Uri::from_static("http://localhost"));

        (connection, event_queue_rx)
    }

    #[tokio::test]
    async fn test_handle_incoming_packet_decode_error() {
        let connection = create_test_connection();
        let garbage_data = b"obviously not valid json or messagepack";
        // Should seamlessly return a Result::Err, not panic
        let result = connection.handle_incoming_packet(garbage_data).await;
        assert!(result.is_err(), "Decoding garbage payload should trigger an error");
    }

    #[tokio::test]
    async fn test_handle_init_packet_in_invalid_state() {
        let connection = create_test_connection();
        assert_eq!(connection.state.get(), ConnectionState::Created);

        // Sending an init packet when the connection is merely `Created` (not yet `AwaitingInit`) should throw an error
        // Init packet JSON encoded (type: 2 = Init) -> serialized as tuple array
        let encoded = b"[2,null,null]";

        // This simulates a manual client Init push before server starts the handshake buffer
        let result = connection.handle_incoming_packet(encoded).await;
        assert!(
            result.is_err(),
            "Should error because state is Created, not AwaitingInit"
        );

        assert!(result.unwrap_err().to_string().contains("invalid state"));
    }

    #[tokio::test]
    async fn test_handle_event_packet_missing_key() {
        let connection = create_test_connection();

        // Force the connection into the Ready state so it accepts Event packets
        connection.state.store(ConnectionState::Ready);

        // Manufacture an Event packet manually without a key (type: 1 = Event) -> serialized as tuple array
        let encoded = b"[1,null,null]";

        let result = connection.handle_incoming_packet(encoded).await;
        assert!(result.is_err(), "Should bail on missing event key");
        assert_eq!(result.unwrap_err().to_string(), "Event packet missing key");
    }

    #[tokio::test]
    async fn test_event_dispatcher_preserves_packet_order() {
        let (connection, event_queue_rx) = create_test_connection_with_event_queue_rx();
        connection.state.store(ConnectionState::Ready);

        let (handled_tx, mut handled_rx) = unbounded_channel();
        connection.on("ordered", move |_connection, payload: Arc<String>| {
            let handled_tx = handled_tx.clone();
            async move {
                handled_tx.send(format!("start:{payload}")).unwrap();
                if payload.as_str() == "first" {
                    sleep(Duration::from_millis(25)).await;
                }

                handled_tx.send(format!("end:{payload}")).unwrap();
                Ok(())
            }
        });

        connection.start_event_dispatcher(event_queue_rx).await;
        let packet_codec = connection.namespace.config.packet_codec;
        for payload in ["first", "second"] {
            let packet_data = packet_codec.encode_data(&payload).unwrap();
            let encoded_packet = packet_codec
                .encode(&WsIoPacket::new_event("ordered", Some(packet_data)))
                .unwrap();

            connection.handle_incoming_packet(&encoded_packet).await.unwrap();
        }

        let mut handled = Vec::with_capacity(4);
        for _ in 0..4 {
            handled.push(
                timeout(Duration::from_secs(1), handled_rx.recv())
                    .await
                    .unwrap()
                    .unwrap(),
            );
        }

        assert_eq!(handled, ["start:first", "end:first", "start:second", "end:second"]);

        connection.cleanup().await;
    }

    #[tokio::test]
    async fn test_connection_close_state_transitions() {
        let connection = create_test_connection();
        assert_eq!(connection.state.get(), ConnectionState::Created);

        connection.close();
        assert_eq!(connection.state.get(), ConnectionState::Closing);

        // Calling close again when Closing shouldn't alter anything
        connection.close();
        assert_eq!(connection.state.get(), ConnectionState::Closing);
    }

    #[tokio::test]
    async fn test_connection_cleanup() {
        let connection = create_test_connection();
        let namespace = connection.namespace();

        // Insert connection manually for test
        namespace.insert_connection(&connection);
        assert_eq!(namespace.connection_count(), 1);

        connection.join(["room_a", "room_b"]);
        assert!(connection.joined_rooms.contains("room_a"));

        connection.cleanup().await;

        assert_eq!(connection.state.get(), ConnectionState::Closed);
        assert!(connection.joined_rooms.is_empty());
        assert_eq!(namespace.connection_count(), 0);
    }
}

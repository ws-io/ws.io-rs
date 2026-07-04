use std::sync::{
    Arc,
    LazyLock,
};

use anyhow::{
    Result,
    bail,
};
use arc_swap::ArcSwap;
use kikiutils::atomic::enum_cell::AtomicEnumCell;
use num_enum::{
    IntoPrimitive,
    TryFromPrimitive,
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

use crate::{
    WsIoClient,
    core::{
        channel_capacity_from_websocket_config,
        packet::{
            WsIoPacket,
            WsIoPacketType,
        },
        traits::task::spawner::TaskSpawner,
        utils::task::abort_locked_task,
    },
    runtime::WsIoClientRuntime,
};

// Enums
#[repr(u8)]
#[derive(Debug, Eq, IntoPrimitive, PartialEq, TryFromPrimitive)]
enum SessionState {
    AwaitingInit,
    AwaitingReady,
    Closed,
    Closing,
    Created,
    Initiating,
    Ready,
    Readying,
}

// Structs
#[derive(Debug)]
pub struct WsIoClientSession {
    cancel_token: ArcSwap<CancellationToken>,
    init_timeout_task: Mutex<Option<JoinHandle<()>>>,
    message_tx: Sender<Arc<Message>>,
    ping_task: Mutex<Option<JoinHandle<()>>>,
    ready_timeout_task: Mutex<Option<JoinHandle<()>>>,
    runtime: Arc<WsIoClientRuntime>,
    state: AtomicEnumCell<SessionState>,
}

impl TaskSpawner for WsIoClientSession {
    #[inline]
    fn cancel_token(&self) -> Arc<CancellationToken> {
        self.cancel_token.load_full()
    }
}

impl WsIoClientSession {
    #[inline]
    pub(crate) fn new(runtime: Arc<WsIoClientRuntime>) -> (Arc<Self>, Receiver<Arc<Message>>) {
        let channel_capacity = channel_capacity_from_websocket_config(&runtime.config.websocket_config);
        let (message_tx, message_rx) = channel(channel_capacity);
        (
            Arc::new(Self {
                cancel_token: ArcSwap::new(Arc::new(CancellationToken::new())),
                init_timeout_task: Mutex::new(None),
                message_tx,
                ping_task: Mutex::new(None),
                ready_timeout_task: Mutex::new(None),
                runtime,
                state: AtomicEnumCell::new(SessionState::Created),
            }),
            message_rx,
        )
    }

    // Private methods
    #[inline]
    fn handle_disconnect_packet(&self) -> Result<()> {
        #[cfg(feature = "tracing")]
        tracing::debug!("received server disconnect packet");
        let runtime = self.runtime.clone();
        spawn(async move { runtime.disconnect().await });
        Ok(())
    }

    #[inline]
    fn handle_event_packet(self: &Arc<Self>, event: &str, packet_data: Option<Vec<u8>>) -> Result<()> {
        #[cfg(feature = "tracing")]
        tracing::trace!(event, has_data = packet_data.is_some(), "received server event packet");
        self.runtime.event_registry.dispatch_event_packet(
            self.clone(),
            event,
            &self.runtime.config.packet_codec,
            packet_data,
            &self.runtime,
        );

        Ok(())
    }

    async fn handle_init_packet(self: &Arc<Self>, packet_data: Option<&[u8]>) -> Result<()> {
        // Verify current state; only valid from AwaitingInit → Initiating
        let state = self.state.get();
        match state {
            SessionState::AwaitingInit => self.state.try_transition(state, SessionState::Initiating)?,
            _ => {
                #[cfg(feature = "tracing")]
                tracing::debug!(?state, "received init packet in invalid client session state");
                bail!("Received init packet in invalid state: {state:?}");
            },
        }

        #[cfg(feature = "tracing")]
        tracing::debug!("received server init packet");

        // Abort init-timeout task
        abort_locked_task(&self.init_timeout_task).await;

        // Invoke init_handler with timeout protection if configured
        let response_data = if let Some(init_handler) = &self.runtime.config.init_handler {
            match timeout(
                self.runtime.config.init_handler_timeout,
                init_handler(self.clone(), packet_data, &self.runtime.config.packet_codec),
            )
            .await
            {
                Ok(result) => result?,
                Err(err) => {
                    #[cfg(feature = "tracing")]
                    tracing::warn!(error = %err, "client init handler timed out");
                    return Err(err.into());
                },
            }
        } else {
            None
        };

        // Transition state to AwaitingReady
        self.state
            .try_transition(SessionState::Initiating, SessionState::AwaitingReady)?;

        // Spawn ready-timeout watchdog to close session if Ready is not received in time
        let session = self.clone();
        *self.ready_timeout_task.lock().await = Some(spawn(async move {
            sleep(session.runtime.config.ready_packet_timeout).await;
            if session.state.is(SessionState::AwaitingReady) {
                #[cfg(feature = "tracing")]
                tracing::warn!("timed out waiting for server ready packet");
                session.close();
            }
        }));

        // Send init packet
        self.send_packet(&WsIoPacket::new_init(response_data)).await
    }

    async fn handle_ready_packet(self: &Arc<Self>) -> Result<()> {
        // Verify current state; only valid from AwaitingReady → Ready
        let state = self.state.get();
        match state {
            SessionState::AwaitingReady => self.state.try_transition(state, SessionState::Ready)?,
            _ => {
                #[cfg(feature = "tracing")]
                tracing::debug!(?state, "received ready packet in invalid client session state");
                bail!("Received ready packet in invalid state: {state:?}");
            },
        }

        #[cfg(feature = "tracing")]
        tracing::debug!("client session is ready");

        // Abort ready-timeout task
        abort_locked_task(&self.ready_timeout_task).await;

        // Wake send event message task
        self.runtime.wake_send_event_message_task_notify.notify_waiters();

        // Invoke on_session_ready_handler if configured
        if let Some(on_session_ready_handler) = self.runtime.config.on_session_ready_handler.clone() {
            // Run handler asynchronously in a detached task
            self.spawn_task(on_session_ready_handler(self.clone()));
        }

        Ok(())
    }

    async fn send_message(&self, message: Arc<Message>) -> Result<()> {
        Ok(self.message_tx.send(message).await?)
    }

    async fn send_packet(&self, packet: &WsIoPacket) -> Result<()> {
        self.send_message(self.runtime.encode_packet_to_message(packet)?).await
    }

    // Protected methods
    pub(crate) async fn cleanup(self: &Arc<Self>) {
        #[cfg(feature = "tracing")]
        tracing::debug!("cleaning up client session");
        // Set state to Closing
        self.state.store(SessionState::Closing);

        // Abort tasks
        abort_locked_task(&self.init_timeout_task).await;
        abort_locked_task(&self.ping_task).await;
        abort_locked_task(&self.ready_timeout_task).await;

        // Cancel all ongoing operations via cancel token
        self.cancel_token.load().cancel();

        // Invoke on_session_close_handler with timeout protection if configured
        if let Some(on_session_close_handler) = &self.runtime.config.on_session_close_handler
            && let Err(_err) = timeout(
                self.runtime.config.on_session_close_handler_timeout,
                on_session_close_handler(self.clone()),
            )
            .await
        {
            #[cfg(feature = "tracing")]
            tracing::warn!(error = %_err, "client session close handler timed out");
        }

        // Set state to Closed
        self.state.store(SessionState::Closed);

        #[cfg(feature = "tracing")]
        tracing::debug!("client session closed");
    }

    #[inline]
    pub(crate) fn close(&self) {
        // Skip if session is already Closing or Closed, otherwise set state to Closing
        match self.state.get() {
            SessionState::Closed | SessionState::Closing => return,
            _state => {
                #[cfg(feature = "tracing")]
                tracing::debug!(state = ?_state, "closing client session");
                self.state.store(SessionState::Closing)
            },
        }

        // Send websocket close frame to initiate graceful shutdown
        let _ = self.message_tx.try_send(Arc::new(Message::Close(None)));
    }

    pub(crate) async fn emit_event_message(&self, message: Arc<Message>) -> Result<()> {
        self.state.ensure(SessionState::Ready, |state| {
            format!("Cannot emit event message in invalid state: {state:?}")
        })?;

        self.send_message(message).await
    }

    pub(crate) async fn handle_incoming_packet(self: &Arc<Self>, encoded_packet: &[u8]) -> Result<()> {
        // TODO: lazy load
        let packet = match self.runtime.config.packet_codec.decode(encoded_packet) {
            Ok(packet) => packet,
            Err(err) => {
                #[cfg(feature = "tracing")]
                tracing::debug!(error = %err, "failed to decode server packet");
                return Err(err);
            },
        };
        match packet.r#type {
            WsIoPacketType::Disconnect => self.handle_disconnect_packet(),
            WsIoPacketType::Event => {
                if self.is_ready() {
                    if let Some(event) = packet.key.as_deref() {
                        return self.handle_event_packet(event, packet.data);
                    } else {
                        bail!("Event packet missing key");
                    }
                }

                Ok(())
            },
            WsIoPacketType::Init => self.handle_init_packet(packet.data.as_deref()).await,
            WsIoPacketType::Ready => self.handle_ready_packet().await,
        }
    }

    pub(crate) async fn init(self: &Arc<Self>) {
        self.state.store(SessionState::AwaitingInit);
        #[cfg(feature = "tracing")]
        tracing::debug!("client session awaiting server init packet");
        let session = self.clone();

        // Create init-timeout watchdog to close session if init not received in time
        *self.init_timeout_task.lock().await = Some(spawn(async move {
            sleep(session.runtime.config.init_packet_timeout).await;
            if session.state.is(SessionState::AwaitingInit) {
                #[cfg(feature = "tracing")]
                tracing::warn!("timed out waiting for server init packet");
                session.close();
            }
        }));

        // Create ping task to send 1-byte heartbeat frame to keep the connection alive
        let session = self.clone();
        *self.ping_task.lock().await = Some(spawn(async move {
            loop {
                sleep(session.runtime.config.ping_interval).await;
                if session.send_message(PING_MESSAGE.clone()).await.is_err() {
                    #[cfg(feature = "tracing")]
                    tracing::warn!("failed to send client heartbeat; closing session");
                    session.close();
                }
            }
        }));
    }

    // Public methods
    #[inline]
    pub fn client(&self) -> WsIoClient {
        WsIoClient(self.runtime.clone())
    }

    #[inline]
    pub fn is_ready(&self) -> bool {
        self.state.is(SessionState::Ready)
    }
}

// Constants/Statics
static PING_MESSAGE: LazyLock<Arc<Message>> = LazyLock::new(|| Arc::new(Message::Binary(vec![0x01].into())));

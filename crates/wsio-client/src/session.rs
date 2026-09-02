use std::{
    panic::AssertUnwindSafe,
    sync::{
        Arc,
        LazyLock,
    },
};

use anyhow::{
    Result,
    anyhow,
    bail,
};
use arc_swap::ArcSwap;
use futures_util::FutureExt;
use kikiutils::atomic::enum_cell::AtomicEnumCell;
use num_enum::{
    IntoPrimitive,
    TryFromPrimitive,
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

#[derive(Debug)]
pub struct WsIoClientSession {
    cancel_token: ArcSwap<CancellationToken>,
    event_dispatcher_task: Mutex<Option<JoinHandle<()>>>,
    event_queue_tx: Sender<WsIoPacket>,
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
    pub(crate) fn new(runtime: Arc<WsIoClientRuntime>) -> (Arc<Self>, Receiver<Arc<Message>>, Receiver<WsIoPacket>) {
        let channel_capacity = channel_capacity_from_websocket_config(&runtime.config.websocket_config);
        let (event_queue_tx, event_queue_rx) = channel(channel_capacity);
        let (message_tx, message_rx) = channel(channel_capacity);

        (
            Arc::new(Self {
                cancel_token: ArcSwap::new(Arc::new(CancellationToken::new())),
                event_dispatcher_task: Mutex::new(None),
                event_queue_tx,
                init_timeout_task: Mutex::new(None),
                message_tx,
                ping_task: Mutex::new(None),
                ready_timeout_task: Mutex::new(None),
                runtime,
                state: AtomicEnumCell::new(SessionState::Created),
            }),
            message_rx,
            event_queue_rx,
        )
    }

    // Private methods
    #[inline]
    #[allow(
        clippy::unnecessary_wraps,
        reason = "packet handlers share a fallible dispatch interface"
    )]
    fn handle_disconnect_packet(&self) -> Result<()> {
        #[cfg(feature = "tracing")]
        tracing::debug!("received server disconnect packet");
        let runtime = self.runtime.clone();
        spawn(async move { runtime.disconnect().await });
        Ok(())
    }

    #[inline]
    async fn handle_event_packet(self: &Arc<Self>, packet: WsIoPacket) -> Result<()> {
        let Some(_event) = packet.key.as_deref() else {
            bail!("Event packet missing key");
        };

        #[cfg(feature = "tracing")]
        tracing::trace!(
            event = _event,
            has_data = packet.data.is_some(),
            "received server event packet"
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
        if state == SessionState::AwaitingInit {
            self.state.try_transition(state, SessionState::Initiating)?;
        } else {
            #[cfg(feature = "tracing")]
            tracing::debug!(?state, "received init packet in invalid client session state");
            bail!("Received init packet in invalid state: {state:?}");
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
        if state == SessionState::AwaitingReady {
            self.state.try_transition(state, SessionState::Ready)?;
        } else {
            #[cfg(feature = "tracing")]
            tracing::debug!(?state, "received ready packet in invalid client session state");
            bail!("Received ready packet in invalid state: {state:?}");
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
    pub(super) async fn cleanup(self: &Arc<Self>) {
        #[cfg(feature = "tracing")]
        tracing::debug!("cleaning up client session");

        // Set state to Closing
        self.state.store(SessionState::Closing);

        // Abort tasks
        let event_dispatcher_task = self.event_dispatcher_task.lock().await.take();
        if let Some(event_dispatcher_task) = event_dispatcher_task {
            event_dispatcher_task.abort();
            let _ = event_dispatcher_task.await;
        }

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
    pub(super) fn close(&self) {
        // Skip if session is already Closing or Closed, otherwise set state to Closing
        match self.state.get() {
            SessionState::Closed | SessionState::Closing => return,
            _state => {
                #[cfg(feature = "tracing")]
                tracing::debug!(state = ?_state, "closing client session");
                self.state.store(SessionState::Closing);
            },
        }

        // Send websocket close frame to initiate graceful shutdown
        let _ = self.message_tx.try_send(Arc::new(Message::Close(None)));
    }

    pub(super) async fn emit_event_message(&self, message: Arc<Message>) -> Result<()> {
        self.state.ensure(SessionState::Ready, |state| {
            format!("Cannot emit event message in invalid state: {state:?}")
        })?;

        self.send_message(message).await
    }

    pub(super) async fn handle_incoming_packet(self: &Arc<Self>, encoded_packet: &[u8]) -> Result<()> {
        // TODO: lazy load
        let packet = match self.runtime.config.packet_codec.decode(encoded_packet) {
            Ok(packet) => packet,
            Err(err) => {
                #[cfg(feature = "tracing")]
                tracing::debug!(error = %err, "failed to decode server packet");
                return Err(err);
            },
        };

        match &packet.r#type {
            WsIoPacketType::Disconnect => self.handle_disconnect_packet(),
            WsIoPacketType::Event => {
                if self.is_ready() {
                    return self.handle_event_packet(packet).await;
                }

                Ok(())
            },
            WsIoPacketType::Init => self.handle_init_packet(packet.data.as_deref()).await,
            WsIoPacketType::Ready => self.handle_ready_packet().await,
        }
    }

    pub(super) async fn init(self: &Arc<Self>) {
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

    pub(super) async fn start_event_dispatcher(self: &Arc<Self>, mut event_queue_rx: Receiver<WsIoPacket>) {
        let cancel_token = self.cancel_token();
        let session = self.clone();
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

                    if let Err(_err) = session
                        .runtime
                        .event_registry
                        .dispatch_event_packet(
                            session.clone(),
                            event,
                            &session.runtime.config.packet_codec,
                            event_packet.data,
                            &cancel_token,
                        )
                        .await
                    {
                        #[cfg(feature = "tracing")]
                        tracing::warn!(error = %_err, "client event dispatcher failed; closing session");
                        session.close();
                        break;
                    }
                }
            };

            if AssertUnwindSafe(dispatcher).catch_unwind().await.is_err() {
                #[cfg(feature = "tracing")]
                tracing::error!("client event dispatcher panicked; closing session");
                session.close();
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

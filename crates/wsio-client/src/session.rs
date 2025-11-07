use std::sync::Arc;

use anyhow::{
    Result,
    bail,
};
use arc_swap::ArcSwap;
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
        atomic::status::AtomicStatus,
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
enum SessionStatus {
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
pub struct WsIoClientSession {
    cancel_token: ArcSwap<CancellationToken>,
    init_timeout_task: Mutex<Option<JoinHandle<()>>>,
    message_tx: Sender<Arc<Message>>,
    ready_timeout_task: Mutex<Option<JoinHandle<()>>>,
    runtime: Arc<WsIoClientRuntime>,
    status: AtomicStatus<SessionStatus>,
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
                ready_timeout_task: Mutex::new(None),
                runtime,
                status: AtomicStatus::new(SessionStatus::Created),
            }),
            message_rx,
        )
    }

    // Private methods
    #[inline]
    fn handle_disconnect_packet(&self) -> Result<()> {
        let runtime = self.runtime.clone();
        spawn(async move { runtime.disconnect().await });
        Ok(())
    }

    #[inline]
    fn handle_event_packet(self: &Arc<Self>, event: &str, packet_data: Option<Vec<u8>>) -> Result<()> {
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
        let status = self.status.get();
        match status {
            SessionStatus::AwaitingInit => self.status.try_transition(status, SessionStatus::Initiating)?,
            _ => bail!("Received init packet in invalid status: {status:?}"),
        }

        // Abort init-timeout task if still active
        abort_locked_task(&self.init_timeout_task).await;

        // Invoke init_handler with timeout protection if configured
        let response_data = if let Some(init_handler) = &self.runtime.config.init_handler {
            timeout(
                self.runtime.config.init_handler_timeout,
                init_handler(self.clone(), packet_data, &self.runtime.config.packet_codec),
            )
            .await??
        } else {
            None
        };

        // Transition state to AwaitingReady
        self.status
            .try_transition(SessionStatus::Initiating, SessionStatus::AwaitingReady)?;

        // Spawn ready-timeout watchdog to close session if Ready is not received in time
        let session = self.clone();
        *self.ready_timeout_task.lock().await = Some(spawn(async move {
            sleep(session.runtime.config.ready_packet_timeout).await;
            if session.status.is(SessionStatus::AwaitingReady) {
                session.close();
            }
        }));

        // Send init packet
        self.send_packet(&WsIoPacket::new_init(response_data)).await
    }

    async fn handle_ready_packet(self: &Arc<Self>) -> Result<()> {
        // Verify current state; only valid from AwaitingReady → Ready
        let status = self.status.get();
        match status {
            SessionStatus::AwaitingReady => self.status.try_transition(status, SessionStatus::Ready)?,
            _ => bail!("Received ready packet in invalid status: {status:?}"),
        }

        // Abort ready-timeout task if still active
        abort_locked_task(&self.ready_timeout_task).await;

        // Wake event message flush task
        self.runtime.event_message_flush_notify.notify_waiters();

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
        // Set state to Closing
        self.status.store(SessionStatus::Closing);

        // Abort timeout tasks if still active
        abort_locked_task(&self.init_timeout_task).await;
        abort_locked_task(&self.ready_timeout_task).await;

        // Cancel all ongoing operations via cancel token
        self.cancel_token.load().cancel();

        // Invoke on_session_close_handler with timeout protection if configured
        if let Some(on_session_close_handler) = &self.runtime.config.on_session_close_handler {
            let _ = timeout(
                self.runtime.config.on_session_close_handler_timeout,
                on_session_close_handler(self.clone()),
            )
            .await;
        }

        // Set state to Closed
        self.status.store(SessionStatus::Closed);
    }

    #[inline]
    pub(crate) fn close(&self) {
        // Skip if session is already Closing or Closed, otherwise set state to Closing
        match self.status.get() {
            SessionStatus::Closed | SessionStatus::Closing => return,
            _ => self.status.store(SessionStatus::Closing),
        }

        // Send websocket close frame to initiate graceful shutdown
        let _ = self.message_tx.try_send(Arc::new(Message::Close(None)));
    }

    pub(crate) async fn emit_event_message(&self, message: Arc<Message>) -> Result<()> {
        self.status.ensure(SessionStatus::Ready, |status| {
            format!("Cannot emit event message in invalid status: {status:?}")
        })?;

        self.send_message(message).await
    }

    pub(crate) async fn handle_incoming_packet(self: &Arc<Self>, encoded_packet: &[u8]) -> Result<()> {
        // TODO: lazy load
        let packet = self.runtime.config.packet_codec.decode(encoded_packet)?;
        match packet.r#type {
            WsIoPacketType::Disconnect => self.handle_disconnect_packet(),
            WsIoPacketType::Event => {
                if let Some(event) = packet.key.as_deref() {
                    self.handle_event_packet(event, packet.data)
                } else {
                    bail!("Event packet missing key");
                }
            }
            WsIoPacketType::Init => self.handle_init_packet(packet.data.as_deref()).await,
            WsIoPacketType::Ready => self.handle_ready_packet().await,
        }
    }

    pub(crate) async fn init(self: &Arc<Self>) {
        self.status.store(SessionStatus::AwaitingInit);
        let session = self.clone();
        *self.init_timeout_task.lock().await = Some(spawn(async move {
            sleep(session.runtime.config.init_packet_timeout).await;
            if session.status.is(SessionStatus::AwaitingInit) {
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
        self.status.is(SessionStatus::Ready)
    }
}

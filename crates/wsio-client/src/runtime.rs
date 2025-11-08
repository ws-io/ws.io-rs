use std::sync::Arc;

use anyhow::Result;
use arc_swap::{
    ArcSwap,
    ArcSwapOption,
};
use futures_util::{
    SinkExt,
    StreamExt,
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
        Notify,
        mpsc::{
            Receiver,
            Sender,
            channel,
        },
    },
    task::JoinHandle,
    time::sleep,
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::Message,
};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{
    config::WsIoClientConfig,
    core::{
        atomic::status::AtomicStatus,
        channel_capacity_from_websocket_config,
        event::registry::WsIoEventRegistry,
        packet::WsIoPacket,
        traits::task::spawner::TaskSpawner,
    },
    session::WsIoClientSession,
};

// Enums
#[repr(u8)]
#[derive(Debug, Eq, IntoPrimitive, PartialEq, TryFromPrimitive)]
enum RuntimeStatus {
    Running,
    Stopped,
    Stopping,
}

// Structs
pub(crate) struct WsIoClientRuntime {
    cancel_token: ArcSwap<CancellationToken>,
    pub(crate) config: WsIoClientConfig,
    connect_url: Url,
    connection_loop_task: Mutex<Option<JoinHandle<()>>>,
    pub(crate) event_message_flush_notify: Notify,
    event_message_flush_task: Mutex<Option<JoinHandle<()>>>,
    event_message_send_rx: Mutex<Receiver<Arc<Message>>>,
    event_message_send_tx: Sender<Arc<Message>>,
    pub(crate) event_registry: WsIoEventRegistry<WsIoClientSession, WsIoClientRuntime>,
    operate_lock: Mutex<()>,
    session: ArcSwapOption<WsIoClientSession>,
    status: AtomicStatus<RuntimeStatus>,
    wake_reconnect_wait_notify: Notify,
}

impl TaskSpawner for WsIoClientRuntime {
    #[inline]
    fn cancel_token(&self) -> Arc<CancellationToken> {
        self.cancel_token.load_full()
    }
}

impl WsIoClientRuntime {
    pub(crate) fn new(config: WsIoClientConfig, connect_url: Url) -> Arc<Self> {
        let channel_capacity = channel_capacity_from_websocket_config(&config.websocket_config);
        let (event_message_send_tx, event_message_send_rx) = channel(channel_capacity);
        Arc::new(Self {
            cancel_token: ArcSwap::new(Arc::new(CancellationToken::new())),
            config,
            connect_url,
            connection_loop_task: Mutex::new(None),
            event_message_flush_notify: Notify::new(),
            event_message_flush_task: Mutex::new(None),
            event_message_send_rx: Mutex::new(event_message_send_rx),
            event_message_send_tx,
            event_registry: WsIoEventRegistry::new(),
            operate_lock: Mutex::new(()),
            session: ArcSwapOption::new(None),
            status: AtomicStatus::new(RuntimeStatus::Stopped),
            wake_reconnect_wait_notify: Notify::new(),
        })
    }

    // Private methods
    async fn run_connection(self: &Arc<Self>) -> Result<()> {
        let (ws_stream, _) =
            connect_async_with_config(self.connect_url.as_str(), Some(self.config.websocket_config), false).await?;

        let (session, mut message_rx) = WsIoClientSession::new(self.clone());
        session.init().await;

        let (mut ws_stream_writer, mut ws_stream_reader) = ws_stream.split();
        let session_clone = session.clone();
        let mut read_ws_stream_task = spawn(async move {
            while let Some(message) = ws_stream_reader.next().await {
                if match message {
                    Ok(Message::Binary(bytes)) => session_clone.handle_incoming_packet(&bytes).await,
                    Ok(Message::Close(_)) => break,
                    Ok(Message::Text(text)) => session_clone.handle_incoming_packet(text.as_bytes()).await,
                    Err(_) => break,
                    _ => Ok(()),
                }
                .is_err()
                {
                    break;
                }
            }
        });

        let mut write_ws_stream_task = spawn(async move {
            while let Some(message) = message_rx.recv().await {
                let message = (*message).clone();
                let is_close = matches!(message, Message::Close(_));
                if ws_stream_writer.send(message).await.is_err() {
                    break;
                }

                if is_close {
                    let _ = ws_stream_writer.close().await;
                    break;
                }
            }
        });

        self.session.store(Some(session.clone()));
        select! {
            _ = &mut read_ws_stream_task => {
                write_ws_stream_task.abort();
            },
            _ = &mut write_ws_stream_task => {
                read_ws_stream_task.abort();
            },
        }

        self.session.store(None);
        session.cleanup().await;
        Ok(())
    }

    // Protected methods
    pub(crate) async fn connect(self: &Arc<Self>) {
        // Lock to prevent concurrent operation
        let _lock = self.operate_lock.lock().await;

        match self.status.get() {
            RuntimeStatus::Running => return,
            RuntimeStatus::Stopped => self.status.store(RuntimeStatus::Running),
            _ => unreachable!(),
        }

        // Create connection loop task
        let runtime = self.clone();
        *self.connection_loop_task.lock().await = Some(spawn(async move {
            loop {
                if !runtime.status.is(RuntimeStatus::Running) {
                    break;
                }

                let _ = runtime.run_connection().await;
                if runtime.status.is(RuntimeStatus::Running) {
                    select! {
                        _ = runtime.wake_reconnect_wait_notify.notified() => {},
                        _ = sleep(runtime.config.reconnect_delay) => {},
                    }
                }
            }
        }));

        // Create flush messages task
        let runtime = self.clone();
        *self.event_message_flush_task.lock().await = Some(spawn(async move {
            let mut event_message_send_rx = runtime.event_message_send_rx.lock().await;
            while let Some(message) = event_message_send_rx.recv().await {
                loop {
                    if let Some(session) = runtime.session.load().as_ref()
                        && session.emit_event_message(message.clone()).await.is_ok()
                    {
                        break;
                    }

                    runtime.event_message_flush_notify.notified().await;
                }
            }
        }));
    }

    pub(crate) async fn disconnect(&self) {
        // Lock to prevent concurrent operation
        let _lock = self.operate_lock.lock().await;

        match self.status.get() {
            RuntimeStatus::Stopped => return,
            RuntimeStatus::Running => self.status.store(RuntimeStatus::Stopping),
            _ => unreachable!(),
        }

        // Close session
        if let Some(session) = self.session.load().as_ref() {
            session.close();
        }

        // Cancel all ongoing operations via cancel token and store a new one
        self.cancel_token.load().cancel();
        self.cancel_token.store(Arc::new(CancellationToken::new()));

        // Abort event-message-flush task if still active
        if let Some(event_message_flush_task) = self.event_message_flush_task.lock().await.take() {
            event_message_flush_task.abort();
        }

        // Drop all pending event messages in the channel
        let mut event_message_send_rx = self.event_message_send_rx.lock().await;
        while event_message_send_rx.try_recv().is_ok() {}

        // Wake reconnect loop to break out of sleep early
        self.wake_reconnect_wait_notify.notify_waiters();

        // Await connection loop task termination
        if let Some(connection_loop_task) = self.connection_loop_task.lock().await.take() {
            let _ = connection_loop_task.await;
        }

        self.status.store(RuntimeStatus::Stopped);
    }

    pub(crate) async fn emit<D: Serialize>(&self, event: &str, data: Option<&D>) -> Result<()> {
        self.status.ensure(RuntimeStatus::Running, |status| {
            format!("Cannot emit in invalid status: {status:?}")
        })?;

        self.event_message_send_tx
            .send(
                self.encode_packet_to_message(&WsIoPacket::new_event(
                    event,
                    data.map(|data| self.config.packet_codec.encode_data(data))
                        .transpose()?,
                ))?,
            )
            .await?;

        Ok(())
    }

    #[inline]
    pub(crate) fn encode_packet_to_message(&self, packet: &WsIoPacket) -> Result<Arc<Message>> {
        let bytes = self.config.packet_codec.encode(packet)?;
        Ok(Arc::new(match self.config.packet_codec.is_text() {
            true => Message::Text(unsafe { String::from_utf8_unchecked(bytes).into() }),
            false => Message::Binary(bytes.into()),
        }))
    }

    #[inline]
    pub(crate) fn is_session_ready(&self) -> bool {
        self.session.load().as_ref().map_or(false, |session| session.is_ready())
    }

    #[inline]
    pub(crate) fn off(&self, event: &str) {
        self.event_registry.off(event);
    }

    #[inline]
    pub(crate) fn off_by_handler_id(&self, event: &str, handler_id: u32) {
        self.event_registry.off_by_handler_id(event, handler_id);
    }

    #[inline]
    pub(crate) fn on<H, Fut, D>(&self, event: &str, handler: H) -> u32
    where
        H: Fn(Arc<WsIoClientSession>, Arc<D>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
        D: DeserializeOwned + Send + Sync + 'static,
    {
        self.event_registry.on(event, handler)
    }
}

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
use kikiutils::atomic::enum_cell::AtomicEnumCell;
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
    pub(crate) event_registry: WsIoEventRegistry<WsIoClientSession, WsIoClientRuntime>,
    operate_lock: Mutex<()>,
    send_event_message_rx: Mutex<Receiver<Arc<Message>>>,
    send_event_message_task: Mutex<Option<JoinHandle<()>>>,
    send_event_message_tx: Sender<Arc<Message>>,
    session: ArcSwapOption<WsIoClientSession>,
    status: AtomicEnumCell<RuntimeStatus>,
    pub(crate) wake_send_event_message_task_notify: Notify,
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
        let (send_event_message_tx, send_event_message_rx) = channel(channel_capacity);
        Arc::new(Self {
            cancel_token: ArcSwap::new(Arc::new(CancellationToken::new())),
            config,
            connect_url,
            connection_loop_task: Mutex::new(None),
            event_registry: WsIoEventRegistry::new(),
            operate_lock: Mutex::new(()),
            send_event_message_rx: Mutex::new(send_event_message_rx),
            send_event_message_task: Mutex::new(None),
            send_event_message_tx,
            session: ArcSwapOption::new(None),
            status: AtomicEnumCell::new(RuntimeStatus::Stopped),
            wake_send_event_message_task_notify: Notify::new(),
        })
    }

    // Private methods
    async fn run_connection(self: &Arc<Self>) -> Result<()> {
        // Connect to server
        let (ws_stream, _) =
            connect_async_with_config(self.connect_url.as_str(), Some(self.config.websocket_config), false).await?;

        // Create session and init
        let (session, mut message_rx) = WsIoClientSession::new(self.clone());
        session.init().await;

        // Create read and write tasks
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

        // Wait for any of the tasks to finish or canceled
        let cancel_token = self.cancel_token();
        select! {
            _ = cancel_token.cancelled() => {
                session.close();
                select! {
                    _ = &mut read_ws_stream_task => {
                        write_ws_stream_task.abort();
                    },
                    _ = &mut write_ws_stream_task => {
                        read_ws_stream_task.abort();
                    },
                }
            }
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

        // Create new cancel token
        self.cancel_token.store(Arc::new(CancellationToken::new()));

        // Create connection loop task
        let runtime = self.clone();
        *self.connection_loop_task.lock().await = Some(spawn(async move {
            while runtime.status.is(RuntimeStatus::Running) {
                #[cfg(feature = "tracing")]
                if let Err(err) = runtime.run_connection().await {
                    tracing::error!("Failed to run connection: {err:#?}");
                }

                #[cfg(not(feature = "tracing"))]
                let _ = runtime.run_connection().await;
                if runtime.status.is(RuntimeStatus::Running) {
                    let cancel_token = runtime.cancel_token();
                    select! {
                        _ = cancel_token.cancelled() => {},
                        _ = sleep(runtime.config.reconnect_delay) => {},
                    }
                }
            }
        }));

        // Create send event message task
        let runtime = self.clone();
        *self.send_event_message_task.lock().await = Some(spawn(async move {
            let mut send_event_message_rx = runtime.send_event_message_rx.lock().await;
            while let Some(message) = send_event_message_rx.recv().await {
                loop {
                    let notified = runtime.wake_send_event_message_task_notify.notified();
                    if let Some(session) = runtime.session.load().as_ref()
                        && session.emit_event_message(message.clone()).await.is_ok()
                    {
                        break;
                    }

                    notified.await;
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

        // Abort send-event-message task
        if let Some(send_event_message_task) = self.send_event_message_task.lock().await.take() {
            send_event_message_task.abort();
        }

        // Cancel token to abort all waiting operations (ongoing operations, connection loop task)
        self.cancel_token.load().cancel();

        // Drop all pending event messages in the channel
        let mut send_event_message_rx = self.send_event_message_rx.lock().await;
        while send_event_message_rx.try_recv().is_ok() {}

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

        self.send_event_message_tx
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
        self.session.load().as_ref().is_some_and(|session| session.is_ready())
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

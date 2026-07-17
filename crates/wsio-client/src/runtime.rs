use std::{
    sync::Arc,
    time::Duration,
};

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
    time::{
        sleep,
        timeout,
    },
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{
        Message,
        client::IntoClientRequest,
    },
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
enum CompletedWebSocketTask {
    Read,
    Write,
}

// Structs
#[repr(u8)]
#[derive(Debug, Eq, IntoPrimitive, PartialEq, TryFromPrimitive)]
enum RuntimeStatus {
    Running,
    Stopped,
    Stopping,
}

// Structs
#[derive(Debug)]
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
        #[cfg(feature = "tracing")]
        tracing::debug!(
            scheme = self.connect_url.scheme(),
            host = self.connect_url.host_str(),
            port = self.connect_url.port(),
            path = self.connect_url.path(),
            connect_timeout_ms = self.config.connect_timeout.map(|duration| duration.as_millis() as u64),
            "starting WebSocket connection"
        );

        let mut request = self.connect_url.as_str().into_client_request()?;
        if let Some(modifier) = &self.config.request_modifier {
            #[cfg(feature = "tracing")]
            tracing::trace!("applying WebSocket request modifier");
            request = modifier(request).await?;
        }

        let connect = connect_async_with_config(request, Some(self.config.websocket_config), false);
        let (ws_stream, _) = if let Some(connect_timeout) = self.config.connect_timeout {
            match timeout(connect_timeout, connect).await {
                Ok(result) => result?,
                Err(err) => {
                    #[cfg(feature = "tracing")]
                    tracing::warn!(
                        error = %err,
                        timeout_ms = connect_timeout.as_millis() as u64,
                        "WebSocket connection timed out"
                    );

                    return Err(err.into());
                },
            }
        } else {
            connect.await?
        };

        #[cfg(feature = "tracing")]
        tracing::debug!("WebSocket connection established");

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
                    Ok(Message::Close(_)) => {
                        #[cfg(feature = "tracing")]
                        tracing::debug!("WebSocket read task received close frame");
                        break;
                    },
                    Err(_err) => {
                        #[cfg(feature = "tracing")]
                        tracing::debug!(error = %_err, "WebSocket read task failed");
                        break;
                    },
                    Ok(Message::Text(text)) => session_clone.handle_incoming_packet(text.as_bytes()).await,
                    _ => Ok(()),
                }
                .is_err()
                {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("WebSocket read task stopped after packet handling error");
                    break;
                }
            }
        });

        let mut write_ws_stream_task = spawn(async move {
            while let Some(message) = message_rx.recv().await {
                let message = (*message).clone();
                let is_close = matches!(message, Message::Close(_));
                if ws_stream_writer.send(message).await.is_err() {
                    #[cfg(feature = "tracing")]
                    tracing::warn!("WebSocket write task failed to send message");
                    break;
                }

                if is_close {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("WebSocket write task sent close frame");
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
                #[cfg(feature = "tracing")]
                tracing::debug!("client connection cancellation requested");
                session.close();
                shutdown_websocket_tasks(
                    read_ws_stream_task,
                    write_ws_stream_task,
                    self.config.disconnect_timeout,
                )
                .await;
            }
            _ = &mut read_ws_stream_task => {
                #[cfg(feature = "tracing")]
                tracing::debug!("client read task finished; aborting write task");
                write_ws_stream_task.abort();
                let _ = write_ws_stream_task.await;
            },
            _ = &mut write_ws_stream_task => {
                #[cfg(feature = "tracing")]
                tracing::debug!("client write task finished; aborting read task");
                read_ws_stream_task.abort();
                let _ = read_ws_stream_task.await;
            },
        }

        self.session.store(None);
        session.cleanup().await;

        #[cfg(feature = "tracing")]
        tracing::debug!("client connection stopped");
        Ok(())
    }

    // Protected methods
    pub(crate) async fn connect(self: &Arc<Self>) {
        // Lock to prevent concurrent operation
        let _lock = self.operate_lock.lock().await;

        match self.status.get() {
            RuntimeStatus::Running => {
                #[cfg(feature = "tracing")]
                tracing::trace!("connect request ignored because client is already running");
                return;
            },
            RuntimeStatus::Stopped => {
                #[cfg(feature = "tracing")]
                tracing::info!("starting client runtime");
                self.status.store(RuntimeStatus::Running)
            },
            _ => unreachable!(),
        }

        // Create new cancel token
        self.cancel_token.store(Arc::new(CancellationToken::new()));

        // Create connection loop task
        let runtime = self.clone();
        *self.connection_loop_task.lock().await = Some(spawn(async move {
            while runtime.status.is(RuntimeStatus::Running) {
                if let Err(_err) = runtime.run_connection().await {
                    #[cfg(feature = "tracing")]
                    tracing::debug!(error = %_err, "client connection attempt failed");
                }

                if runtime.status.is(RuntimeStatus::Running) {
                    let cancel_token = runtime.cancel_token();
                    #[cfg(feature = "tracing")]
                    tracing::trace!(
                        reconnect_delay_ms = runtime.config.reconnect_delay.as_millis() as u64,
                        "waiting before reconnect"
                    );

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
                #[cfg(feature = "tracing")]
                tracing::trace!("dequeued client event message for delivery");
                loop {
                    if let Some(session) = runtime.session.load().as_ref()
                        && session.emit_event_message(message.clone()).await.is_ok()
                    {
                        break;
                    }

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
            RuntimeStatus::Stopped => {
                #[cfg(feature = "tracing")]
                tracing::trace!("disconnect request ignored because client is already stopped");
                return;
            },
            RuntimeStatus::Running => {
                #[cfg(feature = "tracing")]
                tracing::debug!("stopping client runtime");
                self.status.store(RuntimeStatus::Stopping)
            },
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

        #[cfg(feature = "tracing")]
        tracing::info!("client runtime stopped");
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

        #[cfg(feature = "tracing")]
        tracing::trace!(event, has_data = data.is_some(), "queued client event message");
        Ok(())
    }

    #[inline]
    pub(crate) fn encode_packet_to_message(&self, packet: &WsIoPacket) -> Result<Arc<Message>> {
        let bytes = self.config.packet_codec.encode(packet)?;
        Ok(Arc::new(match self.config.packet_codec.is_text() {
            // SAFETY: text packet codecs only produce valid UTF-8 payloads.
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

// Functions
async fn shutdown_websocket_tasks(
    mut read_task: JoinHandle<()>,
    mut write_task: JoinHandle<()>,
    shutdown_timeout: Duration,
) {
    let graceful_shutdown = timeout(shutdown_timeout, async {
        select! {
            _ = &mut read_task => CompletedWebSocketTask::Read,
            _ = &mut write_task => CompletedWebSocketTask::Write,
        }
    })
    .await;

    match graceful_shutdown {
        Ok(CompletedWebSocketTask::Read) => {
            write_task.abort();
            let _ = write_task.await;
        },
        Ok(CompletedWebSocketTask::Write) => {
            read_task.abort();
            let _ = read_task.await;
        },
        Err(_) => {
            #[cfg(feature = "tracing")]
            tracing::warn!(
                timeout_ms = shutdown_timeout.as_millis() as u64,
                "graceful WebSocket shutdown timed out; aborting tasks"
            );

            read_task.abort();
            write_task.abort();
            let _ = read_task.await;
            let _ = write_task.await;
        },
    }
}

#[cfg(test)]
mod tests {
    use std::future::pending;

    use tokio::{
        sync::oneshot,
        time::timeout,
    };

    use super::*;

    async fn pending_task_with_drop_signal(drop_signal: oneshot::Sender<()>) {
        let _drop_signal = drop_signal;
        pending().await
    }

    async fn assert_task_dropped(drop_signal: oneshot::Receiver<()>) {
        timeout(Duration::from_secs(1), drop_signal)
            .await
            .expect("task should be dropped before timeout")
            .expect_err("task sender should be dropped");
    }

    #[tokio::test]
    async fn shutdown_websocket_tasks_handles_read_completion() {
        let (write_task_drop_signal, write_task_dropped) = oneshot::channel();
        shutdown_websocket_tasks(
            spawn(async {}),
            spawn(pending_task_with_drop_signal(write_task_drop_signal)),
            Duration::from_secs(1),
        )
        .await;

        assert_task_dropped(write_task_dropped).await;
    }

    #[tokio::test]
    async fn shutdown_websocket_tasks_handles_write_completion() {
        let (read_task_drop_signal, read_task_dropped) = oneshot::channel();
        shutdown_websocket_tasks(
            spawn(pending_task_with_drop_signal(read_task_drop_signal)),
            spawn(async {}),
            Duration::from_secs(1),
        )
        .await;

        assert_task_dropped(read_task_dropped).await;
    }

    #[tokio::test]
    async fn shutdown_websocket_tasks_aborts_both_on_timeout() {
        let (read_task_drop_signal, read_task_dropped) = oneshot::channel();
        let (write_task_drop_signal, write_task_dropped) = oneshot::channel();
        shutdown_websocket_tasks(
            spawn(pending_task_with_drop_signal(read_task_drop_signal)),
            spawn(pending_task_with_drop_signal(write_task_drop_signal)),
            Duration::ZERO,
        )
        .await;

        assert_task_dropped(read_task_dropped).await;
        assert_task_dropped(write_task_dropped).await;
    }
}

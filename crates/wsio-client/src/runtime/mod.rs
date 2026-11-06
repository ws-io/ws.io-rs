use std::sync::Arc;

use anyhow::{
    Error as AnyhowError,
    Result,
    anyhow,
    bail,
};
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
        mpsc,
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
        event::registry::WsIoEventRegistry,
        packet::WsIoPacket,
        traits::task::spawner::TaskSpawner,
        transport::{
            encode_packet_to_message,
            shutdown_websocket_stream_tasks,
        },
        utils::lifecycle::WsIoLifecycleCompletionSlot,
    },
    session::WsIoClientSession,
};

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
    connect_completion: Arc<WsIoLifecycleCompletionSlot>,
    disconnect_completion: Arc<WsIoLifecycleCompletionSlot>,
    pub(crate) event_registry: WsIoEventRegistry<WsIoClientSession>,
    lifecycle_lock: Mutex<()>,
    send_event_message_task: Mutex<Option<JoinHandle<()>>>,
    send_event_message_tx: ArcSwapOption<mpsc::Sender<Arc<Message>>>,
    session: ArcSwapOption<WsIoClientSession>,
    status: AtomicEnumCell<RuntimeStatus>,
    pub(crate) wake_send_event_message_task_notify: Notify,
}

impl TaskSpawner for WsIoClientRuntime {
    #[inline]
    fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.load().as_ref().clone()
    }
}

impl WsIoClientRuntime {
    pub(crate) fn new(config: WsIoClientConfig, connect_url: Url) -> Arc<Self> {
        Arc::new(Self {
            cancel_token: ArcSwap::new(Arc::new(CancellationToken::new())),
            config,
            connect_url,
            connection_loop_task: Mutex::new(None),
            connect_completion: Arc::new(WsIoLifecycleCompletionSlot::default()),
            disconnect_completion: Arc::new(WsIoLifecycleCompletionSlot::default()),
            event_registry: WsIoEventRegistry::new(),
            lifecycle_lock: Mutex::new(()),
            send_event_message_task: Mutex::new(None),
            send_event_message_tx: ArcSwapOption::new(None),
            session: ArcSwapOption::new(None),
            status: AtomicEnumCell::new(RuntimeStatus::Stopped),
            wake_send_event_message_task_notify: Notify::new(),
        })
    }

    // Private methods
    async fn connect_inner(self: &Arc<Self>) {
        // Lock to prevent concurrent operation
        let _lifecycle_lock = self.lifecycle_lock.lock().await;

        match self.status.get() {
            RuntimeStatus::Running => {
                #[cfg(feature = "tracing")]
                tracing::trace!("connect request ignored because client is already running");
                return;
            },
            RuntimeStatus::Stopped => {
                #[cfg(feature = "tracing")]
                tracing::info!("starting client runtime");
                self.status.store(RuntimeStatus::Running);
            },
            RuntimeStatus::Stopping => unreachable!(),
        }

        // Create new cancel token
        self.cancel_token.store(Arc::new(CancellationToken::new()));

        // Each explicit connect owns a queue; automatic reconnects reuse it.
        let (send_event_message_tx, mut send_event_message_rx) =
            mpsc::channel::<Arc<Message>>(self.config.channel_capacity);

        let runtime = Arc::clone(self);
        *self.send_event_message_task.lock().await = Some(spawn(async move {
            while let Some(message) = send_event_message_rx.recv().await {
                #[cfg(feature = "tracing")]
                tracing::trace!("dequeued client event message for delivery");
                loop {
                    if let Some(session) = runtime.session.load().as_ref()
                        && session.emit_event_message(Arc::clone(&message)).await.is_ok()
                    {
                        break;
                    }

                    let notified = runtime.wake_send_event_message_task_notify.notified();
                    if let Some(session) = runtime.session.load().as_ref()
                        && session.emit_event_message(Arc::clone(&message)).await.is_ok()
                    {
                        break;
                    }

                    notified.await;
                }
            }
        }));

        self.send_event_message_tx.store(Some(Arc::new(send_event_message_tx)));

        // Publish the queue before starting connections and their ready callbacks.
        let runtime = Arc::clone(self);
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
                        reconnect_delay_ms =
                            u64::try_from(runtime.config.reconnect_delay.as_millis()).unwrap_or(u64::MAX),
                        "waiting before reconnect"
                    );

                    select! {
                        biased;
                        () = cancel_token.cancelled() => {},
                        () = sleep(runtime.config.reconnect_delay) => {},
                    }
                }
            }
        }));
    }

    async fn disconnect_inner(&self) {
        // Lock to prevent concurrent operation
        let _lifecycle_lock = self.lifecycle_lock.lock().await;

        match self.status.get() {
            RuntimeStatus::Stopped => {
                #[cfg(feature = "tracing")]
                tracing::trace!("disconnect request ignored because client is already stopped");
                return;
            },
            RuntimeStatus::Running => {
                #[cfg(feature = "tracing")]
                tracing::debug!("stopping client runtime");
                self.status.store(RuntimeStatus::Stopping);
            },
            RuntimeStatus::Stopping => unreachable!(),
        }

        self.send_event_message_tx.store(None);

        // Cancel token to abort all waiting operations (ongoing operations, connection loop task)
        self.cancel_token.load().cancel();

        // Drop the old receiver and its pending messages before another connect.
        if let Some(send_event_message_task) = self.send_event_message_task.lock().await.take() {
            send_event_message_task.abort();
            let _ = send_event_message_task.await;
        }

        // Await connection loop task termination
        if let Some(connection_loop_task) = self.connection_loop_task.lock().await.take() {
            let _ = connection_loop_task.await;
        }

        self.status.store(RuntimeStatus::Stopped);

        #[cfg(feature = "tracing")]
        tracing::info!("client runtime stopped");
    }

    async fn run_connection(self: &Arc<Self>) -> Result<()> {
        // Connect to server
        #[cfg(feature = "tracing")]
        tracing::debug!(
            scheme = self.connect_url.scheme(),
            host = self.connect_url.host_str(),
            port = self.connect_url.port(),
            path = self.connect_url.path(),
            connect_timeout_ms = self
                .config
                .connect_timeout
                .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)),
            "starting WebSocket connection"
        );

        let cancel_token = self.cancel_token();
        let mut request = self.connect_url.as_str().into_client_request()?;
        if let Some(modifier) = &self.config.request_modifier {
            #[cfg(feature = "tracing")]
            tracing::trace!("applying WebSocket request modifier");
            request = select! {
                biased;
                () = cancel_token.cancelled() => return Ok(()),
                result = modifier(request) => result?,
            };
        }

        let websocket_handshake = connect_async_with_config(request, Some(self.config.websocket_config), false);
        let connection_attempt = async {
            if let Some(connect_timeout) = self.config.connect_timeout {
                match timeout(connect_timeout, websocket_handshake).await {
                    Ok(result) => Ok(result?),
                    Err(err) => {
                        #[cfg(feature = "tracing")]
                        tracing::warn!(
                            error = %err,
                            timeout_ms = u64::try_from(connect_timeout.as_millis()).unwrap_or(u64::MAX),
                            "WebSocket connection timed out"
                        );

                        Err(anyhow!(err))
                    },
                }
            } else {
                Ok(websocket_handshake.await?)
            }
        };

        let (ws_stream, _) = select! {
            biased;
            () = cancel_token.cancelled() => return Ok(()),
            result = connection_attempt => result?,
        };

        #[cfg(feature = "tracing")]
        tracing::debug!("WebSocket connection established");

        // Create session and init
        let (session, mut message_rx, event_queue_rx) = WsIoClientSession::new(Arc::clone(self));
        session.init().await;
        session.start_event_dispatcher(event_queue_rx).await;

        // Publish the session before transport tasks can signal readiness.
        self.session.store(Some(Arc::clone(&session)));

        // Create read and write tasks
        let (mut ws_stream_writer, mut ws_stream_reader) = ws_stream.split();
        let session_clone = Arc::clone(&session);
        let mut read_ws_stream_task = spawn(async move {
            while let Some(message) = ws_stream_reader.next().await {
                if match message {
                    Ok(Message::Binary(bytes)) => session_clone.handle_incoming_packet(bytes).await,
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
                    Ok(Message::Text(_)) => Err(anyhow!("text WebSocket frames are not supported")),
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

        // Session cancellation ends this transport without stopping the reconnect loop.
        let session_cancel_token = session.cancel_token();
        let shutdown_requested = async {
            select! {
                () = cancel_token.cancelled() => {},
                () = session_cancel_token.cancelled() => {},
            }
        };

        // Wait for any of the tasks to finish or canceled
        select! {
            () = shutdown_requested => {
                #[cfg(feature = "tracing")]
                tracing::debug!("client connection cancellation requested");
                session.close();
                shutdown_websocket_stream_tasks(
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
        let runtime = Arc::clone(self);
        self.connect_completion
            .wait_or_spawn(async move || runtime.connect_inner().await)
            .await;
    }

    pub(crate) async fn disconnect(self: &Arc<Self>) {
        let runtime = Arc::clone(self);
        self.disconnect_completion
            .wait_or_spawn(async move || runtime.disconnect_inner().await)
            .await;
    }

    pub(crate) async fn emit<D: Serialize>(&self, event: &str, data: Option<&D>) -> Result<()> {
        let send_event_message_tx = self
            .send_event_message_tx
            .load_full()
            .ok_or_else(|| anyhow!("Cannot emit while client is not running"))?;

        select! {
            biased;
            () = send_event_message_tx.closed() => bail!("Client disconnected while emitting"),
            result = async {
                let message = self.encode_packet_to_message(&WsIoPacket::new_event(
                    event,
                    data.map(|data| self.config.packet_codec.encode_data(data))
                        .transpose()?,
                ))
                .await?;

                send_event_message_tx.send(message).await?;
                Ok::<(), AnyhowError>(())
            } => result?,
        }

        #[cfg(feature = "tracing")]
        tracing::trace!(event, has_data = data.is_some(), "queued client event message");
        Ok(())
    }

    #[inline]
    pub(crate) async fn encode_packet_to_message(&self, packet: &WsIoPacket) -> Result<Arc<Message>> {
        encode_packet_to_message(
            &self.config.packet_codec,
            &self.config.packet_transformer,
            self.config.max_outgoing_message_size,
            packet,
        )
        .await
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

#[cfg(test)]
mod tests;

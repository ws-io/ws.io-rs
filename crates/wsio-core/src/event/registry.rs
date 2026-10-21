use std::{
    any::{
        Any,
        TypeId,
    },
    collections::hash_map::Entry,
    fmt::{
        Debug as FmtDebug,
        Formatter,
        Result as FmtResult,
    },
    iter::once,
    panic::AssertUnwindSafe,
    pin::Pin,
    sync::{
        Arc,
        LazyLock,
        atomic::{
            AtomicU32,
            Ordering,
        },
    },
};

use anyhow::Result;
use bytes::Bytes;
use futures_util::FutureExt;
use kikiutils::types::fx_collections::FxHashMap;
use parking_lot::RwLock;
use serde::de::DeserializeOwned;
use tokio::{
    select,
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

use crate::packet::codecs::WsIoPacketCodec;

// Types
type DataDecoder = fn(&[u8], &WsIoPacketCodec) -> Result<Arc<dyn Any + Send + Sync>>;
type Handler<C> = Arc<
    dyn Fn(Arc<C>, Arc<dyn Any + Send + Sync>) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'static>>
        + Send
        + Sync
        + 'static,
>;
type HandlerResult = Result<Result<()>, Box<dyn Any + Send>>;

// Constants/Statics
static EMPTY_EVENT_DATA_ANY_ARC: LazyLock<Arc<dyn Any + Send + Sync>> = LazyLock::new(|| Arc::new(()));

// Enums
enum HandlerSnapshot<C> {
    Single(Handler<C>),
    Multiple(Vec<Handler<C>>),
}

// Structs
struct EventEntry<C> {
    data_decoder: DataDecoder,
    data_type_id: TypeId,
    handlers: RwLock<FxHashMap<u32, Handler<C>>>,
}

impl<C> FmtDebug for EventEntry<C> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let mut debug = f.debug_struct("EventEntry");
        debug
            .field("data_decoder", &self.data_decoder)
            .field("data_type_id", &self.data_type_id);

        match self.handlers.try_read() {
            Some(handlers) => {
                debug.field("handlers_len", &handlers.len());
            },
            None => {
                debug.field("handlers", &"<locked>");
            },
        }

        debug.finish()
    }
}

#[derive(Debug)]
pub struct WsIoEventRegistry<C: Send + Sync + 'static> {
    event_entries: RwLock<FxHashMap<String, Arc<EventEntry<C>>>>,
    next_handler_id: AtomicU32,
}

impl<C: Send + Sync + 'static> Default for WsIoEventRegistry<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Send + Sync + 'static> WsIoEventRegistry<C> {
    #[inline]
    pub fn new() -> Self {
        Self {
            event_entries: RwLock::new(FxHashMap::default()),
            next_handler_id: AtomicU32::new(0),
        }
    }

    // Public methods
    /// Dispatches one event packet and waits until all selected handlers finish or are canceled.
    ///
    /// When polled, decodes `packet_data` using `packet_codec` and invokes matching
    /// handlers concurrently with shared `ctx` and decoded payload values. Handler
    /// execution order is unspecified. Without payload data, only handlers
    /// registered for `()` run. Events without registered handlers are ignored.
    /// A single handler runs inline; multiple handlers run in separate tasks.
    /// Handler errors and panics do not fail dispatch and are logged when
    /// the `tracing` feature is enabled.
    ///
    /// # Errors
    ///
    /// Returns a payload decoding error if the data cannot be decoded as the
    /// registered payload type.
    ///
    /// # Panics
    ///
    /// Panics if multiple handlers are spawned outside a Tokio runtime.
    ///
    /// # Cancellation safety
    ///
    /// Canceling `cancel_token` drops unfinished handler futures. Dropping this
    /// dispatch future drops its inline handler future or aborts its handler tasks.
    /// Neither action rolls back completed side effects; retrying may invoke handlers again.
    #[inline]
    pub async fn dispatch_event_packet(
        &self,
        ctx: Arc<C>,
        event: impl AsRef<str>,
        packet_codec: &WsIoPacketCodec,
        packet_data: Option<Bytes>,
        cancel_token: &CancellationToken,
    ) -> Result<()> {
        let event = event.as_ref();
        let Some(event_entry) = self.event_entries.read().get(event).cloned() else {
            #[cfg(feature = "tracing")]
            tracing::trace!(event, "dropping event packet without registered handlers");
            return Ok(());
        };

        let data = match packet_data {
            Some(bytes) => match (event_entry.data_decoder)(&bytes, packet_codec) {
                Ok(data) => data,
                Err(err) => {
                    #[cfg(feature = "tracing")]
                    tracing::debug!(event, error = %err, "failed to decode event packet data");
                    return Err(err);
                },
            },
            None => EMPTY_EVENT_DATA_ANY_ARC.clone(),
        };

        let handlers = {
            let handlers = event_entry.handlers.read();
            let mut handler_iter = handlers.values().cloned();
            let Some(first_handler) = handler_iter.next() else {
                return Ok(());
            };

            if handlers.len() == 1 {
                HandlerSnapshot::Single(first_handler)
            } else {
                HandlerSnapshot::Multiple(once(first_handler).chain(handler_iter).collect())
            }
        };

        match handlers {
            HandlerSnapshot::Single(handler) => {
                #[cfg(feature = "tracing")]
                tracing::trace!(event, handler_count = 1, "dispatching event handlers");

                if let Some(result) = run_handler(handler, ctx, data, cancel_token).await {
                    log_handler_result(event, result);
                }
            },
            HandlerSnapshot::Multiple(handlers) => {
                #[cfg(feature = "tracing")]
                tracing::trace!(event, handler_count = handlers.len(), "dispatching event handlers");

                let mut handler_tasks = JoinSet::new();
                for handler in handlers {
                    let ctx = Arc::clone(&ctx);
                    let data = Arc::clone(&data);
                    let cancel_token = cancel_token.clone();
                    handler_tasks.spawn(async move { run_handler(handler, ctx, data, &cancel_token).await });
                }

                while let Some(result) = handler_tasks.join_next().await {
                    match result {
                        Ok(Some(result)) => log_handler_result(event, result),
                        Ok(None) => {},
                        Err(_err) => {
                            #[cfg(feature = "tracing")]
                            tracing::debug!(event, error = %_err, "event handler task failed");
                        },
                    }
                }
            },
        }

        Ok(())
    }

    #[inline]
    pub fn off(&self, event: impl AsRef<str>) {
        let event = event.as_ref();
        let _removed = self.event_entries.write().remove(event).is_some();
        #[cfg(feature = "tracing")]
        tracing::trace!(event, removed = _removed, "removed event handlers");
    }

    #[inline]
    pub fn off_by_handler_id(&self, event: impl AsRef<str>, handler_id: u32) {
        let event = event.as_ref();
        if let Some(event_entry) = self.event_entries.read().get(event) {
            let _removed = event_entry.handlers.write().remove(&handler_id).is_some();

            #[cfg(feature = "tracing")]
            tracing::trace!(event, handler_id, removed = _removed, "removed event handler by id");
            if !event_entry.handlers.read().is_empty() {
                return;
            }
        }

        if let Entry::Occupied(entry) = self.event_entries.write().entry(event.to_owned())
            && entry.get().handlers.read().is_empty()
        {
            entry.remove();
        }
    }

    #[inline]
    pub fn on<H, Fut, D>(&self, event: impl AsRef<str>, handler: H) -> u32
    where
        H: Fn(Arc<C>, Arc<D>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
        D: DeserializeOwned + Send + Sync + 'static,
    {
        let event = event.as_ref();
        assert!(!event.is_empty(), "Event key cannot be empty");
        let data_type_id = TypeId::of::<D>();

        let mut event_entries = self.event_entries.write();
        let event_entry = match event_entries.entry(event.to_owned()) {
            Entry::Occupied(occupied) => {
                let event_entry = occupied.into_mut();
                assert_eq!(
                    event_entry.data_type_id, data_type_id,
                    "Event '{event}' already registered with a different data type — each event name must correspond to exactly one payload type."
                );

                event_entry
            },
            Entry::Vacant(vacant) => vacant.insert(Arc::new(EventEntry {
                data_decoder: decode_data_as_any_arc::<D>,
                data_type_id,
                handlers: RwLock::new(FxHashMap::default()),
            })),
        };

        let handler_id = self.next_handler_id.fetch_add(1, Ordering::Relaxed);

        #[cfg(feature = "tracing")]
        tracing::trace!(event, handler_id, "registered event handler");
        event_entry.handlers.write().insert(
            handler_id,
            Arc::new(move |connection, data| {
                if (*data).type_id() != data_type_id {
                    return Box::pin(async { Ok(()) });
                }

                Box::pin(handler(
                    connection,
                    #[allow(clippy::expect_used)]
                    data.downcast()
                        .expect("data type id matched handler registration but Arc::downcast failed"),
                ))
            }),
        );

        handler_id
    }
}

// Functions
#[inline]
fn decode_data_as_any_arc<D: DeserializeOwned + Send + Sync + 'static>(
    bytes: &[u8],
    packet_codec: &WsIoPacketCodec,
) -> Result<Arc<dyn Any + Send + Sync>> {
    Ok(Arc::new(packet_codec.decode_data::<D>(bytes)?))
}

#[inline]
fn log_handler_result(_event: &str, _result: HandlerResult) {
    #[cfg(feature = "tracing")]
    match _result {
        Ok(Ok(())) => {},
        Ok(Err(err)) => tracing::debug!(event = _event, error = %err, "event handler failed"),
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("<non-string panic payload>");

            tracing::debug!(event = _event, panic = message, "event handler panicked");
        },
    }
}

/// Runs one handler with panic isolation, returning `None` when canceled first.
#[inline]
async fn run_handler<C>(
    handler: Handler<C>,
    ctx: Arc<C>,
    data: Arc<dyn Any + Send + Sync>,
    cancel_token: &CancellationToken,
) -> Option<HandlerResult> {
    select! {
        biased;
        () = cancel_token.cancelled() => None,
        // Calling the handler inside the async block also catches panics raised while creating its future.
        result = AssertUnwindSafe(async move { handler(ctx, data).await }).catch_unwind() => Some(result),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::pending,
        time::Duration,
    };

    use anyhow::bail;
    use tokio::{
        spawn,
        sync::mpsc,
        task::{
            try_id,
            yield_now,
        },
        time::{
            sleep,
            timeout,
        },
    };
    use tokio_util::sync::CancellationToken;

    use super::*;

    struct DummyConnection;

    #[tokio::test]
    async fn test_single_handler_runs_inline() {
        let registry = Arc::new(WsIoEventRegistry::<DummyConnection>::new());
        let (tx, mut rx) = mpsc::unbounded_channel();
        registry.on("ping", move |_ctx, _payload: Arc<()>| {
            tx.send(try_id()).unwrap();
            async { Ok(()) }
        });

        // Dispatch from a spawned task so the dispatcher has a task ID to compare against.
        let dispatcher_task_id = spawn(async move {
            registry
                .dispatch_event_packet(
                    Arc::new(DummyConnection),
                    "ping",
                    &WsIoPacketCodec::Msgpack,
                    None,
                    &CancellationToken::new(),
                )
                .await
                .unwrap();

            try_id()
        })
        .await
        .unwrap();

        assert!(dispatcher_task_id.is_some());
        assert_eq!(rx.try_recv().unwrap(), dispatcher_task_id);
    }

    #[tokio::test]
    async fn test_single_handler_isolates_panics_and_errors() {
        let registry = WsIoEventRegistry::<DummyConnection>::new();
        registry.on("creation_panic", |_ctx, _payload: Arc<()>| {
            panic!("panic while creating handler future");
            #[allow(unreachable_code)]
            async {
                Ok(())
            }
        });

        registry.on("poll_panic", async |_ctx, _payload: Arc<()>| {
            yield_now().await;
            panic!("panic while polling handler future");
        });

        registry.on("error", async |_ctx, _payload: Arc<()>| {
            bail!("handler failed");
        });

        for event in ["creation_panic", "poll_panic", "error"] {
            registry
                .dispatch_event_packet(
                    Arc::new(DummyConnection),
                    event,
                    &WsIoPacketCodec::Msgpack,
                    None,
                    &CancellationToken::new(),
                )
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn test_single_handler_prioritizes_cancellation() {
        let registry = WsIoEventRegistry::<DummyConnection>::new();
        let cancel_token = CancellationToken::new();
        cancel_token.cancel();

        let (tx, mut rx) = mpsc::unbounded_channel();
        registry.on("ping", move |_ctx, _payload: Arc<()>| {
            tx.send(()).unwrap();
            async { Ok(()) }
        });

        registry
            .dispatch_event_packet(
                Arc::new(DummyConnection),
                "ping",
                &WsIoPacketCodec::Msgpack,
                None,
                &cancel_token,
            )
            .await
            .unwrap();

        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_multiple_handlers_remain_spawned_and_isolate_panics() {
        let registry = WsIoEventRegistry::<DummyConnection>::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let dispatcher_task_id = try_id();
        registry.on("ping", async |_ctx, _payload: Arc<()>| {
            panic!("handler panic must not affect other handlers");
        });

        registry.on("ping", move |_ctx, _payload: Arc<()>| {
            tx.send(try_id()).unwrap();
            async { Ok(()) }
        });

        registry
            .dispatch_event_packet(
                Arc::new(DummyConnection),
                "ping",
                &WsIoPacketCodec::Msgpack,
                None,
                &CancellationToken::new(),
            )
            .await
            .unwrap();

        let handler_task_id = rx.try_recv().unwrap();
        assert!(handler_task_id.is_some());
        assert_ne!(handler_task_id, dispatcher_task_id);
    }

    #[tokio::test]
    async fn test_single_handler_dropped_on_cancellation_or_dispatch_drop() {
        for cancel in [true, false] {
            let registry = WsIoEventRegistry::<DummyConnection>::new();
            let dropped = CancellationToken::new();
            let handler_dropped = dropped.clone();
            registry.on("ping", move |_ctx, _payload: Arc<()>| {
                let guard = handler_dropped.clone().drop_guard();
                async move {
                    let _guard = guard;
                    pending::<()>().await;
                    Ok(())
                }
            });

            let cancel_token = CancellationToken::new();
            let mut dispatch = Box::pin(registry.dispatch_event_packet(
                Arc::new(DummyConnection),
                "ping",
                &WsIoPacketCodec::Msgpack,
                None,
                &cancel_token,
            ));

            select! {
                biased;
                _ = &mut dispatch => panic!("handler must remain pending"),
                () = async {} => {},
            }

            assert!(!dropped.is_cancelled());
            if cancel {
                cancel_token.cancel();
                timeout(Duration::from_secs(1), &mut dispatch).await.unwrap().unwrap();
                assert!(dropped.is_cancelled(), "cancellation must drop the handler future");
            }

            drop(dispatch);
            assert!(dropped.is_cancelled());
        }
    }

    #[tokio::test]
    async fn test_registry_dispatch_runs_all_handlers() {
        let registry = WsIoEventRegistry::<DummyConnection>::new();
        let cancel_token = CancellationToken::new();

        let ctx = Arc::new(DummyConnection);

        let (handled_tx, mut handled_rx) = mpsc::unbounded_channel();
        let first_handler_tx = handled_tx.clone();

        registry.on("ping", move |_ctx, payload: Arc<String>| {
            assert_eq!(*payload, "hello");
            first_handler_tx.send("first").unwrap();
            async move { Ok(()) }
        });

        registry.on("ping", move |_ctx, payload: Arc<String>| {
            assert_eq!(*payload, "hello");
            handled_tx.send("second").unwrap();
            async move { Ok(()) }
        });

        let packet_codec = WsIoPacketCodec::Msgpack;
        let packet_data = packet_codec.encode_data(&"hello").unwrap();

        registry
            .dispatch_event_packet(ctx, "ping", &packet_codec, Some(packet_data), &cancel_token)
            .await
            .expect("event dispatch should succeed");

        let mut handlers = Vec::with_capacity(2);
        for _ in 0..2 {
            let handler = timeout(Duration::from_secs(1), handled_rx.recv())
                .await
                .expect("handler should run before timeout")
                .expect("handler channel should remain open");

            handlers.push(handler);
        }

        handlers.sort_unstable();
        assert_eq!(handlers, ["first", "second"]);
    }

    #[tokio::test]
    async fn test_registry_dispatch_waits_for_handlers() {
        let registry = WsIoEventRegistry::<DummyConnection>::new();
        let cancel_token = CancellationToken::new();
        let packet_codec = WsIoPacketCodec::Msgpack;
        let (handled_tx, mut handled_rx) = mpsc::unbounded_channel();

        registry.on("ordered", move |_ctx, payload: Arc<String>| {
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

        let ctx = Arc::new(DummyConnection);
        let first_packet = packet_codec.encode_data(&"first").unwrap();
        let second_packet = packet_codec.encode_data(&"second").unwrap();

        registry
            .dispatch_event_packet(
                Arc::clone(&ctx),
                "ordered",
                &packet_codec,
                Some(first_packet),
                &cancel_token,
            )
            .await
            .expect("event dispatch should succeed");

        registry
            .dispatch_event_packet(ctx, "ordered", &packet_codec, Some(second_packet), &cancel_token)
            .await
            .expect("event dispatch should succeed");

        let mut handled = Vec::with_capacity(4);
        for _ in 0..4 {
            handled.push(handled_rx.recv().await.unwrap());
        }

        assert_eq!(handled, ["start:first", "end:first", "start:second", "end:second"]);
    }

    #[tokio::test]
    async fn test_registry_dispatch_returns_payload_decode_error() {
        let registry = WsIoEventRegistry::<DummyConnection>::new();
        let cancel_token = CancellationToken::new();
        let packet_codec = WsIoPacketCodec::Msgpack;

        registry.on("ping", async |_ctx, _payload: Arc<String>| Ok(()));

        let result = registry
            .dispatch_event_packet(
                Arc::new(DummyConnection),
                "ping",
                &packet_codec,
                Some(vec![0].into()),
                &cancel_token,
            )
            .await;

        assert!(result.is_err());
    }

    #[test]
    fn test_registry_on_off() {
        let registry = WsIoEventRegistry::<DummyConnection>::new();

        let handler_id = registry.on("test_event", async |_ctx, _data: Arc<String>| Ok(()));

        // Verify the handler was registered
        assert_eq!(handler_id, 0);
        assert!(registry.event_entries.read().contains_key("test_event"));
        assert_eq!(
            registry
                .event_entries
                .read()
                .get("test_event")
                .unwrap()
                .handlers
                .read()
                .len(),
            1
        );

        // Remove by handler ID
        registry.off_by_handler_id("test_event", handler_id);

        // Verify it was removed and the event entry was cleaned up since it's empty
        assert!(!registry.event_entries.read().contains_key("test_event"));

        // Register multiple and test full off
        registry.on("multi_event", async |_ctx, _data: Arc<String>| Ok(()));
        registry.on("multi_event", async |_ctx, _data: Arc<String>| Ok(()));

        assert_eq!(
            registry
                .event_entries
                .read()
                .get("multi_event")
                .unwrap()
                .handlers
                .read()
                .len(),
            2
        );

        registry.off("multi_event");
        assert!(!registry.event_entries.read().contains_key("multi_event"));
    }

    #[test]
    #[should_panic(expected = "Event key cannot be empty")]
    fn test_registry_rejects_empty_event_key() {
        let registry = WsIoEventRegistry::<DummyConnection>::new();
        registry.on("", async |_ctx, _data: Arc<()>| Ok(()));
    }
}

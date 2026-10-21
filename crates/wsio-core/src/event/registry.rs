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

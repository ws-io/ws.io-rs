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
type DataDecoder = fn(&[u8], WsIoPacketCodec) -> Result<Arc<dyn Any + Send + Sync>>;
type Handler<C> = Arc<
    dyn Fn(Arc<C>, Arc<dyn Any + Send + Sync>) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'static>>
        + Send
        + Sync
        + 'static,
>;

// Constants/Statics
static EMPTY_EVENT_DATA_ANY_ARC: LazyLock<Arc<dyn Any + Send + Sync>> = LazyLock::new(|| Arc::new(()));

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
    /// Dispatches one event packet and waits for every registered handler to finish.
    ///
    /// The handlers for this packet are started concurrently and all must finish
    /// before this method returns. Handler failures are logged and do not fail the
    /// dispatch; payload decoding failures are returned to the connection dispatcher.
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

        let packet_codec = *packet_codec;

        #[cfg(feature = "tracing")]
        let event_name = event.to_owned();
        let data = match packet_data {
            Some(bytes) => match (event_entry.data_decoder)(&bytes, packet_codec) {
                Ok(data) => data,
                Err(err) => {
                    #[cfg(feature = "tracing")]
                    tracing::debug!(event = %event_name, error = %err, "failed to decode event packet data");
                    return Err(err);
                },
            },
            None => EMPTY_EVENT_DATA_ANY_ARC.clone(),
        };

        let handlers = event_entry.handlers.read().values().cloned().collect::<Vec<_>>();

        #[cfg(feature = "tracing")]
        tracing::trace!(
            event = %event_name,
            handler_count = handlers.len(),
            "dispatching event handlers"
        );

        let mut handler_tasks = JoinSet::new();
        for handler in handlers {
            let ctx = ctx.clone();
            let data = data.clone();
            let cancel_token = cancel_token.clone();
            handler_tasks.spawn(async move {
                select! {
                    () = cancel_token.cancelled() => Ok(()),
                    result = handler(ctx, data) => result,
                }
            });
        }

        while let Some(result) = handler_tasks.join_next().await {
            match result {
                Ok(Ok(())) => {},
                Ok(Err(_err)) => {
                    #[cfg(feature = "tracing")]
                    tracing::debug!(event = %event_name, error = %_err, "event handler failed");
                },
                Err(_err) => {
                    #[cfg(feature = "tracing")]
                    tracing::debug!(event = %event_name, error = %_err, "event handler task failed");
                },
            }
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
    packet_codec: WsIoPacketCodec,
) -> Result<Arc<dyn Any + Send + Sync>> {
    Ok(Arc::new(packet_codec.decode_data::<D>(bytes)?))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::{
        sync::mpsc::unbounded_channel,
        time::{
            sleep,
            timeout,
        },
    };
    use tokio_util::sync::CancellationToken;

    use super::*;

    struct DummyConnection;

    #[tokio::test]
    async fn test_registry_dispatch_runs_all_handlers() {
        let registry = WsIoEventRegistry::<DummyConnection>::new();
        let cancel_token = CancellationToken::new();

        let ctx = Arc::new(DummyConnection);

        let (handled_tx, mut handled_rx) = unbounded_channel();
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
        let (handled_tx, mut handled_rx) = unbounded_channel();

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
            .dispatch_event_packet(ctx.clone(), "ordered", &packet_codec, Some(first_packet), &cancel_token)
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

        registry.on("ping", |_ctx, _payload: Arc<String>| async { Ok(()) });

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

        let handler_id = registry.on("test_event", |_ctx, _data: Arc<String>| async { Ok(()) });

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
        registry.on("multi_event", |_ctx, _data: Arc<String>| async { Ok(()) });
        registry.on("multi_event", |_ctx, _data: Arc<String>| async { Ok(()) });

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
}

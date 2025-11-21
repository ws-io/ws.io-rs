use std::{
    any::{
        Any,
        TypeId,
    },
    collections::hash_map::Entry,
    marker::PhantomData,
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
use kikiutils::types::fx_collections::FxHashMap;
use parking_lot::RwLock;
use serde::de::DeserializeOwned;

use crate::{
    packet::codecs::WsIoPacketCodec,
    traits::task::spawner::TaskSpawner,
};

// Types
type DataDecoder = fn(&[u8], &WsIoPacketCodec) -> Result<Arc<dyn Any + Send + Sync>>;
type Handler<C> = Arc<
    dyn Fn(Arc<C>, Arc<dyn Any + Send + Sync>) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'static>>
        + Send
        + Sync
        + 'static,
>;

// Structs
struct EventEntry<C> {
    data_decoder: DataDecoder,
    data_type_id: TypeId,
    handlers: RwLock<FxHashMap<u32, Handler<C>>>,
}

pub struct WsIoEventRegistry<C: Send + Sync + 'static, S: TaskSpawner> {
    _task_spawner: PhantomData<S>,
    event_entries: RwLock<FxHashMap<String, Arc<EventEntry<C>>>>,
    next_handler_id: AtomicU32,
}

impl<C: Send + Sync + 'static, S: TaskSpawner> Default for WsIoEventRegistry<C, S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Send + Sync + 'static, S: TaskSpawner> WsIoEventRegistry<C, S> {
    #[inline]
    pub fn new() -> Self {
        Self {
            _task_spawner: PhantomData,
            event_entries: RwLock::new(FxHashMap::default()),
            next_handler_id: AtomicU32::new(0),
        }
    }

    // Public methods
    #[inline]
    pub fn dispatch_event_packet(
        &self,
        ctx: Arc<C>,
        event: &str,
        packet_codec: &WsIoPacketCodec,
        packet_data: Option<Vec<u8>>,
        task_spawner: &Arc<S>,
    ) {
        let Some(event_entry) = self.event_entries.read().get(event).cloned() else {
            return;
        };

        let packet_codec = *packet_codec;
        let task_spawner_clone = task_spawner.clone();
        task_spawner.spawn_task(async move {
            let data = match packet_data {
                Some(bytes) => match (event_entry.data_decoder)(&bytes, &packet_codec) {
                    Ok(data) => data,
                    Err(_) => return Ok(()),
                },
                None => EMPTY_EVENT_DATA_ANY_ARC.clone(),
            };

            let handlers = event_entry.handlers.read().values().cloned().collect::<Vec<_>>();
            for handler in handlers {
                let ctx = ctx.clone();
                let data = data.clone();
                task_spawner_clone.spawn_task(handler(ctx, data));
            }

            Ok(())
        });
    }

    #[inline]
    pub fn off(&self, event: &str) {
        self.event_entries.write().remove(event);
    }

    #[inline]
    pub fn off_by_handler_id(&self, event: &str, handler_id: u32) {
        if let Some(event_entry) = self.event_entries.write().get(event).cloned() {
            event_entry.handlers.write().remove(&handler_id);
            if event_entry.handlers.read().is_empty() {
                self.event_entries.write().remove(event);
            }
        }
    }

    #[inline]
    pub fn on<H, Fut, D>(&self, event: &str, handler: H) -> u32
    where
        H: Fn(Arc<C>, Arc<D>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
        D: DeserializeOwned + Send + Sync + 'static,
    {
        let data_type_id = TypeId::of::<D>();

        let mut event_entries = self.event_entries.write();
        let event_entry = match event_entries.entry(event.into()) {
            Entry::Occupied(occupied) => {
                let event_entry = occupied.into_mut();
                assert_eq!(
                    event_entry.data_type_id, data_type_id,
                    "Event '{}' already registered with a different data type — each event name must correspond to exactly one payload type.",
                    event
                );

                event_entry
            }
            Entry::Vacant(vacant) => vacant.insert(Arc::new(EventEntry {
                data_decoder: decode_data_as_any_arc::<D>,
                data_type_id,
                handlers: RwLock::new(FxHashMap::default()),
            })),
        };

        let handler_id = self.next_handler_id.fetch_add(1, Ordering::Relaxed);
        event_entry.handlers.write().insert(
            handler_id,
            Arc::new(move |connection, data| {
                if (*data).type_id() != data_type_id {
                    return Box::pin(async { Ok(()) });
                }

                Box::pin(handler(connection, data.downcast().unwrap()))
            }),
        );

        handler_id
    }
}

// Constants/Statics
static EMPTY_EVENT_DATA_ANY_ARC: LazyLock<Arc<dyn Any + Send + Sync>> = LazyLock::new(|| Arc::new(()));

// Functions
#[inline]
fn decode_data_as_any_arc<D: DeserializeOwned + Send + Sync + 'static>(
    bytes: &[u8],
    packet_codec: &WsIoPacketCodec,
) -> Result<Arc<dyn Any + Send + Sync>> {
    Ok(Arc::new(packet_codec.decode_data::<D>(bytes)?))
}

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use std::{
    future::Future,
    hint::black_box,
    sync::Arc,
    task::{
        Context,
        Poll,
        Waker,
    },
};

use criterion::{
    BatchSize,
    BenchmarkId,
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
};
use tokio_util::sync::CancellationToken;
use wsio_core::{
    event::registry::WsIoEventRegistry,
    packet::codecs::WsIoPacketCodec,
    traits::task::spawner::TaskSpawner,
};

// Constants/Statics
const EVENT_NAME: &str = "chat";
const HANDLER_COUNTS: [usize; 4] = [0, 1, 10, 100];

// Structs
struct DummyConnection;

struct ImmediateSpawner;

impl TaskSpawner for ImmediateSpawner {
    fn cancel_token(&self) -> Arc<CancellationToken> {
        Arc::new(CancellationToken::new())
    }

    fn spawn_task<F: Future<Output = anyhow::Result<()>> + Send + 'static>(&self, future: F) {
        let mut context = Context::from_waker(Waker::noop());
        let mut future = Box::pin(future);
        assert!(matches!(future.as_mut().poll(&mut context), Poll::Ready(Ok(()))));
    }
}

// Functions
fn registry_with_handlers(handler_count: usize) -> WsIoEventRegistry<DummyConnection, ImmediateSpawner> {
    let registry = WsIoEventRegistry::<DummyConnection, ImmediateSpawner>::new();
    for _ in 0..handler_count {
        register_handler(&registry);
    }

    registry
}

fn register_handler(registry: &WsIoEventRegistry<DummyConnection, ImmediateSpawner>) -> u32 {
    registry.on(EVENT_NAME, |_ctx: Arc<DummyConnection>, _data: Arc<String>| async {
        Ok(())
    })
}

fn bench_event_dispatch(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("event_registry/dispatch");
    let spawner = Arc::new(ImmediateSpawner);
    let ctx = Arc::new(DummyConnection);
    let packet_codec = WsIoPacketCodec::SerdeJson;
    let packet_data = packet_codec.encode_data(&"Hello world benchmark").unwrap();

    for handler_count in HANDLER_COUNTS {
        let registry = registry_with_handlers(handler_count);
        if handler_count > 0 {
            group.throughput(Throughput::Elements(handler_count as u64));
        }

        group.bench_with_input(
            BenchmarkId::from_parameter(handler_count),
            &handler_count,
            |bencher, _| {
                bencher.iter(|| {
                    registry.dispatch_event_packet(
                        black_box(ctx.clone()),
                        black_box(EVENT_NAME),
                        black_box(&packet_codec),
                        black_box(Some(packet_data.clone())),
                        black_box(&spawner),
                    );
                });
            },
        );
    }

    group.finish();
}

fn bench_event_registry_mutation(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("event_registry/mutation");

    group.bench_function("register_new_event", |bencher| {
        bencher.iter_batched(
            WsIoEventRegistry::<DummyConnection, ImmediateSpawner>::new,
            |registry| {
                black_box(register_handler(&registry));
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("register_existing_event", |bencher| {
        bencher.iter_batched(
            || registry_with_handlers(1),
            |registry| {
                black_box(register_handler(&registry));
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("off_by_handler_id_last_handler", |bencher| {
        bencher.iter_batched(
            || {
                let registry = WsIoEventRegistry::<DummyConnection, ImmediateSpawner>::new();
                let handler_id = register_handler(&registry);
                (registry, handler_id)
            },
            |(registry, handler_id)| registry.off_by_handler_id(black_box(EVENT_NAME), black_box(handler_id)),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(benches, bench_event_dispatch, bench_event_registry_mutation);
criterion_main!(benches);

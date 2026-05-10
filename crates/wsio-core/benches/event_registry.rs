use std::{
    hint::black_box,
    sync::Arc,
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

struct DummySpawner {
    cancel_token: Arc<CancellationToken>,
}

impl TaskSpawner for DummySpawner {
    fn cancel_token(&self) -> Arc<CancellationToken> {
        self.cancel_token.clone()
    }

    fn spawn_task<F: std::future::Future<Output = anyhow::Result<()>> + Send + 'static>(&self, _future: F) {
        // Drop tasks so this benchmark isolates registry lookup, decode scheduling,
        // handler snapshotting, and spawner calls rather than Tokio scheduler cost.
    }
}

struct DummyConnection;

fn spawner() -> Arc<DummySpawner> {
    Arc::new(DummySpawner {
        cancel_token: Arc::new(CancellationToken::new()),
    })
}

fn registry_with_handlers(handler_count: usize) -> WsIoEventRegistry<DummyConnection, DummySpawner> {
    let registry = WsIoEventRegistry::<DummyConnection, DummySpawner>::new();
    for _ in 0..handler_count {
        registry.on("chat", |_ctx: Arc<DummyConnection>, _data: Arc<String>| async {
            Ok(())
        });
    }

    registry
}

fn bench_event_dispatch(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("event_registry/dispatch");
    let spawner = spawner();
    let ctx = Arc::new(DummyConnection);
    let packet_codec = WsIoPacketCodec::SerdeJson;
    let packet_data = packet_codec.encode_data(&"Hello world benchmark").unwrap();

    for handler_count in [0, 1, 10, 100] {
        let registry = Arc::new(registry_with_handlers(handler_count));
        group.throughput(Throughput::Elements(handler_count.max(1) as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(handler_count),
            &handler_count,
            |bencher, _| {
                bencher.iter(|| {
                    registry.dispatch_event_packet(
                        black_box(ctx.clone()),
                        black_box("chat"),
                        black_box(&packet_codec),
                        black_box(Some(packet_data.clone())),
                        black_box(&spawner),
                    );
                })
            },
        );
    }

    group.finish();
}

fn bench_event_registry_mutation(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("event_registry/mutation");

    group.bench_function("register_new_event", |bencher| {
        bencher.iter_batched(
            WsIoEventRegistry::<DummyConnection, DummySpawner>::new,
            |registry| {
                black_box(
                    registry.on("chat", |_ctx: Arc<DummyConnection>, _data: Arc<String>| async {
                        Ok(())
                    }),
                );
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("register_existing_event", |bencher| {
        bencher.iter_batched(
            || registry_with_handlers(1),
            |registry| {
                black_box(
                    registry.on("chat", |_ctx: Arc<DummyConnection>, _data: Arc<String>| async {
                        Ok(())
                    }),
                );
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("off_by_handler_id_last_handler", |bencher| {
        bencher.iter_batched(
            || {
                let registry = WsIoEventRegistry::<DummyConnection, DummySpawner>::new();
                let handler_id = registry.on("chat", |_ctx: Arc<DummyConnection>, _data: Arc<String>| async {
                    Ok(())
                });
                (registry, handler_id)
            },
            |(registry, handler_id)| registry.off_by_handler_id(black_box("chat"), black_box(handler_id)),
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_event_dispatch, bench_event_registry_mutation);
criterion_main!(benches);

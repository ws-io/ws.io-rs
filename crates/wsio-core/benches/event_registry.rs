use std::{
    hint::black_box,
    sync::Arc,
};

use criterion::{
    Criterion,
    criterion_group,
    criterion_main,
};
use tokio_util::sync::CancellationToken;
use wsio_core::{
    event::registry::WsIoEventRegistry,
    packet::codecs::WsIoPacketCodec,
    traits::task::spawner::TaskSpawner,
};

// 1. A dummy spawner that does nothing but drop the task or immediately resolve it
struct DummySpawner {
    cancel_token: Arc<CancellationToken>,
}

impl TaskSpawner for DummySpawner {
    fn cancel_token(&self) -> Arc<CancellationToken> {
        self.cancel_token.clone()
    }

    fn spawn_task<F: std::future::Future<Output = anyhow::Result<()>> + Send + 'static>(&self, _future: F) {
        // We do not actually spawn Tokio tasks in the benchmark to avoid measuring Tokio's scheduler overhead.
        // We solely want to measure the EventRegistry's internal dispatching mechanics (locks, maps, clones).
    }
}

// 2. Dummy Connection Context
struct DummyConnection;

fn bench_event_dispatcher(criterion: &mut Criterion) {
    let registry = Arc::new(WsIoEventRegistry::<DummyConnection, DummySpawner>::new());
    let spawner = Arc::new(DummySpawner {
        cancel_token: Arc::new(CancellationToken::new()),
    });

    let ctx = Arc::new(DummyConnection);

    // Register 100 handlers on the same event to see iteration cost
    for _ in 0..100 {
        registry.on("chat", |_ctx: Arc<DummyConnection>, _data: Arc<String>| async {
            Ok(())
        });
    }

    let packet_codec = WsIoPacketCodec::SerdeJson;
    let packet_data = packet_codec.encode_data(&"Hello world benchmark").unwrap();

    // The core benchmark loop for event registry
    criterion.bench_function("dispatch_100_handlers", |bencher| {
        bencher.iter(|| {
            registry.dispatch_event_packet(
                black_box(ctx.clone()),
                black_box("chat"),
                black_box(&packet_codec),
                black_box(Some(packet_data.clone())),
                black_box(&spawner),
            );
        })
    });
}

criterion_group!(benches, bench_event_dispatcher);
criterion_main!(benches);

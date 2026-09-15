#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use std::{
    hint::black_box,
    time::Duration,
};

use bytes::Bytes;
use criterion::{
    BenchmarkId,
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
};
use tokio::runtime::{
    Builder,
    Runtime,
};
use wsio_core::packet::transformers::{
    WsIoPacketTransformer,
    zstd::WsIoPacketZstdTransformerConfig,
};

// Constants/Statics
const PAYLOAD_SIZES: [usize; 3] = [256, 4096, 64 * 1024];

// Functions
fn runtime() -> Runtime {
    Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("benchmark runtime should build")
}

fn zstd_transformer(compression_threshold: usize, blocking_threshold: usize) -> WsIoPacketTransformer {
    WsIoPacketTransformer::zstd(WsIoPacketZstdTransformerConfig {
        compression_threshold,
        blocking_threshold,
        ..Default::default()
    })
}

fn bench_transformers(criterion: &mut Criterion) {
    let runtime = runtime();
    let mut group = criterion.benchmark_group("packet_transformers");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(2));

    for payload_size in PAYLOAD_SIZES {
        let input = Bytes::from(vec![b'a'; payload_size]);
        let transformers = [
            ("noop", WsIoPacketTransformer::default()),
            ("zstd_raw", zstd_transformer(usize::MAX, usize::MAX)),
            ("zstd_sync", zstd_transformer(0, usize::MAX)),
            ("zstd_blocking", zstd_transformer(0, 0)),
        ];

        for (name, transformer) in transformers {
            let encoded = runtime
                .block_on(transformer.encode(input.clone()))
                .expect("benchmark input should encode");

            group.throughput(Throughput::Bytes(payload_size as u64));
            group.bench_with_input(
                BenchmarkId::new(format!("{name}/encode"), payload_size),
                &input,
                |bencher, input| {
                    bencher.to_async(&runtime).iter(|| async {
                        black_box(
                            transformer
                                .encode(input.clone())
                                .await
                                .expect("benchmark input should encode"),
                        );
                    });
                },
            );

            group.throughput(Throughput::Bytes(payload_size as u64));
            group.bench_with_input(
                BenchmarkId::new(format!("{name}/decode"), payload_size),
                &encoded,
                |bencher, encoded| {
                    bencher.to_async(&runtime).iter(|| async {
                        black_box(
                            transformer
                                .decode(encoded.clone())
                                .await
                                .expect("benchmark packet should decode"),
                        );
                    });
                },
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_transformers);
criterion_main!(benches);

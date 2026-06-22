#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use std::hint::black_box;

use criterion::{
    BenchmarkId,
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
};
use serde::{
    Deserialize,
    Serialize,
};
use wsio_core::packet::{
    WsIoPacket,
    codecs::WsIoPacketCodec,
};

// Constants/Statics
const EVENT_NAME: &str = "benchmark_event";
const PAYLOAD_SIZES: [usize; 3] = [0, 256, 4096];

// Structs
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct BenchPayload {
    id: u32,
    message: String,
    data: Vec<u8>,
}

// Functions
fn payload(bytes: usize) -> BenchPayload {
    BenchPayload {
        id: 12345,
        message: "This is a performance test for packet codecs".to_string(),
        data: vec![0; bytes],
    }
}

fn bench_id(codec_name: &str, operation: &str, payload_size: usize) -> BenchmarkId {
    BenchmarkId::new(format!("{codec_name}/{operation}"), payload_size)
}

fn bench_codecs(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("packet_codecs");

    let codecs = [
        ("SerdeJson", WsIoPacketCodec::SerdeJson),
        #[cfg(feature = "packet-codec-cbor")]
        ("Cbor", WsIoPacketCodec::Cbor),
        #[cfg(feature = "packet-codec-msgpack")]
        ("Msgpack", WsIoPacketCodec::Msgpack),
        #[cfg(feature = "packet-codec-postcard")]
        ("Postcard", WsIoPacketCodec::Postcard),
        #[cfg(feature = "packet-codec-sonic-rs")]
        ("SonicRs", WsIoPacketCodec::SonicRs),
    ];

    for (name, codec) in codecs {
        for payload_size in PAYLOAD_SIZES {
            let payload = payload(payload_size);
            let encoded_data = codec.encode_data(&payload).unwrap();
            let packet = WsIoPacket::new_event(EVENT_NAME, Some(encoded_data.clone()));
            let encoded_packet = codec.encode(&packet).unwrap();

            group.throughput(Throughput::Bytes(payload_size.max(1) as u64));
            group.bench_with_input(
                bench_id(name, "encode_data", payload_size),
                &payload,
                |bencher, payload| {
                    bencher.iter(|| {
                        let _ = codec.encode_data(black_box(payload)).unwrap();
                    })
                },
            );

            group.throughput(Throughput::Bytes(encoded_data.len() as u64));
            group.bench_with_input(
                bench_id(name, "decode_data", payload_size),
                &encoded_data,
                |bencher, encoded_data| {
                    bencher.iter(|| {
                        let _: BenchPayload = codec.decode_data(black_box(encoded_data)).unwrap();
                    })
                },
            );

            group.throughput(Throughput::Bytes(encoded_data.len() as u64));
            group.bench_with_input(
                bench_id(name, "encode_packet", payload_size),
                &packet,
                |bencher, packet| {
                    bencher.iter(|| {
                        let _ = codec.encode(black_box(packet)).unwrap();
                    })
                },
            );

            group.throughput(Throughput::Bytes(encoded_packet.len() as u64));
            group.bench_with_input(
                bench_id(name, "decode_packet", payload_size),
                &encoded_packet,
                |bencher, encoded_packet| {
                    bencher.iter(|| {
                        let _ = codec.decode(black_box(encoded_packet)).unwrap();
                    })
                },
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_codecs);
criterion_main!(benches);

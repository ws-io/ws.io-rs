use std::hint::black_box;

use criterion::{
    Criterion,
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

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct BenchPayload {
    id: u32,
    message: String,
    data: Vec<u8>,
}

fn bench_codecs(criterion: &mut Criterion) {
    let payload = BenchPayload {
        id: 12345,
        message: "This is a performance test for packet codecs".to_string(),
        data: vec![0; 256], // 256 bytes payload
    };

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
        // Benchmark Encode Data
        group.bench_function(format!("{}_encode_data", name), |bencher| {
            bencher.iter(|| {
                let _ = codec.encode_data(black_box(&payload)).unwrap();
            })
        });

        // Benchmark Decode Data
        let encoded_data = codec.encode_data(&payload).unwrap();
        group.bench_function(format!("{}_decode_data", name), |bencher| {
            bencher.iter(|| {
                let _: BenchPayload = codec.decode_data(black_box(&encoded_data)).unwrap();
            })
        });

        // Benchmark Encode Packet
        let packet = WsIoPacket::new_event("benchmark_event", Some(encoded_data.clone()));
        group.bench_function(format!("{}_encode_packet", name), |bencher| {
            bencher.iter(|| {
                let _ = codec.encode(black_box(&packet)).unwrap();
            })
        });

        // Benchmark Decode Packet
        let encoded_packet = codec.encode(&packet).unwrap();
        group.bench_function(format!("{}_decode_packet", name), |bencher| {
            bencher.iter(|| {
                let _ = codec.decode(black_box(&encoded_packet)).unwrap();
            })
        });
    }

    group.finish();
}

criterion_group!(benches, bench_codecs);
criterion_main!(benches);

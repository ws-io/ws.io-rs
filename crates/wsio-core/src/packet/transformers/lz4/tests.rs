use std::task::Poll;

use futures_util::{
    FutureExt,
    poll,
};
use lz4_flex::block::compress;
use tokio::time::timeout;

use super::{
    super::compression::WsIoPacketCompressionTransformer,
    *,
};
use crate::test_support::TEST_TIMEOUT;

#[tokio::test]
async fn wire_layout_threshold_and_raw_fallback_are_exact() {
    for blocking_threshold in [0, usize::MAX] {
        let transformer = WsIoPacketCompressionTransformer::lz4(WsIoPacketLz4TransformerConfig {
            blocking_threshold,
            compression_threshold: 128,
            max_decompressed_size: 1024,
        });

        for input in [
            Bytes::new(),
            Bytes::from_static(b"x"),
            Bytes::from(vec![b'a'; 127]),
            Bytes::from(vec![b'a'; 128]),
            Bytes::from(vec![b'a'; 129]),
            Bytes::from((0..=255).collect::<Vec<u8>>()),
            Bytes::from(vec![b'a'; 1024]),
        ] {
            let compressed = compress(&input);
            let use_compressed = input.len() >= 128 && compressed.len() < input.len();
            let mut expected = vec![if use_compressed { 0x25 } else { 0x24 }];
            expected.extend_from_slice(&u32::try_from(input.len()).unwrap().to_be_bytes());
            expected.extend_from_slice(if use_compressed { &compressed } else { &input });
            let encoded = transformer.encode(input.clone()).await.unwrap();
            assert_eq!(encoded.as_ref(), expected.as_slice());
            assert_eq!(transformer.decode(encoded).await.unwrap(), input);
        }
    }
}

#[test]
fn independent_block_fixture_and_runtime_free_processing_work() {
    let transformer = WsIoPacketCompressionTransformer::lz4(WsIoPacketLz4TransformerConfig {
        blocking_threshold: 0,
        compression_threshold: 0,
        ..WsIoPacketLz4TransformerConfig::default()
    });

    // One literal, an 18-byte match at offset 1, then five final literals.
    let wire = Bytes::from_static(&[0x25, 0, 0, 0, 24, 0x1e, b'a', 1, 0, 0x50, b'a', b'a', b'a', b'a', b'a']);
    assert_eq!(
        transformer.decode(wire).now_or_never().unwrap().unwrap(),
        Bytes::from(vec![b'a'; 24])
    );

    let input = Bytes::from(vec![b'a'; 128]);
    let encoded = transformer.encode(input.clone()).now_or_never().unwrap().unwrap();
    assert_eq!(transformer.decode(encoded).now_or_never().unwrap().unwrap(), input);
}

#[tokio::test]
async fn corrupt_frames_limits_and_algorithm_mismatches_are_rejected() {
    for blocking_threshold in [0, usize::MAX] {
        let transformer = WsIoPacketCompressionTransformer::lz4(WsIoPacketLz4TransformerConfig {
            blocking_threshold,
            compression_threshold: 0,
            max_decompressed_size: 128,
        });

        assert!(transformer.encode(Bytes::from(vec![0; 129])).await.is_err());
        let input = Bytes::from(vec![b'a'; 128]);
        let valid = transformer.encode(input.clone()).await.unwrap();
        let mut too_short = valid.to_vec();
        too_short[4] = 127;
        let mut too_long = valid.to_vec();
        too_long[4] = 129;
        let mut shorter_output = transformer.encode(Bytes::from(vec![b'a'; 127])).await.unwrap().to_vec();
        shorter_output[4] = 128;
        for malformed in [
            Bytes::from_static(&[0x22, 0, 0, 0, 0]),
            Bytes::from_static(&[0x24, 0, 0, 0, 2, 1]),
            Bytes::from_static(&[0x25, 0, 0, 0, 10, 0xff]),
            Bytes::from_static(&[0x25, 0, 0, 0, 8, 0, 0, 0]),
            Bytes::from(too_short),
            Bytes::from(too_long),
            Bytes::from(shorter_output),
            valid.slice(..valid.len() - 1),
        ] {
            assert!(transformer.decode(malformed).await.is_err());
            assert_eq!(transformer.decode(valid.clone()).await.unwrap(), input);
        }

        let empty_only = WsIoPacketCompressionTransformer::lz4(WsIoPacketLz4TransformerConfig {
            max_decompressed_size: 0,
            blocking_threshold,
            compression_threshold: 0,
        });

        assert!(empty_only.encode(Bytes::from_static(b"x")).await.is_err());
        assert_eq!(
            empty_only
                .decode(empty_only.encode(Bytes::new()).await.unwrap())
                .await
                .unwrap(),
            Bytes::new()
        );
    }
}

#[tokio::test]
async fn blocking_uses_original_size_and_cancelled_waits_release_capacity() {
    let transformer = WsIoPacketCompressionTransformer::lz4(WsIoPacketLz4TransformerConfig {
        blocking_threshold: 128,
        compression_threshold: 0,
        max_decompressed_size: 1024,
    });

    let input = Bytes::from(vec![b'a'; 128]);
    let encoded = transformer.encode(input.clone()).await.unwrap();
    assert!(encoded.len() < 128);
    let mut permits = Vec::new();
    while let Poll::Ready(permit) = poll!(Box::pin(transformer.acquire_blocking_permit())) {
        permits.push(permit.unwrap());
    }

    let small = Bytes::from(vec![b'a'; 127]);
    let small_encoded = timeout(TEST_TIMEOUT, transformer.encode(small.clone()))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        timeout(TEST_TIMEOUT, transformer.decode(small_encoded))
            .await
            .unwrap()
            .unwrap(),
        small
    );

    let mut encoding = Box::pin(transformer.encode(input.clone()));
    assert!(matches!(poll!(&mut encoding), Poll::Pending));
    drop(encoding);
    let mut decoding = Box::pin(transformer.decode(encoded));
    assert!(matches!(poll!(&mut decoding), Poll::Pending));
    drop(decoding);
    drop(permits);
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let transformer = transformer.clone();
        let input = input.clone();
        tasks.spawn(async move {
            let encoded = transformer.encode(input.clone()).await.unwrap();
            assert_eq!(transformer.decode(encoded).await.unwrap(), input);
        });
    }

    timeout(TEST_TIMEOUT, async {
        while let Some(result) = tasks.join_next().await {
            result.unwrap();
        }
    })
    .await
    .unwrap();
}

use std::task::Poll;

use futures_util::poll;
use tokio::{
    task::JoinSet,
    time::timeout,
};
use zstd::bulk::compress;

use super::*;
use crate::test_support::TEST_TIMEOUT;

#[tokio::test]
async fn compression_threshold_is_inclusive_and_preserves_payload() {
    // Force both synchronous and blocking paths; test the exact compression threshold.
    for blocking_threshold in [0, usize::MAX] {
        let transformer = WsIoPacketZstdTransformer::new(WsIoPacketZstdTransformerConfig {
            blocking_threshold,
            compression_threshold: 128,
            max_decompressed_size: 1024,
            ..WsIoPacketZstdTransformerConfig::default()
        });

        for size in [0, 127, 128, 129, 1024] {
            let input = Bytes::from(vec![b'a'; size]);
            let encoded = transformer.encode(input.clone()).await.unwrap();
            assert_eq!(encoded[0] & 1 != 0, size >= 128);
            assert_eq!(transformer.decode(encoded).await.unwrap(), input);
        }
    }
}

#[tokio::test]
async fn encoded_frames_match_exact_wire_layout() {
    for blocking_threshold in [0, usize::MAX] {
        let transformer = WsIoPacketZstdTransformer::new(WsIoPacketZstdTransformerConfig {
            blocking_threshold,
            compression_threshold: 0,
            max_decompressed_size: 1024,
            ..WsIoPacketZstdTransformerConfig::default()
        });

        for input in [
            Bytes::new(),
            Bytes::from_static(b"x"),
            Bytes::from((0..=255).collect::<Vec<u8>>()),
            Bytes::from(vec![b'a'; 1024]),
        ] {
            let compressed = compress(&input, WsIoPacketZstdTransformerConfig::default().compression_level).unwrap();
            let use_compressed = compressed.len() < input.len();
            let mut expected = vec![if use_compressed { 0x23 } else { 0x22 }];
            expected.extend_from_slice(&u32::try_from(input.len()).unwrap().to_be_bytes());
            expected.extend_from_slice(if use_compressed { &compressed } else { &input });

            let encoded = transformer.encode(input.clone()).await.unwrap();
            assert_eq!(encoded.as_ref(), expected.as_slice());
            assert_eq!(transformer.decode(encoded).await.unwrap(), input);
        }
    }
}

#[tokio::test]
async fn limits_and_corruption_are_rejected_without_poisoning_reused_contexts() {
    for blocking_threshold in [0, usize::MAX] {
        let transformer = WsIoPacketZstdTransformer::new(WsIoPacketZstdTransformerConfig {
            blocking_threshold,
            compression_threshold: 0,
            max_decompressed_size: 128,
            ..WsIoPacketZstdTransformerConfig::default()
        });

        assert!(transformer.encode(Bytes::from(vec![0; 129])).await.is_err());
        let valid = transformer.encode(Bytes::from(vec![b'a'; 128])).await.unwrap();
        let mut wrong_length = valid.to_vec();
        wrong_length[4] = 127;
        let mut excessive_length = valid.to_vec();
        excessive_length[1..5].copy_from_slice(&129_u32.to_be_bytes());
        for malformed in [
            Bytes::from_static(&[0x22, 0, 0, 0, 2, 1]),
            Bytes::from_static(&[0x23, 0, 0, 0, 10, 0xff]),
            Bytes::from(wrong_length),
            Bytes::from(excessive_length),
            valid.slice(..valid.len() - 1),
        ] {
            assert!(transformer.decode(malformed).await.is_err());
            assert_eq!(
                transformer.decode(valid.clone()).await.unwrap(),
                Bytes::from(vec![b'a'; 128])
            );

            assert_eq!(
                transformer
                    .decode(transformer.encode(Bytes::from_static(b"ok")).await.unwrap())
                    .await
                    .unwrap()
                    .as_ref(),
                b"ok"
            );
        }

        let empty_only = WsIoPacketZstdTransformer::new(WsIoPacketZstdTransformerConfig {
            blocking_threshold,
            compression_threshold: 0,
            max_decompressed_size: 0,
            ..WsIoPacketZstdTransformerConfig::default()
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
async fn cloned_transformers_can_share_contexts_concurrently() {
    let transformer = WsIoPacketZstdTransformer::new(WsIoPacketZstdTransformerConfig {
        blocking_threshold: 0,
        compression_threshold: 0,
        max_decompressed_size: 4096,
        ..WsIoPacketZstdTransformerConfig::default()
    });

    let mut tasks = JoinSet::new();
    for byte in 0..16 {
        let transformer = transformer.clone();
        tasks.spawn(async move {
            let input = Bytes::from(vec![byte; 4096]);
            let encoded = transformer.encode(input.clone()).await.unwrap();
            assert_eq!(transformer.decode(encoded).await.unwrap(), input);
        });
    }

    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
}

#[tokio::test]
async fn blocking_path_is_selected_by_original_size() {
    let transformer = WsIoPacketZstdTransformer::new(WsIoPacketZstdTransformerConfig {
        blocking_threshold: 128,
        compression_threshold: 0,
        max_decompressed_size: 1024,
        ..WsIoPacketZstdTransformerConfig::default()
    });

    let large = Bytes::from(vec![b'a'; 128]);
    let encoded = transformer.encode(large.clone()).await.unwrap();
    assert!(encoded.len() < 128);
    // Occupy every blocking slot so choosing the blocking path is observable.
    let mut permits = Vec::new();
    while let Poll::Ready(permit) = poll!(Box::pin(transformer.context_pool.acquire_blocking_permit())) {
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

    let mut encoding = Box::pin(transformer.encode(large.clone()));
    assert!(poll!(&mut encoding).is_pending());
    drop(encoding);
    let mut decoding = Box::pin(transformer.decode(encoded));
    assert!(
        poll!(&mut decoding).is_pending(),
        "decode must use original length, not just compressed size"
    );

    drop(decoding);
    drop(permits);
    let encoded = timeout(TEST_TIMEOUT, transformer.encode(large.clone()))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        timeout(TEST_TIMEOUT, transformer.decode(encoded))
            .await
            .unwrap()
            .unwrap(),
        large
    );
}

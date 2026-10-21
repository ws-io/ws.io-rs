use std::{
    task::Poll,
    time::Duration,
};

use futures_util::poll;
use tokio::{
    task::JoinSet,
    time::timeout,
};

use super::*;

fn make_transformer(
    blocking_threshold: usize,
    compression_threshold: usize,
    max_decompressed_size: usize,
) -> WsIoPacketZstdTransformer {
    WsIoPacketZstdTransformer::new(WsIoPacketZstdTransformerConfig {
        blocking_threshold,
        compression_threshold,
        max_decompressed_size,
        ..WsIoPacketZstdTransformerConfig::default()
    })
}

#[tokio::test]
async fn thresholds_and_raw_fallback_preserve_payload() {
    // Force both synchronous and blocking paths; test the exact compression threshold.
    for blocking in [0, usize::MAX] {
        let transformer = make_transformer(blocking, 128, 1024);
        for size in [0, 127, 128, 129, 1024] {
            let input = Bytes::from(vec![b'a'; size]);
            let encoded = transformer.encode(input.clone()).await.unwrap();
            assert_eq!(encoded[0] & 1 != 0, size >= 128);
            assert_eq!(transformer.decode(encoded).await.unwrap(), input);
        }

        let transformer = make_transformer(blocking, 0, 1024);
        for input in [
            Bytes::new(),
            Bytes::from_static(b"x"),
            Bytes::from((0..=255).collect::<Vec<u8>>()),
        ] {
            let encoded = transformer.encode(input.clone()).await.unwrap();
            assert_eq!(encoded[0], 0x22, "non-beneficial compression must use raw framing");
            assert_eq!(&encoded[5..], input.as_ref());
            assert_eq!(transformer.decode(encoded).await.unwrap(), input);
        }
    }
}

#[tokio::test]
async fn limits_and_corruption_are_rejected_without_poisoning_reused_contexts() {
    for blocking in [0, usize::MAX] {
        let transformer = make_transformer(blocking, 0, 128);
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

        let empty_only = make_transformer(blocking, 0, 0);
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
    let transformer = make_transformer(0, 0, 4096);
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
async fn blocking_boundary_uses_original_size_and_waiting_for_a_permit_is_cancellable() {
    let transformer = make_transformer(128, 0, 1024);
    let large = Bytes::from(vec![b'a'; 128]);
    let encoded = transformer.encode(large.clone()).await.unwrap();
    assert!(encoded.len() < 128);
    // Occupy every blocking slot so choosing the blocking path is observable.
    let mut permits = Vec::new();
    while let Poll::Ready(permit) = poll!(Box::pin(transformer.context_pool.acquire_blocking_permit())) {
        permits.push(permit.unwrap());
    }

    let small = Bytes::from(vec![b'a'; 127]);
    let small_encoded = timeout(Duration::from_secs(2), transformer.encode(small.clone()))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        timeout(Duration::from_secs(2), transformer.decode(small_encoded))
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
    let encoded = timeout(Duration::from_secs(2), transformer.encode(large.clone()))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        timeout(Duration::from_secs(2), transformer.decode(encoded))
            .await
            .unwrap()
            .unwrap(),
        large
    );
}

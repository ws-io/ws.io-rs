use std::sync::mpsc as std_mpsc;

use futures_util::poll;
use tokio::{
    runtime::Builder,
    sync::oneshot,
    time::timeout,
};

use super::*;
use crate::test_support::TEST_TIMEOUT;

#[tokio::test]
async fn valid_raw_and_compressed_frames_over_the_decode_limit_are_rejected() {
    for compression_threshold in [0, usize::MAX] {
        let transformers = [
            #[cfg(feature = "packet-transformer-lz4")]
            (
                WsIoPacketCompressionTransformer::lz4(WsIoPacketLz4TransformerConfig {
                    compression_threshold,
                    ..WsIoPacketLz4TransformerConfig::default()
                }),
                WsIoPacketCompressionTransformer::lz4(WsIoPacketLz4TransformerConfig {
                    compression_threshold,
                    max_decompressed_size: 128,
                    ..WsIoPacketLz4TransformerConfig::default()
                }),
            ),
            #[cfg(feature = "packet-transformer-zstd")]
            (
                WsIoPacketCompressionTransformer::zstd(WsIoPacketZstdTransformerConfig {
                    compression_threshold,
                    ..WsIoPacketZstdTransformerConfig::default()
                }),
                WsIoPacketCompressionTransformer::zstd(WsIoPacketZstdTransformerConfig {
                    compression_threshold,
                    max_decompressed_size: 128,
                    ..WsIoPacketZstdTransformerConfig::default()
                }),
            ),
        ];

        for (permissive, strict) in transformers {
            let input = Bytes::from(vec![b'a'; 129]);
            let encoded = permissive.encode(input.clone()).await.unwrap();
            assert_eq!(encoded[0] & 1 != 0, compression_threshold == 0);
            assert_eq!(permissive.decode(encoded.clone()).await.unwrap(), input);
            assert!(strict.decode(encoded).await.is_err());
        }
    }
}

#[test]
fn dropping_encoding_keeps_queued_blocking_permits_until_completion() {
    let transformers = [
        #[cfg(feature = "packet-transformer-lz4")]
        WsIoPacketCompressionTransformer::lz4(WsIoPacketLz4TransformerConfig {
            blocking_threshold: 0,
            compression_threshold: 0,
            ..WsIoPacketLz4TransformerConfig::default()
        }),
        #[cfg(feature = "packet-transformer-zstd")]
        WsIoPacketCompressionTransformer::zstd(WsIoPacketZstdTransformerConfig {
            blocking_threshold: 0,
            compression_threshold: 0,
            ..WsIoPacketZstdTransformerConfig::default()
        }),
    ];

    let runtime = Builder::new_current_thread()
        .enable_time()
        .max_blocking_threads(1)
        .build()
        .unwrap();

    for transformer in transformers {
        let (started_tx, started_rx) = oneshot::channel();
        let (release_tx, release_rx) = std_mpsc::channel();
        let blocker = runtime.spawn_blocking(move || {
            started_tx.send(()).unwrap();
            // Dropping the sender also releases this worker if an assertion fails.
            let _ = release_rx.recv();
        });

        runtime.block_on(async move {
            timeout(TEST_TIMEOUT, started_rx).await.unwrap().unwrap();
            let capacity = transformer.0.blocking_semaphore.available_permits();
            let mut encoding = Box::pin(transformer.encode(Bytes::from(vec![b'a'; 1024])));
            assert!(poll!(&mut encoding).is_pending());
            drop(encoding);
            assert_eq!(transformer.0.blocking_semaphore.available_permits(), capacity - 1);

            release_tx.send(()).unwrap();
            timeout(TEST_TIMEOUT, blocker).await.unwrap().unwrap();
            let permits = timeout(
                TEST_TIMEOUT,
                Arc::clone(&transformer.0.blocking_semaphore).acquire_many_owned(u32::try_from(capacity).unwrap()),
            )
            .await
            .unwrap()
            .unwrap();

            drop(permits);
            assert_eq!(transformer.0.blocking_semaphore.available_permits(), capacity);
        });
    }
}

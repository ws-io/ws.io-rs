use std::sync::Arc;

use anyhow::{
    Result,
    ensure,
};
use bytes::Bytes;
use tokio::{
    runtime::Handle,
    task::spawn_blocking,
};

mod context_pool;

use self::context_pool::WsIoPacketZstdContextPool;
use super::compression_frame::{
    COMPRESSION_FRAME_HEADER_SIZE,
    WsIoPacketCompressionAlgorithm,
    WsIoPacketCompressionFrameHeader,
};

// Structs

#[derive(Clone)]
pub(super) struct WsIoPacketZstdTransformer {
    config: WsIoPacketZstdTransformerConfig,
    context_pool: Arc<WsIoPacketZstdContextPool>,
}

impl WsIoPacketZstdTransformer {
    #[inline]
    pub(super) fn new(config: WsIoPacketZstdTransformerConfig) -> Self {
        Self {
            config,
            context_pool: Arc::new(WsIoPacketZstdContextPool::new(config.compression_level)),
        }
    }

    #[inline]
    pub(super) async fn decode(&self, mut bytes: Bytes) -> Result<Bytes> {
        let header = WsIoPacketCompressionFrameHeader::decode(&bytes)?;
        ensure!(
            header.algorithm == WsIoPacketCompressionAlgorithm::Zstd,
            "invalid zstd transformer algorithm"
        );

        let original_length = usize::try_from(header.original_length)?;
        ensure!(
            original_length <= self.config.max_decompressed_size,
            "packet exceeds the configured maximum decompressed size"
        );

        let payload = bytes.split_off(COMPRESSION_FRAME_HEADER_SIZE);
        drop(bytes);

        if !header.compressed {
            ensure!(
                payload.len() == original_length,
                "invalid raw zstd transformer payload length"
            );

            return Ok(payload);
        }

        if self.config.should_use_blocking(payload.len().max(original_length)) && Handle::try_current().is_ok() {
            let permit = self.context_pool.acquire_blocking_permit().await?;
            let context_pool = self.context_pool.clone();
            return spawn_blocking(move || {
                let result = context_pool.decompress(&payload, original_length);
                drop(permit);
                result
            })
            .await?;
        }

        self.context_pool.decompress(&payload, original_length)
    }

    #[inline]
    pub(super) async fn encode(&self, bytes: Bytes) -> Result<Bytes> {
        ensure!(
            bytes.len() <= self.config.max_decompressed_size,
            "packet exceeds the configured maximum decompressed size"
        );

        let raw_header =
            WsIoPacketCompressionFrameHeader::new(WsIoPacketCompressionAlgorithm::Zstd, false, bytes.len())?;

        if bytes.len() < self.config.compression_threshold {
            return raw_header.encode_payload(&bytes);
        }

        if self.config.should_use_blocking(bytes.len()) && Handle::try_current().is_ok() {
            let permit = self.context_pool.acquire_blocking_permit().await?;
            let context_pool = self.context_pool.clone();
            return spawn_blocking(move || {
                let result = context_pool.compress(&bytes, raw_header);
                drop(permit);
                result
            })
            .await?;
        }

        self.context_pool.compress(&bytes, raw_header)
    }
}

/// Configuration for the built-in zstd packet transformer.
#[derive(Clone, Copy, Debug)]
pub struct WsIoPacketZstdTransformerConfig {
    /// Minimum payload size for dispatching zstd work to Tokio's blocking thread
    /// pool.
    ///
    /// Encoding uses the encoded payload size; decoding uses the larger of the
    /// encoded and original sizes. Set this to `usize::MAX` to keep zstd work on
    /// the async worker. Without an active Tokio runtime, processing is
    /// synchronous.
    pub blocking_threshold: usize,

    /// Compression level used by zstd for encoding. Level `3` is the default
    /// low-latency setting.
    pub compression_level: i32,

    /// Minimum packet size for zstd compression. Smaller packets use the raw
    /// payload flag.
    pub compression_threshold: usize,

    /// Maximum uncompressed packet size accepted during encoding and decoding.
    ///
    /// The compression frame stores this length as a `u32`, so packets larger
    /// than `u32::MAX` cannot be encoded regardless of this setting.
    pub max_decompressed_size: usize,
}

impl Default for WsIoPacketZstdTransformerConfig {
    fn default() -> Self {
        Self {
            blocking_threshold: 64 * 1024,
            compression_level: 3,
            compression_threshold: 256,
            max_decompressed_size: 16 * 1024 * 1024,
        }
    }
}

impl WsIoPacketZstdTransformerConfig {
    #[inline]
    fn should_use_blocking(&self, payload_size: usize) -> bool {
        payload_size >= self.blocking_threshold
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::{
        super::WsIoPacketTransformer,
        COMPRESSION_FRAME_HEADER_SIZE,
        WsIoPacketCompressionAlgorithm,
        WsIoPacketCompressionFrameHeader,
        WsIoPacketZstdTransformerConfig,
    };

    fn synchronous_config() -> WsIoPacketZstdTransformerConfig {
        WsIoPacketZstdTransformerConfig {
            blocking_threshold: usize::MAX,
            ..Default::default()
        }
    }

    fn synchronous_zstd_transformer(compression_threshold: usize) -> WsIoPacketTransformer {
        WsIoPacketTransformer::zstd(WsIoPacketZstdTransformerConfig {
            compression_threshold,
            ..synchronous_config()
        })
    }

    #[tokio::test]
    async fn blocking_threshold_round_trips_zstd_packets() {
        let transformer = WsIoPacketTransformer::zstd(WsIoPacketZstdTransformerConfig {
            blocking_threshold: 1,
            ..Default::default()
        });
        let input = Bytes::from(vec![b'a'; 4096]);

        let encoded = transformer.encode(input.clone()).await.unwrap();
        let decoded = transformer.decode(encoded).await.unwrap();

        assert_eq!(decoded, input);
    }

    #[tokio::test]
    async fn compression_threshold_uses_raw_frame() {
        let transformer = synchronous_zstd_transformer(usize::MAX);
        let input = Bytes::from_static(b"small packet");

        let encoded = transformer.encode(input.clone()).await.unwrap();
        let header = WsIoPacketCompressionFrameHeader::decode(&encoded).unwrap();

        assert_eq!(header.algorithm, WsIoPacketCompressionAlgorithm::Zstd);
        assert!(!header.compressed);
        assert_eq!(encoded.len(), COMPRESSION_FRAME_HEADER_SIZE + input.len());
        assert_eq!(&encoded[COMPRESSION_FRAME_HEADER_SIZE..], &input[..]);
        assert_eq!(transformer.decode(encoded).await.unwrap(), input);
    }

    #[tokio::test]
    async fn compressible_payload_uses_compressed_frame() {
        let transformer = synchronous_zstd_transformer(0);
        let input = Bytes::from(vec![b'a'; 16 * 1024]);

        let encoded = transformer.encode(input.clone()).await.unwrap();
        let header = WsIoPacketCompressionFrameHeader::decode(&encoded).unwrap();

        assert_eq!(header.algorithm, WsIoPacketCompressionAlgorithm::Zstd);
        assert!(header.compressed);
        assert!(encoded.len() < COMPRESSION_FRAME_HEADER_SIZE + input.len());
        assert_eq!(transformer.decode(encoded).await.unwrap(), input);
    }

    #[tokio::test]
    async fn incompressible_payload_falls_back_to_raw_frame() {
        let transformer = synchronous_zstd_transformer(0);
        let mut state = 0x9e37_79b9_u32;
        let input = Bytes::from(
            (0..4096)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    (state >> 24) as u8
                })
                .collect::<Vec<_>>(),
        );

        let encoded = transformer.encode(input.clone()).await.unwrap();
        let header = WsIoPacketCompressionFrameHeader::decode(&encoded).unwrap();

        assert!(!header.compressed);
        assert_eq!(&encoded[COMPRESSION_FRAME_HEADER_SIZE..], &input[..]);
        assert_eq!(transformer.decode(encoded).await.unwrap(), input);
    }

    #[tokio::test]
    async fn decode_rejects_payload_over_configured_limit() {
        let encoder = WsIoPacketTransformer::zstd(synchronous_config());
        let encoded = encoder.encode(Bytes::from(vec![b'a'; 64])).await.unwrap();
        let decoder = WsIoPacketTransformer::zstd(WsIoPacketZstdTransformerConfig {
            max_decompressed_size: 32,
            ..synchronous_config()
        });

        let error = decoder.decode(encoded).await.unwrap_err();

        assert!(error.to_string().contains("maximum decompressed size"));
    }

    #[tokio::test]
    async fn decode_rejects_truncated_raw_payload() {
        let transformer = synchronous_zstd_transformer(usize::MAX);
        let input = Bytes::from_static(b"truncated packet");
        let mut encoded = transformer.encode(input).await.unwrap().to_vec();
        encoded.pop();

        let error = transformer.decode(encoded.into()).await.unwrap_err();

        assert!(error.to_string().contains("raw zstd transformer payload length"));
    }

    #[tokio::test]
    async fn decode_rejects_unsupported_frame_version() {
        let transformer = synchronous_zstd_transformer(usize::MAX);
        let mut encoded = transformer
            .encode(Bytes::from_static(b"versioned packet"))
            .await
            .unwrap()
            .to_vec();

        encoded[0] = 0;

        let error = transformer.decode(encoded.into()).await.unwrap_err();

        assert!(error.to_string().contains("compression frame version"));
    }

    #[tokio::test]
    async fn decode_rejects_unsupported_compression_algorithm() {
        let transformer = synchronous_zstd_transformer(usize::MAX);
        let mut encoded = transformer
            .encode(Bytes::from_static(b"algorithm packet"))
            .await
            .unwrap()
            .to_vec();

        encoded[0] = 0x20 | (2 << 1);

        let error = transformer.decode(encoded.into()).await.unwrap_err();

        assert!(error.to_string().contains("compression algorithm"));
    }
}

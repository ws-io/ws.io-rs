use anyhow::{
    bail,
    ensure,
};
use async_trait::async_trait;

use super::*;

struct TaggedTransformer;

#[async_trait]
impl WsIoPacketCustomTransformer for TaggedTransformer {
    async fn decode(&self, bytes: &[u8]) -> Result<Bytes> {
        let Some((0xa5, payload)) = bytes.split_first() else {
            bail!("invalid custom tag")
        };

        Ok(Bytes::copy_from_slice(payload))
    }

    async fn encode(&self, bytes: &[u8]) -> Result<Bytes> {
        ensure!(!bytes.is_empty(), "empty custom input");
        let mut output = vec![0xa5];
        output.extend_from_slice(bytes);
        Ok(output.into())
    }
}

#[tokio::test]
async fn custom_transformer_routes_encode_and_decode_and_propagates_errors() {
    let transformer = WsIoPacketTransformer::custom(Arc::new(TaggedTransformer));
    assert_eq!(
        transformer.encode(Bytes::from_static(b"data")).await.unwrap().as_ref(),
        b"\xa5data"
    );

    assert_eq!(
        transformer
            .decode(Bytes::from_static(b"\xa5data"))
            .await
            .unwrap()
            .as_ref(),
        b"data"
    );

    assert_eq!(
        transformer.encode(Bytes::new()).await.unwrap_err().to_string(),
        "empty custom input"
    );

    assert_eq!(
        transformer
            .decode(Bytes::from_static(b"wrong"))
            .await
            .unwrap_err()
            .to_string(),
        "invalid custom tag"
    );
}

#[cfg(feature = "packet-transformer-zstd")]
#[tokio::test]
async fn public_zstd_transformer_applies_config_and_propagates_errors() {
    let transformer = WsIoPacketTransformer::zstd(WsIoPacketZstdTransformerConfig {
        compression_threshold: usize::MAX,
        max_decompressed_size: 4,
        ..WsIoPacketZstdTransformerConfig::default()
    });

    let wire = Bytes::from_static(&[0x22, 0, 0, 0, 4, b'd', b'a', b't', b'a']);
    assert_eq!(transformer.encode(Bytes::from_static(b"data")).await.unwrap(), wire);
    assert_eq!(transformer.decode(wire).await.unwrap().as_ref(), b"data");
    assert!(transformer.encode(Bytes::from_static(b"large")).await.is_err());
    assert!(
        transformer
            .decode(Bytes::from_static(&[0x22, 0, 0, 0, 5, 1, 2, 3, 4, 5]))
            .await
            .is_err()
    );

    assert!(transformer.decode(Bytes::from_static(b"invalid")).await.is_err());
}

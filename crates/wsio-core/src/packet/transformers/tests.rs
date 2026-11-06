use super::*;
use crate::test_support::tagged_transformer;

#[tokio::test]
async fn custom_transformer_round_trips_and_propagates_errors() {
    let transformer = tagged_transformer(4);
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
        transformer
            .encode(Bytes::from_static(b"large"))
            .await
            .unwrap_err()
            .to_string(),
        "custom input too large"
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
async fn public_zstd_transformer_applies_config() {
    let transformer = WsIoPacketTransformer::zstd(WsIoPacketZstdTransformerConfig {
        compression_threshold: usize::MAX,
        max_decompressed_size: 4,
        ..WsIoPacketZstdTransformerConfig::default()
    });

    let wire = Bytes::from_static(&[0x22, 0, 0, 0, 4, b'd', b'a', b't', b'a']);
    assert_eq!(transformer.encode(Bytes::from_static(b"data")).await.unwrap(), wire);
    assert_eq!(transformer.decode(wire).await.unwrap().as_ref(), b"data");
    assert!(transformer.encode(Bytes::from_static(b"large")).await.is_err());
}

#[cfg(feature = "packet-transformer-lz4")]
#[tokio::test]
async fn public_lz4_transformer_applies_config() {
    let transformer = WsIoPacketTransformer::lz4(WsIoPacketLz4TransformerConfig {
        compression_threshold: usize::MAX,
        max_decompressed_size: 4,
        ..WsIoPacketLz4TransformerConfig::default()
    });

    let wire = Bytes::from_static(&[0x24, 0, 0, 0, 4, b'd', b'a', b't', b'a']);
    assert_eq!(transformer.encode(Bytes::from_static(b"data")).await.unwrap(), wire);
    assert_eq!(transformer.decode(wire).await.unwrap().as_ref(), b"data");
    assert!(transformer.encode(Bytes::from_static(b"large")).await.is_err());
}

#[cfg(all(feature = "packet-transformer-lz4", feature = "packet-transformer-zstd"))]
#[tokio::test]
async fn built_in_compression_algorithms_do_not_silently_accept_each_other() {
    let lz4 = WsIoPacketTransformer::lz4(WsIoPacketLz4TransformerConfig::default());
    let zstd = WsIoPacketTransformer::zstd(WsIoPacketZstdTransformerConfig::default());
    for input in [Bytes::from_static(b"x"), Bytes::from(vec![b'a'; 1024])] {
        assert!(lz4.decode(zstd.encode(input.clone()).await.unwrap()).await.is_err());
        assert!(zstd.decode(lz4.encode(input).await.unwrap()).await.is_err());
    }
}

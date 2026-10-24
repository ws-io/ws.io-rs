use bytes::Bytes;
use tokio::{
    spawn,
    sync::mpsc::{
        self,
        error::TryRecvError,
    },
    time::Instant,
};

use super::*;
#[cfg(feature = "packet-transformer-zstd")]
use crate::packet::transformers::zstd::WsIoPacketZstdTransformerConfig;
use crate::test_support::{
    TEST_TIMEOUT,
    park_until_dropped,
    tagged_transformer,
};

async fn spawn_parked_task() -> (JoinHandle<()>, mpsc::UnboundedReceiver<()>) {
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let (alive_tx, alive_rx) = mpsc::unbounded_channel();
    let task = spawn(async move {
        let _ = park_until_dropped(started_tx, alive_tx).await;
    });

    timeout(TEST_TIMEOUT, started_rx.recv()).await.unwrap().unwrap();
    (task, alive_rx)
}

#[tokio::test]
async fn completion_of_either_side_aborts_and_joins_the_other() {
    for read_finishes in [false, true] {
        let (parked, mut alive) = spawn_parked_task().await;
        let completed = spawn(async {});
        let (read, write) = if read_finishes {
            (completed, parked)
        } else {
            (parked, completed)
        };

        timeout(
            TEST_TIMEOUT,
            shutdown_websocket_stream_tasks(read, write, Duration::from_secs(30)),
        )
        .await
        .unwrap();

        assert_eq!(alive.try_recv(), Err(TryRecvError::Disconnected));
    }
}

#[tokio::test(start_paused = true)]
async fn grace_timeout_aborts_and_joins_both_sides() {
    let (read, mut read_alive) = spawn_parked_task().await;
    let (write, mut write_alive) = spawn_parked_task().await;
    let start = Instant::now();
    shutdown_websocket_stream_tasks(read, write, Duration::from_secs(5)).await;
    assert_eq!(start.elapsed(), Duration::from_secs(5));
    assert_eq!(read_alive.try_recv(), Err(TryRecvError::Disconnected));
    assert_eq!(write_alive.try_recv(), Err(TryRecvError::Disconnected));
}

#[test]
fn config_validation_rejects_invalid_capacities_sizes_and_overflow() {
    let required = 8 + 16 + OUTGOING_FRAME_OVERHEAD;
    let max_capacity = Semaphore::MAX_PERMITS;
    for (channel_capacity, write_buffer_size, max_write_buffer_size, max_outgoing_message_size, error) in [
        (1, 8, required, 16, None),
        (max_capacity, 8, required, 16, None),
        (0, 8, required, 16, Some("channel_capacity")),
        (max_capacity + 1, 8, required, 16, Some("channel_capacity")),
        (1, 8, required, 0, Some("greater than zero")),
        (1, 8, required - 1, 16, Some("must be at least")),
        (1, 1, usize::MAX, usize::MAX, Some("overflowed")),
        (
            1,
            0,
            usize::MAX,
            usize::MAX - OUTGOING_FRAME_OVERHEAD + 1,
            Some("overflowed"),
        ),
    ] {
        let mut config = default_websocket_config();
        config.write_buffer_size = write_buffer_size;
        config.max_write_buffer_size = max_write_buffer_size;
        let result = validate_transport_config(&config, channel_capacity, max_outgoing_message_size);
        match error {
            None => result.unwrap(),
            Some(error) => assert!(result.unwrap_err().to_string().contains(error), "{error}"),
        }
    }
}

#[tokio::test]
async fn outgoing_limit_is_checked_after_transforming() {
    let packet = WsIoPacket::new_event("e", Some(WsIoPacketCodec::Msgpack.encode_data(&vec![7_u8; 8]).unwrap()));
    let mut wire = vec![0xa5];
    wire.extend_from_slice(&WsIoPacketCodec::Msgpack.encode(&packet).unwrap());
    let transformer = tagged_transformer(usize::MAX);
    for limit in [wire.len() - 1, wire.len(), wire.len() + 1] {
        let result = encode_packet_to_message(&WsIoPacketCodec::Msgpack, &transformer, limit, &packet).await;
        if limit < wire.len() {
            assert!(result.unwrap_err().to_string().contains("max_outgoing_message_size"));
        } else {
            assert_eq!(*result.unwrap(), Message::Binary(Bytes::copy_from_slice(&wire)));
        }
    }
}

#[tokio::test]
async fn transformer_failure_is_propagated() {
    let error = encode_packet_to_message(
        &WsIoPacketCodec::Msgpack,
        &tagged_transformer(0),
        usize::MAX,
        &WsIoPacket::new_event("e", None),
    )
    .await
    .unwrap_err();

    assert_eq!(error.to_string(), "custom input too large");
}

#[cfg(feature = "packet-transformer-zstd")]
#[tokio::test]
async fn compression_can_make_an_oversized_codec_packet_fit_the_outgoing_limit() {
    let packet = WsIoPacket::new_event(
        "e",
        Some(WsIoPacketCodec::Msgpack.encode_data(&vec![0_u8; 256]).unwrap()),
    );

    let encoded = WsIoPacketCodec::Msgpack.encode(&packet).unwrap();
    assert!(encoded.len() > 64);
    let transformer = WsIoPacketTransformer::zstd(WsIoPacketZstdTransformerConfig {
        compression_threshold: 0,
        ..WsIoPacketZstdTransformerConfig::default()
    });

    let message = encode_packet_to_message(&WsIoPacketCodec::Msgpack, &transformer, 64, &packet)
        .await
        .unwrap();

    let Message::Binary(wire) = message.as_ref() else {
        panic!("expected binary message")
    };

    assert!(wire.len() <= 64);
    assert_eq!(transformer.decode(wire.clone()).await.unwrap(), encoded);
}

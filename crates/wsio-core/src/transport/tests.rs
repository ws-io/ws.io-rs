use std::future::pending;

use tokio::{
    spawn,
    sync::oneshot::{
        self,
        error::TryRecvError,
    },
    time::Instant,
};

use super::*;

async fn parked_task() -> (JoinHandle<()>, oneshot::Receiver<()>) {
    let (started_tx, started_rx) = oneshot::channel();
    let (alive_tx, alive_rx) = oneshot::channel();
    let task = spawn(async move {
        started_tx.send(()).unwrap();
        pending::<()>().await;
        let _ = alive_tx.send(());
    });

    timeout(Duration::from_secs(2), started_rx).await.unwrap().unwrap();
    (task, alive_rx)
}

#[tokio::test]
async fn completion_of_either_side_aborts_and_joins_the_other() {
    for read_finishes in [false, true] {
        let (parked, mut alive) = parked_task().await;
        let completed = spawn(async {});
        let (read, write) = if read_finishes {
            (completed, parked)
        } else {
            (parked, completed)
        };

        timeout(
            Duration::from_secs(2),
            shutdown_websocket_stream_tasks(read, write, Duration::from_secs(30)),
        )
        .await
        .unwrap();

        assert_eq!(alive.try_recv(), Err(TryRecvError::Closed));
    }
}

#[tokio::test(start_paused = true)]
async fn grace_timeout_aborts_and_joins_both_sides() {
    let (read, mut read_alive) = parked_task().await;
    let (write, mut write_alive) = parked_task().await;
    let start = Instant::now();
    shutdown_websocket_stream_tasks(read, write, Duration::from_secs(5)).await;
    assert_eq!(start.elapsed(), Duration::from_secs(5));
    assert_eq!(read_alive.try_recv(), Err(TryRecvError::Closed));
    assert_eq!(write_alive.try_recv(), Err(TryRecvError::Closed));
}

#[test]
fn config_rejects_invalid_capacities_and_outgoing_sizes() {
    let config = default_websocket_config();
    for capacity in [0, Semaphore::MAX_PERMITS + 1] {
        assert!(
            validate_transport_config(&config, capacity, 1).is_err(),
            "capacity {capacity}"
        );
    }

    for capacity in [1, Semaphore::MAX_PERMITS] {
        assert!(
            validate_transport_config(&config, capacity, 1).is_ok(),
            "capacity {capacity}"
        );
    }

    assert!(validate_transport_config(&config, 1, 0).is_err());
}

#[test]
fn write_buffer_boundary_and_arithmetic_overflow_are_checked() {
    let mut config = default_websocket_config();
    config.write_buffer_size = 8;
    let required = 8 + 16 + OUTGOING_FRAME_OVERHEAD;
    for (buffer_size, valid) in [(required - 1, false), (required, true)] {
        config.max_write_buffer_size = buffer_size;
        assert_eq!(
            validate_transport_config(&config, 1, 16).is_ok(),
            valid,
            "buffer size {buffer_size}"
        );
    }

    for (write_buffer, outgoing) in [(1, usize::MAX), (0, usize::MAX - OUTGOING_FRAME_OVERHEAD + 1)] {
        config.write_buffer_size = write_buffer;
        config.max_write_buffer_size = usize::MAX;
        let error = validate_transport_config(&config, 1, outgoing).unwrap_err();
        assert!(error.to_string().contains("overflowed"));
    }
}

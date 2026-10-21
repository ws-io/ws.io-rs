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

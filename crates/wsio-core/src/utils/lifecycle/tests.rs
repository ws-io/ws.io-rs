use std::{
    sync::atomic::{
        AtomicUsize,
        Ordering,
    },
    time::Duration,
};

use futures_util::poll;
use tokio::{
    sync::oneshot,
    time::timeout,
};

use super::*;

#[tokio::test]
async fn overlapping_waiters_share_work_and_cancellation_does_not_cancel_operation() {
    let slot = Arc::new(WsIoLifecycleCompletionSlot::default());
    let count = Arc::new(AtomicUsize::new(0));
    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let first = spawn({
        let slot = Arc::clone(&slot);
        let count = Arc::clone(&count);
        async move {
            slot.wait_or_spawn(move || async move {
                count.fetch_add(1, Ordering::SeqCst);
                entered_tx.send(()).unwrap();
                release_rx.await.unwrap();
            })
            .await;
        }
    });

    timeout(Duration::from_secs(2), entered_rx).await.unwrap().unwrap();
    let mut second = Box::pin(slot.wait_or_spawn(|| async { panic!("overlapping operation executed") }));
    assert!(poll!(&mut second).is_pending());
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    assert!(poll!(&mut second).is_pending());
    release_tx.send(()).unwrap();
    timeout(Duration::from_secs(2), second).await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let observed = Arc::clone(&count);
    timeout(
        Duration::from_secs(2),
        slot.wait_or_spawn(move || async move {
            observed.fetch_add(1, Ordering::SeqCst);
        }),
    )
    .await
    .unwrap();

    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert!(slot.token.lock().await.is_none());
}

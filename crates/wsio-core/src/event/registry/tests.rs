use std::{
    future::pending,
    panic::catch_unwind,
    sync::atomic::AtomicUsize,
    time::Duration,
};

use anyhow::bail;
use tokio::{
    spawn,
    sync::mpsc,
    time::timeout,
};

use super::*;

async fn dispatch<C: Send + Sync + 'static>(
    registry: &WsIoEventRegistry<C>,
    ctx: Arc<C>,
    data: Option<Bytes>,
    cancel: &CancellationToken,
) -> Result<()> {
    registry
        .dispatch_event_packet(ctx, "e", &WsIoPacketCodec::Msgpack, data, cancel)
        .await
}

#[test]
fn registration_rejects_empty_keys_and_type_conflicts_until_handlers_are_removed() {
    let registry = WsIoEventRegistry::<()>::new();
    assert!(catch_unwind(AssertUnwindSafe(|| registry.on("", |_, _: Arc<()>| async { Ok(()) }))).is_err());
    registry.on("e", |_, _: Arc<u32>| async { Ok(()) });
    assert!(
        catch_unwind(AssertUnwindSafe(
            || registry.on("e", |_, _: Arc<String>| async { Ok(()) })
        ))
        .is_err()
    );

    registry.off("e");
    registry.on("e", |_, _: Arc<String>| async { Ok(()) });
    let id = registry.on("other", |_, _: Arc<()>| async { Ok(()) });
    registry.off_by_handler_id("other", id);
    registry.on("other", |_, _: Arc<String>| async { Ok(()) });
}

#[tokio::test]
async fn removal_and_decode_errors_do_not_invoke_unselected_handlers() {
    let registry = WsIoEventRegistry::<AtomicUsize>::new();
    let count = Arc::new(AtomicUsize::new(0));
    let cancel = CancellationToken::new();
    let first = registry.on("e", |ctx, _: Arc<u32>| async move {
        ctx.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });

    registry.on("e", |ctx, _: Arc<u32>| async move {
        ctx.fetch_add(10, Ordering::SeqCst);
        Ok(())
    });

    registry.off_by_handler_id("e", first);
    registry.off_by_handler_id("e", u32::MAX);
    let data = WsIoPacketCodec::Msgpack.encode_data(&42_u32).unwrap();
    dispatch(&registry, Arc::clone(&count), Some(data), &cancel)
        .await
        .unwrap();

    assert_eq!(count.load(Ordering::SeqCst), 10);
    dispatch(&registry, Arc::clone(&count), None, &cancel).await.unwrap();

    assert!(
        dispatch(&registry, Arc::clone(&count), Some(Bytes::new()), &cancel)
            .await
            .is_err()
    );

    assert_eq!(count.load(Ordering::SeqCst), 10);
    registry.off("e");
    dispatch(&registry, Arc::clone(&count), Some(Bytes::new()), &cancel)
        .await
        .unwrap();

    assert_eq!(count.load(Ordering::SeqCst), 10);
}

#[tokio::test]
async fn errors_and_panics_are_isolated_on_inline_and_spawned_paths() {
    for handlers in [1, 3] {
        for failure in 0..3 {
            let registry = WsIoEventRegistry::<AtomicUsize>::new();
            let count = Arc::new(AtomicUsize::new(0));
            registry.on("e", move |ctx, _: Arc<()>| {
                ctx.fetch_add(1, Ordering::SeqCst);
                assert_ne!(failure, 0, "panic creating handler future");
                async move {
                    assert_ne!(failure, 1, "panic polling handler future");
                    bail!("handler error")
                }
            });

            for _ in 1..handlers {
                registry.on("e", |ctx, _: Arc<()>| async move {
                    ctx.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                });
            }

            dispatch(&registry, Arc::clone(&count), None, &CancellationToken::new())
                .await
                .unwrap();

            assert_eq!(count.load(Ordering::SeqCst), handlers);
        }
    }
}

#[tokio::test]
async fn active_dispatch_keeps_its_snapshot_when_handlers_are_replaced() {
    let registry = Arc::new(WsIoEventRegistry::<AtomicUsize>::new());
    let count = Arc::new(AtomicUsize::new(0));
    let release = CancellationToken::new();
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    for _ in 0..2 {
        let release = release.clone();
        let started_tx = started_tx.clone();
        registry.on("e", move |ctx, _: Arc<()>| {
            let release = release.clone();
            let started_tx = started_tx.clone();
            async move {
                started_tx.send(()).unwrap();
                release.cancelled().await;
                ctx.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        });
    }

    let task = spawn({
        let registry = Arc::clone(&registry);
        let count = Arc::clone(&count);
        async move { dispatch(&registry, count, None, &CancellationToken::new()).await }
    });

    for _ in 0..2 {
        timeout(Duration::from_secs(2), started_rx.recv())
            .await
            .unwrap()
            .unwrap();
    }

    registry.off("e");
    registry.on("e", |ctx, _: Arc<()>| async move {
        ctx.fetch_add(100, Ordering::SeqCst);
        Ok(())
    });

    release.cancel();
    timeout(Duration::from_secs(2), task).await.unwrap().unwrap().unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
    dispatch(&registry, Arc::clone(&count), None, &CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(count.load(Ordering::SeqCst), 102);
}

#[tokio::test]
async fn cancellation_and_dropping_dispatch_release_all_active_handlers() {
    for handlers in [1, 3] {
        for abort_dispatch in [false, true] {
            let registry = Arc::new(WsIoEventRegistry::<()>::new());
            let cancel = CancellationToken::new();
            let (started_tx, mut started_rx) = mpsc::unbounded_channel();
            let (alive_tx, mut alive_rx) = mpsc::unbounded_channel::<()>();
            for _ in 0..handlers {
                let started_tx = started_tx.clone();
                let alive_tx = alive_tx.clone();
                registry.on("e", move |_, _: Arc<()>| {
                    let started_tx = started_tx.clone();
                    let alive = alive_tx.clone();
                    async move {
                        started_tx.send(()).unwrap();
                        pending::<()>().await;
                        drop(alive);
                        Ok(())
                    }
                });
            }

            let task = spawn({
                let registry = Arc::clone(&registry);
                let cancel = cancel.clone();
                async move { dispatch(&registry, Arc::new(()), None, &cancel).await }
            });

            for _ in 0..handlers {
                timeout(Duration::from_secs(2), started_rx.recv())
                    .await
                    .unwrap()
                    .unwrap();
            }

            registry.off("e");
            drop(alive_tx);
            if abort_dispatch {
                task.abort();
                assert!(task.await.unwrap_err().is_cancelled());
            } else {
                cancel.cancel();
                timeout(Duration::from_secs(2), task).await.unwrap().unwrap().unwrap();
            }

            assert!(
                timeout(Duration::from_secs(2), alive_rx.recv())
                    .await
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[tokio::test]
async fn already_cancelled_dispatch_never_invokes_handlers() {
    for handlers in [1, 2] {
        let registry = WsIoEventRegistry::<()>::new();
        let cancel = CancellationToken::new();
        cancel.cancel();
        // Use a side-effect assertion: handler panics are intentionally swallowed by dispatch.
        let count = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&count);
        for _ in 0..handlers {
            let observed = Arc::clone(&observed);
            registry.on("e", move |_, _: Arc<()>| {
                observed.fetch_add(1, Ordering::SeqCst);
                async { Ok(()) }
            });
        }

        dispatch(&registry, Arc::new(()), None, &cancel).await.unwrap();

        assert_eq!(count.load(Ordering::SeqCst), 0);
    }
}

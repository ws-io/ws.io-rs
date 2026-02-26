use std::sync::Arc;

use anyhow::Result;
use tokio::{
    select,
    spawn,
};
use tokio_util::sync::CancellationToken;

pub trait TaskSpawner: Send + Sync + 'static {
    fn cancel_token(&self) -> Arc<CancellationToken>;

    #[inline]
    fn spawn_task<F: Future<Output = Result<()>> + Send + 'static>(&self, future: F) {
        let cancel_token = self.cancel_token();
        spawn(async move {
            select! {
                _ = cancel_token.cancelled() => {},
                _ = future => {},
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{
        AtomicBool,
        Ordering,
    };

    use tokio::{
        sync::oneshot::channel,
        task::yield_now,
    };

    use super::*;

    struct TestSpawner {
        cancel_token: Arc<CancellationToken>,
    }

    impl TaskSpawner for TestSpawner {
        fn cancel_token(&self) -> Arc<CancellationToken> {
            self.cancel_token.clone()
        }
    }

    #[tokio::test]
    async fn test_spawn_task_runs_to_completion() {
        let spawner = TestSpawner {
            cancel_token: Arc::new(CancellationToken::new()),
        };

        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = flag.clone();

        let (tx, rx) = channel::<()>();

        spawner.spawn_task(async move {
            let _ = rx.await;
            flag_clone.store(true, Ordering::Relaxed);
            Ok(())
        });

        // Trigger the task to complete
        let _ = tx.send(());

        // Wait for the task to complete
        yield_now().await;

        assert!(flag.load(Ordering::Relaxed), "Task should have completed");
    }

    #[tokio::test]
    async fn test_spawn_task_is_cancelled() {
        let cancel_token = Arc::new(CancellationToken::new());
        let spawner = TestSpawner {
            cancel_token: cancel_token.clone(),
        };

        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = flag.clone();

        // Cancel the token immediately
        cancel_token.cancel();

        spawner.spawn_task(async move {
            std::future::pending::<()>().await;
            flag_clone.store(true, Ordering::Relaxed);
            Ok(())
        });

        // Wait a bit to ensure the task had time to be aborted or complete if it failed to abort
        yield_now().await;

        assert!(
            !flag.load(Ordering::Relaxed),
            "Task should have been cancelled before completion"
        );
    }
}

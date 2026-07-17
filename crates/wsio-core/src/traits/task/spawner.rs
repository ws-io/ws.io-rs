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
    use std::{
        future::pending,
        time::Duration,
    };

    use tokio::{
        sync::oneshot::channel,
        time::timeout,
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

        let (completed_tx, completed_rx) = channel();

        spawner.spawn_task(async move {
            let _ = completed_tx.send(());
            Ok(())
        });

        timeout(Duration::from_secs(1), completed_rx)
            .await
            .expect("task should complete before timeout")
            .expect("task should run to completion");
    }

    #[tokio::test]
    async fn test_spawn_task_is_cancelled() {
        let cancel_token = Arc::new(CancellationToken::new());
        let spawner = TestSpawner {
            cancel_token: cancel_token.clone(),
        };

        let (started_tx, started_rx) = channel();
        let (dropped_tx, dropped_rx) = channel::<()>();

        spawner.spawn_task(async move {
            let _drop_signal = dropped_tx;
            let _ = started_tx.send(());
            pending::<()>().await;
            Ok(())
        });

        timeout(Duration::from_secs(1), started_rx)
            .await
            .expect("task should start before timeout")
            .expect("task should start before cancellation");

        cancel_token.cancel();

        timeout(Duration::from_secs(1), dropped_rx)
            .await
            .expect("task should be dropped before timeout")
            .expect_err("cancellation should drop the task future");
    }
}

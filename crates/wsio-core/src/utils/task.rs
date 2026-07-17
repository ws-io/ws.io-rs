use tokio::{
    sync::Mutex,
    task::JoinHandle,
};

pub async fn abort_locked_task(task: &Mutex<Option<JoinHandle<()>>>) {
    if let Some(task) = task.lock().await.take() {
        task.abort();
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::pending,
        time::Duration,
    };

    use tokio::{
        spawn,
        sync::oneshot::channel,
        time::timeout,
    };

    use super::*;

    #[tokio::test]
    async fn test_abort_locked_task() {
        let (started_tx, started_rx) = channel();
        let (dropped_tx, dropped_rx) = channel::<()>();
        let task = Mutex::new(Some(spawn(async move {
            let _drop_signal = dropped_tx;
            let _ = started_tx.send(());
            pending::<()>().await;
        })));

        timeout(Duration::from_secs(1), started_rx)
            .await
            .expect("task should start before timeout")
            .expect("task should start before it is aborted");

        abort_locked_task(&task).await;

        assert!(task.lock().await.is_none());
        timeout(Duration::from_secs(1), dropped_rx)
            .await
            .expect("task should be dropped before timeout")
            .expect_err("aborting should drop the task future");
    }

    #[tokio::test]
    async fn test_abort_locked_task_already_none() {
        let task_mutex: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

        // Should do nothing, shouldn't panic
        abort_locked_task(&task_mutex).await;

        assert!(task_mutex.lock().await.is_none());
    }
}

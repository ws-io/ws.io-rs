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
    use std::future::pending;

    use tokio::spawn;

    use super::*;

    #[tokio::test]
    async fn test_abort_locked_task() {
        let handle = spawn(async {
            pending::<()>().await;
        });

        let task_mutex = Mutex::new(Some(handle));

        assert!(task_mutex.lock().await.is_some());

        abort_locked_task(&task_mutex).await;

        assert!(task_mutex.lock().await.is_none());
    }

    #[tokio::test]
    async fn test_abort_locked_task_already_none() {
        let task_mutex: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

        // Should do nothing, shouldn't panic
        abort_locked_task(&task_mutex).await;

        assert!(task_mutex.lock().await.is_none());
    }
}

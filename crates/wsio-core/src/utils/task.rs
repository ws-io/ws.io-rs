pub async fn abort_locked_task(task: &tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>) {
    if let Some(task) = task.lock().await.take() {
        task.abort();
    }
}

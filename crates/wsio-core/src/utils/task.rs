use tokio::{
    sync::Mutex,
    task::JoinHandle,
};

pub async fn abort_locked_task(task: &Mutex<Option<JoinHandle<()>>>) {
    if let Some(task) = task.lock().await.take() {
        task.abort();
    }
}

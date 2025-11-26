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

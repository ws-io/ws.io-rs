use anyhow::Result;
use tokio::{
    select,
    spawn,
};
use tokio_util::sync::CancellationToken;

pub trait TaskSpawner: Send + Sync + 'static {
    fn cancel_token(&self) -> CancellationToken;

    /// Runs work until completion or cancellation, reporting errors with the `tracing` feature.
    ///
    /// Spawns `future` immediately on the current Tokio runtime. Cancellation from
    /// [`Self::cancel_token`] drops unfinished work; completed side effects are not
    /// rolled back. If cancellation and completion are both ready, cancellation
    /// takes precedence. The caller does not receive a result or a join handle.
    ///
    /// # Panics
    ///
    /// Panics if called outside a Tokio runtime.
    #[inline]
    fn spawn_task<F: Future<Output = Result<()>> + Send + 'static>(&self, future: F) {
        let cancel_token = self.cancel_token();
        spawn(async move {
            select! {
                biased;
                () = cancel_token.cancelled() => {},
                result = future => {
                    if let Err(_err) = result {
                        #[cfg(feature = "tracing")]
                        tracing::warn!(error = %_err, "managed background task failed");
                    }
                },
            }
        });
    }
}

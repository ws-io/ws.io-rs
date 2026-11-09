use std::{
    future::pending,
    time::Duration,
};

use anyhow::Result;

// Constants/Statics
pub(crate) const TEST_TIMEOUT: Duration = Duration::from_secs(2);

// Functions

/// Reports that it started, then stays pending while holding `alive` until dropped.
pub(crate) async fn park_until_dropped(
    started: tokio::sync::mpsc::UnboundedSender<()>,
    _alive: tokio::sync::mpsc::UnboundedSender<()>,
) -> Result<()> {
    started.send(()).unwrap();
    pending().await
}

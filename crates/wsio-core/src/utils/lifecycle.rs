use std::{
    future::Future,
    sync::Arc,
};

use tokio::{
    spawn,
    sync::Mutex,
};
use tokio_util::sync::CancellationToken;

// Structs
#[derive(Debug, Default)]
pub struct WsIoLifecycleCompletionSlot {
    token: Mutex<Option<CancellationToken>>,
}

impl WsIoLifecycleCompletionSlot {
    pub async fn wait_or_spawn<F, Fut>(self: &Arc<Self>, operation: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let completion_token = {
            let mut token_slot = self.token.lock().await;

            if let Some(token) = token_slot.as_ref() {
                token.clone()
            } else {
                let token = CancellationToken::new();
                *token_slot = Some(token.clone());

                let slot = self.clone();
                let completion_token = token.clone();
                drop(spawn(async move {
                    operation().await;
                    slot.token.lock().await.take();
                    completion_token.cancel();
                }));

                token
            }
        };

        completion_token.cancelled().await;
    }
}

use anyhow::Result;
use parking_lot::Mutex;

// Structs
pub(super) struct WsIoPacketContextCache<Context> {
    contexts: Mutex<Vec<Context>>,
}

impl<Context> WsIoPacketContextCache<Context> {
    #[inline]
    pub(super) fn new() -> Self {
        Self {
            contexts: Mutex::new(Vec::new()),
        }
    }

    #[inline]
    pub(super) fn with_context<Output>(
        &self,
        max_contexts: usize,
        create: impl FnOnce() -> Result<Context>,
        operation: impl FnOnce(&mut Context) -> Result<Output>,
    ) -> Result<Output> {
        let mut context = self.contexts.lock().pop().map_or_else(create, Ok)?;
        let result = operation(&mut context);

        let mut contexts = self.contexts.lock();
        if contexts.len() < max_contexts {
            contexts.push(context);
        }

        result
    }
}

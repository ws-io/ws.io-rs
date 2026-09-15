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

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn reuses_contexts_and_respects_the_cache_limit() {
        let cache = WsIoPacketContextCache::new();
        let created = Cell::new(0);
        let create = || {
            created.set(created.get() + 1);
            Ok(0)
        };

        assert_eq!(
            cache
                .with_context(1, create, |context| {
                    *context += 1;
                    Ok(*context)
                })
                .unwrap(),
            1
        );

        assert_eq!(
            cache
                .with_context(1, create, |context| {
                    *context += 1;
                    Ok(*context)
                })
                .unwrap(),
            2
        );

        assert_eq!(created.get(), 1);

        assert_eq!(
            cache
                .with_context(0, create, |context| {
                    *context += 1;
                    Ok(*context)
                })
                .unwrap(),
            3
        );

        assert_eq!(
            cache
                .with_context(1, create, |context| {
                    *context += 1;
                    Ok(*context)
                })
                .unwrap(),
            1
        );

        assert_eq!(created.get(), 2);
    }
}

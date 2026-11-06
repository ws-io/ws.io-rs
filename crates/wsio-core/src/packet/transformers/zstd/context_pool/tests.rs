use anyhow::bail;

use super::*;

#[test]
fn operation_errors_return_context_but_creation_errors_do_not_cache() {
    let cache = WsIoPacketContextCache::<usize>::new();
    assert!(cache.with_context(1, || bail!("creation failed"), |_| Ok(())).is_err());

    assert!(cache.contexts.lock().is_empty());
    let result: Result<()> = cache.with_context(
        1,
        || Ok(7),
        |context| {
            *context += 1;
            bail!("operation failed")
        },
    );

    assert!(result.is_err());
    assert_eq!(
        cache
            .with_context(1, || panic!("should reuse context"), |context| Ok(*context))
            .unwrap(),
        8
    );
}

#[test]
fn creation_and_nested_use_do_not_hold_the_cache_lock() {
    let cache = WsIoPacketContextCache::<usize>::new();
    cache
        .with_context(
            1,
            || {
                assert!(
                    cache.contexts.try_lock().is_some(),
                    "creation must not hold the cache lock"
                );

                Ok(1)
            },
            |outer| {
                cache.with_context(
                    1,
                    || Ok(2),
                    |inner| {
                        assert_ne!(*outer, *inner);
                        Ok(())
                    },
                )
            },
        )
        .unwrap();
}

#[test]
fn returned_contexts_are_bounded_by_max_contexts() {
    let cache = WsIoPacketContextCache::<usize>::new();
    cache
        .with_context(1, || Ok(1), |_| cache.with_context(1, || Ok(2), |_| Ok(())))
        .unwrap();

    assert_eq!(cache.contexts.lock().len(), 1);
    let uncached = WsIoPacketContextCache::<()>::new();
    uncached.with_context(0, || Ok(()), |()| Ok(())).unwrap();
    assert!(uncached.contexts.lock().is_empty());
}

use std::{
    any::{
        Any,
        TypeId,
    },
    sync::Arc,
};

use kikiutils::types::fx_collections::FxHashMap;
use parking_lot::RwLock;

// Structs
#[derive(Debug)]
pub struct ConnectionExtensions {
    inner: RwLock<FxHashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl ConnectionExtensions {
    #[inline]
    pub(super) fn new() -> Self {
        Self {
            inner: RwLock::new(FxHashMap::default()),
        }
    }

    // Public methods
    #[inline]
    pub fn clear<T: Send + Sync + 'static>(&self) {
        // Bind the removed value so it drops after the write lock is released.
        let _removed = self.inner.write().remove(&TypeId::of::<T>());
    }

    #[inline]
    pub fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.inner.read().contains_key(&TypeId::of::<T>())
    }

    #[inline]
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        let value = self.inner.read().get(&TypeId::of::<T>()).cloned();
        value.and_then(|value| value.downcast().ok())
    }

    #[inline]
    pub fn insert<T: Send + Sync + 'static>(&self, value: T) {
        let value: Arc<dyn Any + Send + Sync> = Arc::new(value);
        // Bind the replaced value so it drops after the write lock is released.
        let _previous = self.inner.write().insert(TypeId::of::<T>(), value);
    }

    #[inline]
    pub fn remove<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        let value = self.inner.write().remove(&TypeId::of::<T>());
        value.and_then(|value| value.downcast().ok())
    }
}

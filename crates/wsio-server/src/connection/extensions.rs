use std::{
    any::{
        Any,
        TypeId,
    },
    sync::Arc,
};

use kikiutils::types::fx_collections::FxDashMap;

// Structs
pub struct ConnectionExtensions {
    inner: FxDashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl ConnectionExtensions {
    #[inline]
    pub(super) fn new() -> Self {
        Self {
            inner: FxDashMap::default(),
        }
    }

    // Public methods
    #[inline]
    pub fn clear<T: Send + Sync + 'static>(&self) {
        self.inner.remove(&TypeId::of::<T>());
    }

    #[inline]
    pub fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.inner.contains_key(&TypeId::of::<T>())
    }

    #[inline]
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.inner
            .get(&TypeId::of::<T>())
            .and_then(|entry| entry.clone().downcast().ok())
    }

    #[inline]
    pub fn insert<T: Send + Sync + 'static>(&self, value: T) {
        self.inner.insert(TypeId::of::<T>(), Arc::new(value));
    }

    #[inline]
    pub fn remove<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.inner
            .remove(&TypeId::of::<T>())
            .and_then(|(_, v)| v.downcast().ok())
    }
}

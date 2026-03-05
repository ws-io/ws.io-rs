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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct SessionToken(String);

    #[derive(Debug, PartialEq)]
    struct UserData {
        id: u64,
        name: String,
    }

    #[test]
    fn test_connection_extensions() {
        let extensions = ConnectionExtensions::new();

        // 1. Initial State
        assert!(!extensions.contains::<UserData>());
        assert!(extensions.get::<UserData>().is_none());

        // 2. Insert and Get
        extensions.insert(UserData {
            id: 123,
            name: "Alice".into(),
        });

        assert!(extensions.contains::<UserData>());

        let user: Arc<UserData> = extensions.get::<UserData>().unwrap();
        assert_eq!(user.id, 123);
        assert_eq!(user.name, "Alice");

        // 3. Insert multiple types independently
        extensions.insert(SessionToken("secure_token_abc".into()));
        assert!(extensions.contains::<SessionToken>());
        assert!(extensions.contains::<UserData>());

        // 4. Overwrite existing type
        extensions.insert(UserData {
            id: 456,
            name: "Bob".into(),
        });

        let updated_user = extensions.get::<UserData>().unwrap();
        assert_eq!(updated_user.id, 456);
        assert_eq!(updated_user.name, "Bob");

        // 5. Remove
        let removed_token = extensions.remove::<SessionToken>().unwrap();
        assert_eq!(removed_token.0, "secure_token_abc");
        assert!(!extensions.contains::<SessionToken>());
        assert!(extensions.remove::<SessionToken>().is_none());

        // 6. Clear manually
        extensions.clear::<UserData>();
        assert!(!extensions.contains::<UserData>());
        assert!(extensions.get::<UserData>().is_none());
    }
}

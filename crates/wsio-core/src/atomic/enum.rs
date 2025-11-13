use std::{
    marker::PhantomData,
    sync::atomic::{
        AtomicU8,
        Ordering,
    },
};

use anyhow::{
    Result,
    anyhow,
    bail,
};

// Structs
pub struct AtomicEnum<T: Eq + Into<u8> + PartialEq + TryFrom<u8>> {
    _marker: PhantomData<T>,
    inner: AtomicU8,
}

impl<T: Eq + Into<u8> + PartialEq + TryFrom<u8>> AtomicEnum<T> {
    #[inline]
    pub fn new(value: T) -> Self {
        Self {
            _marker: PhantomData,
            inner: AtomicU8::new(value.into()),
        }
    }

    // Public methods
    #[inline]
    pub fn ensure<F: FnOnce(T) -> String>(&self, expected: T, message: F) -> Result<()> {
        let value = self.get();
        if value != expected {
            bail!(message(value));
        }

        Ok(())
    }

    #[inline]
    pub fn get(&self) -> T {
        T::try_from(self.inner.load(Ordering::SeqCst)).ok().unwrap()
    }

    #[inline]
    pub fn is(&self, value: T) -> bool {
        self.inner.load(Ordering::SeqCst) == value.into()
    }

    #[inline]
    pub fn store(&self, value: T) {
        self.inner.store(value.into(), Ordering::SeqCst);
    }

    #[inline]
    pub fn try_transition(&self, from: T, to: T) -> Result<()> {
        self.inner
            .compare_exchange(from.into(), to.into(), Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| ())
            .map_err(|_| anyhow!("Failed to transition value"))
    }
}

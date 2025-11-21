use std::{
    pin::Pin,
    sync::Arc,
};

use anyhow::Result;

type AsyncUnaryResultHandler<T> =
    dyn Fn(Arc<T>) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'static>> + Send + Sync + 'static;

pub type ArcAsyncUnaryResultHandler<T> = Arc<AsyncUnaryResultHandler<T>>;
pub type BoxAsyncUnaryResultHandler<T> = Box<AsyncUnaryResultHandler<T>>;

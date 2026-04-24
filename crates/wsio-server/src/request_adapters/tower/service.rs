use std::{
    fmt::Debug as FmtDebug,
    mem::replace,
    pin::Pin,
    sync::Arc,
    task::{
        Context,
        Poll,
    },
};

use http::{
    Request,
    Response,
};
use http_body::Body;
use tower_service::Service as TowerService;

use crate::{
    request::dispatch_request,
    runtime::WsIoServerRuntime,
};

// Structs
#[derive(Clone)]
pub struct WsIoServerService<S> {
    inner: S,
    runtime: Arc<WsIoServerRuntime>,
}

impl<S> WsIoServerService<S> {
    pub(super) fn new(inner: S, runtime: Arc<WsIoServerRuntime>) -> Self {
        Self { inner, runtime }
    }
}

impl<S, ReqBody, ResBody> TowerService<Request<ReqBody>> for WsIoServerService<S>
where
    ReqBody: Body + Default + FmtDebug + Send + Unpin + 'static,
    ReqBody::Data: Send,
    ReqBody::Error: FmtDebug,
    ResBody: Body + Default + Send + 'static,
    S: TowerService<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Error: Send + 'static,
    S::Future: Send + 'static,
{
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;
    type Response = S::Response;

    #[inline(always)]
    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        if request.uri().path() == self.runtime.config.request_path {
            let runtime = self.runtime.clone();
            Box::pin(async move { dispatch_request(request, runtime).await })
        } else {
            let inner = self.inner.clone();
            let mut inner = replace(&mut self.inner, inner);
            Box::pin(async move { inner.call(request).await })
        }
    }

    #[inline(always)]
    fn poll_ready(&mut self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(ctx)
    }
}

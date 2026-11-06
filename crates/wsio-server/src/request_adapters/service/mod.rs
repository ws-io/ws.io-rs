use std::{
    convert::Infallible,
    fmt::Debug as FmtDebug,
    future::{
        Ready,
        ready,
    },
    pin::Pin,
    sync::Arc,
    task::{
        Context,
        Poll,
    },
};

use bytes::Bytes;
use http::{
    Request,
    Response,
    StatusCode,
};
use http_body::{
    Body,
    Frame,
    SizeHint,
};
use hyper::service::Service as HyperService;

use crate::{
    request::dispatch_request,
    runtime::WsIoServerRuntime,
};

// Structs
/// An empty HTTP response body used by the standalone service.
#[derive(Debug, Clone, Copy, Default)]
pub struct EmptyBody;

impl Body for EmptyBody {
    type Data = Bytes;
    type Error = Infallible;

    #[inline]
    fn is_end_stream(&self) -> bool {
        true
    }

    #[inline]
    fn poll_frame(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        Poll::Ready(None)
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(0)
    }
}

/// The standalone service's fallback for requests outside the ws.io path.
#[derive(Debug, Clone, Copy, Default)]
pub struct NotFoundService;

impl<ReqBody> HyperService<Request<ReqBody>> for NotFoundService {
    type Error = Infallible;
    type Future = Ready<Result<Self::Response, Self::Error>>;
    type Response = Response<EmptyBody>;

    #[inline]
    fn call(&self, _: Request<ReqBody>) -> Self::Future {
        let mut response = Response::new(EmptyBody);
        *response.status_mut() = StatusCode::NOT_FOUND;
        ready(Ok(response))
    }
}

/// Handles ws.io requests and forwards other requests to the inner service.
///
/// Supports Hyper and Tower. The default inner service returns 404 responses
/// for use without another HTTP application.
#[derive(Debug, Clone)]
pub struct WsIoServerService<S = NotFoundService> {
    pub(super) inner: S,
    pub(super) runtime: Arc<WsIoServerRuntime>,
}

impl<S, ReqBody, ResBody> HyperService<Request<ReqBody>> for WsIoServerService<S>
where
    ReqBody: Body + FmtDebug + Send + Unpin + 'static,
    ReqBody::Data: Send,
    ReqBody::Error: FmtDebug,
    ResBody: Body + Default + Send + 'static,
    S: HyperService<Request<ReqBody>, Response = Response<ResBody>>,
    S::Error: Send + 'static,
    S::Future: Send + 'static,
{
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;
    type Response = S::Response;

    #[inline]
    fn call(&self, mut request: Request<ReqBody>) -> Self::Future {
        if request.uri().path() == self.runtime.config.request_path {
            let runtime = Arc::clone(&self.runtime);
            Box::pin(async move { dispatch_request(&mut request, &runtime) })
        } else {
            Box::pin(self.inner.call(request))
        }
    }
}

impl<S> WsIoServerService<S> {
    pub(crate) fn new(inner: S, runtime: Arc<WsIoServerRuntime>) -> Self {
        Self { inner, runtime }
    }
}

#[cfg(test)]
mod tests;

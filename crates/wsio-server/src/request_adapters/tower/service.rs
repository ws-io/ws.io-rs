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

#[cfg(test)]
mod tests {
    use std::{
        convert::Infallible,
        future::{
            Ready,
            ready,
        },
    };

    use axum::body::Body as AxumBody;
    use http::StatusCode;

    use super::*;
    use crate::WsIoServer;

    #[derive(Clone)]
    struct DummyService {
        status: StatusCode,
    }

    impl TowerService<Request<AxumBody>> for DummyService {
        type Error = Infallible;
        type Future = Ready<Result<Self::Response, Self::Error>>;
        type Response = Response<AxumBody>;

        fn call(&mut self, _request: Request<AxumBody>) -> Self::Future {
            ready(Ok(Response::builder()
                .status(self.status)
                .body(AxumBody::empty())
                .unwrap()))
        }

        fn poll_ready(&mut self, _ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn request_path_mismatch_delegates_to_inner_service() {
        let server = WsIoServer::builder().request_path("/ws.io").build();
        let mut service = WsIoServerService::new(
            DummyService {
                status: StatusCode::IM_A_TEAPOT,
            },
            server.0,
        );

        let request = Request::builder().uri("/not-ws.io").body(AxumBody::empty()).unwrap();

        let response = service.call(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::IM_A_TEAPOT);
    }
}

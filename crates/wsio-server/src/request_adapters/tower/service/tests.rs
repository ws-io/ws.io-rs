use std::{
    convert::Infallible,
    future::{
        Ready,
        poll_fn,
        ready,
    },
    mem::take,
};

use bytes::Bytes;
use http::{
    Method,
    StatusCode,
    Version,
};
use http_body::Frame;
use parking_lot::Mutex;
use tokio::{
    spawn,
    sync::{
        mpsc::{
            self,
            error::TryRecvError,
        },
        oneshot,
    },
    time::timeout,
};
use tower_layer::Layer;

use super::*;
use crate::{
    WsIoServer,
    test_support::TEST_TIMEOUT,
};

#[derive(Debug, Default)]
struct TestBody(Option<Bytes>);

impl Body for TestBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        Poll::Ready(self.0.take().map(|bytes| Ok(Frame::data(bytes))))
    }
}

struct TestService {
    ready_rx: Arc<Mutex<mpsc::UnboundedReceiver<Result<(), &'static str>>>>,
    requests_tx: mpsc::UnboundedSender<Request<TestBody>>,
    ready: bool,
    call_error: Option<&'static str>,
}

impl TestService {
    fn new(
        ready_rx: mpsc::UnboundedReceiver<Result<(), &'static str>>,
        requests_tx: mpsc::UnboundedSender<Request<TestBody>>,
    ) -> Self {
        Self {
            ready_rx: Arc::new(Mutex::new(ready_rx)),
            requests_tx,
            ready: false,
            call_error: None,
        }
    }
}

impl Clone for TestService {
    fn clone(&self) -> Self {
        Self {
            ready_rx: Arc::clone(&self.ready_rx),
            requests_tx: self.requests_tx.clone(),
            // Readiness belongs to the polled instance, not its clones.
            ready: false,
            call_error: self.call_error,
        }
    }
}

impl TowerService<Request<TestBody>> for TestService {
    type Error = &'static str;
    type Future = Ready<Result<Response<TestBody>, Self::Error>>;
    type Response = Response<TestBody>;

    fn poll_ready(&mut self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.ready {
            return Poll::Ready(Ok(()));
        }

        match self.ready_rx.lock().poll_recv(ctx) {
            Poll::Ready(Some(result)) => {
                self.ready = result.is_ok();
                Poll::Ready(result)
            },
            Poll::Ready(None) => panic!("readiness control channel closed"),
            Poll::Pending => Poll::Pending,
        }
    }

    fn call(&mut self, request: Request<TestBody>) -> Self::Future {
        assert!(
            take(&mut self.ready),
            "call must use the instance that acquired readiness"
        );

        self.requests_tx.send(request).unwrap();
        ready(if let Some(error) = self.call_error {
            Err(error)
        } else {
            Ok(Response::builder()
                .status(StatusCode::CREATED)
                .header("x-inner", "response")
                .body(TestBody(Some(Bytes::from_static(b"inner response"))))
                .unwrap())
        })
    }
}

#[tokio::test]
async fn request_paths_dispatch_or_forward_without_altering_requests() {
    let server = WsIoServer::builder().request_path("/socket").unwrap().build().unwrap();
    let (ready_tx, ready_rx) = mpsc::unbounded_channel();
    let (requests_tx, mut requests_rx) = mpsc::unbounded_channel();
    let mut service = server.layer().layer(TestService::new(ready_rx, requests_tx));
    ready_tx.send(Ok(())).unwrap();
    timeout(TEST_TIMEOUT, poll_fn(|ctx| service.poll_ready(ctx)))
        .await
        .unwrap()
        .unwrap();

    // Only path matching belongs here; handshake validation is covered in request::tests.
    let response = timeout(
        TEST_TIMEOUT,
        service.call(
            Request::builder()
                .method(Method::POST)
                .uri("/socket?namespace=/test")
                .body(TestBody::default())
                .unwrap(),
        ),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(requests_rx.try_recv().unwrap_err(), TryRecvError::Empty);

    for uri in ["/socket-extra", "/socket/", "/ws.io", "/other?namespace=/test"] {
        ready_tx.send(Ok(())).unwrap();
        timeout(TEST_TIMEOUT, poll_fn(|ctx| service.poll_ready(ctx)))
            .await
            .unwrap()
            .unwrap();

        let mut request = Request::builder()
            .method(Method::PATCH)
            .version(Version::HTTP_2)
            .uri(uri)
            .header("x-request-id", "original")
            .body(TestBody(Some(Bytes::from_static(b"request payload"))))
            .unwrap();

        request.extensions_mut().insert(42_u64);
        let response = timeout(TEST_TIMEOUT, service.call(request)).await.unwrap().unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(response.headers()["x-inner"], "response");
        assert_eq!(response.body().0.as_deref(), Some(b"inner response".as_slice()));

        let forwarded = requests_rx.try_recv().unwrap();
        assert_eq!(forwarded.uri(), uri);
        assert_eq!(forwarded.method(), Method::PATCH);
        assert_eq!(forwarded.version(), Version::HTTP_2);
        assert_eq!(forwarded.headers().len(), 1);
        assert_eq!(forwarded.headers()["x-request-id"], "original");
        assert_eq!(forwarded.body().0.as_deref(), Some(b"request payload".as_slice()));
        assert_eq!(forwarded.extensions().get::<u64>(), Some(&42));
        assert_eq!(requests_rx.try_recv().unwrap_err(), TryRecvError::Empty);
    }
}

#[tokio::test]
async fn readiness_wakes_waiters_and_call_uses_the_ready_instance() {
    let server = WsIoServer::builder().build().unwrap();
    let (ready_tx, ready_rx) = mpsc::unbounded_channel();
    let (requests_tx, mut requests_rx) = mpsc::unbounded_channel();
    let mut service = server.layer().layer(TestService::new(ready_rx, requests_tx));

    for uri in ["/first", "/second"] {
        let (pending_tx, pending_rx) = oneshot::channel();
        let readiness_task = spawn(async move {
            let mut pending_tx = Some(pending_tx);
            let result = poll_fn(|ctx| {
                let result = service.poll_ready(ctx);
                if result.is_pending()
                    && let Some(pending_tx) = pending_tx.take()
                {
                    pending_tx.send(()).unwrap();
                }

                result
            })
            .await;

            (service, result)
        });

        timeout(TEST_TIMEOUT, pending_rx).await.unwrap().unwrap();
        ready_tx.send(Ok(())).unwrap();
        let (ready_service, result) = timeout(TEST_TIMEOUT, readiness_task).await.unwrap().unwrap();
        result.unwrap();
        service = ready_service;
        let request = Request::builder().uri(uri).body(TestBody::default()).unwrap();
        let response = timeout(TEST_TIMEOUT, service.call(request)).await.unwrap().unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(requests_rx.try_recv().unwrap().uri(), uri);
        assert_eq!(requests_rx.try_recv().unwrap_err(), TryRecvError::Empty);
    }
}

#[tokio::test]
async fn inner_readiness_and_call_errors_are_preserved() {
    let server = WsIoServer::builder().build().unwrap();
    for ready_result in [Err("readiness failed"), Ok(())] {
        let (ready_tx, ready_rx) = mpsc::unbounded_channel();
        let (requests_tx, mut requests_rx) = mpsc::unbounded_channel();
        let mut inner = TestService::new(ready_rx, requests_tx);
        inner.call_error = Some("call failed");
        let mut service = server.layer().layer(inner);

        ready_tx.send(ready_result).unwrap();
        let result = timeout(TEST_TIMEOUT, poll_fn(|ctx| service.poll_ready(ctx)))
            .await
            .unwrap();

        assert_eq!(result, ready_result);
        assert_eq!(requests_rx.try_recv().unwrap_err(), TryRecvError::Empty);
        if result.is_err() {
            continue;
        }

        let request = Request::builder().uri("/other").body(TestBody::default()).unwrap();
        let error = timeout(TEST_TIMEOUT, service.call(request)).await.unwrap().unwrap_err();
        assert_eq!(error, "call failed");
        assert_eq!(requests_rx.try_recv().unwrap().uri(), "/other");
        assert_eq!(requests_rx.try_recv().unwrap_err(), TryRecvError::Empty);
    }
}

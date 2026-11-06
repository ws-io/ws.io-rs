use std::time::Duration;

use futures_util::{
    SinkExt,
    StreamExt,
};
use http::{
    Method,
    Version,
};
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use tokio::{
    io::{
        DuplexStream,
        duplex,
    },
    spawn,
    sync::mpsc::{
        self,
        error::TryRecvError,
    },
    time::timeout,
};
use tokio_tungstenite::{
    WebSocketStream,
    client_async,
    tungstenite::Message,
};

use super::*;
use crate::{
    WsIoServer,
    core::packet::{
        WsIoPacket,
        WsIoPacketType,
        codecs::WsIoPacketCodec,
    },
    test_support::{
        TEST_TIMEOUT,
        TestBody,
    },
};

// Intentionally not Clone: native Hyper forwarding must use the supplied service.
struct InnerService {
    requests_tx: mpsc::UnboundedSender<Request<TestBody>>,
    error: Option<&'static str>,
}

impl HyperService<Request<TestBody>> for InnerService {
    type Error = &'static str;
    type Future = Ready<Result<Self::Response, Self::Error>>;
    type Response = Response<TestBody>;

    fn call(&self, request: Request<TestBody>) -> Self::Future {
        self.requests_tx.send(request).unwrap();
        ready(if let Some(error) = self.error {
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

async fn next_packet(client: &mut WebSocketStream<DuplexStream>) -> WsIoPacket {
    let message = timeout(TEST_TIMEOUT, client.next()).await.unwrap().unwrap().unwrap();
    let Message::Binary(bytes) = message else {
        panic!("expected a binary packet")
    };

    WsIoPacketCodec::Msgpack.decode(&bytes).unwrap()
}

#[tokio::test]
async fn hyper_service_routes_requests_and_preserves_inner_results() {
    let server = WsIoServer::builder().request_path("/socket").unwrap().build().unwrap();

    for error in [None, Some("inner failed")] {
        let (requests_tx, mut requests_rx) = mpsc::unbounded_channel();
        let service = server.service_with_inner(InnerService { requests_tx, error });
        let request = Request::builder()
            .method(Method::POST)
            .uri("/socket?namespace=/test")
            .body(TestBody::default())
            .unwrap();

        let response = timeout(TEST_TIMEOUT, service.call(request)).await.unwrap().unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(requests_rx.try_recv().unwrap_err(), TryRecvError::Empty);

        for uri in ["/socket-extra", "/socket/", "/ws.io", "/other?namespace=/test"] {
            let mut request = Request::builder()
                .method(Method::PATCH)
                .version(Version::HTTP_2)
                .uri(uri)
                .header("x-request-id", "original")
                .body(TestBody(Some(Bytes::from_static(b"request payload"))))
                .unwrap();

            request.extensions_mut().insert(42_u64);
            let result = timeout(TEST_TIMEOUT, service.call(request)).await.unwrap();
            if let Some(error) = error {
                assert_eq!(result.unwrap_err(), error);
            } else {
                let response = result.unwrap();
                assert_eq!(response.status(), StatusCode::CREATED);
                assert_eq!(response.headers()["x-inner"], "response");
                assert_eq!(response.body().0.as_deref(), Some(b"inner response".as_slice()));
            }

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
}

#[tokio::test]
async fn standalone_hyper_service_supports_fallback_and_websocket_upgrade() {
    let server = WsIoServer::builder()
        .request_path("/socket")
        .unwrap()
        .disconnect_timeout(Duration::ZERO)
        .build()
        .unwrap();

    let namespace = server.new_namespace_builder("/test").register().unwrap();
    let service = server.service();
    for uri in ["/other", "/socket-extra", "/ws.io"] {
        let request = Request::builder().uri(uri).body(EmptyBody).unwrap();
        let response = timeout(TEST_TIMEOUT, service.call(request)).await.unwrap().unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(response.body().is_end_stream());
        assert_eq!(response.body().size_hint().exact(), Some(0));
    }

    let (server_io, client_io) = duplex(4096);
    let http_task = spawn(async move {
        http1::Builder::new()
            .serve_connection(TokioIo::new(server_io), service)
            .with_upgrades()
            .await
            .unwrap();
    });

    let (mut client, response) = timeout(
        TEST_TIMEOUT,
        client_async("ws://localhost/socket?namespace=/test", client_io),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    assert!(matches!(next_packet(&mut client).await.r#type, WsIoPacketType::Init));

    let init = WsIoPacketCodec::Msgpack.encode(&WsIoPacket::new_init(None)).unwrap();
    timeout(TEST_TIMEOUT, client.send(Message::Binary(init)))
        .await
        .unwrap()
        .unwrap();

    assert!(matches!(next_packet(&mut client).await.r#type, WsIoPacketType::Ready));

    timeout(TEST_TIMEOUT, namespace.emit::<()>("hello", None))
        .await
        .unwrap()
        .unwrap();

    let event = next_packet(&mut client).await;
    assert!(matches!(event.r#type, WsIoPacketType::Event));
    assert_eq!(event.key.as_deref(), Some("hello"));

    timeout(TEST_TIMEOUT, server.shutdown()).await.unwrap();
    drop(client);
    timeout(TEST_TIMEOUT, http_task).await.unwrap().unwrap();
    assert_eq!(server.connection_count(), 0);
}

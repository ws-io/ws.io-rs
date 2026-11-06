use std::{
    convert::Infallible,
    sync::Arc,
    time::Duration,
};

use http::header::{
    SEC_WEBSOCKET_ACCEPT,
    SEC_WEBSOCKET_KEY,
    SEC_WEBSOCKET_VERSION,
    UPGRADE,
};
use hyper::{
    body::Incoming,
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use tokio::{
    io::{
        AsyncWriteExt,
        duplex,
    },
    spawn,
    sync::mpsc,
    time::timeout,
};

use super::*;
use crate::{
    WsIoServer,
    test_support::{
        EmptyBody,
        TEST_TIMEOUT,
    },
};

fn new_request() -> Request<()> {
    Request::builder()
        .uri("/ws.io?namespace=/test")
        .header(CONNECTION, "Upgrade")
        .header(UPGRADE, "websocket")
        .header(SEC_WEBSOCKET_KEY, "dGhlIHNhbXBsZSBub25jZQ==")
        .header(SEC_WEBSOCKET_VERSION, "13")
        .body(())
        .unwrap()
}

#[test]
fn connection_tokens_match_whole_values_across_repeated_fields() {
    for (values, expected) in [
        (&[][..], false),
        (&["Upgrade"][..], true),
        (&["keep-alive", "close, \tUpGrAdE\t"][..], true),
        (&["UpGrAdE", "keep-alive"][..], true),
        (&["keep-alive, \tUPGRADE\t, close"][..], true),
        (&["keep-alive"][..], false),
        (&["x-upgrade, upgrade-extra"][..], false),
        (&["up grade"][..], false),
        (&[" ,\t, "][..], false),
    ] {
        let mut request = Request::new(());
        for value in values {
            request
                .headers_mut()
                .append(CONNECTION, HeaderValue::from_str(value).unwrap());
        }

        assert_eq!(
            check_header_token(&request, CONNECTION, "upgrade"),
            expected,
            "{values:?}"
        );
    }

    let mut request = Request::new(());
    request
        .headers_mut()
        .insert(CONNECTION, HeaderValue::from_bytes(b"\xff").unwrap());

    assert!(!check_header_token(&request, CONNECTION, "upgrade"));
}

#[test]
fn rejected_requests_do_not_consume_upgrade_or_request_metadata() {
    let server = WsIoServer::builder().build().unwrap();
    server.new_namespace_builder("/test").register().unwrap();
    // No Tokio runtime: a rejected request must not spawn an upgrade task.
    for (name, configure, expected_status) in [
        (
            "method",
            (|request: &mut Request<()>| *request.method_mut() = Method::POST) as fn(&mut Request<()>),
            StatusCode::METHOD_NOT_ALLOWED,
        ),
        (
            "HTTP/1.0",
            |request| *request.version_mut() = Version::HTTP_10,
            StatusCode::BAD_REQUEST,
        ),
        (
            "HTTP/2",
            |request| *request.version_mut() = Version::HTTP_2,
            StatusCode::BAD_REQUEST,
        ),
        (
            "missing Connection",
            |request| {
                request.headers_mut().remove(CONNECTION);
            },
            StatusCode::BAD_REQUEST,
        ),
        (
            "partial Connection token",
            |request| {
                request
                    .headers_mut()
                    .insert(CONNECTION, HeaderValue::from_static("x-upgrade"));
            },
            StatusCode::BAD_REQUEST,
        ),
        (
            "missing Upgrade",
            |request| {
                request.headers_mut().remove(UPGRADE);
            },
            StatusCode::BAD_REQUEST,
        ),
        (
            "invalid key",
            |request| {
                request
                    .headers_mut()
                    .insert(SEC_WEBSOCKET_KEY, HeaderValue::from_static("not-a-websocket-key"));
            },
            StatusCode::BAD_REQUEST,
        ),
        (
            "invalid WebSocket version",
            |request| {
                request
                    .headers_mut()
                    .insert(SEC_WEBSOCKET_VERSION, HeaderValue::from_static("12"));
            },
            StatusCode::BAD_REQUEST,
        ),
        (
            "missing namespace",
            |request| *request.uri_mut() = "/ws.io?trace=1".parse().unwrap(),
            StatusCode::BAD_REQUEST,
        ),
        (
            "unknown namespace",
            |request| *request.uri_mut() = "/ws.io?namespace=/missing".parse().unwrap(),
            StatusCode::NOT_FOUND,
        ),
        (
            "invalid namespace",
            |request| *request.uri_mut() = "/ws.io?namespace=%2F..%2Ftest".parse().unwrap(),
            StatusCode::NOT_FOUND,
        ),
        (
            "missing upgrade extension",
            |request| {
                request.extensions_mut().remove::<OnUpgrade>();
            },
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    ] {
        let mut request = new_request();
        request.extensions_mut().insert(hyper::upgrade::on(Request::new(())));
        configure(&mut request);
        let headers = request.headers().clone();
        let uri = request.uri().clone();
        let method = request.method().clone();
        let version = request.version();
        let has_upgrade = request.extensions().get::<OnUpgrade>().is_some();
        let response = dispatch_request::<_, (), Infallible>(&mut request, &server.0).unwrap();

        assert_eq!(response.status(), expected_status, "{name}");
        assert_eq!(request.headers(), &headers, "{name}");
        assert_eq!(request.uri(), &uri, "{name}");
        assert_eq!(request.method(), &method, "{name}");
        assert_eq!(request.version(), version, "{name}");
        assert_eq!(request.extensions().get::<OnUpgrade>().is_some(), has_upgrade, "{name}");
    }

    assert_eq!(server.connection_count(), 0);
}

#[tokio::test]
async fn handshake_normalization_preserves_headers_and_uri_for_connection() {
    let (metadata_tx, mut metadata_rx) = mpsc::unbounded_channel();
    let server = WsIoServer::builder()
        .disconnect_timeout(Duration::ZERO)
        .build()
        .unwrap();

    server
        .new_namespace_builder("/test")
        .with_init_request(move |connection| {
            metadata_tx
                .send((connection.headers().clone(), connection.request_uri().clone()))
                .unwrap();

            async { Ok(None::<()>) }
        })
        .register()
        .unwrap();

    let (server_io, mut client_io) = duplex(4096);
    let runtime = Arc::clone(&server.0);
    let http_task = spawn(async move {
        http1::Builder::new()
            .serve_connection(
                TokioIo::new(server_io),
                service_fn(move |mut request: Request<Incoming>| {
                    let response = dispatch_request::<_, EmptyBody, Infallible>(&mut request, &runtime).unwrap();
                    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
                    assert_eq!(response.headers()[SEC_WEBSOCKET_ACCEPT], "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
                    async { Ok::<_, Infallible>(response) }
                }),
            )
            .with_upgrades()
            .await
            .unwrap();
    });

    // Write raw HTTP: the normal WebSocket client may normalize repeated headers itself.
    timeout(
        TEST_TIMEOUT,
        client_io.write_all(
            concat!(
                "GET /ws.io?trace=one%2Btwo&namespace=%2F%2Ftest%2F%2F&keep=1 HTTP/1.1\r\n",
                "Host: localhost\r\n",
                "Connection: keep-alive\r\n",
                "Connection: close, UpGrAdE\r\n",
                "Upgrade: websocket\r\n",
                "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n",
                "Sec-WebSocket-Version: 13\r\n",
                "Origin: https://example.test\r\n",
                "X-Trace-Id: first\r\n",
                "X-Trace-Id: second\r\n\r\n",
            )
            .as_bytes(),
        ),
    )
    .await
    .unwrap()
    .unwrap();

    let (headers, uri) = timeout(TEST_TIMEOUT, metadata_rx.recv()).await.unwrap().unwrap();
    let mut expected_headers = new_request().into_parts().0.headers;
    expected_headers.insert("host", HeaderValue::from_static("localhost"));
    expected_headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
    expected_headers.append(CONNECTION, HeaderValue::from_static("close, UpGrAdE"));
    expected_headers.insert("origin", HeaderValue::from_static("https://example.test"));
    expected_headers.append("x-trace-id", HeaderValue::from_static("first"));
    expected_headers.append("x-trace-id", HeaderValue::from_static("second"));
    assert_eq!(headers, expected_headers);
    assert_eq!(uri, "/ws.io?trace=one%2Btwo&namespace=%2F%2Ftest%2F%2F&keep=1");

    timeout(TEST_TIMEOUT, server.shutdown()).await.unwrap();
    drop(client_io);
    timeout(TEST_TIMEOUT, http_task).await.unwrap().unwrap();
    assert_eq!(server.connection_count(), 0);
}

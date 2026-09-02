use std::sync::Arc;

use http::{
    HeaderName,
    HeaderValue,
    Method,
    Request,
    Response,
    StatusCode,
    Version,
    header::CONNECTION,
};
use hyper::upgrade::OnUpgrade;
use tokio_tungstenite::tungstenite::handshake::server::create_response;
use url::form_urlencoded;

use crate::runtime::WsIoServerRuntime;

// Functions
#[inline]
fn check_header_token<ReqBody>(request: &Request<ReqBody>, name: HeaderName, expected_token: &str) -> bool {
    request.headers().get_all(name).iter().any(|value| {
        value.to_str().is_ok_and(|value| {
            value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case(expected_token))
        })
    })
}

pub(super) async fn dispatch_request<ReqBody, ResBody: Default, E: Send>(
    mut request: Request<ReqBody>,
    runtime: Arc<WsIoServerRuntime>,
) -> Result<Response<ResBody>, E> {
    // Check method
    if request.method() != Method::GET {
        #[cfg(feature = "tracing")]
        tracing::trace!(
            method = %request.method(),
            path = request.uri().path(),
            "rejecting WebSocket request with unsupported method"
        );

        return respond(StatusCode::METHOD_NOT_ALLOWED);
    }

    // Traditional WebSocket upgrades use HTTP/1.1. HTTP/2 requires the
    // extended CONNECT protocol, which this server does not implement.
    if request.version() != Version::HTTP_11 {
        #[cfg(feature = "tracing")]
        tracing::trace!(
            path = request.uri().path(),
            version = ?request.version(),
            "rejecting WebSocket request with unsupported HTTP version"
        );

        return respond(StatusCode::BAD_REQUEST);
    }

    // Tungstenite reads one Connection value, while HTTP permits the token
    // across repeated fields. Validate all values before normalizing them.
    if !check_header_token(&request, CONNECTION, "upgrade") {
        #[cfg(feature = "tracing")]
        tracing::trace!(
            path = request.uri().path(),
            "rejecting invalid WebSocket Connection header"
        );

        return respond(StatusCode::BAD_REQUEST);
    }

    // Preserve the original handshake metadata and headers, normalizing only
    // the Connection field already validated above.
    let mut handshake_request = Request::new(());
    *handshake_request.method_mut() = request.method().clone();
    *handshake_request.uri_mut() = request.uri().clone();
    *handshake_request.version_mut() = request.version();
    *handshake_request.headers_mut() = request.headers().clone();
    handshake_request
        .headers_mut()
        .insert(CONNECTION, HeaderValue::from_static("Upgrade"));

    let Ok(response) = create_response(&handshake_request) else {
        #[cfg(feature = "tracing")]
        tracing::trace!(path = request.uri().path(), "rejecting invalid WebSocket handshake");
        return respond(StatusCode::BAD_REQUEST);
    };

    // Get namespace path
    let Some((_, namespace_path)) = request
        .uri()
        .query()
        .and_then(|q| form_urlencoded::parse(q.as_bytes()).find(|(k, _)| k == "namespace"))
    else {
        #[cfg(feature = "tracing")]
        tracing::trace!(
            path = request.uri().path(),
            "rejecting WebSocket request without namespace"
        );

        return respond(StatusCode::BAD_REQUEST);
    };
    let namespace_path = namespace_path.into_owned();

    // Get namespace
    let Some(namespace) = runtime.get_namespace(&namespace_path) else {
        #[cfg(feature = "tracing")]
        tracing::trace!(namespace = %namespace_path, "rejecting WebSocket request for unknown namespace");
        return respond(StatusCode::NOT_FOUND);
    };

    // Upgrade
    let Some(on_upgrade) = request.extensions_mut().remove::<OnUpgrade>() else {
        #[cfg(feature = "tracing")]
        tracing::debug!(namespace = %namespace_path, "rejecting WebSocket request without upgrade extension");
        return respond(StatusCode::INTERNAL_SERVER_ERROR);
    };

    namespace.handle_on_upgrade_request(request.headers().clone(), on_upgrade, request.uri().clone());

    #[cfg(feature = "tracing")]
    tracing::debug!(namespace = %namespace_path, "accepted WebSocket upgrade request");
    Ok(response.map(|()| ResBody::default()))
}

#[inline]
#[allow(
    clippy::unnecessary_wraps,
    reason = "the adapter response must preserve the wrapped service error type"
)]
fn respond<ResBody: Default, E: Send>(status: StatusCode) -> Result<Response<ResBody>, E> {
    let mut response = Response::new(ResBody::default());
    *response.status_mut() = status;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use http::header::{
        CONNECTION,
        SEC_WEBSOCKET_KEY,
        SEC_WEBSOCKET_VERSION,
        UPGRADE,
    };

    use super::*;
    use crate::WsIoServer;

    fn valid_upgrade_request(uri: &str) -> Request<()> {
        Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header(UPGRADE, "websocket")
            .header(CONNECTION, "Upgrade")
            .header(SEC_WEBSOCKET_VERSION, "13")
            .header(SEC_WEBSOCKET_KEY, "dGhlIHNhbXBsZSBub25jZQ==")
            .body(())
            .unwrap()
    }

    async fn dispatch_status(request: Request<()>, server: &WsIoServer) -> StatusCode {
        dispatch_request::<_, (), Infallible>(request, server.0.clone())
            .await
            .unwrap()
            .status()
    }

    #[test]
    fn check_header_token_accepts_comma_separated_connection_values() {
        let request = Request::builder()
            .header(CONNECTION, "keep-alive, Upgrade")
            .body(())
            .unwrap();

        assert!(check_header_token(&request, CONNECTION, "upgrade"));
    }

    #[test]
    fn check_header_token_accepts_repeated_connection_headers() {
        let request = Request::builder()
            .header(CONNECTION, "keep-alive")
            .header(CONNECTION, "Upgrade")
            .body(())
            .unwrap();

        assert!(check_header_token(&request, CONNECTION, "upgrade"));
    }

    #[test]
    fn check_header_token_rejects_partial_token_matches() {
        let request = Request::builder().header(CONNECTION, "not-upgrade").body(()).unwrap();

        assert!(!check_header_token(&request, CONNECTION, "upgrade"));
    }

    #[tokio::test]
    async fn dispatch_request_rejects_non_get_method() {
        let server = WsIoServer::builder().build();
        let request = Request::builder()
            .method(Method::POST)
            .uri("/ws.io?namespace=/socket")
            .body(())
            .unwrap();

        assert_eq!(dispatch_status(request, &server).await, StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn dispatch_request_rejects_missing_upgrade_headers() {
        let server = WsIoServer::builder().build();
        let request = Request::builder()
            .method(Method::GET)
            .uri("/ws.io?namespace=/socket")
            .body(())
            .unwrap();

        assert_eq!(dispatch_status(request, &server).await, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn dispatch_request_rejects_missing_sec_websocket_key() {
        let server = WsIoServer::builder().build();
        let request = Request::builder()
            .method(Method::GET)
            .uri("/ws.io?namespace=/socket")
            .header(UPGRADE, "websocket")
            .header(CONNECTION, "Upgrade")
            .header(SEC_WEBSOCKET_VERSION, "13")
            .body(())
            .unwrap();

        assert_eq!(dispatch_status(request, &server).await, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn dispatch_request_rejects_invalid_sec_websocket_key() {
        let server = WsIoServer::builder().build();
        server.new_namespace_builder("/socket").register().unwrap();
        let mut request = valid_upgrade_request("/ws.io?namespace=/socket");
        request
            .headers_mut()
            .insert(SEC_WEBSOCKET_KEY, HeaderValue::from_static("invalid"));

        assert_eq!(dispatch_status(request, &server).await, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn dispatch_request_rejects_http_2_upgrade() {
        let server = WsIoServer::builder().build();
        let mut request = valid_upgrade_request("/ws.io?namespace=/socket");
        *request.version_mut() = Version::HTTP_2;

        assert_eq!(dispatch_status(request, &server).await, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn dispatch_request_accepts_repeated_connection_headers() {
        let server = WsIoServer::builder().build();
        server.new_namespace_builder("/socket").register().unwrap();
        let mut request = valid_upgrade_request("/ws.io?namespace=/socket");
        let headers = request.headers_mut();
        headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
        headers.append(CONNECTION, HeaderValue::from_static("Upgrade"));

        assert_eq!(
            dispatch_status(request, &server).await,
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[tokio::test]
    async fn dispatch_request_rejects_missing_namespace_query() {
        let server = WsIoServer::builder().build();

        assert_eq!(
            dispatch_status(valid_upgrade_request("/ws.io"), &server).await,
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn dispatch_request_rejects_unknown_namespace() {
        let server = WsIoServer::builder().build();

        assert_eq!(
            dispatch_status(valid_upgrade_request("/ws.io?namespace=/missing"), &server).await,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn dispatch_request_requires_hyper_on_upgrade_extension() {
        let server = WsIoServer::builder().build();
        server.new_namespace_builder("/socket").register().unwrap();

        assert_eq!(
            dispatch_status(valid_upgrade_request("/ws.io?namespace=/socket"), &server).await,
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

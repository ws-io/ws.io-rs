use std::sync::Arc;

use http::{
    HeaderName,
    Method,
    Request,
    Response,
    StatusCode,
    header::{
        CONNECTION,
        SEC_WEBSOCKET_ACCEPT,
        SEC_WEBSOCKET_KEY,
        SEC_WEBSOCKET_VERSION,
        UPGRADE,
    },
};
use hyper::upgrade::OnUpgrade;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
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

#[inline]
fn check_header_value<ReqBody>(request: &Request<ReqBody>, name: HeaderName, expected_value: &[u8]) -> bool {
    match request.headers().get(name) {
        Some(value) => value.as_bytes().eq_ignore_ascii_case(expected_value),
        None => false,
    }
}

pub(super) async fn dispatch_request<ReqBody, ResBody: Default, E: Send>(
    mut request: Request<ReqBody>,
    runtime: Arc<WsIoServerRuntime>,
) -> Result<Response<ResBody>, E> {
    // Check method
    if request.method() != Method::GET {
        return respond(StatusCode::METHOD_NOT_ALLOWED);
    }

    // Check required headers
    if !check_header_value(&request, UPGRADE, b"websocket")
        || !check_header_token(&request, CONNECTION, "upgrade")
        || !check_header_value(&request, SEC_WEBSOCKET_VERSION, b"13")
    {
        return respond(StatusCode::BAD_REQUEST);
    }

    // Get websocket sec key
    let Some(ws_sec_key) = request.headers().get(SEC_WEBSOCKET_KEY).and_then(|v| v.to_str().ok()) else {
        return respond(StatusCode::BAD_REQUEST);
    };

    // Get namespace path
    let Some((_, namespace_path)) = request
        .uri()
        .query()
        .and_then(|q| form_urlencoded::parse(q.as_bytes()).find(|(k, _)| k == "namespace"))
    else {
        return respond(StatusCode::BAD_REQUEST);
    };

    // Get namespace
    let Some(namespace) = runtime.get_namespace(&namespace_path) else {
        return respond(StatusCode::NOT_FOUND);
    };

    // Generate accept key
    let ws_accept_key = derive_accept_key(ws_sec_key.as_bytes());

    // Upgrade
    let on_upgrade = match request.extensions_mut().remove::<OnUpgrade>() {
        Some(upgrade) => upgrade,
        None => return respond(StatusCode::INTERNAL_SERVER_ERROR),
    };

    namespace
        .handle_on_upgrade_request(request.headers().clone(), on_upgrade, request.uri().clone())
        .await;

    Ok(Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(CONNECTION, "Upgrade")
        .header(SEC_WEBSOCKET_ACCEPT, ws_accept_key)
        .header(UPGRADE, "websocket")
        .body(ResBody::default())
        .unwrap())
}

#[inline]
fn respond<ResBody: Default, E: Send>(status: StatusCode) -> Result<Response<ResBody>, E> {
    Ok(Response::builder().status(status).body(ResBody::default()).unwrap())
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use http::header::CONNECTION;

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

    #[test]
    fn check_header_value_rejects_missing_header() {
        let request = Request::builder().body(()).unwrap();

        assert!(!check_header_value(&request, UPGRADE, b"websocket"));
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

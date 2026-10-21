use std::mem::take;

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

pub(super) fn dispatch_request<ReqBody, ResBody: Default, E: Send>(
    request: &mut Request<ReqBody>,
    runtime: &WsIoServerRuntime,
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
    if !check_header_token(request, CONNECTION, "upgrade") {
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

    namespace.handle_on_upgrade_request(take(request.headers_mut()), on_upgrade, take(request.uri_mut()));

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

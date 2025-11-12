use std::sync::{
    Arc,
    LazyLock,
};

use anyhow::Result;
use kikiutils::{
    signal::wait_for_shutdown_signal,
    tracing::init_tracing_with_local_time_format,
};
use tokio::join;
use wsio_client::{
    WsIoClient,
    session::WsIoClientSession,
    core::packet::codecs::WsIoPacketCodec,
};

// Constants/Statics
static BINCODE: LazyLock<WsIoClient> = LazyLock::new(|| {
    const NAMESPACE: &str = "/bincode";
    let client = WsIoClient::builder(format!("ws://127.0.0.1:8000/{NAMESPACE}").as_str())
        .unwrap()
        .on_session_close(|session| on_session_close(session, NAMESPACE))
        .on_session_ready(|session| on_session_ready(session, NAMESPACE))
        .packet_codec(WsIoPacketCodec::Bincode)
        .build();

    client.on("test", |_, _: Arc<()>| on_event(NAMESPACE));
    client
});

static CBOR: LazyLock<WsIoClient> = LazyLock::new(|| {
    const NAMESPACE: &str = "/cbor";
    let client = WsIoClient::builder(format!("ws://127.0.0.1:8000/{NAMESPACE}").as_str())
        .unwrap()
        .on_session_close(|session| on_session_close(session, NAMESPACE))
        .on_session_ready(|session| on_session_ready(session, NAMESPACE))
        .packet_codec(WsIoPacketCodec::Cbor)
        .build();

    client.on("test", |_, _: Arc<()>| on_event(NAMESPACE));
    client
});

static DISCONNECT: LazyLock<WsIoClient> = LazyLock::new(|| {
    const NAMESPACE: &str = "/disconnect";
    let client = WsIoClient::builder(format!("ws://127.0.0.1:8000/{NAMESPACE}").as_str())
        .unwrap()
        .on_session_close(|session| on_session_close(session, NAMESPACE))
        .on_session_ready(|session| on_session_ready(session, NAMESPACE))
        .build();

    client.on("test", |_, _: Arc<()>| on_event(NAMESPACE));
    client
});

static MSGPACK: LazyLock<WsIoClient> = LazyLock::new(|| {
    const NAMESPACE: &str = "/msgpack";
    let client = WsIoClient::builder(format!("ws://127.0.0.1:8000/{NAMESPACE}").as_str())
        .unwrap()
        .on_session_close(|session| on_session_close(session, NAMESPACE))
        .on_session_ready(|session| on_session_ready(session, NAMESPACE))
        .packet_codec(WsIoPacketCodec::Msgpack)
        .build();

    client.on("test", |_, _: Arc<()>| on_event(NAMESPACE));
    client
});

static INIT: LazyLock<WsIoClient> = LazyLock::new(|| {
    const NAMESPACE: &str = "/init";
    let client = WsIoClient::builder(format!("ws://127.0.0.1:8000/{NAMESPACE}").as_str())
        .unwrap()
        .on_session_close(|session| on_session_close(session, NAMESPACE))
        .on_session_ready(|session| on_session_ready(session, NAMESPACE))
        .with_init_handler(|_, _: Option<()>| async { Ok(Some(())) })
        .build();

    client.on("test", |_, _: Arc<()>| on_event(NAMESPACE));
    client
});

static POSTCARD: LazyLock<WsIoClient> = LazyLock::new(|| {
    const NAMESPACE: &str = "/postcard";
    let client = WsIoClient::builder(format!("ws://127.0.0.1:8000/{NAMESPACE}").as_str())
        .unwrap()
        .on_session_close(|session| on_session_close(session, NAMESPACE))
        .on_session_ready(|session| on_session_ready(session, NAMESPACE))
        .packet_codec(WsIoPacketCodec::Postcard)
        .build();

    client.on("test", |_, _: Arc<()>| on_event(NAMESPACE));
    client
});

static SERDE_JSON: LazyLock<WsIoClient> = LazyLock::new(|| {
    const NAMESPACE: &str = "/serde-json";
    let client = WsIoClient::builder(format!("ws://127.0.0.1:8000/{NAMESPACE}").as_str())
        .unwrap()
        .on_session_close(|session| on_session_close(session, NAMESPACE))
        .on_session_ready(|session| on_session_ready(session, NAMESPACE))
        .packet_codec(WsIoPacketCodec::SerdeJson)
        .build();

    client.on("test", |_, _: Arc<()>| on_event(NAMESPACE));
    client
});

static SONIC_RS: LazyLock<WsIoClient> = LazyLock::new(|| {
    const NAMESPACE: &str = "/sonic-rs";
    let client = WsIoClient::builder(format!("ws://127.0.0.1:8000/{NAMESPACE}").as_str())
        .unwrap()
        .on_session_close(|session| on_session_close(session, NAMESPACE))
        .on_session_ready(|session| on_session_ready(session, NAMESPACE))
        .packet_codec(WsIoPacketCodec::SonicRs)
        .build();

    client.on("test", |_, _: Arc<()>| on_event(NAMESPACE));
    client
});

// Functions
async fn on_session_close(_: Arc<WsIoClientSession>, namespace: &str) -> Result<()> {
    tracing::info!("{namespace}: on_session_close");
    Ok(())
}

async fn on_session_ready(_: Arc<WsIoClientSession>, namespace: &str) -> Result<()> {
    tracing::info!("{namespace}: on_session_ready");
    Ok(())
}

async fn on_event(namespace: &str) -> Result<()> {
    tracing::info!("{namespace}: on_event");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = init_tracing_with_local_time_format();
    join!(
        BINCODE.connect(),
        CBOR.connect(),
        DISCONNECT.connect(),
        INIT.connect(),
        MSGPACK.connect(),
        POSTCARD.connect(),
        SERDE_JSON.connect(),
        SONIC_RS.connect(),
    );

    let _ = wait_for_shutdown_signal().await;
    join!(
        BINCODE.disconnect(),
        CBOR.disconnect(),
        DISCONNECT.disconnect(),
        INIT.disconnect(),
        MSGPACK.disconnect(),
        POSTCARD.disconnect(),
        SERDE_JSON.disconnect(),
        SONIC_RS.disconnect(),
    );

    tracing::info!("Stopped");
    Ok(())
}

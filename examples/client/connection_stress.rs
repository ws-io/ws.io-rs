use std::{
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use tikv_jemallocator::Jemalloc;
use tokio::{
    sync::Semaphore,
    time::sleep,
};
use wsio_client::{
    WsIoClient,
    core::packet::codecs::WsIoPacketCodec,
};

// Constants/Statics
const CLIENT_COUNT: usize = 10000;
const CONNECT_CONCURRENCY: usize = 1000;
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[tokio::main]
async fn main() -> Result<()> {
    let sem = Arc::new(Semaphore::new(CONNECT_CONCURRENCY));
    let mut handles = Vec::with_capacity(CLIENT_COUNT);
    for i in 0..CLIENT_COUNT {
        let permit = sem.clone().acquire_owned().await?;
        handles.push(tokio::spawn(async move {
            let client = WsIoClient::builder("ws://127.0.0.1:8000/postcard")
                .unwrap()
                .packet_codec(WsIoPacketCodec::Postcard)
                .build();

            client.connect().await;
            drop(permit);

            if i % 1000 == 0 {
                println!("connected {i}");
            }

            sleep(Duration::from_secs(10)).await;
            client.disconnect().await;

            if i % 1000 == 0 {
                println!("disconnected {i}");
            }
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

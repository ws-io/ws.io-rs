#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use std::{
    hint::black_box,
    sync::Arc,
    time::Duration,
};

use axum::{
    Router,
    serve,
};
use criterion::{
    BenchmarkId,
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
};
use tokio::{
    net::TcpListener,
    runtime::{
        Builder,
        Runtime,
    },
    sync::Semaphore,
    task::JoinHandle,
    time::{
        sleep,
        timeout,
    },
};
use wsio_client::WsIoClient;
use wsio_server::{
    WsIoServer,
    namespace::WsIoServerNamespace,
};

// Constants/Statics
const ACK_EVENTS: [&str; 2] = ["joined", "left"];
const CLIENT_COUNTS: [usize; 3] = [1, 10, 50];
const CLIENT_READY_TIMEOUT: Duration = Duration::from_secs(3);
const LARGE_PAYLOAD_CLIENT_COUNT: usize = 10;
const LARGE_PAYLOAD_SIZES: [usize; 2] = [1024, 16 * 1024];
const POLL_INTERVAL: Duration = Duration::from_millis(5);
const ROOM_A: &str = "room-a";
const TEST_NAMESPACE: &str = "/socket";

// Structs
struct BenchServer {
    acks: Arc<Semaphore>,
    clients: Vec<WsIoClient>,
    namespace: Arc<WsIoServerNamespace>,
    server_task: JoinHandle<()>,
}

impl BenchServer {
    async fn shutdown(self) {
        for client in self.clients {
            client.disconnect().await;
        }

        self.server_task.abort();
        let _ = self.server_task.await;
    }
}

// Functions
async fn wait_for_condition(mut condition: impl FnMut() -> bool, failure: &str) {
    timeout(CLIENT_READY_TIMEOUT, async {
        while !condition() {
            sleep(POLL_INTERVAL).await;
        }
    })
    .await
    .expect(failure);
}

async fn wait_for_client_ready(client: &WsIoClient) {
    wait_for_condition(|| client.is_session_ready(), "benchmark client should become ready").await;
}

fn register_ack_counter(client: &WsIoClient, acks: &Arc<Semaphore>) {
    for event in ACK_EVENTS {
        let acks = acks.clone();
        client.on(event, move |_session, _data: Arc<()>| {
            let acks = acks.clone();
            async move {
                acks.add_permits(1);
                Ok(())
            }
        });
    }
}

async fn setup_room_server(client_count: usize, joined_room_count: usize) -> BenchServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let ws_url = format!("ws://{local_addr}{TEST_NAMESPACE}");

    let server = Arc::new(WsIoServer::builder().build());
    let namespace = server
        .new_namespace_builder(TEST_NAMESPACE)
        .on_connect(|connection| async move {
            connection.on("join", |connection, room: Arc<String>| async move {
                connection.join([room.as_str()]);
                connection.emit::<()>("joined", None).await?;
                Ok(())
            });

            connection.on("leave", |connection, room: Arc<String>| async move {
                connection.leave([room.as_str()]);
                connection.emit::<()>("left", None).await?;
                Ok(())
            });

            Ok(())
        })
        .register()
        .unwrap();

    let app = Router::new().layer(server.layer());
    let server_task = tokio::spawn(async move {
        serve(listener, app).await.unwrap();
    });

    let mut clients = Vec::with_capacity(client_count);
    let acks = Arc::new(Semaphore::new(0));
    for index in 0..client_count {
        let client = WsIoClient::builder(ws_url.as_str()).unwrap().build();
        register_ack_counter(&client, &acks);

        client.connect().await;
        wait_for_client_ready(&client).await;

        if index < joined_room_count {
            client.emit("join", Some(&ROOM_A)).await.unwrap();
        }

        clients.push(client);
    }

    wait_for_acks(&acks, joined_room_count).await;

    BenchServer {
        acks,
        clients,
        namespace,
        server_task,
    }
}

fn runtime() -> Runtime {
    Builder::new_current_thread().enable_all().build().unwrap()
}

async fn wait_for_acks(acks: &Semaphore, count: usize) {
    timeout(CLIENT_READY_TIMEOUT, acks.acquire_many(count as u32))
        .await
        .expect("room operation acknowledgements should arrive")
        .expect("ack semaphore should remain open")
        .forget();
}

fn bench_broadcast_emit(criterion: &mut Criterion) {
    let runtime = runtime();
    let mut group = criterion.benchmark_group("server/broadcast_emit");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(3));

    for client_count in CLIENT_COUNTS {
        let server = runtime.block_on(setup_room_server(client_count, client_count));

        group.bench_with_input(
            BenchmarkId::new("global", client_count),
            &server.namespace,
            |bencher, namespace| {
                bencher.to_async(&runtime).iter(|| async {
                    namespace.emit::<()>(black_box("bench"), None).await.unwrap();
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("room", client_count),
            &server.namespace,
            |bencher, namespace| {
                bencher.to_async(&runtime).iter(|| async {
                    namespace
                        .to([black_box(ROOM_A)])
                        .emit::<()>(black_box("bench"), None)
                        .await
                        .unwrap();
                });
            },
        );

        runtime.block_on(server.shutdown());
    }

    group.finish();
}

fn bench_broadcast_payload(criterion: &mut Criterion) {
    let runtime = runtime();
    let mut group = criterion.benchmark_group("server/broadcast_payload");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(3));

    for payload_size in LARGE_PAYLOAD_SIZES {
        let payload = vec![0; payload_size];
        let aggregate_payload_size = payload_size * LARGE_PAYLOAD_CLIENT_COUNT;
        let server = runtime.block_on(setup_room_server(
            LARGE_PAYLOAD_CLIENT_COUNT,
            LARGE_PAYLOAD_CLIENT_COUNT,
        ));

        // Report aggregate logical payload bytes delivered across recipients;
        // protocol encoding and WebSocket framing are intentionally excluded.
        group.throughput(Throughput::Bytes(aggregate_payload_size as u64));
        group.bench_with_input(
            BenchmarkId::new("global", payload_size),
            &payload,
            |bencher, payload| {
                bencher.to_async(&runtime).iter(|| async {
                    server
                        .namespace
                        .emit(black_box("bench_payload"), Some(black_box(payload)))
                        .await
                        .unwrap();
                });
            },
        );

        group.bench_with_input(BenchmarkId::new("room", payload_size), &payload, |bencher, payload| {
            bencher.to_async(&runtime).iter(|| async {
                server
                    .namespace
                    .to([black_box(ROOM_A)])
                    .emit(black_box("bench_payload"), Some(black_box(payload)))
                    .await
                    .unwrap();
            });
        });

        runtime.block_on(server.shutdown());
    }

    group.finish();
}

fn bench_room_churn(criterion: &mut Criterion) {
    let runtime = runtime();
    let mut group = criterion.benchmark_group("server/room_churn");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(3));

    for client_count in CLIENT_COUNTS {
        let server = runtime.block_on(setup_room_server(client_count, 0));
        let mut room_index = 0usize;

        group.bench_with_input(
            BenchmarkId::new("join_leave_roundtrip", client_count),
            &server,
            |bencher, server| {
                bencher.to_async(&runtime).iter(|| {
                    room_index += 1;
                    let room = format!("room-{room_index}");
                    async move {
                        for client in &server.clients {
                            client.emit("join", Some(&room)).await.unwrap();
                        }

                        wait_for_acks(&server.acks, client_count).await;

                        for client in &server.clients {
                            client.emit("leave", Some(&room)).await.unwrap();
                        }

                        wait_for_acks(&server.acks, client_count).await;
                    }
                });
            },
        );

        runtime.block_on(server.shutdown());
    }

    group.finish();
}

criterion_group!(benches, bench_broadcast_emit, bench_broadcast_payload, bench_room_churn);
criterion_main!(benches);

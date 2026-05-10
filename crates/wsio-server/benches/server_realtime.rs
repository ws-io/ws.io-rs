use std::{
    hint::black_box,
    sync::{
        Arc,
        atomic::{
            AtomicUsize,
            Ordering,
        },
    },
    time::Duration,
};

use axum::{
    Router,
    serve,
};
use criterion::{
    BenchmarkId,
    Criterion,
    criterion_group,
    criterion_main,
};
use tokio::{
    net::TcpListener,
    runtime::{
        Builder,
        Runtime,
    },
    task::{
        JoinHandle,
        yield_now,
    },
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

const CLIENT_READY_TIMEOUT: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(5);

struct BenchServer {
    ack_count: Arc<AtomicUsize>,
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

async fn wait_for_client_ready(client: &WsIoClient) {
    timeout(CLIENT_READY_TIMEOUT, async {
        while !client.is_session_ready() {
            sleep(POLL_INTERVAL).await;
        }
    })
    .await
    .expect("benchmark client should become ready");
}

async fn setup_room_server(client_count: usize, joined_room_count: usize) -> BenchServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let ws_url = format!("ws://{local_addr}/socket");

    let server = Arc::new(WsIoServer::builder().build());
    let namespace = server
        .new_namespace_builder("/socket")
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
    let ack_count = Arc::new(AtomicUsize::new(0));
    for index in 0..client_count {
        let client = WsIoClient::builder(ws_url.as_str()).unwrap().build();
        for event in ["joined", "left"] {
            let ack_count = ack_count.clone();
            client.on(event, move |_session, _data: Arc<()>| {
                let ack_count = ack_count.clone();
                async move {
                    ack_count.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }
            });
        }

        client.connect().await;
        wait_for_client_ready(&client).await;

        if index < joined_room_count {
            client.emit("join", Some(&"room-a")).await.unwrap();
        }

        clients.push(client);
    }

    sleep(Duration::from_millis(25)).await;

    BenchServer {
        ack_count,
        clients,
        namespace,
        server_task,
    }
}

async fn wait_for_ack_count(ack_count: &AtomicUsize, target: usize) {
    timeout(CLIENT_READY_TIMEOUT, async {
        while ack_count.load(Ordering::Relaxed) < target {
            yield_now().await;
        }
    })
    .await
    .expect("room operation acknowledgements should arrive");
}

fn runtime() -> Runtime {
    Builder::new_current_thread().enable_all().build().unwrap()
}

fn bench_broadcast_emit(criterion: &mut Criterion) {
    let runtime = runtime();
    let mut group = criterion.benchmark_group("server/broadcast_emit");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(3));

    for client_count in [1, 10, 50] {
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
                        .to([black_box("room-a")])
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

fn bench_room_churn(criterion: &mut Criterion) {
    let runtime = runtime();
    let mut group = criterion.benchmark_group("server/room_churn");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(3));

    for client_count in [1, 10, 50] {
        let server = runtime.block_on(setup_room_server(client_count, 0));
        let mut room_index = 0usize;

        group.bench_with_input(
            BenchmarkId::new("join_leave_roundtrip", client_count),
            &server,
            |bencher, server| {
                bencher.to_async(&runtime).iter(|| {
                    room_index += 1;
                    let room = format!("room-{room_index}");
                    let target_acks = server.ack_count.load(Ordering::Relaxed) + (client_count * 2);

                    async move {
                        for client in &server.clients {
                            client.emit("join", Some(&room)).await.unwrap();
                        }

                        for client in &server.clients {
                            client.emit("leave", Some(&room)).await.unwrap();
                        }

                        wait_for_ack_count(&server.ack_count, target_acks).await;
                    }
                });
            },
        );

        runtime.block_on(server.shutdown());
    }

    group.finish();
}

criterion_group!(benches, bench_broadcast_emit, bench_room_churn);
criterion_main!(benches);

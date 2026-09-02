# Rust Migration Notes

This document summarizes the user-visible migration items from baseline commit `2f552b1f1200a45d0ed0f7fe187db1ee95918c93` to the current HEAD `fa37ec1dc7c903b4e39498ee0db57bf1b5447de6`. Routine dependency, script, and license-file updates are intentionally omitted.

## Breaking Changes

### 1. Packets now use binary MsgPack

- The default codec changed from `SerdeJson` to `WsIoPacketCodec::Msgpack`.
- The JSON and SonicRs codecs were removed. MsgPack is always available; CBOR and Postcard remain optional Rust codecs. Both endpoints must use the same codec, and the TypeScript client currently supports only MsgPack and CBOR.
- All protocol packets use binary WebSocket frames. Rust no longer accepts text frames, so an older JSON/text client must be upgraded together with the server.
- `WsIoPacketCodec::encode` and `encode_data` now return `bytes::Bytes`, and `WsIoPacket.data` changed from `Option<Vec<u8>>` to `Option<Bytes>`.

```rust
use bytes::Bytes;

let data: Bytes = WsIoPacketCodec::Msgpack.encode_data(&payload)?;
let packet = WsIoPacket::new_event("event", Some(data));
```

### 2. Rust and TypeScript event payloads must have the same shape

TypeScript `emit("event", value)` always encodes its arguments as `[value]`, and multiple arguments as `[a, b, ...]`. Rust `emit` serializes the supplied type directly, so cross-language events must use a tuple, array, or `Vec` instead of a scalar or object that the other side is expected to expand automatically.

```rust
// Corresponds to TS emit("stdin", value)
client.emit("stdin", Some(&(value,))).await?;

// Corresponds to TS emit("resize", width, height)
client.emit("resize", Some(&(width, height))).await?;
```

Rust handlers receiving TypeScript events should also use tuple or sequence types such as `(T,)` and `(A, B)`. Custom codecs must preserve this payload shape.

### 3. Events are dispatched FIFO within each connection

Each server connection and client session owns a bounded event queue and a single dispatcher. Packets from one connection are processed in wire order, and the dispatcher waits for the current packet's handler batch before processing the next packet. Different connections remain independent and can run in parallel. A full queue applies backpressure; queued events are discarded during shutdown without persistence or replay.

Multiple handlers for one event are still supported. Handlers for the same packet start concurrently and are all awaited, so registration order does not define handler execution order. Individual handler errors and panics are isolated and logged; payload decode failures or a fatal dispatcher error/panic close the affected connection.

### 4. Event keys must be non-empty

- `WsIoEventRegistry::on` rejects an empty key with an assertion panic.
- Decoding an event packet with a missing or empty key returns an error; the reader then stops and enters cleanup.
- Rust `emit` and `WsIoPacket::new_event` currently do not validate outbound keys. Callers must ensure that keys are non-empty instead of relying on the receiver to repair malformed packets.

### 5. Public types and cancellation APIs changed

- `WsIoEventRegistry<C, S>` became `WsIoEventRegistry<C>`; the unused `TaskSpawner` generic was removed.
- `WsIoEventRegistry::dispatch_event_packet` changed from a detached, fire-and-forget call to `async fn`. It no longer takes a `TaskSpawner`; callers pass `&CancellationToken`, await the method, and handle its `Result<()>`.
- `WsIoPacketCodec::is_text()` was removed because packet transport is always binary.
- `TaskSpawner::cancel_token()` and `WsIoClient::cancel_token()` now return `CancellationToken` rather than `Arc<CancellationToken>`. `CancellationToken` is cloneable by itself, so custom `TaskSpawner` implementations should remove the outer `Arc`.
- Each session and connection owns its own cancellation token; the runtime may still replace its lifecycle token between runs.

```rust
impl TaskSpawner for MySpawner {
    fn cancel_token(&self) -> CancellationToken {
        self.token.clone()
    }
}
```

## Runtime and Lifecycle Changes

- Client request modifiers and the WebSocket handshake now observe cancellation, so cancelling connection setup no longer waits for the modifier or handshake to finish. The existing `connect_timeout` behavior is preserved.
- The server namespace uses `TaskTracker` to reap completed connection tasks; shutdown closes the tracker and waits for remaining tasks.
- Server runtime and namespace shutdown operations are serialized by asynchronous operation locks, preventing concurrent state transitions from racing.
- Client query reconstruction now preserves percent-encoding. For example, `token=a%26b` remains encoded instead of becoming an additional query field. Callers should use standard URL/query APIs rather than relying on manually decoded query strings.

## Migration Checklist

- [ ] Upgrade both endpoints to a common binary codec, using MsgPack by default or CBOR on both sides.
- [ ] Remove JSON codec usage and any obsolete codec feature flags from application configuration.
- [ ] Change packet data and codec call sites from `Vec<u8>` to `Bytes`.
- [ ] Encode cross-language event arguments as tuples or other sequences that match the TypeScript array payload.
- [ ] Ensure every `on`, `emit`, and `off` event key is non-empty.
- [ ] Update custom `TaskSpawner` implementations and direct `dispatch_event_packet` callers.
- [ ] Account for bounded-queue backpressure and the fact that pending events are dropped on shutdown.

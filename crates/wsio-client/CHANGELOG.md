# Changelog

## 0.7.13 - 2025-11-23 11:04

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.7.12...wsio-client-v0.7.13)

### 🩹 Fixes

- change some iterator impls from `AsRef<str>` to `Into<String>` to fix certain errors ([33a0675](https://github.com/ws-io/ws.io-rs/commit/33a0675))

## 0.7.12 - 2025-11-21 06:24

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.7.11...wsio-client-v0.7.12)

### 🚀 Enhancements

- *(client)* periodically send ping frames after connection to prevent it from being closed ([1219b60](https://github.com/ws-io/ws.io-rs/commit/1219b60))

## 0.7.11 - 2025-11-21 05:44

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.7.9...wsio-client-v0.7.11)

### 🚀 Enhancements

- *(client)* add TLS-related features for `tokio-tungstenite` and validate TLS feature when using `wss` in client builder ([144a6a5](https://github.com/ws-io/ws.io-rs/commit/144a6a5))

## 0.7.9 - 2025-11-21 05:26

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.7.8...wsio-client-v0.7.9)

### 🚀 Enhancements

- *(client)* add additional tracing features ([afe0f1e](https://github.com/ws-io/ws.io-rs/commit/afe0f1e))

## 0.7.8 - 2025-11-21 01:56

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.7.7...wsio-client-v0.7.8)

### 💅 Refactors

- extract some internal types and utilities into a separate crate ([3b02fd0](https://github.com/ws-io/ws.io-rs/commit/3b02fd0))

## 0.7.7 - 2025-11-17 01:44

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.7.6...wsio-client-v0.7.7)

### 🏡 Chore

- unify and organize all default timeout durations ([277ea3b](https://github.com/ws-io/ws.io-rs/commit/277ea3b))

## 0.7.6 - 2025-11-13 04:25

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.7.5...wsio-client-v0.7.6)

### 💅 Refactors

- rename `AtomicStatus` to `AtomicEnum`, `SessionStatus` to `SessionState`, `ConnectionStatus` to `ConnectionState` ([15d6f96](https://github.com/ws-io/ws.io-rs/commit/15d6f96))
- adjust timing of status check in handle event packet ([034c3e8](https://github.com/ws-io/ws.io-rs/commit/034c3e8))

## 0.7.5 - 2025-11-12 09:15

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.7.4...wsio-client-v0.7.5)

### 🏡 Chore

- updated the following local packages: wsio-core ([0000000](https://github.com/ws-io/ws.io-rs/commit/0000000))

## 0.7.4 - 2025-11-12 01:24

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.7.3...wsio-client-v0.7.4)

### 🏡 Chore

- update some comments ([23a2919](https://github.com/ws-io/ws.io-rs/commit/23a2919))

### 💅 Refactors

- *(client)* rename some internal variables ([774dcb9](https://github.com/ws-io/ws.io-rs/commit/774dcb9))
- *(client)* rename some internal variables ([6dc637e](https://github.com/ws-io/ws.io-rs/commit/6dc637e))

### 🩹 Fixes

- ignore event packets received when not in ready state ([a651d33](https://github.com/ws-io/ws.io-rs/commit/a651d33))

## [0.7.3](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.7.2...wsio-client-v0.7.3) - 2025-11-08 15:17

### 🏡 Chore

- update some comments ([c8e62ac](https://github.com/ws-io/ws.io-rs/commit/c8e62ac))

### 💅 Refactors

- *(client)* remove unnecessary break ([57748ce](https://github.com/ws-io/ws.io-rs/commit/57748ce))

### 🩹 Fixes

- *(client)* abort event-message-flush task before canceling cancel_token on disconnect ([f9ca3fb](https://github.com/ws-io/ws.io-rs/commit/f9ca3fb))
- *(client)* adjust timing of calling `session.close` in `disconnect` ([6809ed7](https://github.com/ws-io/ws.io-rs/commit/6809ed7))

## [0.7.2](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.7.1...wsio-client-v0.7.2) - 2025-11-06 09:35

### 🏡 Chore

- update dependencies ([c99dd2e](https://github.com/ws-io/ws.io-rs/commit/c99dd2e))

### 💅 Refactors

- rename some params name ([2bd3db2](https://github.com/ws-io/ws.io-rs/commit/2bd3db2))

## [0.7.1](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.7.0...wsio-client-v0.7.1) - 2025-11-02 10:21

### 🏡 Chore

- update package metadata ([9d8c0de](https://github.com/ws-io/ws.io-rs/commit/9d8c0de))

### 🚀 Enhancements

- *(client)* add `is_ready` method to client and session ([c26b878](https://github.com/ws-io/ws.io-rs/commit/c26b878))

## [0.7.0](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.6.0...wsio-client-v0.7.0) - 2025-10-31 16:59

### 💅 Refactors

- *(client)* [**breaking**] rename `connection` to `session` and `reconnection_delay` config to `reconnect_delay` ([fb4e96c](https://github.com/ws-io/ws.io-rs/commit/fb4e96c))

## [0.6.0](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.5.1...wsio-client-v0.6.0) - 2025-10-30 12:55

### 🎨 Styles

- update string formatting style in some parts of the code ([1195fd0](https://github.com/ws-io/ws.io-rs/commit/1195fd0))

### 💅 Refactors

- *(client)* separate `connect_url` field from config and normalize path when setting via `builder.request_path` ([5f9bfa5](https://github.com/ws-io/ws.io-rs/commit/5f9bfa5))

### 🚀 Enhancements

- [**breaking**] update custom protocol handshake behavior, all auth-related behavior and replace with init-based flow ([a85248b](https://github.com/ws-io/ws.io-rs/commit/a85248b))

### 🩹 Fixes

- *(client)* preserve original query when processing input URI on client side ([5216c9b](https://github.com/ws-io/ws.io-rs/commit/5216c9b))

## [0.5.1](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.5.0...wsio-client-v0.5.1) - 2025-10-29 06:31

### 🏡 Chore

- update packet meta ([8ada5df](https://github.com/ws-io/ws.io-rs/commit/8ada5df))

### 💅 Refactors

- replace all `impl Into<String>` with `impl AsRef<str>` and update internal string-related function parameters to `&str` ([7452d7b](https://github.com/ws-io/ws.io-rs/commit/7452d7b))

### 🚀 Enhancements

- implement server namespace broadcast functionality, refactor and clean up code ([7619362](https://github.com/ws-io/ws.io-rs/commit/7619362))

## [0.5.0](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.4.1...wsio-client-v0.5.0) - 2025-10-28

### Added

- implement spawn management in `WsIoEventRegistry`, update and clean up code

### Other

- clean up, modify, and optimize code

## [0.4.1](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.4.0...wsio-client-v0.4.1) - 2025-10-27 08:04

### 💅 Refactors

- further simplify and merge parts of code ([bed226c](https://github.com/ws-io/ws.io-rs/commit/bed226c))
- simplify and modify parts of the code ([4923c46](https://github.com/ws-io/ws.io-rs/commit/4923c46))
- clean up and optimize code ([0282065](https://github.com/ws-io/ws.io-rs/commit/0282065))

### 🚀 Enhancements

- add postcard packet codec ([1f1297f](https://github.com/ws-io/ws.io-rs/commit/1f1297f))
- clean up and optimize code, implement initial event handling after receiving event ([8c6e461](https://github.com/ws-io/ws.io-rs/commit/8c6e461))
- add event registration functionality ([2dfcb1d](https://github.com/ws-io/ws.io-rs/commit/2dfcb1d))

## [0.4.0](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.3.0...wsio-client-v0.4.0) - 2025-10-25 07:21

### 🏡 Chore

- remove unused or unnecessary TODO comments ([ddb0c19](https://github.com/ws-io/ws.io-rs/commit/ddb0c19))

### 💅 Refactors

- *(client)* lint code ([a846571](https://github.com/ws-io/ws.io-rs/commit/a846571))
- *(client)* change functions that unnecessarily returned `Result` to return directly ([0f7e3fc](https://github.com/ws-io/ws.io-rs/commit/0f7e3fc))
- *(client)* remove unreachable condition check in `.disconnect` method ([16a691a](https://github.com/ws-io/ws.io-rs/commit/16a691a))
- [**breaking**] update auth handler to require sending `data` ([4a273c2](https://github.com/ws-io/ws.io-rs/commit/4a273c2))
- merge/extract parts of code and replace some `Arc` with `Box` ([65a6b50](https://github.com/ws-io/ws.io-rs/commit/65a6b50))

### 🚀 Enhancements

- *(client)* implement `emit` with buffering and automatic retry functionality ([da0ede9](https://github.com/ws-io/ws.io-rs/commit/da0ede9))

### 🩹 Fixes

- resolve issue where calling `.auth` methods on builder captured current codec, causing inconsistent results when codec and auth builder method call order differed ([d4f7bd2](https://github.com/ws-io/ws.io-rs/commit/d4f7bd2))

## [0.3.0](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.2.2...wsio-client-v0.3.0) - 2025-10-23 07:04

### 🏡 Chore

- [**breaking**] adjust default timeout durations ([4e882f7](https://github.com/ws-io/ws.io-rs/commit/4e882f7))
- add some todo ([6bfd3f6](https://github.com/ws-io/ws.io-rs/commit/6bfd3f6))
- *(client)* format code ([18ae855](https://github.com/ws-io/ws.io-rs/commit/18ae855))

### 💅 Refactors

- change `message_tx.send` to `try_send` in `connection.close` method ([100e2c6](https://github.com/ws-io/ws.io-rs/commit/100e2c6))
- *(client)* [**breaking**] rename some config fields and add some config comments ([16503b5](https://github.com/ws-io/ws.io-rs/commit/16503b5))
- *(client)* simplify `WsIoClientConnection` abort-timeout task logic using core utils ([344d718](https://github.com/ws-io/ws.io-rs/commit/344d718))
- *(client)* merge duplicate code sections ([8640aeb](https://github.com/ws-io/ws.io-rs/commit/8640aeb))
- *(client)* replace `matches!(self.status.get(), RuntimeStatus::Running)` with `self.status.is(RuntimeStatus::Running)` ([ad58649](https://github.com/ws-io/ws.io-rs/commit/ad58649))
- *(client)* code cleanup and handler improvements ([9cac566](https://github.com/ws-io/ws.io-rs/commit/9cac566))
- *(client)* clean up, simplify, and review status transition logic within runtime ([bcb2a80](https://github.com/ws-io/ws.io-rs/commit/bcb2a80))
- *(client)* rename `WsIoClientRuntimeStatus` to `RuntimeStatus` ([eab14e3](https://github.com/ws-io/ws.io-rs/commit/eab14e3))
- dynamically determine internal channel capacity during connection creation based on WebSocket config ([a7526f6](https://github.com/ws-io/ws.io-rs/commit/a7526f6))
- spawn `on_ready_handler` execution and prevent connection interruption if it panics ([52eddc9](https://github.com/ws-io/ws.io-rs/commit/52eddc9))

### 🚀 Enhancements

- *(client)* add `WsIoClientConnection.spawn_task` method ([70b86bd](https://github.com/ws-io/ws.io-rs/commit/70b86bd))

### 🩹 Fixes

- *(client)* force abort the counterpart task when either `read_ws_stream_task` or `write_ws_stream_task` completes in `run_connection` ([5365513](https://github.com/ws-io/ws.io-rs/commit/5365513))
- *(client)* spawn `disconnect` call after receiving `disconnect` packet to prevent potential deadlock ([dabb379](https://github.com/ws-io/ws.io-rs/commit/dabb379))
- *(client)* immediately break `run_connection_loop` upon receiving `break_notify.notified` ([1310e0d](https://github.com/ws-io/ws.io-rs/commit/1310e0d))
- *(client)* replace `ws_stream_writer.flush()` with `close` method ([0b25ad7](https://github.com/ws-io/ws.io-rs/commit/0b25ad7))

## [0.2.2](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.2.1...wsio-client-v0.2.2) - 2025-10-22 05:54

### 🏀 Examples

- move files to `examples` workspace ([667bfe5](https://github.com/ws-io/ws.io-rs/commit/667bfe5))
- add disconnect example ([dacb448](https://github.com/ws-io/ws.io-rs/commit/dacb448))
- add `connection_stress` client example ([61719a0](https://github.com/ws-io/ws.io-rs/commit/61719a0))
- rename files ([cf536ad](https://github.com/ws-io/ws.io-rs/commit/cf536ad))

### 🏡 Chore

- *(client)* format `Cargo.toml` ([03bd228](https://github.com/ws-io/ws.io-rs/commit/03bd228))
- disable or replace certain dependency features to reduce overall dependencies ([1d88ae3](https://github.com/ws-io/ws.io-rs/commit/1d88ae3))

### 💅 Refactors

- *(client)* change `WsIoClientRuntime.connection` to `ArcSwapOption<WsIoClientConnection>` ([e1576a2](https://github.com/ws-io/ws.io-rs/commit/e1576a2))
- change all `status` fields to use `AtomicU8` and add operation lock for major actions like connect/disconnect ([5321b97](https://github.com/ws-io/ws.io-rs/commit/5321b97))
- change return type of some `struct::new` functions to `Arc<Self>` ([a7ce497](https://github.com/ws-io/ws.io-rs/commit/a7ce497))
- rename `xxxConnectionStatus` to `ConnectionStatus` ([3863d68](https://github.com/ws-io/ws.io-rs/commit/3863d68))
- update some format usage ([efdba68](https://github.com/ws-io/ws.io-rs/commit/efdba68))
- simplify status checking and transitions within `connection.close` ([d5c478e](https://github.com/ws-io/ws.io-rs/commit/d5c478e))
- replace `match` statements for extracting and converting `Option` values with chained `map` and `transpose` calls ([cf7f9b3](https://github.com/ws-io/ws.io-rs/commit/cf7f9b3))

### 🚀 Enhancements

- *(client)* add clone derive to `WsIoClient` ([53c3476](https://github.com/ws-io/ws.io-rs/commit/53c3476))
- allow configuration of WebSocket settings such as `max_frame_size` ([0b2b491](https://github.com/ws-io/ws.io-rs/commit/0b2b491))
- *(client)* handle disconnect packet ([4da8353](https://github.com/ws-io/ws.io-rs/commit/4da8353))

### 🩹 Fixes

- *(client)* ensure `disconnect` immediately breaks `run_connection_loop` even if it's sleeping ([0f4a780](https://github.com/ws-io/ws.io-rs/commit/0f4a780))
- *(client)* normalize multiple consecutive slashes in URL namespace to a single slash ([0322671](https://github.com/ws-io/ws.io-rs/commit/0322671))
- *(client)* resolve issue where leading `/` in connection URL path caused connection failure ([fa5ca8c](https://github.com/ws-io/ws.io-rs/commit/fa5ca8c))

## [0.2.1](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.2.0...wsio-client-v0.2.1) - 2025-10-20 17:48

### 🏀 Examples

- add client and server examples ([88a2fce](https://github.com/ws-io/ws.io-rs/commit/88a2fce))

### 💅 Refactors

- update `handle_incoming_packet` to require successful decoding before processing; return error to upper layer and exit `read_ws_stream_task` on failure ([76bf3dd](https://github.com/ws-io/ws.io-rs/commit/76bf3dd))
- tidy up code ([4e5a362](https://github.com/ws-io/ws.io-rs/commit/4e5a362))
- change `Connection` message `tx/rx` from `unbounded_channel` to bounded `channel` ([4e6a130](https://github.com/ws-io/ws.io-rs/commit/4e6a130))
- *(client)* rename `namespace_url` to `url` ([97e7675](https://github.com/ws-io/ws.io-rs/commit/97e7675))
- *(client)* move `connection.init` call in `run_connection` to occur before spawning read/write tasks ([0fcf536](https://github.com/ws-io/ws.io-rs/commit/0fcf536))
- *(client)* rename `WsIoClientBuilder.on_ready` to `on_connection_ready` ([ed0c7ca](https://github.com/ws-io/ws.io-rs/commit/ed0c7ca))

### 🚀 Enhancements

- add cbor packet codec ([f3e1fa9](https://github.com/ws-io/ws.io-rs/commit/f3e1fa9))
- *(client)* allow custom configuration of `init_timeout`, `ready_timeout`, and `reconnection_delay` ([161e055](https://github.com/ws-io/ws.io-rs/commit/161e055))
- *(client)* add `WsIoClientBuilder.on_connection_close` method and invoke it inside `connection.cleanup` ([7f8fb23](https://github.com/ws-io/ws.io-rs/commit/7f8fb23))
- *(client)* add `on_ready` method to builder and invoke configured handler after connection transitions to `ready` state ([167d618](https://github.com/ws-io/ws.io-rs/commit/167d618))

## [0.2.0](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.1.1...wsio-client-v0.2.0) - 2025-10-20 05:35

### 💅 Refactors

- *(client)* rename `WsIoClientConfig.auth` to `auth_handler` ([0a73a04](https://github.com/ws-io/ws.io-rs/commit/0a73a04))

### 🚀 Enhancements

- *(client)* implement connection establishment with init/ready packet handling and add connection close/cleanup functionality ([28bb1a1](https://github.com/ws-io/ws.io-rs/commit/28bb1a1))

### 🩹 Fixes

- add missing Tokio features ([0fa2c13](https://github.com/ws-io/ws.io-rs/commit/0fa2c13))

## [0.1.1](https://github.com/ws-io/ws.io-rs/compare/wsio-client-v0.1.0...wsio-client-v0.1.1) - 2025-10-19 18:40

### 🚀 Enhancements

- *(client)* add empty `WsIoClientConnection` struct ([78df31b](https://github.com/ws-io/ws.io-rs/commit/78df31b))
- *(client)* add namespace url and auth configs ([6934beb](https://github.com/ws-io/ws.io-rs/commit/6934beb))
- *(client)* add base config, builder and runtime files ([859e39a](https://github.com/ws-io/ws.io-rs/commit/859e39a))

## [0.1.0](https://github.com/ws-io/ws.io-rs/releases/tag/wsio-client-v0.1.0) - 2025-10-19 03:28

### 🏡 Chore

- *(client)* add base files ([a70927d](https://github.com/ws-io/ws.io-rs/commit/a70927d))

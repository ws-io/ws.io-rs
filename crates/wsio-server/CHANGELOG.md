# Changelog

## 0.12.1 - 2026-03-24 07:00

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.12.0...wsio-server-v0.12.1)

### 🏡 Chore

- upgrade deps ([378bf42](https://github.com/ws-io/ws.io-rs/commit/378bf42))

## 0.12.0 - 2026-03-22 19:45

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.11.3...wsio-server-v0.12.0)

### ✅ Tests

- add more unit and e2e ([24b6bac](https://github.com/ws-io/ws.io-rs/commit/24b6bac))
- *(server)* add `ConnectionExtensions` unit ([1d2d8bb](https://github.com/ws-io/ws.io-rs/commit/1d2d8bb))
- add more units ([d973d57](https://github.com/ws-io/ws.io-rs/commit/d973d57))

### 🏡 Chore

- lint code ([7c03d10](https://github.com/ws-io/ws.io-rs/commit/7c03d10))

### 💅 Refactors

- *(server)* [**breaking**] remove name check from `new_namespace_builder` and return `WsIoServerNamespaceBuilder` direct ([9600364](https://github.com/ws-io/ws.io-rs/commit/9600364))

### 🚀 Enhancements

- *(server)* add `http_request_upgrade_timeout` config to server and namespace to resolve `handle_on_upgrade_request` may wait forever issue ([e9a0078](https://github.com/ws-io/ws.io-rs/commit/e9a0078))

### 🩹 Fixes

- *(server)* resolved `WsIoServerNamespace.remove_connection_id_from_room` method race issue ([46db631](https://github.com/ws-io/ws.io-rs/commit/46db631))

## 0.11.3 - 2026-03-02 03:01

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.11.2...wsio-server-v0.11.3)

### 🏡 Chore

- updated the following local packages: wsio-core ([0000000](https://github.com/ws-io/ws.io-rs/commit/0000000))

## 0.11.2 - 2026-02-26 05:59

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.11.1...wsio-server-v0.11.2)

### 🏡 Chore

- *(server)* remove `roaring/simd` feature ([5c794a7](https://github.com/ws-io/ws.io-rs/commit/5c794a7))

## 0.11.1 - 2026-01-27 06:16

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.11.0...wsio-server-v0.11.1)

### 💅 Refactors

- optimize server room system ([e09aee0](https://github.com/ws-io/ws.io-rs/commit/e09aee0))

### 🔥 Performance

- change `WsIoServerRuntime.connection_ids` to use `RoaringTreemap` ([90f4829](https://github.com/ws-io/ws.io-rs/commit/90f4829))

## 0.11.0 - 2026-01-13 02:14

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.10.2...wsio-server-v0.11.0)

### 🏡 Chore

- update deps ([86c5861](https://github.com/ws-io/ws.io-rs/commit/86c5861))
- update deps ([902b742](https://github.com/ws-io/ws.io-rs/commit/902b742))

### 💅 Refactors

- [**breaking**] completely remove bincode-related packet codec support due to bincode being permanently unmaintained ([9bd8cc0](https://github.com/ws-io/ws.io-rs/commit/9bd8cc0))

### 🩹 Fixes

- lint code ([47f5268](https://github.com/ws-io/ws.io-rs/commit/47f5268))

## 0.10.2 - 2025-11-29 07:30

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.10.1...wsio-server-v0.10.2)

### 🏡 Chore

- update deps ([e13c875](https://github.com/ws-io/ws.io-rs/commit/e13c875))

## 0.10.1 - 2025-11-26 06:01

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.10.0...wsio-server-v0.10.1)

### 🏡 Chore

- *(server)* upgrade deps ([2c85994](https://github.com/ws-io/ws.io-rs/commit/2c85994))

## 0.10.0 - 2025-11-23 13:22

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.9.9...wsio-server-v0.10.0)

### 💅 Refactors

- [**breaking**] make `close`, `disconnect`, and `emit` methods on `WsIoServerNamespaceBroadcastOperator` consume `self`, and add `Clone` derive ([4e50742](https://github.com/ws-io/ws.io-rs/commit/4e50742))

## 0.9.9 - 2025-11-23 11:04

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.9.8...wsio-server-v0.9.9)

### 🩹 Fixes

- change some iterator impls from `AsRef<str>` to `Into<String>` to fix certain errors ([33a0675](https://github.com/ws-io/ws.io-rs/commit/33a0675))

## 0.9.8 - 2025-11-21 01:56

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.9.7...wsio-server-v0.9.8)

### 💅 Refactors

- extract some internal types and utilities into a separate crate ([3b02fd0](https://github.com/ws-io/ws.io-rs/commit/3b02fd0))

## 0.9.7 - 2025-11-17 01:44

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.9.6...wsio-server-v0.9.7)

### 🏡 Chore

- unify and organize all default timeout durations ([277ea3b](https://github.com/ws-io/ws.io-rs/commit/277ea3b))
- *(server)* update deps ([741b869](https://github.com/ws-io/ws.io-rs/commit/741b869))

## 0.9.6 - 2025-11-13 09:32

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.9.5...wsio-server-v0.9.6)

### 🚀 Enhancements

- *(server)* treat 1-byte binary data from client as heartbeat and ignore processing ([0084288](https://github.com/ws-io/ws.io-rs/commit/0084288))

## 0.9.5 - 2025-11-13 04:25

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.9.4...wsio-server-v0.9.5)

### 💅 Refactors

- rename `AtomicStatus` to `AtomicEnum`, `SessionStatus` to `SessionState`, `ConnectionStatus` to `ConnectionState` ([15d6f96](https://github.com/ws-io/ws.io-rs/commit/15d6f96))
- adjust timing of status check in handle event packet ([034c3e8](https://github.com/ws-io/ws.io-rs/commit/034c3e8))

### 🚀 Enhancements

- *(server)* directly close connections during shutdown and add `close_all`/`disconnect_all` methods ([1dd4868](https://github.com/ws-io/ws.io-rs/commit/1dd4868))

## 0.9.4 - 2025-11-12 10:03

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.9.3...wsio-server-v0.9.4)

### 🚀 Enhancements

- *(server)* make `WsIoServerLayer` is public ([1a484cf](https://github.com/ws-io/ws.io-rs/commit/1a484cf))

## 0.9.3 - 2025-11-12 09:15

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.9.2...wsio-server-v0.9.3)

### 🏡 Chore

- updated the following local packages: wsio-core ([0000000](https://github.com/ws-io/ws.io-rs/commit/0000000))

## 0.9.2 - 2025-11-12 01:24

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.9.1...wsio-server-v0.9.2)

### 🏡 Chore

- update some comments ([23a2919](https://github.com/ws-io/ws.io-rs/commit/23a2919))

### 🩹 Fixes

- ignore event packets received when not in ready state ([a651d33](https://github.com/ws-io/ws.io-rs/commit/a651d33))

## [0.9.1](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.9.0...wsio-server-v0.9.1) - 2025-11-08 15:17

### 🏡 Chore

- update some comments ([c8e62ac](https://github.com/ws-io/ws.io-rs/commit/c8e62ac))

## [0.9.0](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.8.1...wsio-server-v0.9.0) - 2025-11-06 09:35

### 🏡 Chore

- update dependencies ([c99dd2e](https://github.com/ws-io/ws.io-rs/commit/c99dd2e))

### 💅 Refactors

- rename some params name ([2bd3db2](https://github.com/ws-io/ws.io-rs/commit/2bd3db2))
- *(server)* update generic parameter definition for `IntoIterator` ([c5545ec](https://github.com/ws-io/ws.io-rs/commit/c5545ec))

## [0.8.1](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.8.0...wsio-server-v0.8.1) - 2025-11-02 10:21

### 🏡 Chore

- update package metadata ([9d8c0de](https://github.com/ws-io/ws.io-rs/commit/9d8c0de))

### 💅 Refactors

- replace all `.to_string` calls with `.into` ([91b178b](https://github.com/ws-io/ws.io-rs/commit/91b178b))

### 🚀 Enhancements

- *(server)* add `.to` and `.except` methods to connection (similar to socket.io's `socket.to/except`) ([a23054e](https://github.com/ws-io/ws.io-rs/commit/a23054e))

## [0.8.0](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.7.0...wsio-server-v0.8.0) - 2025-10-30 12:55

### 🎨 Styles

- update string formatting style in some parts of the code ([1195fd0](https://github.com/ws-io/ws.io-rs/commit/1195fd0))

### 📖 Documentation

- update README ([d5ccbd1](https://github.com/ws-io/ws.io-rs/commit/d5ccbd1))

### 🚀 Enhancements

- *(server)* add `request_uri` getter to `WsIoServerConnection` ([6ddfec2](https://github.com/ws-io/ws.io-rs/commit/6ddfec2))
- [**breaking**] update custom protocol handshake behavior, all auth-related behavior and replace with init-based flow ([a85248b](https://github.com/ws-io/ws.io-rs/commit/a85248b))

## [0.7.0](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.6.0...wsio-server-v0.7.0) - 2025-10-29 06:31

### 🏡 Chore

- update packet meta ([8ada5df](https://github.com/ws-io/ws.io-rs/commit/8ada5df))

### 💅 Refactors

- *(server)* use reference instead of converting to `String` when retrieving namespace path in `dispatch_request` ([60e9860](https://github.com/ws-io/ws.io-rs/commit/60e9860))
- replace all maps and sets with versions using `rustc_hash::FxBuildHasher` ([14cd911](https://github.com/ws-io/ws.io-rs/commit/14cd911))
- *(server)* clean up code ([eb5d465](https://github.com/ws-io/ws.io-rs/commit/eb5d465))
- replace all `impl Into<String>` with `impl AsRef<str>` and update internal string-related function parameters to `&str` ([7452d7b](https://github.com/ws-io/ws.io-rs/commit/7452d7b))

### 🚀 Enhancements

- implement server namespace broadcast functionality, refactor and clean up code ([7619362](https://github.com/ws-io/ws.io-rs/commit/7619362))
- *(server)* add initial room join/leave functionality and auto-leave all rooms on disconnect ([a6f3acf](https://github.com/ws-io/ws.io-rs/commit/a6f3acf))

### 🩹 Fixes

- avoid potential deadlocks by collecting map values into `Vec` before iterating and executing operations ([4913a78](https://github.com/ws-io/ws.io-rs/commit/4913a78))

## [0.6.0](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.5.1...wsio-server-v0.6.0) - 2025-10-28

### Added

- implement spawn management in `WsIoEventRegistry`, update and clean up code
- *(server)* public namespace

### Other

- *(server)* [**breaking**] update tower-related service architecture
- clean up, modify, and optimize code

## [0.5.1](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.5.0...wsio-server-v0.5.1) - 2025-10-27 08:04

### 💅 Refactors

- further simplify and merge parts of code ([bed226c](https://github.com/ws-io/ws.io-rs/commit/bed226c))
- simplify and modify parts of the code ([4923c46](https://github.com/ws-io/ws.io-rs/commit/4923c46))
- clean up and optimize code ([0282065](https://github.com/ws-io/ws.io-rs/commit/0282065))

### 🚀 Enhancements

- add postcard packet codec ([1f1297f](https://github.com/ws-io/ws.io-rs/commit/1f1297f))
- *(server)* add `emit` method to server ([6439a60](https://github.com/ws-io/ws.io-rs/commit/6439a60))
- *(server)* add namespace emit method ([8343ee8](https://github.com/ws-io/ws.io-rs/commit/8343ee8))
- clean up and optimize code, implement initial event handling after receiving event ([8c6e461](https://github.com/ws-io/ws.io-rs/commit/8c6e461))
- add event registration functionality ([2dfcb1d](https://github.com/ws-io/ws.io-rs/commit/2dfcb1d))

## [0.5.0](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.4.0...wsio-server-v0.5.0) - 2025-10-25 07:21

### 🏡 Chore

- remove unused or unnecessary TODO comments ([ddb0c19](https://github.com/ws-io/ws.io-rs/commit/ddb0c19))

### 💅 Refactors

- *(server)* change `emit` event parameter type to `impl Into<String>` ([b190c9e](https://github.com/ws-io/ws.io-rs/commit/b190c9e))
- [**breaking**] update auth handler to require sending `data` ([4a273c2](https://github.com/ws-io/ws.io-rs/commit/4a273c2))
- *(server)* store `Weak` instead of `Arc` for connections in runtime ([f678413](https://github.com/ws-io/ws.io-rs/commit/f678413))
- merge/extract parts of code and replace some `Arc` with `Box` ([65a6b50](https://github.com/ws-io/ws.io-rs/commit/65a6b50))
- *(server)* split `WsIoServerNamespace.handle_on_upgrade_request` to reduce complexity and fix incorrect variable naming ([9dbea5e](https://github.com/ws-io/ws.io-rs/commit/9dbea5e))

### 🔥 Performance

- *(server)* optimize `dispatch_request` ([f327974](https://github.com/ws-io/ws.io-rs/commit/f327974))

### 🚀 Enhancements

- *(server)* in `handle_upgraded_request`, check status and send disconnect packet if not running before ending process ([e57702e](https://github.com/ws-io/ws.io-rs/commit/e57702e))
- *(server)* add `remove_namespace` method ([d77896a](https://github.com/ws-io/ws.io-rs/commit/d77896a))
- *(server)* add `shutdown` method to server and namespace ([172808e](https://github.com/ws-io/ws.io-rs/commit/172808e))

### 🩹 Fixes

- resolve issue where calling `.auth` methods on builder captured current codec, causing inconsistent results when codec and auth builder method call order differed ([d4f7bd2](https://github.com/ws-io/ws.io-rs/commit/d4f7bd2))
- *(server)* correct incorrect parameter type ([9c7f2d7](https://github.com/ws-io/ws.io-rs/commit/9c7f2d7))

## [0.4.0](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.3.1...wsio-server-v0.4.0) - 2025-10-23 07:04

### 🏡 Chore

- [**breaking**] adjust default timeout durations ([4e882f7](https://github.com/ws-io/ws.io-rs/commit/4e882f7))
- add some todo ([6bfd3f6](https://github.com/ws-io/ws.io-rs/commit/6bfd3f6))

### 💅 Refactors

- change `message_tx.send` to `try_send` in `connection.close` method ([100e2c6](https://github.com/ws-io/ws.io-rs/commit/100e2c6))
- *(server)* [**breaking**] code cleanup and optimization ([3b1486e](https://github.com/ws-io/ws.io-rs/commit/3b1486e))
- dynamically determine internal channel capacity during connection creation based on WebSocket config ([a7526f6](https://github.com/ws-io/ws.io-rs/commit/a7526f6))
- spawn `on_ready_handler` execution and prevent connection interruption if it panics ([52eddc9](https://github.com/ws-io/ws.io-rs/commit/52eddc9))

### 🩹 Fixes

- *(server)* lint code ([72eac7c](https://github.com/ws-io/ws.io-rs/commit/72eac7c))
- *(server)* ensure `connection.emit` only executes when connection status is `ready` ([98e151e](https://github.com/ws-io/ws.io-rs/commit/98e151e))

## [0.3.1](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.3.0...wsio-server-v0.3.1) - 2025-10-22 05:54

### 🏀 Examples

- move files to `examples` workspace ([667bfe5](https://github.com/ws-io/ws.io-rs/commit/667bfe5))
- add disconnect example ([dacb448](https://github.com/ws-io/ws.io-rs/commit/dacb448))
- rename files ([cf536ad](https://github.com/ws-io/ws.io-rs/commit/cf536ad))

### 🏡 Chore

- disable or replace certain dependency features to reduce overall dependencies ([1d88ae3](https://github.com/ws-io/ws.io-rs/commit/1d88ae3))

### 💅 Refactors

- change all `status` fields to use `AtomicU8` and add operation lock for major actions like connect/disconnect ([5321b97](https://github.com/ws-io/ws.io-rs/commit/5321b97))
- *(server)* tidy up code ([2b7e382](https://github.com/ws-io/ws.io-rs/commit/2b7e382))
- *(server)* move `WsIoServerRuntime.handle_on_upgrade_request` into `WsIoServerNamespace` ([a7d1157](https://github.com/ws-io/ws.io-rs/commit/a7d1157))
- change return type of some `struct::new` functions to `Arc<Self>` ([a7ce497](https://github.com/ws-io/ws.io-rs/commit/a7ce497))
- *(server)* move spawn of request upgrade task in `dispatch_request` to runtime and register task in map for better management ([63bdb1a](https://github.com/ws-io/ws.io-rs/commit/63bdb1a))
- rename `xxxConnectionStatus` to `ConnectionStatus` ([3863d68](https://github.com/ws-io/ws.io-rs/commit/3863d68))
- update some format usage ([efdba68](https://github.com/ws-io/ws.io-rs/commit/efdba68))
- simplify status checking and transitions within `connection.close` ([d5c478e](https://github.com/ws-io/ws.io-rs/commit/d5c478e))
- replace `match` statements for extracting and converting `Option` values with chained `map` and `transpose` calls ([cf7f9b3](https://github.com/ws-io/ws.io-rs/commit/cf7f9b3))

### 🚀 Enhancements

- *(server)* add `WsIoServerConnectionExtensions` ([3dca472](https://github.com/ws-io/ws.io-rs/commit/3dca472))
- allow configuration of WebSocket settings such as `max_frame_size` ([0b2b491](https://github.com/ws-io/ws.io-rs/commit/0b2b491))
- *(server)* add `WsIoServerConnection.emit` method ([f6ff682](https://github.com/ws-io/ws.io-rs/commit/f6ff682))
- *(server)* add `WsIoServerConnection.spawn_task` method ([8a25fcf](https://github.com/ws-io/ws.io-rs/commit/8a25fcf))

## [0.3.0](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.2.1...wsio-server-v0.3.0) - 2025-10-20 17:48

### 🏀 Examples

- add client and server examples ([88a2fce](https://github.com/ws-io/ws.io-rs/commit/88a2fce))
- *(server)* add examples file ([ff1444b](https://github.com/ws-io/ws.io-rs/commit/ff1444b))

### 🏡 Chore

- *(server)* reduce default `auth_timeout` duration ([3bf00cd](https://github.com/ws-io/ws.io-rs/commit/3bf00cd))

### 💅 Refactors

- update `handle_incoming_packet` to require successful decoding before processing; return error to upper layer and exit `read_ws_stream_task` on failure ([76bf3dd](https://github.com/ws-io/ws.io-rs/commit/76bf3dd))
- *(server)* tidy up code ([90f94c5](https://github.com/ws-io/ws.io-rs/commit/90f94c5))
- tidy up code ([4e5a362](https://github.com/ws-io/ws.io-rs/commit/4e5a362))
- change `Connection` message `tx/rx` from `unbounded_channel` to bounded `channel` ([4e6a130](https://github.com/ws-io/ws.io-rs/commit/4e6a130))
- *(server)* remove `WsIoServerConnection.on` method and related code ([dfe85d8](https://github.com/ws-io/ws.io-rs/commit/dfe85d8))
- *(server)* remove unnecessary state checks ([153a9f4](https://github.com/ws-io/ws.io-rs/commit/153a9f4))

### 🚀 Enhancements

- add cbor packet codec ([f3e1fa9](https://github.com/ws-io/ws.io-rs/commit/f3e1fa9))
- *(server)* add `cancel_token` getter to `WsIoServerConnection` and invoke `cancel_token.cancel` in `cleanup` ([3b1076d](https://github.com/ws-io/ws.io-rs/commit/3b1076d))
- *(server)* add `namespace_count` method ([51d4867](https://github.com/ws-io/ws.io-rs/commit/51d4867))

### 🩹 Fixes

- *(server)* resolve status deadlock issue in `connection.handle_auth_packet` ([a36b54b](https://github.com/ws-io/ws.io-rs/commit/a36b54b))

## [0.2.1](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.2.0...wsio-server-v0.2.1) - 2025-10-20 05:35

### 💅 Refactors

- *(server)* refine error messages and prevent conflicting state transitions in `conn.handle_auth_packet` ([12feb87](https://github.com/ws-io/ws.io-rs/commit/12feb87))
- *(server)* extract `auth` match block in `connection.handle_incoming_packet` into a separate method and modify packet type handling to close connection on handler error ([f46f73c](https://github.com/ws-io/ws.io-rs/commit/f46f73c))
- *(server)* rename `cleanup_connection` to `remove_connection` ([8ebde37](https://github.com/ws-io/ws.io-rs/commit/8ebde37))
- *(server)* move all type handlers into their respective individual files instead of defining them in `types/file` ([9d3e9d5](https://github.com/ws-io/ws.io-rs/commit/9d3e9d5))

### 🚀 Enhancements

- *(server)* add `connection.off` method ([9a2ceac](https://github.com/ws-io/ws.io-rs/commit/9a2ceac))

### 🩹 Fixes

- add missing Tokio features ([0fa2c13](https://github.com/ws-io/ws.io-rs/commit/0fa2c13))

## [0.2.0](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.1.3...wsio-server-v0.2.0) - 2025-10-19 18:40

### 🏡 Chore

- *(server)* mark some functions is inline ([b238875](https://github.com/ws-io/ws.io-rs/commit/b238875))
- *(server)* tidy up code ([4f7d5e5](https://github.com/ws-io/ws.io-rs/commit/4f7d5e5))

### 💅 Refactors

- major code overhaul ([09c6773](https://github.com/ws-io/ws.io-rs/commit/09c6773))
- *(server)* use `= Some(...)` instead of `.replace(...)` when setting optional configuration values ([2feb398](https://github.com/ws-io/ws.io-rs/commit/2feb398))
- remove functionality that sends codec type data after connection establishment ([f8190ff](https://github.com/ws-io/ws.io-rs/commit/f8190ff))

## [0.1.3](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.1.2...wsio-server-v0.1.3) - 2025-10-19 03:28

### 💅 Refactors

- *(server)* change `connection.on` method event parameter type to `impl AsRef<str>` ([acb8e50](https://github.com/ws-io/ws.io-rs/commit/acb8e50))

## [0.1.2](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.1.1...wsio-server-v0.1.2) - 2025-10-19 00:35

### 🏡 Chore

- lint code ([945b186](https://github.com/ws-io/ws.io-rs/commit/945b186))

### 💅 Refactors

- *(server)* update `namespace builder.with_auth` to change handler `data` parameter to `Option<&D>` ([61179f7](https://github.com/ws-io/ws.io-rs/commit/61179f7))

### 🚀 Enhancements

- *(server)* add namespace middleware functionality ([4893bbc](https://github.com/ws-io/ws.io-rs/commit/4893bbc))
- *(server)* add connection.server method ([44d4c46](https://github.com/ws-io/ws.io-rs/commit/44d4c46))
- add `connection.on` method to register event handlers ([3e352f6](https://github.com/ws-io/ws.io-rs/commit/3e352f6))

## [0.1.1](https://github.com/ws-io/ws.io-rs/compare/wsio-server-v0.1.0...wsio-server-v0.1.1) - 2025-10-18 14:57

### 💅 Refactors

- *(server)* lint code ([b97bdda](https://github.com/ws-io/ws.io-rs/commit/b97bdda))

### 🚀 Enhancements

- *(server)* implement connection handling for `on_message` and `auth` packet reception ([3aefaab](https://github.com/ws-io/ws.io-rs/commit/3aefaab))

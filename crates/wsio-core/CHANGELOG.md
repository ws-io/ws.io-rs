# Changelog

## 0.11.0 - 2025-11-21 01:56

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-core-v0.10.0...wsio-core-v0.11.0)

### 💅 Refactors

- extract some internal types and utilities into a separate crate ([3b02fd0](https://github.com/ws-io/ws.io-rs/commit/3b02fd0))

## 0.10.0 - 2025-11-13 04:25

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-core-v0.9.0...wsio-core-v0.10.0)

### 💅 Refactors

- rename `AtomicStatus` to `AtomicEnum`, `SessionStatus` to `SessionState`, `ConnectionStatus` to `ConnectionState` ([15d6f96](https://github.com/ws-io/ws.io-rs/commit/15d6f96))

## 0.9.0 - 2025-11-12 09:15

[compare changes](https://github.com/ws-io/ws.io-rs/compare/wsio-core-v0.8.0...wsio-core-v0.9.0)

### 💅 Refactors

- rename `MsgPack` to `Msgpack` and use `to_vec_named` for internal encoding ([2dc3615](https://github.com/ws-io/ws.io-rs/commit/2dc3615))

## [0.8.0](https://github.com/ws-io/ws.io-rs/compare/wsio-core-v0.7.1...wsio-core-v0.8.0) - 2025-11-06 09:35

### 🏡 Chore

- update dependencies ([c99dd2e](https://github.com/ws-io/ws.io-rs/commit/c99dd2e))

### 💅 Refactors

- *(core)* [**breaking**] unify actual packet transmission data structure as tuple regardless of codec used ([38e5948](https://github.com/ws-io/ws.io-rs/commit/38e5948))

## [0.7.1](https://github.com/ws-io/ws.io-rs/compare/wsio-core-v0.7.0...wsio-core-v0.7.1) - 2025-11-02 10:21

### 🏡 Chore

- update package metadata ([9d8c0de](https://github.com/ws-io/ws.io-rs/commit/9d8c0de))

### 💅 Refactors

- replace all `.to_string` calls with `.into` ([91b178b](https://github.com/ws-io/ws.io-rs/commit/91b178b))

## [0.7.0](https://github.com/ws-io/ws.io-rs/compare/wsio-core-v0.6.1...wsio-core-v0.7.0) - 2025-10-30 12:55

### 🎨 Styles

- update string formatting style in some parts of the code ([1195fd0](https://github.com/ws-io/ws.io-rs/commit/1195fd0))

### 🚀 Enhancements

- [**breaking**] update custom protocol handshake behavior, all auth-related behavior and replace with init-based flow ([a85248b](https://github.com/ws-io/ws.io-rs/commit/a85248b))

## [0.6.1](https://github.com/ws-io/ws.io-rs/compare/wsio-core-v0.6.0...wsio-core-v0.6.1) - 2025-10-29 06:31

### 🏡 Chore

- update packet meta ([8ada5df](https://github.com/ws-io/ws.io-rs/commit/8ada5df))

### 💅 Refactors

- *(core)* update all codec `encode` methods to support packet references, separating handling of `InnerPacketRef` and `InnerPacket` ([a870635](https://github.com/ws-io/ws.io-rs/commit/a870635))
- replace all maps and sets with versions using `rustc_hash::FxBuildHasher` ([14cd911](https://github.com/ws-io/ws.io-rs/commit/14cd911))
- replace all `impl Into<String>` with `impl AsRef<str>` and update internal string-related function parameters to `&str` ([7452d7b](https://github.com/ws-io/ws.io-rs/commit/7452d7b))

### 🚀 Enhancements

- implement server namespace broadcast functionality, refactor and clean up code ([7619362](https://github.com/ws-io/ws.io-rs/commit/7619362))

### 🩹 Fixes

- *(core)* resolve warning caused by `InnerPacket` not being gated by feature flag ([2a412e9](https://github.com/ws-io/ws.io-rs/commit/2a412e9))
- avoid potential deadlocks by collecting map values into `Vec` before iterating and executing operations ([4913a78](https://github.com/ws-io/ws.io-rs/commit/4913a78))

## [0.6.0](https://github.com/ws-io/ws.io-rs/compare/wsio-core-v0.5.0...wsio-core-v0.6.0) - 2025-10-28

### Added

- implement spawn management in `WsIoEventRegistry`, update and clean up code

### Fixed

- *(core)* enable tokio macros feature

### Other

- *(core)* lint code
- clean up, modify, and optimize code

## [0.5.0](https://github.com/ws-io/ws.io-rs/compare/wsio-core-v0.4.0...wsio-core-v0.5.0) - 2025-10-27 08:04

### 🏡 Chore

- *(core)* remove unused or unimplemented TODOs ([a2435d4](https://github.com/ws-io/ws.io-rs/commit/a2435d4))
- *(core)* tidy up dependencies ([da06bcb](https://github.com/ws-io/ws.io-rs/commit/da06bcb))
- mark `WsIoPacketCodec.is_text` method is inline ([d43ff08](https://github.com/ws-io/ws.io-rs/commit/d43ff08))

### 💅 Refactors

- further simplify and merge parts of code ([bed226c](https://github.com/ws-io/ws.io-rs/commit/bed226c))
- simplify and modify parts of the code ([4923c46](https://github.com/ws-io/ws.io-rs/commit/4923c46))
- clean up and optimize code ([0282065](https://github.com/ws-io/ws.io-rs/commit/0282065))
- *(core)* remove unused generic type definitions ([73387d7](https://github.com/ws-io/ws.io-rs/commit/73387d7))

### 🚀 Enhancements

- add postcard packet codec ([1f1297f](https://github.com/ws-io/ws.io-rs/commit/1f1297f))
- clean up and optimize code, implement initial event handling after receiving event ([8c6e461](https://github.com/ws-io/ws.io-rs/commit/8c6e461))
- add event registration functionality ([2dfcb1d](https://github.com/ws-io/ws.io-rs/commit/2dfcb1d))

## [0.4.0](https://github.com/ws-io/ws.io-rs/compare/wsio-core-v0.3.2...wsio-core-v0.4.0) - 2025-10-25 07:21

### 💅 Refactors

- [**breaking**] update auth handler to require sending `data` ([4a273c2](https://github.com/ws-io/ws.io-rs/commit/4a273c2))
- merge/extract parts of code and replace some `Arc` with `Box` ([65a6b50](https://github.com/ws-io/ws.io-rs/commit/65a6b50))

## [0.3.2](https://github.com/ws-io/ws.io-rs/compare/wsio-core-v0.3.1...wsio-core-v0.3.2) - 2025-10-23 07:04

### 🏡 Chore

- *(core)* remove some todo comments ([b2dcc04](https://github.com/ws-io/ws.io-rs/commit/b2dcc04))

### 🚀 Enhancements

- *(core)* add `abort_locked_task` utils ([e712702](https://github.com/ws-io/ws.io-rs/commit/e712702))
- *(core)* add `is` method to `AtomicStatus` ([ac954e1](https://github.com/ws-io/ws.io-rs/commit/ac954e1))

## [0.3.1](https://github.com/ws-io/ws.io-rs/compare/wsio-core-v0.3.0...wsio-core-v0.3.1) - 2025-10-22 05:54

### 🎨 Styles

- *(core)* format code ([af9329d](https://github.com/ws-io/ws.io-rs/commit/af9329d))

### 💅 Refactors

- change all `status` fields to use `AtomicU8` and add operation lock for major actions like connect/disconnect ([5321b97](https://github.com/ws-io/ws.io-rs/commit/5321b97))

## [0.3.0](https://github.com/ws-io/ws.io-rs/compare/wsio-core-v0.2.0...wsio-core-v0.3.0) - 2025-10-20 17:48

### 💅 Refactors

- *(core)* serialize and deserialize `WsIoPacketType` as numeric values instead of stringified numbers ([0112ebb](https://github.com/ws-io/ws.io-rs/commit/0112ebb))

### 🚀 Enhancements

- add cbor packet codec ([f3e1fa9](https://github.com/ws-io/ws.io-rs/commit/f3e1fa9))

## [0.2.0](https://github.com/ws-io/ws.io-rs/compare/wsio-core-v0.1.1...wsio-core-v0.2.0) - 2025-10-19 18:40

### 💅 Refactors

- major code overhaul ([09c6773](https://github.com/ws-io/ws.io-rs/commit/09c6773))
- remove functionality that sends codec type data after connection establishment ([f8190ff](https://github.com/ws-io/ws.io-rs/commit/f8190ff))

## [0.1.1](https://github.com/ws-io/ws.io-rs/compare/wsio-core-v0.1.0...wsio-core-v0.1.1) - 2025-10-19 00:35

### 🏡 Chore

- lint code ([945b186](https://github.com/ws-io/ws.io-rs/commit/945b186))

### 🚀 Enhancements

- add `connection.on` method to register event handlers ([3e352f6](https://github.com/ws-io/ws.io-rs/commit/3e352f6))

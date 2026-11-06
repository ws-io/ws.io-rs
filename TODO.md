# TODO

- [ ] Make `WsIoServerConnection::close()` public (currently `pub(super)` in `crates/wsio-server/src/connection/mod.rs`).
  - Servers cannot drop a single connection without sending a protocol Disconnect, which makes clients stop reconnecting for good.
  - Needed for cases such as rebalancing connections during horizontal scaling; implement when a consumer needs it.

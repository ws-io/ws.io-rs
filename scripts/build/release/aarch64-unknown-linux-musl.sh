#!/usr/bin/env bash

set -euo pipefail

if [[ -d "${HOME}/.cargo/bin" ]]; then
    export PATH="${HOME}/.cargo/bin:${PATH}"
fi

if ! command -v zig >/dev/null 2>&1; then
    echo "missing zig; install Zig with your package manager or setup-zig in CI" >&2
    exit 1
fi

if ! command -v cargo-zigbuild >/dev/null 2>&1; then
    echo "missing cargo-zigbuild; install it with: cargo install cargo-zigbuild" >&2
    exit 1
fi

sep=$'\x1f'
flags=(
    # Optional CPU tuning for deployment fleets with a known ARMv8-A baseline.
    # Keep disabled for generic release binaries because it can emit instructions
    # that are unavailable on older or lower-end aarch64 Linux machines.
    # -C target-cpu=cortex-a72
    # -C target-cpu=neoverse-n1

    # Optional ARMv8 extensions. Keep disabled for generic release binaries; use
    # only when all target machines are known to support the selected feature.
    # -C target-feature=+crc
    # -C target-feature=+crypto
    # -C target-feature=+lse

    # Optional fully static CRT. Usually redundant for this musl target because
    # aarch64-unknown-linux-musl defaults to static CRT, but kept here as a clear
    # opt-in marker for projects that want to be explicit.
    # -C target-feature=+crt-static

    # Optional size/link optimization for ELF linkers that support identical code
    # folding. Keep disabled by default because --icf=all can merge functions with
    # identical machine code and therefore change function pointer identity.
    # -C link-arg=-Wl,--icf=all
)

if ((${#flags[@]} == 0)); then
    exec cargo zigbuild -r --target aarch64-unknown-linux-musl "$@"
fi

encoded=""
for flag in "${flags[@]}"; do
    if [[ -n "${encoded}" ]]; then
        encoded+="$sep"
    fi

    encoded+="$flag"
done

exec env CARGO_ENCODED_RUSTFLAGS="${encoded}" \
    cargo zigbuild -r --target aarch64-unknown-linux-musl "$@"

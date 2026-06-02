#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/libs/common.sh
. "${SCRIPT_DIR}/../../libs/common.sh"

prepend_cargo_bin_to_path
ensure_cargo_target x86_64-unknown-linux-musl

if ! command_exists x86_64-linux-musl-gcc; then
    if command_exists musl-gcc; then
        export CC_x86_64_unknown_linux_musl="${CC_x86_64_unknown_linux_musl:-musl-gcc}"
    else
        log_error "missing x86_64-linux-musl-gcc/musl-gcc; install musl-tools with your package manager"
        exit 1
    fi
fi

# shellcheck disable=SC2034 # Used indirectly by exec_with_encoded_rustflags.
rustflags=(
    # Optional CPU baseline tuning for deployment fleets with known x86-64
    # support. Keep disabled for generic release binaries; x86-64-v3, for
    # example, requires AVX/AVX2-class machines and excludes older CPUs.
    # -C target-cpu=x86-64-v2
    # -C target-cpu=x86-64-v3

    # Optional CPU extensions. Keep disabled for generic release binaries; use
    # only when all target machines are known to support the selected feature.
    # -C target-feature=+aes
    # -C target-feature=+avx2
    # -C target-feature=+sse4.2

    # Optional fully static CRT. Usually redundant for this musl target because
    # x86_64-unknown-linux-musl defaults to static CRT, but kept here as a clear
    # opt-in marker for projects that want to be explicit.
    # -C target-feature=+crt-static

    # Optional size/link optimization for ELF linkers that support identical code
    # folding. Keep disabled by default because --icf=all can merge functions with
    # identical machine code and therefore change function pointer identity.
    # -C link-arg=-Wl,--icf=all
)

exec_with_encoded_rustflags rustflags cargo b -r --target x86_64-unknown-linux-musl "$@"

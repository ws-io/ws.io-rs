#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
. "${SCRIPT_DIR}/../../libs/common.sh"

prepend_cargo_bin_to_path
ensure_cargo_target aarch64-unknown-linux-gnu

require_cargo_zigbuild

rustflags=(
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

    # Optional size/link optimization for ELF linkers that support identical code
    # folding. Keep disabled by default because --icf=all can merge functions with
    # identical machine code and therefore change function pointer identity.
    # -C link-arg=-Wl,--icf=all
)

exec_with_encoded_rustflags cargo zigbuild -r --target aarch64-unknown-linux-gnu "$@"

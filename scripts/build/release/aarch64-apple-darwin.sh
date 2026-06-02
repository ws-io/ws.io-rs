#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/libs/common.sh
. "${SCRIPT_DIR}/../../libs/common.sh"

prepend_cargo_bin_to_path
ensure_cargo_target aarch64-apple-darwin

# shellcheck disable=SC2034 # Used indirectly by exec_with_encoded_rustflags.
rustflags=(
    # Optional CPU tuning for deployment fleets with a known Apple Silicon
    # baseline. Keep disabled for generic release binaries because it can emit
    # instructions that are unavailable on older Apple Silicon machines.
    # -C target-cpu=apple-m1
    # -C target-cpu=apple-m2

    # Optional ARMv8 extensions. Keep disabled for generic release binaries; use
    # only when all target machines are known to support the selected feature.
    # -C target-feature=+crc
    # -C target-feature=+crypto
    # -C target-feature=+lse
)

exec_with_encoded_rustflags rustflags cargo b -r --target aarch64-apple-darwin "$@"

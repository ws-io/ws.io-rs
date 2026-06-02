#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/libs/common.sh
. "${SCRIPT_DIR}/../../libs/common.sh"

prepend_cargo_bin_to_path
ensure_cargo_target x86_64-apple-darwin

# shellcheck disable=SC2034 # Used indirectly by exec_with_encoded_rustflags.
rustflags=(
    # Optional CPU baseline tuning for deployment fleets with known x86-64
    # support. Keep disabled for generic release binaries; x86-64-v3, for
    # example, requires AVX/AVX2-class machines and excludes older Intel Macs.
    # -C target-cpu=x86-64-v2
    # -C target-cpu=x86-64-v3

    # Optional CPU extensions. Keep disabled for generic release binaries; use
    # only when all target machines are known to support the selected feature.
    # -C target-feature=+aes
    # -C target-feature=+avx2
    # -C target-feature=+sse4.2
)

exec_with_encoded_rustflags rustflags cargo b -r --target x86_64-apple-darwin "$@"

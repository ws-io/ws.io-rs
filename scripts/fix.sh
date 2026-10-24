#!/usr/bin/env bash

# shellcheck disable=SC1091

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
source "${SCRIPT_DIR}/libs/common.sh"

# Run
main() {
    prepend_cargo_bin_to_path
    require_cmd cargo
    cd -- "${REPO_ROOT}"

    local sysroot
    sysroot="$(rustc --print sysroot)"
    export LIBRARY_PATH="${sysroot}/lib${LIBRARY_PATH:+:${LIBRARY_PATH}}"

    log_info 'Applying Dylint fixes...'
    cargo dlint-fix --allow-dirty --allow-staged

    log_info 'Applying Clippy fixes...'
    cargo lint-fix --allow-dirty --allow-staged

    log_info 'Applying Mordant fixes...'
    cargo mlint-fix --allow-dirty --allow-staged

    log_info 'Formatting Rust sources...'
    exec cargo format-fix
}

main "$@"

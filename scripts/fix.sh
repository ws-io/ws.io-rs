#!/usr/bin/env bash

# shellcheck disable=SC1091

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
source "${SCRIPT_DIR}/libs/common.sh"

# Functions
parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
        # Keep imports used only by other cfg targets until every target has been fixed.
        -k | --keep-unused-imports) export RUSTFLAGS="${RUSTFLAGS:+${RUSTFLAGS} }-Aunused_imports" ;;
        *)
            log_error "Unknown argument: $1"
            exit 1
            ;;
        esac
        shift
    done
}

# Run
main() {
    parse_args "$@"
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

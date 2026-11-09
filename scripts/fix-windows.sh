#!/usr/bin/env bash

# shellcheck disable=SC1091

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
source "${SCRIPT_DIR}/libs/common.sh"
source "${SCRIPT_DIR}/libs/windows/msvc.sh"

# Functions
parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
        # Keep imports used only by other cfg targets until every target has been fixed.
        -k | --keep-unused-imports) keep_unused_imports=true ;;
        *)
            log_error "Unknown argument: $1"
            exit 1
            ;;
        esac
        shift
    done
}

# Run
# Keep the cross-compilation environment isolated from the calling shell.
main() (
    local keep_unused_imports=false
    parse_args "$@"
    cd -- "${REPO_ROOT}"
    load_windows_msvc_env

    # cargo xwin env resets RUSTFLAGS, so apply this after loading the environment.
    if [[ "${keep_unused_imports}" == true ]]; then
        export RUSTFLAGS="${RUSTFLAGS:+${RUSTFLAGS} }-Aunused_imports"
    fi

    log_info 'Applying Dylint fixes for Windows MSVC...'
    cargo dlint-fix --target x86_64-pc-windows-msvc --allow-dirty --allow-staged

    log_info 'Applying Clippy fixes for Windows MSVC...'
    cargo lint-fix --target x86_64-pc-windows-msvc --allow-dirty --allow-staged

    # Mordant cross-target support has not been verified; keep it out of this flow.
    log_info 'Formatting Rust sources...'
    exec cargo format-fix
)

main "$@"

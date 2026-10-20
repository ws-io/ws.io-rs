#!/usr/bin/env bash

# shellcheck disable=SC1091

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
source "${SCRIPT_DIR}/libs/common.sh"
source "${SCRIPT_DIR}/libs/windows/msvc.sh"

# Run
# Keep the cross-compilation environment isolated from the calling shell.
main() (
    cd -- "${REPO_ROOT}"
    load_windows_msvc_env

    log_info 'Applying Dylint fixes for Windows MSVC...'
    cargo dlint-fix --target x86_64-pc-windows-msvc --allow-dirty --allow-staged

    log_info 'Applying Clippy fixes for Windows MSVC...'
    cargo lint-fix --target x86_64-pc-windows-msvc --allow-dirty --allow-staged

    # Mordant cross-target support has not been verified; keep it out of this flow.
    log_info 'Formatting Rust sources...'
    exec cargo format-fix
)

main "$@"

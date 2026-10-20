#!/usr/bin/env bash

# shellcheck disable=SC1091

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
source "${SCRIPT_DIR}/../libs/common.sh"
source "${SCRIPT_DIR}/../libs/windows/msvc.sh"

# Run
main() {
    local -a apt_command=(apt-get)

    require_windows_msvc_host
    prepend_cargo_bin_to_path
    require_cmd cargo rustup apt-get
    cd -- "${REPO_ROOT}"

    if ((EUID != 0)); then
        require_cmd sudo
        apt_command=(sudo apt-get)
    fi

    log_info 'Installing LLVM/Clang 19 and Ninja packages...'
    "${apt_command[@]}" update
    "${apt_command[@]}" install -y --no-install-recommends clang-19 lld-19 llvm-19 llvm-19-tools ninja-build

    log_info 'Installing Rust Windows MSVC target and llvm-tools component...'
    rustup target add x86_64-pc-windows-msvc
    rustup component add llvm-tools

    log_info 'Installing cargo-xwin...'
    cargo install --locked cargo-xwin

    prepare_windows_msvc
    clang --version
    lld-link --version
    cargo-xwin --version

    log_success 'Windows MSVC xwin toolchain ready; use scripts/tools/cargo-xwin.sh or scripts/fix-windows.sh'
}

main "$@"

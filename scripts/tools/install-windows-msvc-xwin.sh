#!/usr/bin/env bash

# shellcheck disable=SC1091

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
source "${SCRIPT_DIR}/../libs/common.sh"
source "${SCRIPT_DIR}/../libs/windows/msvc.sh"

# Constants/Variables
readonly LLVM_APT_KEY_PATH='/etc/apt/keyrings/apt.llvm.org.asc'
readonly LLVM_APT_KEY_URL='https://apt.llvm.org/llvm-snapshot.gpg.key'
readonly LLVM_APT_SOURCE_PATH="/etc/apt/sources.list.d/apt-llvm-org-${WINDOWS_MSVC_LLVM_VERSION}.sources"

# Functions
# The distribution archive does not ship every LLVM release, so the packages come from apt.llvm.org.
add_llvm_apt_source() {
    local -a run_as_root=("$@")
    local codename

    codename="$(. /etc/os-release && printf '%s' "${VERSION_CODENAME:?}")"
    "${run_as_root[@]}" install -d -m 755 "$(dirname -- "${LLVM_APT_KEY_PATH}")"
    curl -fsSL "${LLVM_APT_KEY_URL}" | "${run_as_root[@]}" tee "${LLVM_APT_KEY_PATH}" >/dev/null
    printf 'Types: deb\nURIs: https://apt.llvm.org/%s/\nSuites: llvm-toolchain-%s-%s\nComponents: main\nSigned-By: %s\n' \
        "${codename}" "${codename}" "${WINDOWS_MSVC_LLVM_VERSION}" "${LLVM_APT_KEY_PATH}" |
        "${run_as_root[@]}" tee "${LLVM_APT_SOURCE_PATH}" >/dev/null
}

# Run
main() {
    local -a run_as_root=(env)
    local version="${WINDOWS_MSVC_LLVM_VERSION}"

    require_windows_msvc_host
    prepend_cargo_bin_to_path
    require_cmd apt-get cargo curl rustup
    cd -- "${REPO_ROOT}"

    if ((EUID != 0)); then
        require_cmd sudo
        run_as_root=(sudo)
    fi

    log_info "Adding the apt.llvm.org LLVM ${version} repository..."
    add_llvm_apt_source "${run_as_root[@]}"

    log_info "Installing LLVM/Clang ${version} and Ninja packages..."
    "${run_as_root[@]}" apt-get update
    "${run_as_root[@]}" apt-get install -y --no-install-recommends \
        "clang-${version}" "lld-${version}" "llvm-${version}" "llvm-${version}-tools" ninja-build

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

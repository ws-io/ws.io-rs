#!/usr/bin/env bash

# shellcheck shell=bash
# shellcheck disable=SC1091

# Source libs/common.sh before this file.

# Functions
require_windows_msvc_host() {
    if [[ "$(host_os)" != 'linux' ]]; then
        log_error 'Windows MSVC cross-compilation tools must run on Linux'
        return 1
    fi
}

prepare_windows_msvc() {
    local llvm_bin="${LLVM_BIN:-/usr/lib/llvm-19/bin}"

    require_windows_msvc_host || return
    prepend_cargo_bin_to_path
    require_cmd cargo cargo-xwin || return

    [[ -x "${llvm_bin}/clang" && -x "${llvm_bin}/lld-link" ]] || {
        log_error "Clang and LLD linker were not found in ${llvm_bin}; run scripts/tools/install-windows-msvc-xwin.sh"
        return 1
    }

    export PATH="${llvm_bin}:${PATH}"
}

load_windows_msvc_env() {
    local xwin_env sysroot

    prepare_windows_msvc || return
    xwin_env="$(cargo xwin env --target x86_64-pc-windows-msvc)" || return
    eval "${xwin_env}"

    sysroot="$(rustc --print sysroot)" || return
    export LIBRARY_PATH="${sysroot}/lib${LIBRARY_PATH:+:${LIBRARY_PATH}}"
}

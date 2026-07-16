#!/usr/bin/env bash
# Rust and Cargo helpers.

# shellcheck shell=bash
# shellcheck disable=SC2034,SC2154,SC2317

if [[ -n "${SCRIPT_LIBS_RUST_LOADED:-}" ]]; then
    return 0 2>/dev/null || true
fi

SCRIPT_LIBS_RUST_LOADED=1

prepend_cargo_bin_to_path() {
    [[ -d "${HOME}/.cargo/bin" ]] || return 0
    case ":${PATH}:" in
    *:"${HOME}/.cargo/bin":*) return 0 ;;
    esac
    export PATH="${HOME}/.cargo/bin:${PATH}"
}

encode_rustflags() {
    local separator=$'\x1f'
    local encoded=''
    local flag

    for flag in "$@"; do
        [[ -z "${encoded}" ]] || encoded+="${separator}"
        encoded+="${flag}"
    done

    printf '%s' "${encoded}"
}

exec_with_encoded_rustflags() {
    local rustflags_name='rustflags'
    local -a rustflags_values=()

    if (($# == 0)); then
        log_error 'Missing command.'
        exit 1
    fi

    if (($# > 1)) && [[ "$1" =~ ^[a-zA-Z_][a-zA-Z0-9_]*$ ]] && declare -p "$1" >/dev/null 2>&1; then
        rustflags_name="$1"
        shift
    fi

    if declare -p "${rustflags_name}" >/dev/null 2>&1; then
        local -n selected_rustflags="${rustflags_name}"
        rustflags_values=("${selected_rustflags[@]}")
    fi

    if ((${#rustflags_values[@]} == 0)); then
        exec "$@"
    fi

    exec env CARGO_ENCODED_RUSTFLAGS="$(encode_rustflags "${rustflags_values[@]}")" "$@"
}

ensure_cargo_target() {
    local target="$1"

    require_cmd rustup grep
    rustup target list --installed | grep -Fxq "${target}" || rustup target add "${target}"
}

require_cargo_zigbuild() {
    require_cmd cargo-zigbuild zig
}

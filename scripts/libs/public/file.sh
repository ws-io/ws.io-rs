#!/usr/bin/env bash
# File and install helpers.

# shellcheck shell=bash
# shellcheck disable=SC2317

if [[ -n "${LINUX_CONFIGS_LIBS_FILE_LOADED:-}" ]]; then
    return 0 2>/dev/null || true
fi

LINUX_CONFIGS_LIBS_FILE_LOADED=1

require_file() {
    local path="$1"

    [[ -f "${path}" ]] || {
        log_error "Required file not found: ${path}"
        return 1
    }
}

require_dir() {
    local path="$1"

    [[ -d "${path}" ]] || {
        log_error "Required directory not found: ${path}"
        return 1
    }
}

install_file() {
    local src="$1"
    local dest="$2"
    local mode="${3:-}"

    require_file "${src}" || return 1
    mkdir -p "$(dirname -- "${dest}")"
    cp -f "${src}" "${dest}"
    [[ -z "${mode}" ]] || chmod "${mode}" "${dest}"
}

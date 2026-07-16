#!/usr/bin/env bash
# Command and privilege checks.

# shellcheck shell=bash
# shellcheck disable=SC2317

if [[ -n "${SCRIPT_LIBS_COMMAND_LOADED:-}" ]]; then
    return 0 2>/dev/null || true
fi

SCRIPT_LIBS_COMMAND_LOADED=1

command_exists() {
    (($# == 1)) || {
        log_error "command_exists requires exactly one command name."
        return 2
    }

    command -v "$1" >/dev/null 2>&1
}

require_cmd() {
    local optional=false
    local command_name
    local command_count=0
    local missing=()

    while [[ $# -gt 0 ]]; do
        case "$1" in
        --optional) optional=true ;;
        *)
            command_name="$1"
            ((command_count += 1))
            command_exists "${command_name}" || missing+=("${command_name}")
            ;;
        esac
        shift
    done

    if ((command_count == 0)); then
        log_error 'require_cmd requires at least one command name.'
        exit 1
    fi

    ((${#missing[@]} == 0)) && return 0

    if "${optional}"; then
        log_warn "Commands not found: ${missing[*]}"
        return 1
    fi

    log_error "Required commands not found: ${missing[*]}"
    log_error "Install them and try again."
    exit 1
}

require_any_cmd() {
    local command_name

    (($# > 0)) || {
        log_error "require_any_cmd requires at least one command name."
        exit 1
    }

    for command_name in "$@"; do
        if command_exists "${command_name}"; then
            printf '%s\n' "${command_name}"
            return 0
        fi
    done

    log_error "None of the required commands were found: $*"
    exit 1
}

require_root() {
    if [[ ${EUID:-$(id -u)} -ne 0 ]]; then
        log_error "This script must be run as root."
        log_error "Hint: try 'sudo ${0}'"
        exit 1
    fi
}

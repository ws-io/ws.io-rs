#!/usr/bin/env bash
# Color-aware logging helpers.

# shellcheck shell=bash
# shellcheck disable=SC2317

if [[ -n "${LINUX_CONFIGS_LIBS_LOG_LOADED:-}" ]]; then
    return 0 2>/dev/null || true
fi

LINUX_CONFIGS_LIBS_LOG_LOADED=1

LC_COLOR_RESET=''
LC_COLOR_RED=''
LC_COLOR_GREEN=''
LC_COLOR_YELLOW=''
LC_COLOR_BLUE=''
LC_COLOR_CYAN=''

lc_setup_colors() {
    LC_COLOR_RESET=''
    LC_COLOR_RED=''
    LC_COLOR_GREEN=''
    LC_COLOR_YELLOW=''
    LC_COLOR_BLUE=''
    LC_COLOR_CYAN=''

    [[ -t 1 && -z "${NO_COLOR:-}" ]] || return 0

    LC_COLOR_RESET=$'\033[0m'
    LC_COLOR_RED=$'\033[1;31m'
    LC_COLOR_GREEN=$'\033[1;32m'
    LC_COLOR_YELLOW=$'\033[1;33m'
    LC_COLOR_BLUE=$'\033[1;34m'
    LC_COLOR_CYAN=$'\033[1;36m'
}

lc_log_line() {
    local stream="$1"
    local color="$2"
    local level="$3"
    shift 3

    printf '%s[%s] %s:%s %s\n' \
        "${color}" "${SCRIPT_NAME:-script}" "${level}" "${LC_COLOR_RESET}" "$*" \
        >&"${stream}"
}

log_debug() {
    case "${VERBOSE:-0}" in
    1 | yes | true) lc_log_line 2 "${LC_COLOR_BLUE}" DEBUG "$@" ;;
    esac
}

log_info() { lc_log_line 1 "${LC_COLOR_CYAN}" INFO "$@"; }
log_success() { lc_log_line 1 "${LC_COLOR_GREEN}" SUCCESS "$@"; }
log_warn() { lc_log_line 2 "${LC_COLOR_YELLOW}" WARN "$@"; }
log_error() { lc_log_line 2 "${LC_COLOR_RED}" ERROR "$@"; }

lc_setup_colors

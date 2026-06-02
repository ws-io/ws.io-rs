#!/usr/bin/env bash
# Interactive prompt helpers.

# shellcheck shell=bash
# shellcheck disable=SC2317

if [[ -n "${LINUX_CONFIGS_LIBS_PROMPT_LOADED:-}" ]]; then
    return 0 2>/dev/null || true
fi

LINUX_CONFIGS_LIBS_PROMPT_LOADED=1

confirm() {
    local prompt="$1"
    local default='no'
    local force=false
    local answer options
    shift || true

    while [[ $# -gt 0 ]]; do
        case "$1" in
        --default=yes | -y) default='yes' ;;
        --default=no | -n) default='no' ;;
        --force | -f) force=true ;;
        esac
        shift
    done

    "${force}" && return 0

    options='[y/N]'
    [[ "${default}" == 'yes' ]] && options='[Y/n]'

    while true; do
        printf '%s %s: ' "${prompt}" "${options}" >&2
        if ! { read -r answer </dev/tty; } 2>/dev/null; then
            [[ "${default}" == 'yes' ]]
            return
        fi

        [[ -n "${answer}" ]] || answer="${default}"
        case "${answer}" in
        y | Y | yes | Yes | YES) return 0 ;;
        n | N | no | No | NO) return 1 ;;
        *) printf 'Please answer y or n.\n' >&2 ;;
        esac
    done
}

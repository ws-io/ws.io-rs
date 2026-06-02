#!/usr/bin/env bash
# Path helpers.

# shellcheck shell=bash
# shellcheck disable=SC2317

if [[ -n "${LINUX_CONFIGS_LIBS_PATH_LOADED:-}" ]]; then
    return 0 2>/dev/null || true
fi

LINUX_CONFIGS_LIBS_PATH_LOADED=1

absolute_path() {
    local path="$1"
    local base="${2:-$(pwd)}"

    case "${path}" in
    /*) printf '%s\n' "${path}" ;;
    *) printf '%s/%s\n' "$(cd -P -- "${base}" && pwd)" "${path}" ;;
    esac
}

canonical_path() {
    local path="$1"
    local dir base

    if [[ -d "${path}" ]]; then
        cd -P -- "${path}" && pwd
        return
    fi

    dir="$(dirname -- "${path}")"
    base="$(basename -- "${path}")"
    printf '%s/%s\n' "$(cd -P -- "${dir}" && pwd)" "${base}"
}

repo_root_from() {
    local dir="${1:-$(pwd)}"
    dir="$(cd -P -- "${dir}" && pwd)"

    while [[ "${dir}" != '/' ]]; do
        if [[ -e "${dir}/.git" ]]; then
            printf '%s\n' "${dir}"
            return 0
        fi
        dir="$(dirname -- "${dir}")"
    done

    return 1
}

require_under_root() {
    local path root
    path="$(canonical_path "$1")"
    root="$(canonical_path "$2")"

    case "${path%/}" in
    "${root%/}" | "${root%/}"/*) return 0 ;;
    esac

    log_error "Path is outside allowed root: ${path} (root: ${root})"
    exit 1
}

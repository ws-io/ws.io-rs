#!/usr/bin/env bash
# Path helpers.

# shellcheck shell=bash
# shellcheck disable=SC2317

if [[ -n "${SCRIPT_LIBS_PATH_LOADED:-}" ]]; then
    return 0 2>/dev/null || true
fi

SCRIPT_LIBS_PATH_LOADED=1

absolute_path() {
    local path="$1"
    local base="${2:-$(pwd)}"

    case "${path}" in
    /*) printf '%s\n' "${path}" ;;
    *)
        base="$(cd -P -- "${base}" && pwd)"
        printf '%s/%s\n' "${base%/}" "${path}"
        ;;
    esac
}

canonical_path() {
    local path="$1"
    local dir base target
    local symlink_count=0

    while [[ -L "${path}" ]]; do
        ((symlink_count += 1))
        ((symlink_count <= 40)) || return 1
        dir="$(cd -P -- "$(dirname -- "${path}")" && pwd)"
        target="$(readlink "${path}")"
        case "${target}" in
        /*) path="${target}" ;;
        *) path="${dir}/${target}" ;;
        esac
    done

    if [[ -d "${path}" ]]; then
        cd -P -- "${path}" && pwd
        return
    fi

    dir="$(dirname -- "${path}")"
    base="$(basename -- "${path}")"
    dir="$(cd -P -- "${dir}" && pwd)"
    printf '%s/%s\n' "${dir%/}" "${base}"
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
    path="$(canonical_path "$1")" || {
        log_error "Failed to canonicalize path: $1"
        exit 1
    }

    root="$(canonical_path "$2")" || {
        log_error "Failed to canonicalize root: $2"
        exit 1
    }

    path="${path%/}"
    root="${root%/}"

    case "${path}" in
    "${root}" | "${root}"/*) return 0 ;;
    esac

    log_error "Path is outside allowed root: ${path} (root: ${root})"
    exit 1
}

safe_rm_under_root() {
    local root target target_path target_parent target_name canonical_target
    local -a removal_paths=()

    (($# >= 2)) || {
        log_error 'safe_rm_under_root requires a root and at least one target'
        return 2
    }

    root="$1"
    shift
    [[ -n "${root}" && -d "${root}" ]] || {
        log_error "Safe removal root is not an existing directory: ${root:-<empty>}"
        return 1
    }

    root="$(canonical_path "${root}")" || {
        log_error "Failed to canonicalize safe removal root: ${root}"
        return 1
    }

    [[ "${root}" != '/' ]] || {
        log_error 'Safe removal root must not be /'
        return 1
    }

    root="${root%/}"

    for target in "$@"; do
        [[ -n "${target}" ]] || {
            log_error 'Safe removal target must not be empty'
            return 1
        }

        target_path="$(absolute_path "${target}" "${root}")" || return 1
        while [[ "${target_path}" != '/' && "${target_path}" == */ ]]; do
            target_path="${target_path%/}"
        done

        target_name="$(basename -- "${target_path}")"
        case "${target_name}" in
        . | ..)
            log_error "Refusing unsafe removal target: ${target_path}"
            return 1
            ;;
        esac

        target_parent="$(canonical_path "$(dirname -- "${target_path}")")" || {
            log_error "Failed to canonicalize safe removal target parent: ${target_path}"
            return 1
        }

        canonical_target="$(absolute_path "${target_name}" "${target_parent}")" || return 1

        case "${canonical_target}" in
        "${root}"/*) removal_paths+=("${target_path}") ;;
        "${root}")
            log_error "Refusing to remove safe root itself: ${root}"
            return 1
            ;;
        *)
            log_error "Refusing to remove path outside safe root: ${canonical_target}; root: ${root}"
            return 1
            ;;
        esac
    done

    rm -rf -- "${removal_paths[@]}"
}

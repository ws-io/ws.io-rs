# libs quick reference

Unnecessary modifications to files under the "public" directory are prohibited; only their use is permitted.

- Source all helpers: `source "<repo>/scripts/libs/public/common.sh"`
- `libs/common.sh`: sets `LIBS_DIR`, `REPO_ROOT`, `SCRIPT_NAME`, `SCRIPT_DIR`; loads all `libs/*.sh` modules.
- `log_debug msg...`: print DEBUG to stderr when `VERBOSE=1|yes|true`.
- `log_info msg...`: print INFO to stdout.
- `log_success msg...`: print SUCCESS to stdout.
- `log_warn msg...`: print WARN to stderr.
- `log_error msg...`: print ERROR to stderr.
- `command_exists cmd`: return 0 if exactly one command exists; return 2 on bad arity.
- `require_cmd cmd...`: require at least one command name; exit if any command is missing; supports `--optional` to warn/return 1 instead.
- `require_any_cmd cmd...`: print first available command; exit if none are found.
- `require_root`: exit unless running as root.
- `require_file path`: return 0 if regular file exists; log error/return 1 otherwise.
- `require_dir path`: return 0 if directory exists; log error/return 1 otherwise.
- `install_file src dest [mode]`: copy file, create parent dir, optionally chmod destination.
- `absolute_path path [base]`: print absolute path without resolving symlinks/nonexistent final component.
- `canonical_path path`: print physical path; resolves existing dirs and parent dir of files/nonexistent targets.
- `repo_root_from [dir]`: walk upward to nearest `.git`; print root or return 1.
- `require_under_root path root`: exit if path is outside root after canonicalization.
- `host_os`: print `linux|macos|windows|unknown` from `uname -s`.
- `host_arch`: print normalized arch (`x86_64`, `aarch64`, `armv7`, or raw `uname -m`).
- `detect_architecture`: print supported install arch (`x86_64|aarch64`); error otherwise.
- `confirm prompt [--default=yes|--default=no] [--force]`: interactive yes/no; returns default when no tty.
- `prepend_cargo_bin_to_path`: add `$HOME/.cargo/bin` to PATH if present and not already included.
- `encode_rustflags flag...`: print flags joined with Cargo unit-separator encoding.
- `exec_with_encoded_rustflags [array_name] command...`: exec command with `CARGO_ENCODED_RUSTFLAGS`; uses `rustflags` by default, or an explicit existing Bash array name when the first argument names one.
- `ensure_cargo_target target`: install Rust target with rustup if missing.
- `require_cargo_zigbuild`: require `cargo-zigbuild` and `zig`.
- Internal: `lc_*` functions/vars in `libs/log.sh` are private implementation details; avoid calling directly.

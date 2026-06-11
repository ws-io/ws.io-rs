#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

[[ " $@ " =~ ' -c ' ]] && rm -rf ./Cargo.lock ./target

cargo upgrade -i allow
cargo update

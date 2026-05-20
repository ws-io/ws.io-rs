#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

$SCRIPT_DIR = Split-Path -Parent $PSCommandPath
. (Join-Path $SCRIPT_DIR '..\..\..\libs\common.ps1')

Ensure-CargoTarget 'aarch64-pc-windows-msvc'

$rustFlags = @(
    "-C"
    "control-flow-guard=yes"

    # Optional CPU tuning for deployment fleets with a known ARMv8-A baseline.
    # Keep disabled for generic release binaries because it can emit instructions
    # that are unavailable on older or lower-end Windows ARM64 machines.
    # "-C"
    # "target-cpu=cortex-a72"
    # "-C"
    # "target-cpu=neoverse-n1"

    # Optional ARMv8 extensions. Keep disabled for generic release binaries; use
    # only when all target machines are known to support the selected feature.
    # "-C"
    # "target-feature=+crc"
    # "-C"
    # "target-feature=+crypto"
    # "-C"
    # "target-feature=+lse"

    # Optional static CRT for single-file deployment. Keep disabled by default
    # because some dependency stacks expect the dynamic MSVC runtime.
    # "-C"
    # "target-feature=+crt-static"
)

$cargoArgs = @('b', '-r', '--target', 'aarch64-pc-windows-msvc') + $args
exit (Invoke-WithEncodedRustflags $rustFlags 'cargo' $cargoArgs)

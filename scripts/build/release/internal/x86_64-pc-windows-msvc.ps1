#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

$SCRIPT_DIR = Split-Path -Parent $PSCommandPath
. (Join-Path $SCRIPT_DIR '..\..\..\libs\common.ps1')

Ensure-CargoTarget 'x86_64-pc-windows-msvc'

$rustFlags = @(
    "-C"
    "control-flow-guard=yes"

    # Optional CPU baseline tuning for deployment fleets with known x86-64
    # support. Keep disabled for generic release binaries; x86-64-v3, for
    # example, requires AVX/AVX2-class machines and excludes older CPUs.
    # "-C"
    # "target-cpu=x86-64-v2"
    # "-C"
    # "target-cpu=x86-64-v3"

    # Optional CPU extensions. Keep disabled for generic release binaries; use
    # only when all target machines are known to support the selected feature.
    # "-C"
    # "target-feature=+aes"
    # "-C"
    # "target-feature=+avx2"
    # "-C"
    # "target-feature=+sse4.2"

    # Optional static CRT for single-file deployment. Keep disabled by default
    # because some dependency stacks expect the dynamic MSVC runtime.
    # "-C"
    # "target-feature=+crt-static"
)

$cargoArgs = @('b', '-r', '--target', 'x86_64-pc-windows-msvc') + $args
exit (Invoke-WithEncodedRustflags $rustFlags 'cargo' $cargoArgs)

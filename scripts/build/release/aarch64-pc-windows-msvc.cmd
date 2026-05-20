@echo off
color 0f
chcp 65001 > nul

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0internal\aarch64-pc-windows-msvc.ps1" %*

@echo off
REM Dev runner: init MSVC env, then launch `tauri dev` with the hot-reload frontend server.
REM Usage: scripts\dev.bat
setlocal enabledelayedexpansion

set "VCVARS="
for /d %%d in ("%ProgramFiles%\Microsoft Visual Studio\2022\*") do if not defined VCVARS if exist "%%~fd\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%%~fd\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS for /d %%d in ("!ProgramFiles(x86)!\Microsoft Visual Studio\2022\*") do if not defined VCVARS if exist "%%~fd\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%%~fd\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS for /d %%d in ("!ProgramFiles(x86)!\Microsoft Visual Studio\2019\*") do if not defined VCVARS if exist "%%~fd\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%%~fd\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS for /d %%d in ("%ProgramFiles%\Microsoft Visual Studio\2019\*") do if not defined VCVARS if exist "%%~fd\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%%~fd\VC\Auxiliary\Build\vcvars64.bat"

if not defined VCVARS (
  echo [dev] No Visual Studio C++ toolchain found. Install "Desktop development with C++".
  exit /b 1
)

call "!VCVARS!" >nul
if errorlevel 1 (
  echo [dev] Failed to init MSVC env: !VCVARS!
  exit /b 1
)

cd /d "%~dp0.."

REM devUrl lives only in tauri.dev.conf.json (merged via --config) so the plain
REM `cargo build --release` keeps embedding client/ and is not hijacked by devUrl.
where cargo-tauri >nul 2>nul
if %errorlevel%==0 (
  echo [dev] using cargo-tauri ...
  cargo tauri dev --config src-tauri/tauri.dev.conf.json
) else (
  echo [dev] using bunx @tauri-apps/cli ...
  bunx @tauri-apps/cli dev --config src-tauri/tauri.dev.conf.json
)

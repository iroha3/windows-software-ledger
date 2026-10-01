@echo off
REM Wrapper to run cargo with the MSVC environment (needed when invoked from Git Bash/MSYS).
REM Usage: scripts\cargo-msvc.bat check
REM        scripts\cargo-msvc.bat build --release
setlocal enabledelayedexpansion

set "VCVARS="
for /d %%d in ("%ProgramFiles%\Microsoft Visual Studio\2022\*") do if not defined VCVARS if exist "%%~fd\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%%~fd\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS for /d %%d in ("!ProgramFiles(x86)!\Microsoft Visual Studio\2022\*") do if not defined VCVARS if exist "%%~fd\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%%~fd\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS for /d %%d in ("!ProgramFiles(x86)!\Microsoft Visual Studio\2019\*") do if not defined VCVARS if exist "%%~fd\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%%~fd\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS for /d %%d in ("%ProgramFiles%\Microsoft Visual Studio\2019\*") do if not defined VCVARS if exist "%%~fd\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%%~fd\VC\Auxiliary\Build\vcvars64.bat"

if not defined VCVARS (
  echo [cargo-msvc] No Visual Studio C++ toolchain found. Install "Desktop development with C++".
  exit /b 1
)

call "!VCVARS!" >nul
if errorlevel 1 (
  echo [cargo-msvc] Failed to init MSVC env: !VCVARS!
  exit /b 1
)

cd /d "%~dp0..\src-tauri"
cargo %*

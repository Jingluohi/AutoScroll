@echo off
setlocal enabledelayedexpansion

:: ============================================================
:: AutoScroll Portable Build Script
:: Builds the Tauri application as a single portable executable.
:: No installer or zip archive is produced.
:: ============================================================

echo.
echo ============================================================
echo AutoScroll Portable Build
echo ============================================================
echo.

:: Verify npm is available
where npm >nul 2>nul
if errorlevel 1 (
    echo [ERROR] npm is not installed or not in PATH.
    echo Please install Node.js first: https://nodejs.org/
    pause
    exit /b 1
)

:: Verify Cargo is available
where cargo >nul 2>nul
if errorlevel 1 (
    echo [ERROR] Rust / Cargo is not installed or not in PATH.
    echo Please install Rust first: https://rustup.rs/
    pause
    exit /b 1
)

:: Install Node.js dependencies if missing
if not exist "node_modules\" (
    echo [INFO] Installing Node.js dependencies...
    call npm install
    if errorlevel 1 (
        echo [ERROR] Failed to install Node.js dependencies.
        pause
        exit /b 1
    )
    echo [OK] Dependencies installed.
) else (
    echo [OK] Node.js dependencies already installed.
)

:: Build the Tauri application in release mode
echo.
echo [INFO] Building portable executable. This may take a few minutes...
call npm run tauri build
if errorlevel 1 (
    echo [ERROR] Build failed. Check the output above for details.
    pause
    exit /b 1
)

:: Source executable produced by Cargo
echo.
echo [OK] Build completed.

:: Use the directory where this batch file is located as the project root.
:: This guarantees the output always lands in d:\Tools\自动滚屏\auto-scroll\release-portable
:: regardless of where the script is launched from.
set "PROJECT_ROOT=%~dp0"
set "SOURCE_EXE=%PROJECT_ROOT%src-tauri\target\release\auto-scroll.exe"
set "OUTPUT_DIR=%PROJECT_ROOT%release-portable"
set "OUTPUT_EXE=%OUTPUT_DIR%\auto-scroll.exe"

:: Ensure the portable output directory exists
if not exist "%OUTPUT_DIR%" (
    mkdir "%OUTPUT_DIR%"
)

:: Copy the executable to the portable output directory, overwriting any existing file
if exist "%SOURCE_EXE%" (
    echo [INFO] Copying executable to %OUTPUT_DIR% and overwriting existing file...
    copy /Y "%SOURCE_EXE%" "%OUTPUT_EXE%" >nul
    if errorlevel 1 (
        echo [ERROR] Failed to copy executable.
        pause
        exit /b 1
    )
) else (
    echo [ERROR] Built executable not found at: %SOURCE_EXE%
    pause
    exit /b 1
)

echo.
echo ============================================================
echo [SUCCESS] Portable executable: %OUTPUT_EXE%
echo ============================================================
echo.
pause

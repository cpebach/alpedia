@echo off
chcp 65001 >nul
rem Alpedia の物理コア（Rust）の単体テスト（cargo test）を動かす。
rem テストは Python の拡張モジュールとしてではなく、同梱の Python 3.12（packages\windows\Alpedia\runtime）につないで動かす。
rem ビルドの途中のファイルは %LOCALAPPDATA%\alpedia_build\test に置く（OneDrive で同期されないように）。
setlocal EnableExtensions
set "HERE=%~dp0"
set "ROOT=%~dp0.."
set "WORK=%LOCALAPPDATA%\alpedia_build"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
set "TC=stable-x86_64-pc-windows-gnu"
set "RT=%ROOT%\..\packages\windows\Alpedia\runtime"
if not exist "%RT%\python312.dll" goto nopy
if not exist "%WORK%\py312" mkdir "%WORK%\py312"
copy /Y "%RT%\python312.dll" "%WORK%\py312\python312.dll" >nul
"%RT%\python.exe" "%HERE%make_import_lib.py" "%RT%\python312.dll" "%WORK%\py312" >nul
if errorlevel 1 exit /b 1
set "CFG=%WORK%\pyo3_cp312.txt"
> "%CFG%" echo implementation=CPython
>> "%CFG%" echo version=3.12
>> "%CFG%" echo shared=true
>> "%CFG%" echo abi3=false
>> "%CFG%" echo lib_name=python312
>> "%CFG%" echo lib_dir=%WORK%\py312
>> "%CFG%" echo pointer_width=64
>> "%CFG%" echo suppress_build_script_link_lines=false
set "PYO3_CONFIG_FILE=%CFG%"
set "CARGO_TARGET_DIR=%WORK%\test"
set "PATH=%WORK%\py312;%PATH%"
cd /d "%HERE%alpedia_core"
cargo +%TC% test --no-default-features --lib %*
set "ERR=%ERRORLEVEL%"
if not "%ERR%"=="0" echo テストが失敗しました（上の FAILED を見てください）
exit /b %ERR%

:nopy
echo 同梱の Python 3.12（packages\windows\Alpedia\runtime\python312.dll）がありません。
echo 先にパッケージを作ってください（packages\windows\README.md）。
exit /b 1

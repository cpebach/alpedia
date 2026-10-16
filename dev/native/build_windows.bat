@echo off
chcp 65001 >nul
rem Alpedia の物理コア（Rust）を Windows 用にビルドして lib に置く。
rem   元のフォルダ用 ：lib\alpedia_core.pyd（この PC の Python 3.10 以上で使える形）
rem   パッケージ版用 ：packages\windows\Alpedia\game\lib\alpedia_core.cp312-win_amd64.pyd（同梱の Python 3.12 用）
rem ビルドの途中のファイルは %LOCALAPPDATA%\alpedia_build に置く（OneDrive で同期されないように）。
rem 記録は native\build_log.txt に残る。
setlocal EnableExtensions
set "HERE=%~dp0"
set "ROOT=%~dp0.."
set "LOG=%HERE%build_log.txt"
set "WORK=%LOCALAPPDATA%\alpedia_build"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
set "TC=stable-x86_64-pc-windows-gnu"
cd /d "%HERE%alpedia_core"
echo Alpedia 物理コアのビルド %date% %time% > "%LOG%"
where rustup >nul 2>nul
if errorlevel 1 goto norust
if not exist "%WORK%" mkdir "%WORK%"
if not exist "%ROOT%\lib" mkdir "%ROOT%\lib"

echo [1/3] Rust（Windows 用 GNU ツールチェーン）を準備しています...
rustup toolchain install %TC% --profile minimal >> "%LOG%" 2>&1
if errorlevel 1 goto fail
set BUILT=0

rem ---------------------------------------------------------------- 元のフォルダ用（PC の Python）
set "PYDIR="
for /f "delims=" %%i in ('python -c "import sys,os;print(os.path.dirname(sys.executable))" 2^>nul') do set "PYDIR=%%i"
if not defined PYDIR for /f "delims=" %%i in ('py -3 -c "import sys,os;print(os.path.dirname(sys.executable))" 2^>nul') do set "PYDIR=%%i"
if defined PYDIR if not exist "%PYDIR%\python3.dll" for /f "delims=" %%i in ('py -3 -c "import sys,os;print(os.path.dirname(sys.executable))" 2^>nul') do set "PYDIR=%%i"
echo Python: %PYDIR% >> "%LOG%"
if not defined PYDIR goto skip_abi3
if not exist "%PYDIR%\python3.dll" goto skip_abi3
echo [2/3] 元のフォルダ用をビルドしています（初回は数分かかります）...
if not exist "%WORK%\py3" mkdir "%WORK%\py3"
copy /Y "%PYDIR%\python3.dll" "%WORK%\py3\python3.dll" >nul
set "CFG=%WORK%\pyo3_abi3.txt"
> "%CFG%" echo implementation=CPython
>> "%CFG%" echo version=3.10
>> "%CFG%" echo shared=true
>> "%CFG%" echo abi3=true
>> "%CFG%" echo lib_name=python3
>> "%CFG%" echo lib_dir=%WORK%\py3
>> "%CFG%" echo pointer_width=64
>> "%CFG%" echo suppress_build_script_link_lines=false
set "PYO3_CONFIG_FILE=%CFG%"
echo ==== abi3 ==== >> "%LOG%"
cargo +%TC% build --release --features abi3 --target-dir "%WORK%\abi3" >> "%LOG%" 2>&1
if errorlevel 1 goto fail
copy /Y "%WORK%\abi3\release\alpedia_core.dll" "%ROOT%\lib\alpedia_core.pyd" >nul
if errorlevel 1 goto fail
echo     できました: lib\alpedia_core.pyd
set BUILT=1
goto after_abi3
:skip_abi3
echo [2/3] この PC の Python が見つからないので、元のフォルダ用は作りません
echo python が見つからない、または python3.dll がありません >> "%LOG%"
:after_abi3

rem ---------------------------------------------------------------- パッケージ版用（同梱の Python 3.12）
set "RT=%ROOT%\..\packages\windows\Alpedia\runtime"
if not exist "%RT%\python312.dll" goto skip_312
echo [3/3] パッケージ版用をビルドしています...
if not exist "%WORK%\py312" mkdir "%WORK%\py312"
copy /Y "%RT%\python312.dll" "%WORK%\py312\python312.dll" >nul
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
echo ==== cp312 ==== >> "%LOG%"
cargo +%TC% build --release --target-dir "%WORK%\cp312" >> "%LOG%" 2>&1
if errorlevel 1 goto fail
set "GL=%ROOT%\..\packages\windows\Alpedia\game\lib"
if not exist "%GL%" mkdir "%GL%"
copy /Y "%WORK%\cp312\release\alpedia_core.dll" "%GL%\alpedia_core.cp312-win_amd64.pyd" >nul
copy /Y "%WORK%\cp312\release\alpedia_core.dll" "%ROOT%\lib\alpedia_core.cp312-win_amd64.pyd" >nul
echo     できました: packages\windows\Alpedia\game\lib\alpedia_core.cp312-win_amd64.pyd
set BUILT=1
goto done
:skip_312
echo [3/3] パッケージ版が見つからないので、パッケージ版用は作りません

:done
if "%BUILT%"=="0" goto fail
echo.
echo 完了しました。ゲームを起動すると、画面左上に「物理:Rust」と出ます。
echo 完了 >> "%LOG%"
pause
exit /b 0

:norust
echo.
echo Rust が見つかりません。https://rustup.rs の rustup-init.exe で入れてから、もう一度実行してください。
echo Rust が見つかりません >> "%LOG%"
pause
exit /b 1

:fail
echo.
echo ビルドに失敗しました。くわしくは native\build_log.txt を見てください。
echo 失敗 >> "%LOG%"
pause
exit /b 1

"""予備の標準ライブラリ（python312.zip）を作る。Python 3.12 で実行すること。

python.org から公式の「埋め込み用 Python」を取れない環境のための予備。
Panda3D の Windows 用 wheel に入っている python312.dll と拡張モジュール（*.pyd）に、
この zip（標準ライブラリを 3.12 のバイトコードにしたもの）を組み合わせて Python 一式にする。

  python3.12 make_stdlib.py [出力先 zip]
"""
import os
import sys
import sysconfig
import zipfile
from pathlib import Path

if sys.version_info[:2] != (3, 12):
    sys.exit("Python 3.12 で実行してください（バイトコードの形式が版ごとに違うため）")

SKIP_DIRS = {"test", "tests", "idlelib", "tkinter", "turtledemo", "ensurepip", "lib2to3", "venv", "curses",
             "__pycache__", "site-packages", "dist-packages", "lib-dynload", "__phello__", "pydoc_data"}
SKIP_FILES = {"sitecustomize.py", "_distutils_system_mod.py", "turtle.py", "antigravity.py", "this.py",
              "__hello__.py", "_aix_support.py", "_osx_support.py"}


def main(out):
    src = Path(sysconfig.get_paths()["stdlib"])
    out = Path(out)
    out.parent.mkdir(parents=True, exist_ok=True)
    n = 0
    with zipfile.PyZipFile(out, "w", compression=zipfile.ZIP_DEFLATED, optimize=0) as z:
        for root, dirs, files in os.walk(src):
            rel = Path(root).relative_to(src)
            dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS and not d.startswith("config-"))
            for f in sorted(files):
                if not f.endswith(".py") or f in SKIP_FILES or f.startswith("_sysconfigdata"):
                    continue
                p = Path(root) / f
                arc = (rel / f).as_posix()
                code = compile(p.read_bytes(), "Lib/" + arc, "exec", dont_inherit=True)
                import importlib._bootstrap_external as be
                data = be._code_to_timestamp_pyc(code, 0, 0)
                z.writestr(arc[:-3] + ".pyc", bytes(data))
                n += 1
        lic = src / "LICENSE.txt"
        if lic.exists():
            z.write(lic, "LICENSE.txt")
    print(f"{out}: {n} モジュール, {out.stat().st_size / 1e6:.1f} MB（Python {sys.version.split()[0]}）")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent / "runtime_fallback" / "python312.zip")

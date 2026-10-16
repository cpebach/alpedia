"""Alpedia.exe / Alpedia-console.exe を作る（どの OS でも可）。

  pip install ziglang
  python build_launcher.py            → このフォルダに2つの exe ができる

zig の C コンパイラ（clang）で Windows 用にクロスコンパイルする。アイコンは alpedia.rc / alpedia.ico。
"""
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


def build(out_dir=HERE):
    out_dir = Path(out_dir)
    ico = HERE / "alpedia.ico"
    if not ico.exists():
        src = HERE.parents[2] / "sauce" / "textures" / "alpedia.ico"   # alpedia/sauce/textures/alpedia.ico
        if src.exists():
            shutil.copy2(src, ico)
    base = [sys.executable, "-m", "ziglang", "cc", "-target", "x86_64-windows-gnu", "-O2", "-s",
            "-municode", str(HERE / "launcher.c")]
    if ico.exists():
        base.append(str(HERE / "alpedia.rc"))
    for name, extra in (("Alpedia.exe", ["-DALPEDIA_GUI", "-Wl,--subsystem,windows"]),
                        ("Alpedia-console.exe", [])):
        cmd = base + extra + ["-o", str(out_dir / name), "-lshell32", "-luser32"]
        print(" ".join(cmd))
        subprocess.run(cmd, check=True, cwd=HERE)
        for junk in out_dir.glob(name.replace(".exe", ".pdb")):
            junk.unlink()
    return [out_dir / "Alpedia.exe", out_dir / "Alpedia-console.exe"]


if __name__ == "__main__":
    for p in build(sys.argv[1] if len(sys.argv) > 1 else HERE):
        print(p, p.stat().st_size)

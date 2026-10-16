"""Alpedia パッケージ版（Windows 64bit）を組み立てる。

  python build_package.py              → ../Alpedia/ にゲーム一式（Alpedia.exe をダブルクリックで起動）
  python build_package.py --zip        → さらに ../dist/Alpedia-win64.zip（配布用）も作る
  python build_package.py --test       → Windows 上なら組み立て後に起動テスト（描画なし）もする

必要なもの：Python 3.8 以上（OS は問わない）と、PyPI（pypi.org）につながるネット。
Windows 用の部品はすべてダウンロードして組み立てるので、組み立てる PC 自体が Windows でなくてよい。

何をしているか
1. Python 本体（runtime/）
   - python.org の「埋め込み用 Python 3.12」が取れればそれを使う（いちばん素直）
   - 取れなければ、Panda3D の Windows 用 wheel に入っている python312.dll・拡張モジュールと、
     このフォルダの runtime_fallback/python312.zip（標準ライブラリ、make_stdlib.py で作ったもの）を組み合わせる
2. ライブラリ（runtime/Lib/site-packages/）：requirements-windows.txt の版を Windows 用 wheel で入れる
3. ゲーム本体（game/）：このフォルダの2つ上（alpedia フォルダ）からコピーする。元のフォルダは一切変更しない
   - game/saves・game/logs・game/config/settings.json・game/mods に後から足したものは、作り直しても消さない
4. GPU 高速化（alpedia_gpu/）と起動用の boot.py、ランチャー（Alpedia.exe / Alpedia-console.exe）
"""
import argparse
import io
import json
import os
import shutil
import subprocess
import sys
import urllib.request
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent                 # packages/windows/build
PKG = HERE.parent                                      # packages/windows
SRC = PKG.parent.parent / "dev"                        # alpedia/dev（開発版のゲーム）
OUT = PKG / "Alpedia"
CACHE = HERE / "cache"

PY_VER = "3.12.10"
PY_TAG = "312"
EMBED_URL = f"https://www.python.org/ftp/python/{PY_VER}/python-{PY_VER}-embed-amd64.zip"

GAME_ITEMS = ["main", "sauce", "run.py", "README.md"]          # 毎回入れ替える
GAME_KEEP = ["mods"]                                           # 足りないものだけ足す（本体の MOD は入れ替える）
SKIP_NAMES = {"__pycache__", ".trash", ".git"}
TRIM_TESTS = ("numpy", "scipy", "skimage", "numba", "llvmlite", "networkx", "PIL", "imageio", "tifffile")


def log(*a):
    print("[build]", *a, flush=True)


# ---------------------------------------------------------------------------
# ダウンロード
# ---------------------------------------------------------------------------
def fetch(url, dest):
    dest = Path(dest)
    if dest.exists() and dest.stat().st_size > 0:
        return dest
    dest.parent.mkdir(parents=True, exist_ok=True)
    log("ダウンロード:", url)
    with urllib.request.urlopen(url, timeout=60) as r:
        data = r.read()
    dest.write_bytes(data)
    return dest


def download_wheels():
    wh = CACHE / "wheels"
    wh.mkdir(parents=True, exist_ok=True)
    cmd = [sys.executable, "-m", "pip", "download", "-r", str(HERE / "requirements-windows.txt"),
           "--only-binary=:all:", "--platform", "win_amd64", "--python-version", PY_VER.rsplit(".", 1)[0],
           "--implementation", "cp", "--abi", f"cp{PY_TAG}", "-d", str(wh), "--disable-pip-version-check", "-q"]
    log("Windows 用ライブラリを取得中（初回は数百 MB）")
    subprocess.run(cmd, check=True)
    return sorted(wh.glob("*.whl"))


# ---------------------------------------------------------------------------
# Python 本体
# ---------------------------------------------------------------------------
PTH = f"""python{PY_TAG}.zip
.
Lib\\site-packages
import site
"""


def build_runtime(rt, wheels):
    if rt.exists():
        shutil.rmtree(rt)
    rt.mkdir(parents=True)
    how = None
    try:
        z = fetch(EMBED_URL, CACHE / Path(EMBED_URL).name)
        with zipfile.ZipFile(z) as zf:
            zf.extractall(rt)
        how = f"python.org 埋め込み用 Python {PY_VER}"
    except Exception as e:  # noqa: BLE001
        log("python.org から取得できませんでした（予備の組み合わせを使います）:", e)
    panda = next(w for w in wheels if w.name.startswith("panda3d-") and "gltf" not in w.name)
    with zipfile.ZipFile(panda) as zf:
        names = zf.namelist()
        if how is None:
            for n in names:
                if n.startswith("deploy_libs/") and not n.endswith("/"):
                    (rt / Path(n).name).write_bytes(zf.read(n))
            fb = HERE / "runtime_fallback" / f"python{PY_TAG}.zip"
            if not fb.exists():
                sys.exit(f"{fb} がありません（make_stdlib.py を Python 3.12 で実行して作ってください）")
            shutil.copy2(fb, rt / f"python{PY_TAG}.zip")
            how = "Panda3D 同梱の python312.dll ＋ 予備の標準ライブラリ（runtime_fallback）"
        # C++ ランタイム（llvmlite などが使う）
        for n in names:
            if Path(n).name.lower() == "msvcp140.dll" and not (rt / "MSVCP140.dll").exists():
                (rt / "MSVCP140.dll").write_bytes(zf.read(n))
    for p in rt.glob("python*._pth"):
        p.unlink()
    (rt / f"python{PY_TAG}._pth").write_text(PTH, encoding="utf-8")
    (rt / "RUNTIME.txt").write_text(how + "\n", encoding="utf-8")
    log("Python 本体:", how)
    return how


def install_wheels(site, wheels):
    site.mkdir(parents=True, exist_ok=True)
    for w in wheels:
        with zipfile.ZipFile(w) as zf:
            for info in zf.infolist():
                n = info.filename
                if n.endswith("/"):
                    continue
                parts = n.split("/")
                if parts[0].endswith(".data"):
                    if len(parts) > 2 and parts[1] in ("purelib", "platlib"):
                        parts = parts[2:]
                    else:
                        continue                         # scripts / headers / data は使わない
                if parts[0] in ("deploy_libs", "panda3d_tools"):
                    continue                             # Panda3D の配布用ツール（ゲームには不要）
                if parts[0] in TRIM_TESTS and "tests" in parts[1:-1]:
                    continue                             # ライブラリ自身のテスト
                dest = site.joinpath(*parts)
                dest.parent.mkdir(parents=True, exist_ok=True)
                with zf.open(info) as src, open(dest, "wb") as dst:
                    shutil.copyfileobj(src, dst)
        log("  入れた:", w.name)


# ---------------------------------------------------------------------------
# ゲーム本体と起動まわり
# ---------------------------------------------------------------------------
def _ignore(_d, names):
    return [n for n in names if n in SKIP_NAMES or n.endswith(".pyc")]


def copy_game(game):
    game.mkdir(parents=True, exist_ok=True)
    for item in GAME_ITEMS:
        s, d = SRC / item, game / item
        if not s.exists():
            continue
        if d.is_dir():
            shutil.rmtree(d)
        elif d.exists():
            d.unlink()
        if s.is_dir():
            shutil.copytree(s, d, ignore=_ignore)
        else:
            shutil.copy2(s, d)
    # 本体に入っている MOD（mod.json の "builtin": true。サバイバル・街）は毎回入れ替える
    for mj in sorted((SRC / "mods").glob("*/mod.json")):
        try:
            builtin = json.loads(mj.read_text(encoding="utf-8")).get("builtin")
        except Exception:  # noqa: BLE001
            builtin = False
        if builtin:
            d = game / "mods" / mj.parent.name
            if d.exists():
                shutil.rmtree(d)
            shutil.copytree(mj.parent, d, ignore=_ignore)
    for item in GAME_KEEP:                               # 後から足したものは残し、足りないものだけ足す
        s = SRC / item
        if not s.exists():
            continue
        for p in s.rglob("*"):
            if p.is_dir() or any(x in SKIP_NAMES for x in p.parts):
                continue
            d = game / item / p.relative_to(s)
            if not d.exists():
                d.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(p, d)
    # Rust の物理コア（native/build_windows.bat で作った lib/alpedia_core*.pyd。無ければ numba で動く）
    #   同梱の Python 3.12 には python3.dll が無いので、使われるのは alpedia_core.cp312-win_amd64.pyd
    pyds = sorted((SRC / "lib").glob("alpedia_core*.pyd"))
    if pyds:
        (game / "lib").mkdir(exist_ok=True)
        for pyd in pyds:
            shutil.copy2(pyd, game / "lib" / pyd.name)
        log("Rust の物理コアを入れました:", ", ".join(p.name for p in pyds))
    else:
        log("Rust の物理コア（lib/alpedia_core.pyd）がありません。numba で計算します（native/build_windows.bat で作れます）")
    for d in ("saves", "logs", "config"):
        (game / d).mkdir(exist_ok=True)
    cfg_path = game / "config" / "settings.json"
    if not cfg_path.exists():                            # 設定はパッケージ版で変えたものを優先（初回だけ元からコピー）
        cfg = {}
        try:
            cfg = json.loads((SRC / "config" / "settings.json").read_text(encoding="utf-8"))
        except Exception:  # noqa: BLE001
            pass
        cfg.setdefault("graphics", {}).setdefault("gpu", "auto")
        cfg_path.write_text(json.dumps(cfg, indent=2, ensure_ascii=False), encoding="utf-8")
    log("ゲーム本体をコピー:", SRC)


def copy_boot(out):
    overlay = HERE / "overlay"
    shutil.copy2(overlay / "boot.py", out / "boot.py")
    if (out / "alpedia_gpu").exists():
        shutil.rmtree(out / "alpedia_gpu")
    shutil.copytree(overlay / "alpedia_gpu", out / "alpedia_gpu", ignore=_ignore)
    launcher = HERE / "launcher"
    exes = [launcher / "Alpedia.exe", launcher / "Alpedia-console.exe"]
    try:                                                 # ziglang があれば作り直す（なければ同梱の exe を使う）
        import ziglang  # noqa: F401
        sys.path.insert(0, str(launcher))
        import build_launcher
        exes = build_launcher.build(launcher)
    except ImportError:
        pass
    except Exception as e:  # noqa: BLE001
        log("ランチャーの作り直しに失敗（同梱の exe を使います）:", e)
    for e in exes:
        shutil.copy2(e, out / e.name)


def make_zip(out):
    dist = PKG / "dist"
    dist.mkdir(exist_ok=True)
    zp = dist / "Alpedia-win64.zip"
    log("配布用 zip を作成:", zp)
    with zipfile.ZipFile(zp, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as z:
        for p in sorted(out.rglob("*")):
            rel = p.relative_to(out)
            top = rel.parts[0]
            if p.is_dir() or top == "cache" or "__pycache__" in rel.parts:
                continue
            if top == "game" and len(rel.parts) > 1 and rel.parts[1] in ("saves", "logs"):
                continue
            z.write(p, Path("Alpedia") / rel)
    log(f"  {zp.stat().st_size / 1e6:.0f} MB")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--zip", action="store_true", help="配布用 zip も作る")
    ap.add_argument("--test", action="store_true", help="組み立て後に起動テスト（Windows のみ）")
    ap.add_argument("--skip-runtime", action="store_true", help="Python 本体とライブラリは作り直さない（ゲームだけ更新）")
    args = ap.parse_args()
    if not (SRC / "run.py").exists():
        sys.exit(f"元のゲームが見つかりません: {SRC}")
    OUT.mkdir(parents=True, exist_ok=True)
    rt = OUT / "runtime"
    if not args.skip_runtime or not rt.exists():
        wheels = download_wheels()
        build_runtime(rt, wheels)
        install_wheels(rt / "Lib" / "site-packages", wheels)
    copy_game(OUT / "game")
    copy_boot(OUT)
    (OUT / "cache").mkdir(exist_ok=True)
    if args.zip:
        make_zip(OUT)
    if args.test:
        if os.name != "nt":
            log("起動テストは Windows でだけできます（スキップ）")
        else:
            r = subprocess.run([str(OUT / "Alpedia-console.exe"), "--headless", "--steps", "120"], cwd=OUT)
            log("起動テスト:", "成功" if r.returncode == 0 else f"失敗（{r.returncode}）")
            if r.returncode:
                sys.exit(r.returncode)
    log("完了:", OUT / "Alpedia.exe")


if __name__ == "__main__":
    main()

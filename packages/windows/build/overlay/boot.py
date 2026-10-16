"""Alpedia パッケージ版の起動（Alpedia.exe から呼ばれる）。

1. 同梱の game/ と GPU 高速化（alpedia_gpu）を読み込めるようにする
2. config/settings.json の graphics.gpu に従って GPU 描画を差し込む
3. game/run.py をそのまま実行する（引数もそのまま渡す：--world 名前 など）
"""
import json
import os
import sys
import traceback
from pathlib import Path

ROOT = Path(__file__).resolve().parent
GAME = ROOT / "game"


def _console_log():
    """ウィンドウ版（コンソールなし）では標準出力をファイルに残す。"""
    if sys.stdout is None or sys.stderr is None:
        (GAME / "logs").mkdir(parents=True, exist_ok=True)
        f = open(GAME / "logs" / "console.log", "w", encoding="utf-8", buffering=1)
        sys.stdout = sys.stdout or f
        sys.stderr = sys.stderr or f


def _error_box(text):
    try:
        import ctypes
        ctypes.windll.user32.MessageBoxW(None, text, "Alpedia", 0x10)
    except Exception:  # noqa: BLE001
        pass


def main():
    _console_log()
    sys.path.insert(0, str(GAME))
    sys.path.insert(0, str(ROOT))
    os.chdir(GAME)
    # numba のコンパイル結果はこのフォルダに保存（2回目以降の起動が速くなる）
    os.environ.setdefault("NUMBA_CACHE_DIR", str(ROOT / "cache" / "numba"))
    mode = "auto"
    try:
        cfg = json.loads((GAME / "config" / "settings.json").read_text(encoding="utf-8"))
        mode = cfg.get("graphics", {}).get("gpu", "auto")
    except Exception:  # noqa: BLE001
        pass
    if "--headless" not in sys.argv and str(mode).lower() != "off":
        try:
            import alpedia_gpu
            alpedia_gpu.install(mode)
        except Exception:  # noqa: BLE001
            traceback.print_exc()
    import runpy
    sys.argv = [str(GAME / "run.py")] + sys.argv[1:]
    runpy.run_path(str(GAME / "run.py"), run_name="__main__")


if __name__ == "__main__":
    try:
        main()
    except SystemExit:
        raise
    except BaseException:  # noqa: BLE001
        msg = traceback.format_exc()
        try:
            (GAME / "logs").mkdir(parents=True, exist_ok=True)
            (GAME / "logs" / "crash.log").write_text(msg, encoding="utf-8")
        except Exception:  # noqa: BLE001
            pass
        print(msg, file=sys.stderr)
        _error_box("Alpedia が異常終了しました。\n詳しくは game\\logs\\crash.log を見てください。\n\n" + msg[-1500:])
        sys.exit(1)

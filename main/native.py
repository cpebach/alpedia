"""Rust の物理コア（native/alpedia_core）を読み込む。

ビルドしたもの（Linux: alpedia_core.so、Windows: alpedia_core.pyd）は <ゲームフォルダ>/lib/ に置く。
作り方は native/README.md（Windows は native/build_windows.bat をダブルクリック）。
無いとき・設定 physics.backend が "python" のときは、今までどおり Python（numba）で計算する。
"""
import os
import sys

from . import log, paths

L = log.get("native")
LIB = paths.ROOT / "lib"

core = None          # 読み込めた alpedia_core モジュール（使わないときは None）
reason = ""          # 使わない理由（ログ・画面の表示用）


def load(backend="auto"):
    """backend: "auto"（あれば Rust）/ "rust"（Rust が必須）/ "python"（使わない）。"""
    global core, reason
    core = None
    if os.environ.get("ALPEDIA_BACKEND"):
        backend = os.environ["ALPEDIA_BACKEND"]
    if backend == "python":
        reason = "設定で Python（numba）を選択"
        L.info("物理コア: Python（numba）")
        return None
    if str(LIB) not in sys.path:
        sys.path.insert(0, str(LIB))
    try:
        import alpedia_core as m
        core = m
        reason = ""
        L.info("物理コア: Rust（alpedia_core %s）", m.version())
    except ImportError as e:
        reason = f"Rust の物理コアがありません（{e}）"
        if backend == "rust":
            raise
        L.info("物理コア: Python（numba）— %s", reason)
    return core


_backend = "auto"


def ensure(backend):
    """設定の backend に合わせる（変わったときだけ読み直す）。"""
    global _backend
    backend = backend or "auto"
    if backend != _backend:
        _backend = backend
        load(backend)


def name():
    return "Rust" if core is not None else "Python"

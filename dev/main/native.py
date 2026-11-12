"""Rust の物理コア（native/alpedia_core）を読み込む。ゲームの物理はこれで計算する（無いと動かない）。

ビルドしたもの（Linux: alpedia_core.so、Windows: alpedia_core.pyd）は <ゲームフォルダ>/lib/ に置く。
作り方は native/README.md（Windows は native/build_windows.bat をダブルクリック）。
"""
import sys

from . import log, paths

L = log.get("native")
LIB = paths.ROOT / "lib"

core = None          # 読み込めた alpedia_core モジュール
reason = ""          # 読み込めなかった理由


class MissingCore(RuntimeError):
    pass


def load(backend=None):
    """alpedia_core を読み込む（backend は前の版の名残り。使わない）。"""
    global core, reason
    if core is not None:
        return core
    if str(LIB) not in sys.path:
        sys.path.insert(0, str(LIB))
    try:
        import alpedia_core as m
    except ImportError as e:
        reason = f"Rust の物理コア（lib/alpedia_core）がありません（{e}）。native/build_windows.bat で作ってください"
        L.error("%s", reason)
        return None
    core = m
    reason = ""
    L.info("物理コア: Rust（alpedia_core %s）", m.version())
    return core


def require():
    """物理コア（無ければ作り方を書いた例外）。"""
    if core is None and load() is None:
        raise MissingCore(reason)
    return core


def ensure(backend=None):
    """前の版の名残り（設定の physics.backend）。読み込まれていなければ読み込む。"""
    require()


def name():
    return "Rust"


load()

"""MOD のプログラム（mods/<MOD名>/code/）を読み込む。

MOD は定義（素材・機器などの JSON）に加えて、プログラムを持てる:

    mods/<MOD名>/code/__init__.py
        def world_addon():        ワールド側の拡張（hooks.Addon）を1つ返す
        def client_addon(game):   （任意）画面側の拡張（client.addon_hooks.ClientAddon）を返す

プログラムは alpedia_mods.<MOD名> というパッケージとして読み込まれる（中では相対 import が使える）。
本体は `from main.hooks import Addon` のように main から import する。
ワールドで使う MOD は saves/<ワールド>/mods.json（メニューのワールド設定の MOD 選択）で決まる。
"""
import importlib.util
import sys
import types

from . import log, paths

L = log.get("modcode")
PREFIX = "alpedia_mods"


def _package(mod_id):
    """MOD のプログラムのパッケージ（code/ が無い MOD は None）。"""
    d = paths.MODS / mod_id / "code"
    init = d / "__init__.py"
    if not init.exists():
        return None
    name = f"{PREFIX}.{mod_id}"
    if name in sys.modules:
        return sys.modules[name]
    if PREFIX not in sys.modules:
        root = types.ModuleType(PREFIX)
        root.__path__ = []
        sys.modules[PREFIX] = root
    spec = importlib.util.spec_from_file_location(name, init, submodule_search_locations=[str(d)])
    m = importlib.util.module_from_spec(spec)
    sys.modules[name] = m
    try:
        spec.loader.exec_module(m)
    except Exception:
        sys.modules.pop(name, None)
        raise
    return m


def world_addons(mod_ids, errors=None):
    """MOD のワールド側の拡張を作る（プログラムを持たない MOD は飛ばす）。"""
    out = []
    for mid in mod_ids:
        try:
            pkg = _package(mid)
            if pkg is None or not hasattr(pkg, "world_addon"):
                continue
            a = pkg.world_addon()
            a.mod_id = mid
            out.append(a)
            L.info("MOD のプログラムを読み込み: %s（%s）", mid, type(a).__name__)
        except Exception as e:  # noqa: BLE001
            L.exception("MOD '%s' のプログラムを読み込めません", mid)
            if errors is not None:
                errors.append(f"MOD '{mid}' のプログラムを読み込めません: {type(e).__name__}: {e}")
    return out


def client_addons(game):
    """ワールドに入っている拡張に合わせて、画面側の拡張を作る。"""
    out = []
    for a in game.world.addons:
        mid = getattr(a, "mod_id", None)
        pkg = sys.modules.get(f"{PREFIX}.{mid}") if mid else None
        if pkg is None or not hasattr(pkg, "client_addon"):
            continue
        try:
            c = pkg.client_addon(game)
            if c is not None:
                out.append(c)
        except Exception:  # noqa: BLE001
            L.exception("MOD '%s' の画面を作れません", mid)
            game.world.message(f"⚠ MOD '{mid}' の画面を作れませんでした（logs/latest.log）")
    return out

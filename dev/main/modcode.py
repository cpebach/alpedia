"""MOD のプログラム（mods/<MOD名>/code/）を読み込む。

MOD は定義（素材・機器などの JSON）に加えて、プログラムを持てる:

    mods/<MOD名>/code/__init__.py
        def world_addon():        ワールド側の拡張（hooks.Addon）を1つ返す
        def client_addon(game):   （任意）画面側の拡張（client.addon_hooks.ClientAddon）を返す

プログラムは alpedia_mods.<MOD名> というパッケージとして読み込まれる（中では相対 import が使える）。
本体は `from main.hooks import Addon` のように main から import する。
ワールドで使う MOD は saves/<ワールド>/mods.json（メニューのワールド設定の MOD 選択）で決まる。

MOD はワールドを始めるときに一括で読み込む（code/ の中の .py は全部、このとき import する）。
エラーがあればこのときにまとめて出し、ワールドは始めない。プレイ中に MOD のファイルを読みに行くことはない
（プレイ中に MOD を書き換えても反映されない。次にワールドを始めたときに読み直す）。
"""
import importlib
import importlib.util
import sys
import traceback
import types

from . import log, paths

L = log.get("modcode")
PREFIX = "alpedia_mods"


class ModLoadError(Exception):
    """MOD（定義・プログラム・テクスチャ）や、ワールドが使っている MOD の中身に問題があり、ワールドを始められない。"""

    def __init__(self, errors):
        self.errors = list(errors)
        super().__init__("MOD の読み込みでエラーがあります:\n" + "\n".join(self.errors))


def reset():
    """読み込み済みの MOD のプログラムを捨てる（次のワールドを始めるときに読み直すため）。"""
    for name in [n for n in sys.modules if n == PREFIX or n.startswith(PREFIX + ".")]:
        del sys.modules[name]


def _where(e, root):
    """例外が起きた MOD の中の場所（ファイル名と行）。文法の誤りはメッセージに場所が入っているので付けない。"""
    if isinstance(e, SyntaxError):
        return ""
    for fr in reversed(traceback.extract_tb(e.__traceback__)):
        try:
            rel = paths.Path(fr.filename).resolve().relative_to(root.resolve())
        except (ValueError, OSError):
            continue
        return f"（{rel} {fr.lineno} 行目）"
    return ""


CLIENT_LIBS = ("ursina", "panda3d", "direct", "main.client")
_deferred = []        # 画面が無いと読み込めないモジュール（画面を作るときに読み込む）


def _needs_client(e):
    """画面用のライブラリ（ursina・panda3d・main.client）が無くて読み込めなかったか。"""
    if not isinstance(e, ImportError):
        return False
    txt = f"{getattr(e, 'name', '') or ''} {e}"
    return any(lib in txt for lib in CLIENT_LIBS)


def load(mod_ids, errors):
    """MOD のプログラムを全部読み込む（code/ の中の .py をすべて import）。エラーは errors に足す。
    画面用のモジュール（ursina などを使うもの）が画面の無い所（サーバー・テスト）で読めないときは、
    load_client（画面を作るとき）まで後回しにする。"""
    reset()
    _deferred.clear()
    guard = None if _client_ready() else _ClientGuard()
    if guard is not None:
        sys.meta_path.insert(0, guard)
    try:
        _load(mod_ids, errors)
    finally:
        if guard is not None:
            sys.meta_path.remove(guard)


def _load(mod_ids, errors):
    for mid in mod_ids:
        d = paths.MODS / mid / "code"
        if not (d / "__init__.py").exists():
            continue
        try:
            _package(mid)
        except Exception as e:  # noqa: BLE001
            L.exception("MOD '%s' のプログラムを読み込めません", mid)
            errors.append(f"MOD '{mid}' のプログラムを読み込めません: {type(e).__name__}: {e}{_where(e, d)}")
            continue
        for f in sorted(d.rglob("*.py")):
            rel = f.relative_to(d).with_suffix("")
            parts = [p for p in rel.parts if p != "__init__"]
            if not parts or "__pycache__" in rel.parts:
                continue
            name = f"{PREFIX}.{mid}." + ".".join(parts)
            try:
                importlib.import_module(name)
            except Exception as e:  # noqa: BLE001
                if _needs_client(e) and not _client_ready():
                    _deferred.append((mid, name, rel, d))
                    continue
                L.exception("MOD '%s' の %s を読み込めません", mid, f.name)
                errors.append(f"MOD '{mid}' の {rel.as_posix()}.py を読み込めません: {type(e).__name__}: {e}{_where(e, d)}")


class _ClientGuard:
    """画面の無い所で MOD を読み込む間、画面用のライブラリを読み込ませない（そのモジュールは後回しになる）。"""

    def find_spec(self, name, path=None, target=None):
        if any(name == b or name.startswith(b + ".") for b in CLIENT_LIBS):
            raise ImportError(f"画面用のライブラリ {name} は画面を作るときに読み込みます", name=name)
        return None


def _client_ready():
    """画面のライブラリが読み込める状態か（ゲームの画面から起動しているか）。"""
    return "ursina" in sys.modules


def load_client(errors):
    """後回しにした画面用のモジュールを読み込む（画面を作るとき。プレイ中には読みに行かない）。"""
    for mid, name, rel, d in list(_deferred):
        try:
            importlib.import_module(name)
        except Exception as e:  # noqa: BLE001
            L.exception("MOD '%s' の %s を読み込めません", mid, name)
            errors.append(f"MOD '{mid}' の {rel.as_posix()}.py を読み込めません: {type(e).__name__}: {e}{_where(e, d)}")
    _deferred.clear()


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
                errors.append(f"MOD '{mid}' のプログラムを読み込めません: {type(e).__name__}: {e}"
                              f"{_where(e, paths.MODS / mid / 'code')}")
    return out


def client_addons(game, errors=None):
    """ワールドに入っている拡張に合わせて、画面側の拡張を作る。errors があればエラーをそこに足す。"""
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
        except Exception as e:  # noqa: BLE001
            L.exception("MOD '%s' の画面を作れません", mid)
            if errors is not None:
                errors.append(f"MOD '{mid}' の画面を作れません: {type(e).__name__}: {e}"
                              f"{_where(e, paths.MODS / mid / 'code')}")
            else:
                game.world.message(f"⚠ MOD '{mid}' の画面を作れませんでした（logs/latest.log）")
    return out

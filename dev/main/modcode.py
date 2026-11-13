"""MOD のプログラム（mods/<MOD名>/code/）を読み込む。本体は Rust（alpedia_core の modcode_*）。

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
from . import log, native

_c = native.require()
L = log.get("modcode")
PREFIX = "alpedia_mods"
CLIENT_LIBS = ("ursina", "panda3d", "direct", "main.client")

ModLoadError = _c.ModLoadError       # MOD やワールドが使っている MOD の中身に問題があり、ワールドを始められない（.errors）
_deferred = _c.modcode_deferred()    # 画面が無いと読み込めないモジュール（画面を作るときに読み込む）
_ClientGuard = _c.ClientGuard
reset = _c.modcode_reset
_where = _c.modcode_where
_client_ready = _c.modcode_client_ready
load = _c.modcode_load
load_client = _c.modcode_load_client
_package = _c.modcode_package
world_addons = _c.modcode_world_addons
client_addons = _c.modcode_client_addons

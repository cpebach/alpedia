"""MOD の情報（mods/<MOD名>/mod.json）と、MOD どうしの依存関係（中身は Rust：game/modmeta.rs）。

mod.json の項目：
    id / name / version / description
    builtin     本体に入っている MOD（一覧で先に並べる）
    requires    一緒に読み込む必要がある MOD の ID のリスト（無ければ読み込みのときに足す）
    world       この MOD を選んで「新しいワールド」を作ると、ワールドをこの形で作る（シナリオなど）：
                {"planets": [...] または "bodies": "default", "settings": {world.json に足す項目}, "note": "説明"}
                すでにあるワールドに後から入れても、ワールドの形は変わらない
    category    一覧の分け方（"ブロック"・"登場人物"・"場所"・"見た目" など。省略可）
    type        MOD の種類："scenario"（シナリオ：メニューの「シナリオ」で選んで始める。MOD の選択には出ない）、
                "addon"（要素を足す MOD：新規作成・ワールド設定の MOD の選択に出る。省略時）、
                "assets"（素材の置き場：BGM など。どちらの一覧にも出ず、ワールドで選ばなくても使われる）
"""
TYPES = ("addon", "scenario", "assets")
BUILTIN_MODES = ("survival",)    # 前の版では MOD だった本体の遊び方（mods/ に残っていても一覧に出さない）
from . import native

_C = native.require()


def read(mod_id, mods_dir=None):
    """mod.json の中身（無い・読めない MOD は最低限の項目だけ）。"""
    return _C.modmeta_read(mod_id, mods_dir)


def list_all(mods_dir=None, kind=None):
    """mods/ にある MOD の情報のリスト（kind="addon" / "scenario" でその種類だけ）。本体に入っている MOD を先に。"""
    return _C.modmeta_list_all(mods_dir, kind)


def is_scenario(mod_id, mods_dir=None):
    return _C.modmeta_is_scenario(mod_id, mods_dir)


def resolve(enabled, mods_dir=None):
    """選んだ MOD に、依存する MOD を足して読み込む順に並べる（依存する方が先）。
    戻り値：(並べた MOD のリスト, 足した MOD のリスト, 見つからない MOD の [(MOD, 必要とした MOD)] )"""
    return _C.modmeta_resolve(list(enabled), mods_dir)


def dependents(mod_id, selected, mods_dir=None):
    """selected の中で mod_id を（間接的にでも）必要とする MOD。"""
    return _C.modmeta_dependents(mod_id, list(selected), mods_dir)


def name_of(mod_id, mods_dir=None):
    return _C.modmeta_name_of(mod_id, mods_dir)


def world_preset(mods, mods_dir=None):
    """選んだ MOD のうち、ワールドの形を決めるもの（最初の1つ）：(MOD の ID, world の設定) か (None, None)。"""
    return _C.modmeta_world_preset(list(mods), mods_dir)

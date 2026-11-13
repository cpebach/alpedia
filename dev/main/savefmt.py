"""セーブデータの決まった形（保存形式）。中身は Rust（alpedia_core の game/savefmt.rs）。

saves/<ワールド名>/ の各ファイルに入れてよい項目と型は Rust の表で決める。
読み込み時に check_*() で調べ、形が違う所はログに出す（読める所は読む）。機器の状態の中身は devstate で決める。
"""
from . import native

FORMAT = 2     # 2: クリエイティブ／サバイバル・街の ON/OFF を world.json ではなく MOD（mods.json）で決める


def upgrade_world(settings, mods):
    """古いワールドの読み替え：(settings, mods, 読み替えたか)。"""
    s, m, changed = native.require().savefmt_upgrade_world(settings, list(mods))
    return s, m, changed


def check_world(s, extra_keys=()):
    return native.require().savefmt_check_world(s, [str(k) for k in extra_keys])


def check_construct(d):
    return native.require().savefmt_check_construct(d)


def check_spawn(sp):
    return native.require().savefmt_check_spawn(sp)


def check_player(d):
    return native.require().savefmt_check_player(d)

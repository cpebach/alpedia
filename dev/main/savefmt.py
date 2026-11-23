"""セーブデータの決まった形（保存形式）。中身は Rust（alpedia_core の game/savefmt.rs）。

saves/<ワールド名>/ の各ファイルに入れてよい項目と型は Rust の表で決める。
読み込み時に check_*() で調べ、形が違う所はログに出す（読める所は読む）。機器の状態の中身は devstate で決める。
"""
from . import native

# 3: いまの形式（前の形式のセーブは読み替えない。後方互換は持たない）
FORMAT = native.require().SAVE_FORMAT


def check_world(s, extra_keys=()):
    return native.require().savefmt_check_world(s, [str(k) for k in extra_keys])


def check_construct(d):
    return native.require().savefmt_check_construct(d)


def check_spawn(sp):
    return native.require().savefmt_check_spawn(sp)


def check_player(d):
    return native.require().savefmt_check_player(d)

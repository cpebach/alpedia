"""機器の状態の決まった形（スキーマ）。中身は Rust（alpedia_core の game/devstate.rs）。

機器（ブロック）ごとの状態 d["state"] に入れてよい項目は、機器の種類ごとに Rust の表で決める。
- 保存する項目と、計算中だけ使う項目（保存しない）を分ける
- 読み込み・設置のときに normalize() で、足りない項目を既定値で埋め、型をそろえ、知らない項目を捨てる
- "_" で始まる項目は、実行中だけの内部用（スクリプトの実行環境など）。保存も検査もしない
"""
from . import native


def fields(dtype):
    """その種類の項目：{名前: (型, 保存するか, 無くてよいか)}"""
    return native.require().devstate_fields(dtype)


def normalize(dtype, params, state, where=""):
    """状態をこの種類の決まった形にした新しい辞書を返す（元の辞書は変えない）。"""
    return native.require().devstate_normalize(dtype, params, state, where)


def fill(dtype, params, st):
    """足りない項目を既定値で埋める（その辞書をそのまま書き換える。動いている機器用）。"""
    return native.require().devstate_fill(dtype, params, st)


def to_saved(dtype, state):
    """保存する分だけ（計算中だけの項目と "_" で始まる項目を除く）。"""
    return native.require().devstate_to_saved(dtype, state)


def check(dtype, state):
    """決まった形になっているか（テスト用）：問題のリスト。"""
    return native.require().devstate_check(dtype, state)

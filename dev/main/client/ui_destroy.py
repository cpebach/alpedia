"""画面部品（ursina の Entity）を中身ごと消す。本体は Rust（alpedia_core.ui_destroy）。

ursina の destroy() は渡した部品だけを消し、子の部品（ボタン・文字など）は消さない。
消されなかった子は画面からは見えなくなるが部品一覧やマウスの当たり判定の一覧に残り続け、
毎フレームの処理が少しずつ重くなっていく。そこで子から順にまとめて消す。
"""
from .. import native

destroy = native.require().ui_destroy

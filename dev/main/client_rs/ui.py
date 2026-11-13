"""Rust の画面の UI の部品：板・文字・ボタン・画像（本体は Rust の alpedia_core.Screen など）。

座標は画面の上（縦の高さが 1、真ん中が原点、x 右・y 上）。画面（Screen）は部品を持ち、
マウスの位置・クリック・キーを受け取る。部品は作り直す（build）のが基本。
"""
from .. import native

_c = native.require()
item_tex = _c.ui_item_tex
rect_mesh = _c.ui_rect_mesh
Screen = _c.Screen
Field = _c.Field
UI = _c.UI

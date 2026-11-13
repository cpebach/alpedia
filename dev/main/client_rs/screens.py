"""Rust の画面の画面（一時停止メニュー・インベントリ）。本体は Rust（alpedia_core.Pause / Inventory）。"""
from .. import native

_c = native.require()
GREY = (0.75, 0.75, 0.75, 1)
Pause = _c.Pause
Inventory = _c.Inventory

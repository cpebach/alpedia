"""画面の上の部品（ursina と同じ書き方の Entity・Text・Button・InputField・mouse・camera.ui・color など）。
本体は Rust（alpedia_core の Entity など）。

プログラムの画面・メニュー・MOD の画面側は ursina の書き方で作られている（ursina 自体はもう使わない）。
このモジュールを sys.modules["ursina"] に入れておくと（install）、そのコードが Rust の画面に描かれる。
座標は ursina と同じ（画面の縦の高さが 1、真ん中が原点、x 右・y 上。z が小さいほど手前）。
"""
import math  # noqa: F401（from ursina import * で使えた名前）
import sys

import numpy as np  # noqa: F401

from .. import native
from ..client import gfx  # noqa: F401

_c = native.require()
globals().update(_c.ursina_names())


def install():
    """sys.modules["ursina"] をこのまねにする（Rust の画面のときだけ）。"""
    return _c.ursina_install(sys.modules[__name__])

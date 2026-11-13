"""Panda3D が入っていない PC で、MOD の画面側が使う panda3d.core の名前の一部をまねる（Rust の画面のとき）。
本体は Rust（alpedia_core.panda_names）。

Rust の画面の部品（gfx.RsNode）は、setTransparency・setAttrib・setMat などに渡された値を見て動くので、
ここでは値の入れ物（定数・行列・ファイル名）だけを用意する。描画そのもの（Geom など）は無い（import するとエラー）。
"""
import sys

from .. import native

_c = native.require()
globals().update(_c.panda_names())


def install():
    """本物の panda3d が無いときだけ、このまねを panda3d.core として入れる。"""
    return _c.panda_install(sys.modules[__name__])

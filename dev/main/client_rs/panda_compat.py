"""Panda3D が入っていない PC で、MOD の画面側が使う panda3d.core の名前の一部をまねる（Rust の画面のとき）。

Rust の画面の部品（gfx.RsNode）は、setTransparency・setAttrib・setMat などに渡された値を見て動くので、
ここでは値の入れ物（定数・行列・ファイル名）だけを用意する。描画そのもの（Geom など）は無い（import するとエラー）。
"""
import sys
import types


class TransparencyAttrib:
    M_none, M_alpha, M_premultiplied_alpha, M_multisample, M_binary, M_dual = range(6)
    MNone, MAlpha = M_none, M_alpha


class ColorBlendAttrib:
    M_none, M_add, M_subtract, M_inv_subtract, M_min, M_max = range(6)
    O_zero, O_one, O_incoming_color, O_one_minus_incoming_color, O_fbuffer_color, O_one_minus_fbuffer_color, \
        O_incoming_alpha, O_one_minus_incoming_alpha = range(8)

    def __init__(self, *a):
        self.args = a

    @classmethod
    def make(cls, *a):
        return cls(*a)


class SamplerState:
    FT_nearest, FT_linear, FT_nearest_mipmap_nearest, FT_linear_mipmap_nearest, FT_nearest_mipmap_linear, \
        FT_linear_mipmap_linear = range(6)
    WM_clamp, WM_repeat = 0, 1


class LMatrix4f(list):
    """4×4 行列（16 個の数。行ベクトルにかける形）。gfx.RsNode.setMat にそのまま渡せる。"""

    def __init__(self, *a):
        if len(a) == 1:
            a = list(a[0])
        super().__init__(float(x) for x in a)


LMatrix4 = LMatrix4f


class _Vec(tuple):
    def __new__(cls, *a):
        if len(a) == 1 and not isinstance(a[0], (int, float)):
            a = tuple(a[0])
        return super().__new__(cls, (float(x) for x in a))


class Vec3(_Vec):
    pass


class Vec4(_Vec):
    pass


Point3 = LPoint3f = LVector3f = LVecBase3f = Vec3
Point2 = Vec2 = _Vec


class Filename(str):
    @classmethod
    def fromOsSpecific(cls, p):
        return cls(str(p))

    def toOsSpecific(self):
        return str(self)


class WindowProperties:
    def __getattr__(self, name):
        return lambda *a, **k: None


def install():
    """本物の panda3d が無いときだけ、このまねを panda3d.core として入れる。"""
    try:
        import panda3d.core  # noqa: F401
        return False
    except ImportError:
        pass
    pkg = types.ModuleType("panda3d")
    pkg.__path__ = []
    core = sys.modules[__name__]
    pkg.core = core
    sys.modules["panda3d"] = pkg
    sys.modules["panda3d.core"] = core
    return True

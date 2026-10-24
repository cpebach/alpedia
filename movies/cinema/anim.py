"""時刻 → 値（キーフレーム・イージング・カメラの揺れ）と、向きの計算。"""
import math

import numpy as np


def clamp(x, lo=0.0, hi=1.0):
    return lo if x < lo else hi if x > hi else x


def lerp(a, b, k):
    return a + (b - a) * k


def remap(t, t0, t1):
    """t0〜t1 の t を 0〜1 に（外は端に張りつく）。"""
    return clamp((t - t0) / (t1 - t0)) if t1 != t0 else (1.0 if t >= t1 else 0.0)


# --- イージング ---------------------------------------------------------------
def linear(x):
    return x


def smooth(x):
    return x * x * (3 - 2 * x)


def smoother(x):
    return x * x * x * (x * (x * 6 - 15) + 10)


def ease_in(x):
    return x * x * x


def ease_out(x):
    return 1 - (1 - x) ** 3


def ease_in_out(x):
    return 4 * x * x * x if x < 0.5 else 1 - (-2 * x + 2) ** 3 / 2


def expo_out(x):
    return 1.0 if x >= 1 else 1 - 2 ** (-10 * x)


EASE = {"linear": linear, "smooth": smooth, "smoother": smoother, "in": ease_in, "out": ease_out,
        "inout": ease_in_out, "expo": expo_out, "hold": lambda x: 0.0}


class Track:
    """キーフレーム [(時刻, 値, イージング), ...]。値は数でもベクトルでもよい。
    イージングは「そのキーから次のキーまで」の動き方（省略時 smooth）。"""

    def __init__(self, *keys):
        ks = []
        for k in keys:
            t, v = k[0], k[1]
            e = k[2] if len(k) > 2 else "smooth"
            ks.append((float(t), np.asarray(v, dtype=np.float64), EASE[e] if isinstance(e, str) else e))
        ks.sort(key=lambda k: k[0])
        self.keys = ks

    def __call__(self, t):
        ks = self.keys
        if t <= ks[0][0]:
            return _out(ks[0][1])
        for (t0, v0, e), (t1, v1, _) in zip(ks, ks[1:]):
            if t < t1:
                return _out(v0 + (v1 - v0) * e(remap(t, t0, t1)))
        return _out(ks[-1][1])


def _out(v):
    return float(v) if v.ndim == 0 else v.copy()


def const(v):
    return Track((0, v))


# --- 揺れ（決まった値になる。乱数は使わない） ------------------------------------
def wobble(t, seed=0.0, hz=1.0):
    """-1〜1 くらいのなめらかな揺れ（いくつかの sin の和）。"""
    s = seed * 12.9898
    return (math.sin(t * hz * 2.1 + s) * 0.5 + math.sin(t * hz * 3.7 + s * 1.7) * 0.3
            + math.sin(t * hz * 7.3 + s * 2.3) * 0.2)


def shake(t, amount, hz=9.0, seed=1.0):
    """カメラの揺れ（位置のずれ）。"""
    return np.array([wobble(t, seed, hz), wobble(t, seed + 3.1, hz), wobble(t, seed + 7.7, hz)]) * amount


# --- 向き -------------------------------------------------------------------------
def unit(v):
    v = np.asarray(v, dtype=np.float64)
    n = np.linalg.norm(v)
    return v / n if n > 1e-12 else v


def look_basis(fwd, up=(0, 1, 0), roll=0.0):
    """前 fwd・上の目安 up → 行が (右, 上, 前) の回転行列（ゲームと同じ：右 = 上 × 前）。roll は前を軸に（度）。"""
    f = unit(fwd)
    u = np.asarray(up, dtype=np.float64)
    r = np.cross(u, f)
    if np.linalg.norm(r) < 1e-6:
        r = np.cross([0.0, 0.0, 1.0] if abs(f[2]) < 0.9 else [1.0, 0.0, 0.0], f)
    r = unit(r)
    u = np.cross(f, r)
    if roll:
        a = math.radians(roll)
        r, u = r * math.cos(a) + u * math.sin(a), u * math.cos(a) - r * math.sin(a)
    return np.stack([r, u, f])


def axis_angle(axis, deg):
    """軸 axis のまわりに deg 度回す回転行列（列ベクトルにかける形）。"""
    a = unit(axis)
    th = math.radians(deg)
    c, s = math.cos(th), math.sin(th)
    x, y, z = a
    return np.array([[c + x * x * (1 - c), x * y * (1 - c) - z * s, x * z * (1 - c) + y * s],
                     [y * x * (1 - c) + z * s, c + y * y * (1 - c), y * z * (1 - c) - x * s],
                     [z * x * (1 - c) - y * s, z * y * (1 - c) + x * s, c + z * z * (1 - c)]])


def yaw_pitch_roll(yaw=0.0, pitch=0.0, roll=0.0):
    """度。yaw は上 (+Y) まわり、pitch は右 (+X) まわり、roll は前 (+Z) まわり。列ベクトルにかける形の回転行列。"""
    return axis_angle((0, 1, 0), yaw) @ axis_angle((1, 0, 0), pitch) @ axis_angle((0, 0, 1), roll)

"""景色の箱（ボクセルではない、軽い建物・崖・街）と、惑星の表面の地図。シナリオ・場所の MOD が使う。

- 地図 Map：惑星の表面の座標 (x, z) [m]。原点の向き up0 と、その点での前 fwd0。
- Scenery：箱の集まり（1か所）。世界座標の箱 C (中心)・A (行が軸)・E (半分の大きさ)。
  惑星の地形（掘った跡は含まない）から毎回同じ形に作る（セーブには入らない）。プレイヤー・構築物は当たって上に乗れる。
- SceneryHost：Addon に混ぜると、self.scenes の箱に当たり判定・視線・プレイヤーの押し出しが付く。
  画面側は main/client/scenery_view.py の SceneView で描く。
"""
import numpy as np

from .geom import box_sdf, ray_boxes

# ---------------------------------------------------------------------------
# 色（景色の箱）
# ---------------------------------------------------------------------------
ROCK = (0.46, 0.42, 0.38)
ROCK2 = (0.40, 0.37, 0.34)
ROCK3 = (0.52, 0.47, 0.41)
CONC = (0.62, 0.62, 0.60)
DARK = (0.22, 0.23, 0.26)
METAL = (0.55, 0.58, 0.62)
WHITE = (0.88, 0.89, 0.90)
ORANGE = (0.95, 0.45, 0.12)
RED = (0.80, 0.16, 0.14)
NAVY = (0.12, 0.18, 0.35)
MOON_G = (0.50, 0.50, 0.53)
GLASS = (0.55, 0.78, 0.92)

WARM = (1.0, 0.82, 0.45)      # 明かり（窓・街灯）
CYAN = (0.25, 0.95, 1.0)
PINK = (1.0, 0.3, 0.78)
GREEN = (0.35, 1.0, 0.5)
LIME = (0.6, 1.0, 0.3)
REDL = (1.0, 0.2, 0.15)
OFF = (0.16, 0.16, 0.18)      # 消えている明かり


def unit(v):
    v = np.asarray(v, dtype=np.float64)
    return v / np.linalg.norm(v)


_unit = unit


class Map:
    """惑星の表面の地図。原点の向き up0 と、その点での前 fwd0。"""

    def __init__(self, planet, up0, fwd0, craters=()):
        self.pl = planet
        self.craters = np.asarray(craters, float).reshape(-1, 4)    # ストーリーの不時着の跡（惑星の中心から）
        self.up0 = _unit(up0)
        f = np.asarray(fwd0, float) - self.up0 * np.dot(fwd0, self.up0)
        self.fwd0 = _unit(f)
        self.right0 = np.cross(self.up0, self.fwd0)

    def dir(self, x, z):
        return _unit(self.up0 * self.pl.radius + self.right0 * x + self.fwd0 * z)

    def radius_at(self, dirs):
        """方向 (N,3) の地表の半径。元の地形から、不時着の跡（球）を削ったもの（プレイヤーが掘った所は含まない）。"""
        r = self.pl.surface_radius(dirs)
        for c in self.craters:
            dc = dirs @ c[:3]
            disc = dc * dc - c[:3] @ c[:3] + c[3] * c[3]
            ok = disc > 0
            lo = dc - np.sqrt(np.maximum(disc, 0))
            hi = dc + np.sqrt(np.maximum(disc, 0))
            cut = ok & (r > lo) & (r <= hi)
            r = np.where(cut, lo, r)
        return r

    def ground(self, x, z, lift=0.0):
        """地表の点と、その点の上方向。"""
        d = self.dir(x, z)
        r = float(self.radius_at(d[None])[0])
        return self.pl.center + d * (r + lift), d

    def frame(self, x, z, yaw=0.0):
        """(x, z) に立てた向き（列が 右・上・前 のワールド方向）。yaw は左回り（度）。"""
        up = self.dir(x, z)
        f = _unit(self.fwd0 - up * np.dot(self.fwd0, up))
        r = np.cross(up, f)
        a = np.radians(yaw)
        f2 = f * np.cos(a) - r * np.sin(a)
        r2 = np.cross(up, f2)
        return np.stack([r2, up, f2], axis=1)

    def height(self, P, up, base):
        """点 P (N,3) の地表の、base から up 方向の高さ。"""
        d = P - self.pl.center
        d /= np.linalg.norm(d, axis=1)[:, None]
        r = self.radius_at(d)
        S = self.pl.center + d * r[:, None]
        return (S - base) @ up


# ---------------------------------------------------------------------------
# 景色の箱
# ---------------------------------------------------------------------------
class Scenery:
    """景色の箱の集まり（1つの惑星の1か所）。世界座標の箱 C (中心)・A (行が軸)・E (半分の大きさ)。"""

    def __init__(self, sid, map_):
        self.id = sid
        self.map = map_
        self.parts = []
        self.points = {}

    def box(self, x, z, w, h, d, y0=0.0, yaw=0.0, color=CONC, glow=None, group=None, collide=True,
            blink=None, tag=None, ground=True, pitch=0.0, roll=0.0, sink=0.4):
        """地図の (x, z) に、幅 w・高さ h・奥行き d の箱。y0 は地面からの高さ（ground=False なら地図の原点の高さから）。
        glow：光る色（影をつけない）。group：同じ group の明かりはまとめて点けたり消したりできる。
        blink：点滅の周期（秒）。tag：触れる物の名前（E・近づくとボタン）。"""
        R = self.map.frame(x, z, yaw)
        up = R[:, 1]
        base = self.map.pl.center + up * self.map.pl.radius
        if ground:
            hw, hd = w / 2, d / 2
            pts = np.array([[0, 0], [-hw, -hd], [hw, -hd], [-hw, hd], [hw, hd], [-hw, 0], [hw, 0], [0, -hd], [0, hd]])
            P = base + np.outer(pts[:, 0], R[:, 0]) + np.outer(pts[:, 1], R[:, 2])
            hs = self.map.height(P, up, base)
            hc = float(hs[0])
            bottom = hc + y0 if y0 > 0.05 else min(float(hs.min()), hc + y0) - sink
            top = hc + y0 + h
        else:
            g0 = float(self.map.height(base[None], up, base)[0])
            bottom, top = g0 + y0, g0 + y0 + h
        if pitch or roll:
            p, r_ = np.radians(pitch), np.radians(roll)
            Rx = np.array([[1, 0, 0], [0, np.cos(p), -np.sin(p)], [0, np.sin(p), np.cos(p)]])
            Rz = np.array([[np.cos(r_), -np.sin(r_), 0], [np.sin(r_), np.cos(r_), 0], [0, 0, 1]])
            R = R @ Rx @ Rz
        c = base + up * ((bottom + top) / 2)
        self.parts.append({"c": c, "A": R.T.copy(), "e": np.array([w / 2, (top - bottom) / 2, d / 2]),
                           "color": np.array(color, float), "glow": None if glow is None else np.array(glow, float),
                           "group": group, "collide": collide, "blink": blink, "tag": tag})
        return len(self.parts) - 1

    def raw(self, c, R, e, color=CONC, glow=None, group=None, collide=True, blink=None, tag=None):
        """世界座標の中心 c・向き R（列が 右・上・前）・半分の大きさ e の箱。"""
        self.parts.append({"c": np.asarray(c, float), "A": np.asarray(R, float).T.copy(), "e": np.asarray(e, float),
                           "color": np.array(color, float), "glow": None if glow is None else np.array(glow, float),
                           "group": group, "collide": collide, "blink": blink, "tag": tag})
        return len(self.parts) - 1

    def point_raw(self, name, p):
        self.points[name] = np.asarray(p, float)
        return self.points[name]

    def point(self, name, x, z, y=0.0):
        p, up = self.map.ground(x, z)
        self.points[name] = p + up * y
        return self.points[name]

    def finish(self):
        P = self.parts
        self.C = np.array([p["c"] for p in P])
        self.A = np.array([p["A"] for p in P])
        self.E = np.array([p["e"] for p in P])
        self.col = np.array([p["collide"] for p in P], bool)
        self.center = self.C.mean(0)
        self.radius = float(np.max(np.linalg.norm(self.C - self.center, axis=1) + np.linalg.norm(self.E, axis=1))) + 1.0
        self.tags = {p["tag"]: k for k, p in enumerate(P) if p["tag"]}
        return self

    # 動かない物（当たり判定の持ち主として）
    def is_moving(self):
        return False

    def point_velocity(self, p):
        return np.zeros_like(np.asarray(p, float))


class SceneryHost:
    """Addon に混ぜる：self.scenes（Scenery のリスト）の当たり判定・視線・押し出し。"""
    scenes = ()
    scene_kind = "scenery"           # raycast の結果の "kind"（画面側がどの MOD の物か見分ける）

    def colliders(self, world, lo, hi, margin):
        out = []
        for s in self.scenes:
            ctr = (lo + hi) / 2
            if np.linalg.norm(ctr - s.center) > s.radius + np.linalg.norm(hi - lo) / 2 + margin:
                continue
            rb = np.linalg.norm(s.E, axis=1)
            m = s.col & np.all(s.C + rb[:, None] + margin > lo, axis=1) & np.all(s.C - rb[:, None] - margin < hi, axis=1)
            if m.any():
                idx = np.nonzero(m)[0]
                out.append((s, s.C[m], s.A[m], s.E[m], idx))
        return out

    def raycast(self, world, o, dvec, max_t):
        best = None
        for s in self.scenes:
            if np.linalg.norm(o - s.center) > s.radius + max_t:
                continue
            r = ray_boxes(o, dvec, s.C, s.A, s.E, max_t)
            if r and (best is None or r[0] < best["t"]):
                k = int(r[1])
                best = {"kind": self.scene_kind, "t": r[0], "scene": s, "part": k, "point": o + dvec * r[0],
                        "normal": r[2], "placeable": True, "tag": s.parts[k]["tag"]}
        return best

    def push_sphere(self, world, center, r):
        out = []
        for s, C, A, E, idx in self.colliders(world, center - r, center + r, 1.0):
            d, n, k = box_sdf(center[None], C, A, E)
            if d[0] < r:
                out.append((n[0] * (r - d[0]), n[0], s))
        return out

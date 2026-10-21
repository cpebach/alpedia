"""ストーリーの舞台：地図の座標、景色の箱（建物・崖・街）、構築物（ボクセル）の置き場所。

- 地図の座標 (x, z) [m]：テラは北極（ワールドの +Y）が原点で、+X が右、+Z が前（奥）。
  月ルナは発着場が原点で、+Z が「月の北」（ワールドの +Y の方）。
- 景色の箱は惑星の地形（掘った跡は含まない）から毎回同じ形に作る（セーブには入らない）。
  ボクセルではないので軽い。プレイヤー・構築物は当たって上に乗れる（街 MOD の建物と同じ仕組み）。
- 構築物は最初に読み込んだときに1回だけ作る（addon.build_world）。そのあとはふつうの構築物としてセーブされる。
"""
import numpy as np

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

LUNA_DIST = 4000.0            # 月の中心（ワールド (0, 0, 4000)）


def _unit(v):
    v = np.asarray(v, dtype=np.float64)
    return v / np.linalg.norm(v)


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


def terra_map(planet, craters=()):
    return Map(planet, (0, 1, 0), (0, 0, 1), craters)


PAD_TILT = 70.0               # 月の発着場：テラの真下の点から、月の北へこの角度（テラが地平線の上に低く見える）


def luna_pad_dir(terra_center, luna_center):
    """月の発着場の向き。テラ（ルミナス号の墜落地点の方）の真下の点から、月の北（ワールドの +Y）へ PAD_TILT 度。
    街からは南の空の低い所にテラが見える。"""
    p = np.asarray(terra_center, float) + np.array([0.0, 600.0, 150.0])
    u0 = _unit(p - np.asarray(luna_center, float))
    n = _unit(np.array([0.0, 1.0, 0.0]) - u0 * u0[1])
    a = np.radians(PAD_TILT)
    return _unit(u0 * np.cos(a) + n * np.sin(a))


def luna_map(planet, terra_center):
    u = luna_pad_dir(terra_center, planet.center)
    return Map(planet, u, (0, 1, 0))


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


# ---------------------------------------------------------------------------
# テラ：ポッドの不時着地点 → 尾根の谷 → 観測基地 → ルミナス号の墜落地点
# ---------------------------------------------------------------------------
Z_RIDGE = (20.0, 38.0)        # 尾根（谷）の奥行き
NOTCH = 3.5                   # 谷の半分の幅
Z_LEDGE = 37.0                # 谷の出口の段差
OUTPOST = 70.0                # 観測基地の中心
SHIP = (2.0, 150.0)           # ルミナス号の墜落地点

# 構築物の置き場所（地図の x, z, 向き（度）, 地面からの高さ）
PLACES = {
    "pod": (1.4, 1.0, 20.0, 0.3),
    "beacon": (3.5, 10.0, 180.0, 0.05),
    "rock1": (-2.4, 28.0, 0.0, 0.05),
    "rock2": (-0.3, 27.4, 0.0, 0.05),
    "rock3": (1.6, 28.4, 0.0, 0.05),
    "power": (-11.0, 52.0, 0.0, 0.05),
    "model": (-16.0, 64.0, 0.0, 0.05),
    "task": (-9.0, 64.0, 0.0, 0.05),
    "console": (8.0, 66.0, 0.0, 0.05),
    "drone": (12.0, 72.0, 0.0, 0.05),
    "gate": (-16.6, 93.5, 0.0, 0.05),
    "bike": (-13.0, 85.0, 0.0, 0.05),
    "ship": (SHIP[0], SHIP[1], 0.0, 0.05),
    "sign_base": (4.0, 47.0, 180.0, 0.05),
}
RING = (16.0, 76.0, 15.0)     # ドローンでくぐる輪（x, z, 高さ）
RING_R = 2.4


def terra_craters(m):
    """不時着の跡（球の形に地面を削る）：(惑星の中心から見た x, y, z, 半径) のリスト。"""
    out = []

    def cr(x, z, depth, r):
        p, up = m.ground(x, z)
        c = p + up * (r - depth) - m.pl.center
        out.append([float(c[0]), float(c[1]), float(c[2]), float(r)])
    cr(2.1, 1.7, 1.1, 4.0)                        # ポッド
    for k, z in enumerate((-6.0, -11.0, -15.0)):  # ポッドが滑った跡
        cr(0.3 - k * 0.2, z, 0.6 - k * 0.15, 2.2)
    cr(SHIP[0], SHIP[1], 1.6, 16.0)               # ルミナス号（広く浅いくぼ地）
    for k in range(6):                            # 船が滑った跡（溝）
        cr(SHIP[0] - 0.5 - k * 0.6, SHIP[1] - 9.0 - k * 4.5, 1.2 - k * 0.15, 3.6)
    return out


def _rng(seed):
    return np.random.default_rng(seed)


def build_terra(planet):
    m = terra_map(planet, terra_craters(terra_map(planet)))
    s = Scenery("terra", m)
    rng = _rng(11)
    # --- ポッドの不時着地点 ---
    s.box(-3.5, 4.5, 3.2, 0.12, 2.4, yaw=25, color=ORANGE, ground=True, sink=0.05)          # パラシュート
    s.box(-2.2, 6.0, 2.0, 0.1, 1.6, yaw=-10, color=WHITE, ground=True, sink=0.05)
    for k in range(5):                                                                   # 岩
        a = rng.uniform(0, 2 * np.pi)
        r = rng.uniform(7, 12)
        sz = rng.uniform(0.8, 1.6)
        s.box(np.cos(a) * r, np.sin(a) * r * 0.6 - 2, sz, sz * 0.8, sz * 1.1, yaw=rng.uniform(0, 90),
              color=ROCK3, pitch=rng.uniform(-8, 8))
    # --- 尾根（左右に長く続く岩の壁）と、真ん中の谷 ---
    z0, z1 = Z_RIDGE
    zc, dz = (z0 + z1) / 2, z1 - z0
    for side in (-1, 1):
        x = NOTCH + 1.6
        k = 0
        while x < 75:
            w = rng.uniform(6.0, 9.0) if k else 3.2
            h = rng.uniform(7.0, 11.0) if k else 8.5
            cx = side * (x + w / 2)
            col = (ROCK, ROCK2, ROCK3)[k % 3]
            s.box(cx, zc + rng.uniform(-1.5, 1.5) * (k > 0), w + 0.6, h, dz + rng.uniform(-2, 3) * (k > 0),
                  yaw=rng.uniform(-6, 6) * (k > 0), color=col)
            if k:          # 上の段（でこぼこに）
                s.box(cx + rng.uniform(-1, 1), zc + rng.uniform(-3, 3), w * 0.6, rng.uniform(2, 4), dz * 0.5,
                      y0=h - 0.3, yaw=rng.uniform(-20, 20), color=ROCK2)
            x += w
            k += 1
        # 谷の内側の壁（でこぼこ）
        for z in np.arange(z0 + 1, z1, 3.0):
            s.box(side * (NOTCH + 0.6), z, 1.2, rng.uniform(5.5, 7.5), 3.2, yaw=rng.uniform(-8, 8), color=ROCK3)
    # 谷の出口の段差（ジェットパックで越える）
    s.box(0.0, Z_LEDGE, NOTCH * 2 + 0.6, 3.2, 1.6, color=ROCK2)
    s.box(0.0, Z_LEDGE + 1.6, NOTCH * 2 + 0.6, 3.2, 1.6, color=ROCK)
    s.point("ledge", 0.0, Z_LEDGE + 0.8, 3.4)
    s.point("rocks", 0.0, 28.0, 1.0)
    # --- 観測基地 ---
    zb = OUTPOST
    for x, z, w, d in ((0.0, 48.5, 3.0, 9.0), (0.0, 60.0, 3.0, 14.0), (0.0, 79.0, 3.0, 24.0),
                       (-8.0, 62.0, 13.0, 2.4), (8.0, 62.0, 13.0, 2.4), (-7.0, 92.0, 14.0, 3.0),
                       (0.0, 118.0, 2.6, 30.0)):
        s.box(x, z, w, 0.18, d, color=CONC, sink=0.25)                 # 道
    # 本館（明かりは発電を始めると点く）
    s.box(10.0, 52.0, 9.0, 4.6, 7.0, color=(0.70, 0.71, 0.70))
    s.box(10.0, 52.0, 9.6, 0.4, 7.6, y0=4.6, color=DARK)
    for k in range(4):
        s.box(6.2 + k * 2.4, 48.45, 1.4, 1.1, 0.12, y0=2.4, color=OFF, glow=WARM, group="base")
    s.box(10.0, 48.45, 1.6, 2.2, 0.12, y0=0.0, color=DARK, glow=(0.95, 0.75, 0.35), group="base")  # 扉
    s.box(12.5, 54.0, 0.3, 4.0, 0.3, y0=5.0, color=METAL)                                    # 屋根のアンテナ
    s.box(12.5, 54.0, 0.5, 0.5, 0.5, y0=9.0, color=OFF, glow=REDL, group="base", blink=1.2)
    s.box(10.0, 48.42, 6.0, 0.6, 0.1, y0=3.6, color=OFF, glow=CYAN, group="base")             # 看板の光
    # 電源設備の小屋（屋根だけ）
    s.box(-11.0, 53.0, 8.0, 0.3, 5.0, y0=3.2, color=DARK)
    for x, z in ((-14.8, 50.7), (-7.2, 50.7), (-14.8, 55.3), (-7.2, 55.3)):
        s.box(x, z, 0.3, 3.2, 0.3, color=METAL)
    # 試験台の作業小屋（屋根と柱）
    s.box(-12.5, 65.5, 12.0, 0.3, 7.0, y0=5.6, color=(0.30, 0.34, 0.40))
    for x, z in ((-18.3, 62.3), (-6.7, 62.3), (-18.3, 68.7), (-6.7, 68.7)):
        s.box(x, z, 0.3, 5.6, 0.3, color=METAL)
    s.box(-12.5, 62.1, 11.0, 0.15, 0.1, y0=5.5, color=OFF, glow=WARM, group="base")
    # ドローン発着場
    s.box(12.0, 72.0, 4.0, 0.15, 4.0, color=DARK, sink=0.2)
    for x, z in ((10.1, 70.1), (13.9, 70.1), (10.1, 73.9), (13.9, 73.9)):
        s.box(x, z, 0.3, 0.12, 0.3, y0=0.15, color=OFF, glow=LIME, group="base", collide=False)
    # 電波塔と、ドローンでくぐる輪
    tx, tz = 20.0, 80.0
    for dx, dz in ((-1.2, -1.2), (1.2, -1.2), (-1.2, 1.2), (1.2, 1.2)):
        s.box(tx + dx, tz + dz, 0.35, 20.0, 0.35, color=METAL, roll=-dx * 2.2, pitch=dz * 2.2)
    for y in (5.0, 10.0, 15.0):
        s.box(tx, tz, 2.4, 0.25, 2.4, y0=y, color=METAL)
    s.box(tx, tz, 0.3, 4.0, 0.3, y0=20.0, color=METAL)
    s.box(tx, tz, 0.6, 0.6, 0.6, y0=24.0, color=OFF, glow=REDL, blink=1.0)
    rx, rz, ry = RING
    for k in range(24):
        a = 2 * np.pi * k / 24
        s.box(rx + np.cos(a) * RING_R, rz + np.sin(a) * RING_R, 0.7, 0.3, 0.3, y0=ry, yaw=-np.degrees(a) + 90,
              color=OFF, glow=CYAN, collide=False, blink=2.0, ground=True)
    s.point("ring", rx, rz, ry + 0.1)
    # 格納庫（正面は +Z の側。入口に遮断機）
    hx, hz = -13.0, 86.0
    s.box(hx - 4.3, hz, 0.5, 5.0, 12.0, color=(0.56, 0.60, 0.66))
    s.box(hx + 4.3, hz, 0.5, 5.0, 12.0, color=(0.56, 0.60, 0.66))
    s.box(hx, hz - 6.0, 9.1, 5.0, 0.5, color=(0.56, 0.60, 0.66))
    s.box(hx, hz, 9.6, 0.5, 12.6, y0=5.0, color=DARK)
    s.box(hx, hz + 6.15, 8.0, 0.25, 0.1, y0=4.6, color=OFF, glow=(1.0, 0.75, 0.2), group="base")
    s.box(hx, hz, 8.0, 0.1, 11.0, color=(0.40, 0.42, 0.45), sink=0.2)
    s.box(-10.0, 94.75, 2.2, 2.0, 0.4, color=(0.85, 0.75, 0.2))            # 遮断機の横の柵
    # 街灯
    for x, z in ((2.2, 50.0), (-2.2, 58.0), (2.2, 66.0), (-2.2, 74.0), (2.2, 82.0), (-2.2, 90.0),
                 (1.8, 104.0), (-1.8, 118.0), (1.8, 132.0)):
        s.box(x, z, 0.18, 3.4, 0.18, color=METAL)
        s.box(x, z, 0.45, 0.3, 0.45, y0=3.4, color=OFF, glow=WARM, group="base")
    # ソーラーパネル
    for k in range(4):
        s.box(-22.0, 48.0 + k * 4.0, 3.0, 0.1, 2.0, y0=1.0, color=NAVY, pitch=-25)
        s.box(-22.0, 48.0 + k * 4.0, 0.15, 1.0, 0.15, color=METAL)
    s.point("ring_view", 12.0, 66.0, 1.6)
    # --- ルミナス号の墜落地点：飛び散った破片と、焦げた跡 ---
    sx, sz = SHIP
    for k in range(9):
        a = rng.uniform(-1.2, 1.2) + (np.pi if k % 3 == 0 else 0)
        r = rng.uniform(5.5, 11)
        col = (RED, WHITE, DARK)[k % 3]
        s.box(sx + np.sin(a) * r, sz + np.cos(a) * r, rng.uniform(0.4, 1.0), rng.uniform(0.06, 0.2),
              rng.uniform(0.3, 0.9), yaw=rng.uniform(0, 180), pitch=rng.uniform(-25, 25), roll=rng.uniform(-25, 25),
              color=col, sink=0.15)
    s.box(sx + 7.5, sz - 3.0, 0.2, 2.0, 0.2, color=METAL)                             # 目印の旗
    s.box(sx + 7.5, sz - 3.0, 0.05, 0.6, 1.0, y0=1.4, color=ORANGE)
    return s.finish()


# ---------------------------------------------------------------------------
# 月：ルナ・ポート（発着場・管制塔・ドーム・住居・温室）
# ---------------------------------------------------------------------------
PAD_HALF = 6.0


def _dome(s, x, z, r, color, glow, n=6, tag=None):
    for k in range(n):
        y = r * k / n
        hw = r * np.sqrt(max(1 - (k / n) ** 2, 0.02))
        s.box(x, z, hw * 2, r / n + 0.05, hw * 2, y0=y if k else 0.0, yaw=22.5 * (k % 2), color=color)
    s.box(x, z, r * 2 + 0.3, 0.25, 0.3, y0=r * 0.25, color=OFF, glow=glow, collide=False)
    s.box(x, z, 0.3, 0.25, r * 2 + 0.3, y0=r * 0.25, color=OFF, glow=glow, collide=False)
    if tag:
        s.box(x, z - r - 0.1, 2.0, 2.2, 0.4, color=DARK, glow=(glow[0] * 0.8, glow[1] * 0.8, glow[2] * 0.8),
              tag=tag)


def pad_frame(luna, terra, top_point, fwd0):
    """発着場の向き：上の面が、月とテラの重力を合わせた向きに垂直（テラに引かれて船がすべらないように）。"""
    def g(pl):
        d = pl.center - top_point
        return d * pl.gm / float(np.linalg.norm(d)) ** 3
    up = _unit(-(g(luna) + g(terra)))
    f = _unit(fwd0 - up * np.dot(fwd0, up))
    return np.stack([np.cross(up, f), up, f], axis=1)


def build_luna(planet, terra):
    m = luna_map(planet, terra.center)
    s = Scenery("luna", m)
    rng = _rng(23)
    # 発着場（平らな台。上の面は、下の地面のいちばん高い所より少し上）
    g = np.linspace(-PAD_HALF, PAD_HALF, 7)
    P = np.array([m.ground(x, z)[0] for x in g for z in g])
    base = m.pl.center + m.up0 * m.pl.radius
    hs = m.height(P, m.up0, base)
    g0 = float(m.height(base[None], m.up0, base)[0])
    top = float(hs.max()) - g0 + 0.6
    T = base + m.up0 * (g0 + top)
    R = pad_frame(planet, terra, T, m.fwd0)
    up = R[:, 1]
    depth = top + 4.0
    s.raw(T - up * depth / 2, R, (PAD_HALF, depth / 2, PAD_HALF), color=(0.30, 0.31, 0.34))
    s.raw(T + up * 0.03, R, (3.0, 0.03, 0.25), color=OFF, glow=(1.0, 0.85, 0.2), collide=False)
    s.raw(T + up * 0.03, R, (0.25, 0.03, 3.0), color=OFF, glow=(1.0, 0.85, 0.2), collide=False)
    for k in range(12):
        a = 2 * np.pi * k / 12
        c = T + up * 0.06 + R[:, 0] * np.cos(a) * (PAD_HALF - 0.6) + R[:, 2] * np.sin(a) * (PAD_HALF - 0.6)
        s.raw(c, R, (0.18, 0.06, 0.18), color=OFF, glow=CYAN, blink=1.6, collide=False)
    s.points["pad"] = T
    s.points["pad_up"] = up
    # 発着場までの誘導灯
    for k in range(6):
        s.box(0.0, -PAD_HALF - 2.0 - k * 2.5, 0.4, 0.3, 0.4, color=OFF, glow=REDL, blink=0.8, collide=False)
    # 管制塔（ミオがいる）
    tx, tz = -15.0, 2.0
    s.box(tx, tz, 6.0, 4.0, 6.0, color=(0.80, 0.82, 0.85))
    s.box(tx, tz, 3.0, 12.0, 3.0, y0=4.0, color=(0.75, 0.77, 0.80))
    s.box(tx, tz, 7.0, 3.0, 7.0, y0=16.0, color=(0.25, 0.27, 0.32))
    s.box(tx, tz, 7.2, 1.4, 7.2, y0=16.8, color=OFF, glow=CYAN, collide=False)
    s.box(tx, tz, 7.6, 0.4, 7.6, y0=19.0, color=WHITE)
    s.box(tx, tz, 0.4, 3.0, 0.4, y0=19.4, color=METAL)
    s.box(tx, tz, 0.6, 0.6, 0.6, y0=22.4, color=OFF, glow=REDL, blink=1.0)
    s.box(tx + 3.05, tz, 0.15, 2.4, 2.0, color=DARK, glow=(0.95, 0.8, 0.5))          # 入口
    s.box(tx + 3.3, tz + 2.0, 0.6, 1.1, 0.8, color=(0.25, 0.3, 0.4), glow=(0.5, 0.95, 1.0), tag="mio")  # 受付
    s.point("mio", tx + 4.2, tz + 2.0, 0.0)
    s.box(tx + 3.1, tz, 0.1, 0.6, 5.0, y0=3.0, color=OFF, glow=PINK)                  # 看板の光
    # ドーム（居住・温室・研究）
    _dome(s, 14.0, 8.0, 7.0, (0.70, 0.84, 0.92), CYAN)
    _dome(s, 4.0, 22.0, 9.0, (0.62, 0.86, 0.66), GREEN)
    _dome(s, -14.0, 20.0, 5.5, (0.88, 0.80, 0.92), PINK)
    # 温室の中が見えるように、ドームの前に光る畑
    for k in range(4):
        s.box(-1.0 + k * 3.3, 12.4, 2.6, 0.25, 1.0, color=(0.2, 0.35, 0.18), glow=(0.35, 0.9, 0.35), collide=False)
    # 住居ブロックと通路
    for k, (x, z) in enumerate(((20.0, -6.0), (26.0, 2.0), (22.0, 16.0))):
        s.box(x, z, 5.0, 3.0, 4.0, color=(0.78, 0.79, 0.80))
        s.box(x, z - 2.05, 3.6, 0.6, 0.1, y0=1.5, color=OFF, glow=(WARM, CYAN, PINK)[k])
        s.box(x, z, 5.4, 0.3, 4.4, y0=3.0, color=DARK)
    for (x0, z0), (x1, z1) in (((14.0, 1.0), (20.0, -4.0)), ((21.0, 8.0), (26.0, 4.0)), ((18.0, 12.0), (21.0, 14.0)),
                               ((8.0, 14.0), (8.5, 10.0)), ((-9.0, 18.0), (-3.0, 18.0)), ((-12.0, 5.0), (-13.0, 15.0))):
        cx, cz = (x0 + x1) / 2, (z0 + z1) / 2
        L_ = float(np.hypot(x1 - x0, z1 - z0))
        yaw = -np.degrees(np.arctan2(x1 - x0, z1 - z0))
        s.box(cx, cz, 1.6, 1.8, L_, yaw=yaw, color=(0.85, 0.86, 0.88))
        s.box(cx, cz, 0.2, 0.15, L_, y0=1.8, yaw=yaw, color=OFF, glow=CYAN, collide=False)
    # ソーラーパネルと電波の皿
    for k in range(5):
        s.box(-26.0, -10.0 + k * 4.2, 3.4, 0.1, 2.2, y0=1.2, color=NAVY, pitch=30)
        s.box(-26.0, -10.0 + k * 4.2, 0.15, 1.2, 0.15, color=METAL)
    s.box(10.0, -16.0, 0.4, 4.0, 0.4, color=METAL)
    s.box(10.0, -16.0, 4.0, 0.3, 4.0, y0=4.0, pitch=35, color=WHITE)
    s.box(10.0, -16.0, 0.3, 0.3, 0.3, y0=5.4, color=OFF, glow=REDL, blink=1.4)
    # 止めてある探査車（ローバー）
    for k, (x, z, yaw) in enumerate(((6.0, -10.0, 30.0), (-6.0, -11.0, -20.0))):
        R = m.frame(x, z, yaw)
        s.box(x, z, 2.0, 0.8, 3.0, y0=0.55, yaw=yaw, color=(ORANGE, WHITE)[k])
        s.box(x, z, 1.6, 0.6, 1.2, y0=1.35, yaw=yaw, color=GLASS)
        for dx, dz in ((-1.1, -1.0), (1.1, -1.0), (-1.1, 1.0), (1.1, 1.0)):
            off = R[:, 0] * dx + R[:, 2] * dz
            wx = x + float(off @ m.right0)
            wz = z + float(off @ m.fwd0)
            s.box(wx, wz, 0.35, 0.9, 0.9, yaw=yaw, color=(0.1, 0.1, 0.1))
        s.box(x, z, 0.3, 0.15, 0.1, y0=1.2, yaw=yaw, color=OFF, glow=WARM)
    # 岩
    for k in range(14):
        a = rng.uniform(0, 2 * np.pi)
        r = rng.uniform(32, 55)
        sz = rng.uniform(1.0, 3.0)
        s.box(np.cos(a) * r, np.sin(a) * r, sz, sz * 0.7, sz * 1.2, yaw=rng.uniform(0, 90), color=MOON_G,
              pitch=rng.uniform(-10, 10))
    return s.finish()


# ---------------------------------------------------------------------------
# 構築物（ボクセル）
# ---------------------------------------------------------------------------
# ルミナス号（5×5、前 = +Z、右 = +X、上 = +Y）。下の段の四隅と各辺の真ん中が推進機
SHIP_THRUSTERS = [(0, 0, 0), (4, 0, 0), (0, 0, 4), (4, 0, 4)]                 # 墜落でなくなった推進機
SHIP_THRUSTERS_OK = [(2, 0, 0), (0, 0, 2), (4, 0, 2), (2, 0, 4)]
SHIP_HOLES = [(0, 3, 2), (4, 3, 1), (1, 2, 0), (2, 1, 0), (4, 1, 3), (-1, 1, 2)]   # 墜落でなくなった外壁
SHIP_EMPTY_TANKS = [(1, 1, 1), (3, 1, 2), (2, 1, 3)]
SHIP_SEAT = (2, 2, 2)
SHIP_NAVI = (2, 2, 3)


def ship_voxels(broken=False):
    V = []
    for x in range(5):
        for z in range(5):
            if (x, 0, z) in SHIP_THRUSTERS or (x, 0, z) in SHIP_THRUSTERS_OK:
                V.append(((x, 0, z), "steel", {"def": "thruster", "dir": 2, "name": "推進機"}))
            else:
                V.append(((x, 0, z), "steel", {"def": "pipe", "dir": 2}))
            inner = 1 <= x <= 3 and 1 <= z <= 3
            if inner:
                st = {} if broken and (x, 1, z) in SHIP_EMPTY_TANKS else {"contents": {"fuel": 800.0}}
                V.append(((x, 1, z), "steel", {"def": "tank", "dir": 2, "name": "燃料タンク", "state": st}))
            else:
                V.append(((x, 1, z), "hull_red" if z in (0, 4) else "hull_white", None))
    deck = {(2, 2): {"def": "seat", "dir": 4, "name": "操縦席"},
            (2, 3): {"def": "computer", "dir": 5, "name": "航法コンピューター",
                     "state": {"script": "" if broken else "moon.py"}},
            (1, 1): {"def": "battery", "dir": 2, "name": "バッテリー"},
            (3, 1): {"def": "antenna", "dir": 2, "name": "アンテナ"}}
    for x in range(5):
        for z in range(5):
            d = deck.get((x, z))
            V.append(((x, 2, z), "steel" if d else "hull_white", d))
    # 風よけ（前はガラス）と、後ろ・横の囲い
    V += [((x, 3, 4), "glass", None) for x in (1, 2, 3)]
    V += [((0, 3, 4), "hull_red", None), ((4, 3, 4), "hull_red", None)]
    V += [((x, 3, z), "hull_white", None) for x in (0, 4) for z in (1, 2, 3)]
    V += [((x, 3, 0), "hull_white", None) for x in (1, 2, 3)]
    V.append(((2, 4, 0), "hull_red", None))
    # ひれ
    V += [((-1, 0, 2), "hull_red", None), ((5, 0, 2), "hull_red", None), ((2, 0, -1), "hull_red", None),
          ((2, 0, 5), "hull_red", None), ((-1, 1, 2), "hull_white", None), ((5, 1, 2), "hull_white", None)]
    if broken:
        V = [v for v in V if v[0] not in SHIP_HOLES and v[0] not in SHIP_THRUSTERS]
    return V


def pod_voxels():
    """脱出ポッド（3×3×3 の角を落とした形、オレンジと白、窓、記録の表示）。"""
    V = []
    for x in range(3):
        for y in range(3):
            for z in range(3):
                if sum(c in (0, 2) for c in (x, y, z)) == 3:
                    continue                                   # 角を落として丸く
                mat = "hull_orange" if y == 1 else "hull_white"
                if (x, y, z) == (1, 1, 2):
                    mat = "glass"
                if (x, y, z) == (1, 2, 0):
                    continue
                V.append(((x, y, z), mat, None))
    V.append(((1, 2, 0), "steel", {"def": "computer", "dir": 5, "name": "ポッドの記録", "state": {"display": {
        "text": "脱出ポッド 3号\n記録：ルミナス号から緊急分離\n着地：惑星テラ", "face": 5, "width": 1.8,
        "color": [1.0, 0.8, 0.5], "background": [0.12, 0.05, 0.02, 0.92]}}}))
    return V


BEACON_SCRIPT = '''"""救難ビーコン：起動すると救難信号を送り、ランプを点滅させる。"""


def tick(api):
    on = int(api.time * 2) % 2 == 0
    api.display("救難ビーコン\\n" + ("● 送信中 SOS" if on else "○ 送信中 SOS"), width=1.4,
                color=(1.0, 0.35, 0.3) if on else (0.6, 0.25, 0.2), background=(0.08, 0.02, 0.02))
'''

POWER_SCRIPT = '''"""観測基地の電源の状態表示"""


def tick(api):
    p = api.power
    valves = api.devices("valve")
    gens = api.devices("generator")
    fuel = sum(t.contents.get("fuel", 0) for t in api.devices("tank"))
    v = "開" if valves and valves[0].open else "閉"
    g = "運転中" if gens and gens[0].enabled else "停止"
    api.display(f"観測基地 電源\\nバルブ {v}　発電機 {g}\\n発電 {p['production'] / 1000:.1f} kW\\n燃料 {fuel:.0f} kg",
                width=1.6, color=(1.0, 0.9, 0.5))
'''


def beacon_voxels():
    V = [((x, 0, z), "rock", None) for x in range(3) for z in range(3)]
    V += [((1, y, 1), "steel", None) for y in (1, 2)]
    V.append(((1, 3, 1), "steel", {"def": "battery", "dir": 2, "name": "バッテリー"}))
    V.append(((1, 4, 1), "steel", {"def": "computer", "dir": 4, "name": "救難ビーコン",
                                   "state": {"script": "beacon.py", "display": {
                                       "text": "救難ビーコン\n（E で起動）", "face": 4, "width": 1.4,
                                       "color": [1, 0.8, 0.6], "background": [0.08, 0.02, 0.02, 0.9]}}}))
    V.append(((1, 5, 1), "aluminum", {"def": "antenna", "dir": 2, "name": "アンテナ"}))
    V.append(((1, 6, 1), "hull_red", None))
    return V


def rock_voxels(seed, w=4, h=4, d=3):
    rng = _rng(seed)
    V = []
    for x in range(w):
        for y in range(h):
            for z in range(d):
                cx, cy, cz = (x - (w - 1) / 2) / w, y / h, (z - (d - 1) / 2) / d
                if cx * cx * 2 + cy * cy + cz * cz * 2 > 0.95 + rng.uniform(-0.15, 0.1):
                    continue
                V.append(((x, y, z), "rock", None))
    return V


def power_voxels():
    """観測基地の電源：タンク → バルブ（閉）→ 配管 → 発電機（停止）→ バッテリー（空に近い）＋状態の表示。"""
    V = [((x, 0, z), "rock", None) for x in range(-1, 7) for z in range(-1, 2)]
    V += [((0, 1, 0), "steel", {"def": "tank", "dir": 2, "name": "燃料タンク", "state": {"contents": {"fuel": 800.0}}}),
          ((0, 2, 0), "steel", {"def": "tank", "dir": 2, "name": "燃料タンク", "state": {"contents": {"fuel": 800.0}}}),
          ((1, 1, 0), "steel", {"def": "valve", "dir": 0, "name": "バルブ", "state": {"open": False}}),
          ((2, 1, 0), "steel", {"def": "pipe", "dir": 0}),
          ((3, 1, 0), "steel", {"def": "pipe", "dir": 0}),
          ((4, 1, 0), "steel", {"def": "generator", "dir": 2, "name": "発電機", "state": {"enabled": False}}),
          ((5, 1, 0), "steel", {"def": "battery", "dir": 2, "name": "基地のバッテリー", "state": {"charge": 4.0e5}}),
          ((5, 2, 0), "steel", {"def": "computer", "dir": 5, "name": "電源の表示",
                                "state": {"script": "power.py", "running": True}})]
    return V


def gate_voxels():
    """格納庫の遮断機：台・柱・バッテリー・モーター（出力は +Z の面）、その先の横棒（+X へ）。"""
    V = [((x, 0, z), "rock", None) for x in range(-1, 2) for z in range(-1, 2)]
    V += [((0, y, 0), "steel", None) for y in (1, 2)]
    V.append(((0, 3, 0), "steel", {"def": "battery", "dir": 2, "name": "バッテリー"}))
    V.append(((0, 3, 1), "steel", {"def": "actuator", "dir": 4, "name": "遮断機のモーター",
                                   "state": {"preset": {"action": "rotate", "amount": 90.0, "speed": 40.0}}}))
    V.append(((0, 3, 2), "bar_red", None))
    V += [((x, 3, 2), "bar_red" if (x // 2) % 2 == 0 else "bar_white", None) for x in range(1, 10)]
    return V


def bike_voxels():
    """ホバーバイク（前 = +Z）。四隅が推進機、真ん中に座席。"""
    V = []
    corners = {(0, 0), (2, 0), (0, 4), (2, 4)}
    for x in range(3):
        for z in range(5):
            if (x, z) in corners:
                V.append(((x, 0, z), "steel", {"def": "thruster", "dir": 2, "name": "推進機"}))
            elif x == 1:
                V.append(((x, 0, z), "steel", {"def": "pipe", "dir": 2}))
            else:
                V.append(((x, 0, z), "hull_blue", None))
    V += [((1, 1, 1), "steel", {"def": "tank", "dir": 2, "name": "燃料タンク", "state": {"contents": {"fuel": 800.0}}}),
          ((1, 1, 2), "steel", {"def": "seat", "dir": 4, "name": "バイクの座席"}),
          ((1, 1, 0), "steel", {"def": "battery", "dir": 2, "name": "バッテリー"}),
          ((1, 1, 4), "steel", {"def": "computer", "dir": 5, "name": "バイクの制御",
                                "state": {"script": "bike.py", "running": True}}),
          ((1, 1, 3), "glass", None)]
    return V


def sign_voxels(text, width=3.0, height=3, color=(1, 1, 1), bg=(0.05, 0.07, 0.12, 0.93)):
    v = [((x, 0, z), "rock", None) for x in range(-1, 2) for z in range(-1, 2)]
    v += [((0, y, 0), "steel", None) for y in range(1, height + 1)]
    v.append(((0, height + 1, 0), "steel", {
        "def": "computer", "dir": 5, "name": "看板",
        "state": {"display": {"text": text, "face": 5, "width": width, "color": list(color), "background": list(bg)}}}))
    return v

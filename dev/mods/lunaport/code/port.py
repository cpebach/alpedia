"""ルナ・ポートの舞台：月の発着場・管制塔・ドーム・住居・温室（景色の箱）。

月の座標 (x, z) は発着場が原点で、+Z が「月の北」（ワールドの +Y の方）。発着場の上の面は、月とガイアの重力を合わせた
向きに垂直（ガイアに引かれて船がすべらないように）。ガイア（主な惑星）からの高さ・向きで場所を決めるので、
月とガイア（または主な惑星）があるワールドなら、どのワールドでも同じ形になる。
"""
import numpy as np

from main.scenery import (CYAN, DARK, GLASS, GREEN, METAL, MOON_G, NAVY, OFF, ORANGE, PINK, REDL, WARM, WHITE,
                          Map, Scenery, unit as _unit)


def _rng(seed):
    return np.random.default_rng(seed)


PAD_TILT = 70.0               # 月の発着場：ガイアの真下の点から、月の北へこの角度（ガイアが地平線の上に低く見える）


def luna_pad_dir(terra_center, luna_center):
    """月の発着場の向き。ガイア（ルミナス号の墜落地点の方）の真下の点から、月の北（ワールドの +Y）へ PAD_TILT 度。
    街からは南の空の低い所にガイアが見える。"""
    p = np.asarray(terra_center, float) + np.array([0.0, 600.0, 150.0])
    u0 = _unit(p - np.asarray(luna_center, float))
    n = _unit(np.array([0.0, 1.0, 0.0]) - u0 * u0[1])
    a = np.radians(PAD_TILT)
    return _unit(u0 * np.cos(a) + n * np.sin(a))


def luna_map(planet, terra_center):
    u = luna_pad_dir(terra_center, planet.center)
    return Map(planet, u, (0, 1, 0))


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
    """発着場の向き：上の面が、月とガイアの重力を合わせた向きに垂直（ガイアに引かれて船がすべらないように）。"""
    def g(pl):
        d = pl.center - top_point
        return d * pl.gm / float(np.linalg.norm(d)) ** 3
    up = _unit(-(g(luna) + g(terra)))
    f = _unit(fwd0 - up * np.dot(fwd0, up))
    return np.stack([np.cross(up, f), up, f], axis=1)


def build(planet, terra):
    """ルナ・ポートの景色の箱（月 planet の、ガイア terra の方の地平線が見える所）。"""
    m = luna_map(planet, terra.center)
    s = Scenery("lunaport", m)
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
    # 管制塔（受付に管制官がいる）
    tx, tz = -15.0, 2.0
    s.box(tx, tz, 6.0, 4.0, 6.0, color=(0.80, 0.82, 0.85))
    s.box(tx, tz, 3.0, 12.0, 3.0, y0=4.0, color=(0.75, 0.77, 0.80))
    s.box(tx, tz, 7.0, 3.0, 7.0, y0=16.0, color=(0.25, 0.27, 0.32))
    s.box(tx, tz, 7.2, 1.4, 7.2, y0=16.8, color=OFF, glow=CYAN, collide=False)
    s.box(tx, tz, 7.6, 0.4, 7.6, y0=19.0, color=WHITE)
    s.box(tx, tz, 0.4, 3.0, 0.4, y0=19.4, color=METAL)
    s.box(tx, tz, 0.6, 0.6, 0.6, y0=22.4, color=OFF, glow=REDL, blink=1.0)
    s.box(tx + 3.05, tz, 0.15, 2.4, 2.0, color=DARK, glow=(0.95, 0.8, 0.5))          # 入口
    s.box(tx + 3.3, tz + 2.0, 0.6, 1.1, 0.8, color=(0.25, 0.3, 0.4), glow=(0.5, 0.95, 1.0), tag="desk")  # 受付
    s.point("desk", tx + 4.2, tz + 2.0, 0.0)
    s.point("plaza", 6.0, -2.0, 0.0)
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

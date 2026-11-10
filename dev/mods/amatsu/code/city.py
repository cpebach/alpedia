"""軌道都市アマツの形（景色の箱）。

主な惑星の中心から DIST の高さに、円筒の軸を地平に沿わせて浮かべる（上の外壁は惑星と反対を向く）。
甲板の座標 (x, z)：甲板の真ん中が原点、+Z が円筒の軸の向き（前）、+X が右。高さ y は甲板の上の面から。
"""
import numpy as np

from main.scenery import CYAN, DARK, METAL, OFF, REDL, WARM, WHITE, Scenery, unit

DIR = (-0.6, 0.55, -0.58)        # 主な惑星の中心から見たアマツの向き（ガイアの北の基地から、振り返った空に見える）
DIST = 1550.0                    # 主な惑星の中心からの距離 m
LENGTH = 240.0                   # 円筒の長さ
RADIUS = 40.0                    # 円筒の半径
DECK_W = 64.0                    # 上の甲板の幅
DECK_L = 200.0                   # 上の甲板の長さ
SIDES = 16
NAVY_D = (0.16, 0.22, 0.38)


def frame(center):
    """アマツの中心と向き（列が 右・上・前 のワールド方向）。上＝惑星と反対。"""
    up = unit(DIR)
    ref = np.array([0.0, 1.0, 0.0])
    f = unit(np.cross(up, unit(np.cross(ref, up))))
    r = np.cross(up, f)
    c = np.asarray(center, float) + up * DIST
    return c, np.stack([r, up, f], axis=1)


class Deck:
    """甲板の座標 → ワールド。"""

    def __init__(self, c, R):
        self.c, self.R = c, R
        self.top = c + R[:, 1] * (RADIUS + 2.0)      # 甲板の上の面の真ん中

    def point(self, x, z, y=0.0):
        R = self.R
        return self.top + R[:, 0] * x + R[:, 1] * y + R[:, 2] * z

    def frame(self, yaw=0.0):
        """甲板に立てた向き（yaw は左回り度）。"""
        a = np.radians(yaw)
        R = self.R
        f = R[:, 2] * np.cos(a) - R[:, 0] * np.sin(a)
        return np.stack([np.cross(R[:, 1], f), R[:, 1], f], axis=1)


def build(center):
    c, R = frame(center)
    s = Scenery("amatsu", None)
    rx, up, fw = R[:, 0], R[:, 1], R[:, 2]
    # 円筒（16 枚の板）
    w = 2 * RADIUS * np.tan(np.pi / SIDES) + 0.4
    for k in range(SIDES):
        a = 2 * np.pi * k / SIDES
        n = up * np.cos(a) + rx * np.sin(a)
        t = np.cross(fw, n)
        Rk = np.stack([t, n, fw], axis=1)
        col = WHITE if k % 4 else (0.75, 0.77, 0.80)
        s.raw(c + n * RADIUS, Rk, (w / 2, 1.5, LENGTH / 2), color=col)
        if k not in (0, 1, SIDES - 1):              # 窓の明かりの帯（上の甲板の下は無し）
            for j in range(-5, 6):
                s.raw(c + n * (RADIUS + 1.55) + fw * (j * 20.0), Rk, (w * 0.3, 0.06, 6.0), color=OFF,
                      glow=WARM if (j + k) % 3 else CYAN, collide=False)
    # 両端のふた・港の塔・推進ブロックの輪
    for sgn in (-1, 1):
        s.raw(c + fw * sgn * (LENGTH / 2 + 1.0), R, (RADIUS, RADIUS, 1.5), color=(0.62, 0.64, 0.68))
        s.raw(c + fw * sgn * (LENGTH / 2 + 2.6), R, (RADIUS * 0.55, RADIUS * 0.55, 0.3), color=OFF, glow=(0.6, 0.85, 1.0),
              collide=False)
        for k in range(8):
            a = 2 * np.pi * k / 8 + np.pi / 8
            n = up * np.cos(a) + rx * np.sin(a)
            p = c + n * (RADIUS + 3.0) + fw * sgn * (LENGTH / 2 - 6.0)
            s.raw(p, R, (2.5, 2.5, 6.0), color=DARK)
            s.raw(p + fw * sgn * 6.3, R, (1.6, 1.6, 0.3), color=OFF, glow=CYAN, collide=False)
    s.raw(c + fw * (LENGTH / 2 + 16.0), R, (6.0, 6.0, 14.0), color=METAL)                # 港の塔
    s.raw(c + fw * (LENGTH / 2 + 30.5), R, (3.0, 3.0, 0.5), color=OFF, glow=REDL, blink=1.2, collide=False)
    # 上の甲板（広い外壁の作業場）
    d = Deck(c, R)
    s.raw(d.point(0, 0, -1.0), R, (DECK_W / 2, 1.0, DECK_L / 2), color=(0.70, 0.72, 0.75))
    for x in (-DECK_W / 2 + 0.6, DECK_W / 2 - 0.6):                                       # 縁の明かり
        for j in range(-9, 10):
            s.raw(d.point(x, j * 10.0, 0.08), R, (0.3, 0.08, 1.2), color=OFF, glow=CYAN, blink=2.4, collide=False)
    for j in range(-4, 5):                                                                # 甲板の線
        s.raw(d.point(0, j * 22.0, 0.02), R, (DECK_W / 2 - 2, 0.02, 0.25), color=(0.45, 0.47, 0.52), collide=False)
    for x, z in ((-22, -70), (22, -40), (-18, 30), (20, 75), (-24, 80)):   # 甲板の建物はブロック（blocks.py）。屋根の明かり
        s.raw(d.point(x, z, 4.1), R, (2.0, 0.08, 4.0), color=OFF, glow=WARM, collide=False)
    # 非常灯（落ちていく間、赤く点く：lights の group）
    for j in range(-8, 9):
        for x in (-DECK_W / 2 + 3.0, DECK_W / 2 - 3.0):
            s.raw(d.point(x, j * 12.0 + 6.0, 0.4), R, (0.4, 0.4, 0.4), color=OFF, glow=REDL, group="alert",
                  collide=False)
    s.point_raw("center", c)
    s.point_raw("deck", d.top)
    s.point_raw("deck_up", up)
    s.point_raw("deck_fwd", fw)
    s = s.finish()
    s.deck = d
    return s

"""04 第8章「発進」（約 24 秒）

夜明けの観測基地。修理を終えたルミナス号（ゲームと同じブロックの船）にトワが乗り込み、
サクラが大きく手を振って見送る。推進機に火が入り、船は空へ。ミカガの無線「行っておいで」。
"""
import math

import numpy as np

from cinema import anim, fx
from cinema.film import Cam, Shot
from scenes import lib
from scenes.base import Movie

SHIP = np.array([14.0, 0.0, 34.0])     # 船の置き場所（左下の角のブロックの下面）
IGNITE = 13.3
LIFT = 14.6


class FILM(Movie):
    name = "04_launch"
    duration = 24.0
    near, far = 0.05, 30000.0
    sun = (0.62, 0.18, 0.76)
    ambient = 0.34

    def __init__(self):
        super().__init__()
        self.captions = [(0.5, 3.8, "観測基地 ALP-7　夜明け")]
        self.shots = [
            Shot(0.0, 5.0, self.cam_wide, "船"),
            Shot(5.0, 9.0, self.cam_towa, "トワ"),
            Shot(9.0, 13.0, self.cam_sakura, "サクラ"),
            Shot(13.0, 19.0, self.cam_lift, "発進"),
            Shot(19.0, 24.0, self.cam_sky, "空へ"),
        ]

    def height(self, t):
        a = max(0.0, t - LIFT)
        return 1.2 * a * a + 0.35 * a ** 3

    def build(self, st):
        fx.Sky(st, top=(0.2, 0.32, 0.6), horizon=(1.0, 0.72, 0.5), below=(0.72, 0.62, 0.6), sun_glow=0.9)
        self.ground = fx.Ground(st, size=1600, n=220, style="tundra", seed=12, flat=[(0, 5, 26, 0.0), (15, 35, 14, 0.0)],
                                    fog=(0.95, 0.75, 0.62))
        base = st.ship(lib.base_exterior().voxels(), size=1.0)
        base.place((0, 0.0, 0), np.eye(3))
        sh = self.ship = st.ship(lib.luminous_voxels(), size=0.5, pivot=(0, 0, 0), name="luminous")
        sh.motion = lambda t: (SHIP + np.array([0, self.height(t), 0.25 * self.height(t)]),
                               anim.axis_angle((1, 0, 0), -6 * anim.smooth(anim.remap(t, LIFT, LIFT + 3))))
        self.seat = np.array([1.25, 1.5, 1.15])
        tw = self.towa = lib.actor(st, "towa")
        tw.place = lambda t: (sh.world(self.seat), 0.0)
        tw.poses = [(0, lib.pose("grip"))]
        tw.look = lambda t: sh.world(self.seat) + np.array([0, 1.0, 30.0]) if t < 7.6 else self.sakura.head_pos(t=t)
        tw.expr = {"sad": anim.const(0.1)}
        tw.blinks = [5.6, 8.2, 11.0, 16.0]
        sk = self.sakura = lib.actor(st, "sakura", (5.5, 0.0, 26.0), lib.yaw_to((5.5, 0, 26), SHIP))
        sk.poses = [(0, {}), (8.8, {}), (9.3, lib.pose("wave_big")), (9.8, lib.pose("wave")), (10.3, lib.pose("wave_big")),
                    (10.8, lib.pose("wave")), (11.3, lib.pose("wave_big")), (11.8, lib.pose("wave")), (13.0, lib.pose("hand_chest")),
                    (15, lib.pose("hand_chest", head=(0.0, 0.5, 1.0))), (24, lib.pose("wave", head=(0.0, 0.6, 1.0)))]
        sk.look = lambda t: sh.world(np.array([1.25, 1.5, 1.25]))
        sk.expr = {"happy": anim.Track((0, 0.5), (12.6, 0.8), (13.5, 0.2), (18, 0.6))}
        sk.blinks = lib.blink_every(0, 24, 3.2)
        for a in (tw, sk):
            a.light(key_dir=(0.62, 0.25, 0.76), key_col=(1.08, 0.9, 0.75), fill_col=(0.72, 0.72, 0.9), rim_col=(0.6, 0.35, 0.2))
        glow = fx.Sprites(st, "glow", "add", order=12)
        beam = fx.Sprites(st, "beam", "add", order=13)
        smoke = fx.Sprites(st, "smoke", "alpha", order=8)
        on = lambda t: 0.0 if t < IGNITE else min(1.0, (t - IGNITE) / 0.4) * (0.75 + 0.25 * min(1.0, (t - IGNITE) / 1.2))  # noqa: E731
        noz = [np.array([x * 0.5 + 0.25, -0.1, z * 0.5 + 0.25]) for x, z in ((0, 0), (0, 2), (0, 4), (2, 0), (2, 4), (4, 0), (4, 2), (4, 4))]
        lib.thruster_flames(glow, beam, sh, noz, dirs=(0, -1, 0), on=on, size=0.5)
        rng = np.random.default_rng(6)
        ang = rng.uniform(0, 2 * math.pi, 40)
        spd = rng.uniform(4, 11, 40)

        def dust(t, pts, lines):
            """吹き上がる雪煙（地面の近くだけ）。"""
            if t < IGNITE:
                return
            hgt = self.height(t)
            k = max(0.0, 1 - hgt / 25)
            for i in range(40):
                a = (t - IGNITE) * spd[i] * 0.4 + 0.5
                p = SHIP + np.array([1.25 + math.cos(ang[i]) * a, 0.4 + 0.15 * i % 1.5, 1.25 + math.sin(ang[i]) * a])
                pts.append((p, 1.5 + a * 0.5, (0.88, 0.88, 0.92, 0.45 * k * max(0.0, 1 - a / 18)), ang[i]))
        smoke.add(fx.Func(dust))
        smoke.add(fx.Trail(lambda te: sh.world(np.array([1.25, -1.0, 1.25])) if te > LIFT + 0.5 else None, LIFT + 0.5, 30, rate=16,
                           life=3.0, size=(1.0, 6.0), color=(0.85, 0.85, 0.88, 0.35), drift=1.0, seed=4))
        self.say("mikaga", 1.0, 4.6, "推力、安定。燃料も満タン。……準備はいい？")
        self.say("towa", 6.4, 7.4, "……行く", tw)
        self.say("sakura", 9.3, 12.6, "トワーっ！ 特ダネ、待ってるからねー！", sk)
        self.say("mikaga", 16.0, 18.4, "……行っておいで。月で待ってる")

    # --- カメラ ------------------------------------------------------------
    def cam_wide(self, t, u):
        pos = np.array([1.0, 1.4, 44.0]) + np.array([2.5, 0.2, -1.5]) * u
        return Cam(pos, SHIP + np.array([-1.0, 1.5, 0.0]), fov=36, aperture=2.0)

    def cam_towa(self, t, u):
        h = self.towa.head_pos(t=t)
        pos = h + np.array([0.45, 0.55, 1.45 - 0.15 * u])
        return Cam(pos, h - np.array([0, 0.05, 0]), fov=30, aperture=10.0)

    def cam_sakura(self, t, u):
        h = self.sakura.head_pos(t=t)
        f = self.sakura.face_dir(t)
        r = np.cross([0, 1, 0], f)
        pos = h + f * 2.6 + r * 0.6 - np.array([0, 0.3, 0])
        return Cam(pos, h - np.array([0, 0.25, 0]), fov=32, aperture=8.0)

    def cam_lift(self, t, u):
        pos = np.array([9.5, 0.6, 23.5])
        c = self.ship.world(np.array([1.25, 1.0, 1.25]))
        sh = 0.0 if t < IGNITE else 0.06 * max(0.0, 1 - (t - IGNITE) / 5)
        return Cam(pos, c + np.array([0, 0.5, 0]), fov=38, shake=sh)

    def cam_sky(self, t, u):
        c = self.ship.world(np.array([1.25, 1.0, 1.25]))
        pos = np.array([-30.0, 3.0, 60.0]) + np.array([0, 40.0, 0]) * u
        return Cam(pos, c, fov=34)

    def grade(self, t):
        g = self.base_grade(t)
        g.tint = (1.05, 0.98, 0.95)
        g.bloom = 0.45
        if IGNITE <= t < IGNITE + 0.3:
            g.flash = 0.25 * (1 - (t - IGNITE) / 0.3)
        return g

    def audio(self, path):
        from cinema import sound as S
        mx = S.Mixer(self.duration)
        mx.add(S.bandpass(S.noise(14, 2), 200, 1200) * 1.2, 0, 0.14)
        mx.add(S.pad(S.chord("D3 A3 D4"), 9.0, a=2.0, r=1.5, bright=1200), 0.2, 0.22)
        mx.add(S.pad(S.chord("B2 F#3 B3 D4"), 5.0, a=1.0, r=1.2, bright=1300), 8.5, 0.24)
        roar = S.lowpass(S.noise(11, 5), 900) * S.env(11, 0.4, 0, 1, 2.0) * 2.0
        mx.add(roar, IGNITE, 0.6)
        mx.add(S.boom(2.0, 0.8, 3) * 0.6, IGNITE, 0.4)
        mx.add(S.riser(1.4, 200, 1800), LIFT - 1.2, 0.3)
        mx.add(S.hit(2.0, 48), LIFT, 0.7)
        mx.add(S.pad(S.chord("D3 F#3 A3 D4 F#4"), 9.0, a=0.6, r=2.5, bright=2600), LIFT, 0.32)
        mx.add(S.pad(S.chord("D2 A2 D3"), 9.0, a=0.6, r=2.5, bright=600), LIFT, 0.3)
        return mx.save(path)

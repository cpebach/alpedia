"""03 第5章「見張り」（約 27 秒）

観測基地 ALP-7 の上に、黒い偵察ドローンが現れる。映像は月の帝国の総統府へ。
キンギが「財団の三号」の到着を報告し、総統ネムリは窓の外を見たまま「目を離すな。殺す必要はない」と命じる。
"""
import math

import numpy as np

from cinema import anim, fx
from cinema.film import Cam, Shot
from scenes import lib
from scenes.base import Movie

ROOM = np.array([10000.0, 0.0, 0.0])          # 総統府の部屋（基地とは離れた所）
HOLO = ROOM + np.array([0.25, 1.12, -1.0])    # 机の上の立体映像


class FILM(Movie):
    name = "03_watch"
    duration = 27.0
    near, far = 0.05, 60000.0
    sun = (-0.5, 0.45, 0.7)
    ambient = 0.36

    def __init__(self):
        super().__init__()
        self.captions = [(0.5, 3.8, "惑星テラ　観測基地 ALP-7"), (8.3, 11.5, "月　帝国首都　総統府")]
        self.shots = [
            Shot(0.0, 5.0, self.cam_base, "基地"),
            Shot(5.0, 8.0, self.cam_drone, "ドローン"),
            Shot(8.0, 13.0, self.cam_kingi, "キンギ"),
            Shot(13.0, 18.6, self.cam_nemuri_wide, "ネムリ"),
            Shot(18.6, 23.0, self.cam_nemuri, "ネムリ寄り"),
            Shot(23.0, 27.0, self.cam_kingi2, "キンギ"),
        ]

    def build(self, st):
        outside = lambda t: t < 8.0  # noqa: E731
        inside = lambda t: t >= 8.0  # noqa: E731
        sky = fx.Sky(st, top=(0.42, 0.5, 0.62), horizon=(0.78, 0.8, 0.84), below=(0.7, 0.71, 0.73), sun_glow=0.15)
        sky.when = outside
        g = self.ground = fx.Ground(st, size=1400, n=200, style="tundra", seed=12, flat=[(0, 5, 26, 0.0)],
                                    fog=(0.78, 0.8, 0.84))
        g.when = outside
        base = st.ship(lib.base_exterior().voxels(), size=1.0)
        base.place((0, 0.0, 0), np.eye(3))
        base.when = outside
        towa = self.towa = lib.actor(st, "towa", (0.8, 0.0, 9.5), 20.0)
        towa.visible = outside
        self.drone_path = anim.Track((0.0, (-70.0, 26.0, 60.0)), (4.2, (5.0, 4.6, 20.0), "out"), (8.0, (4.4, 4.4, 19.0)))
        dr = st.ship(lib.drone().voxels(), size=0.5, name="drone")
        dr.motion = lambda t: ((self.drone_path(t) + np.array([0, 0.25 * math.sin(t * 3), 0]),
                                anim.look_basis(np.array([0.8, 1.0, 9.5]) - self.drone_path(t)).T @ anim.axis_angle((1, 0, 0), 8))
                               if outside(t) else None)
        self.dr = dr
        towa.look = lambda t: self.drone_path(t) if t > 2.5 else np.array([0.0, 1.5, 40.0])
        towa.expr = {"surprised": anim.Track((2.8, 0), (3.5, 0.4))}
        # 総統府
        moon = fx.Ground(st, center=ROOM + np.array([0, -60, 0]), size=3000, n=160, style="moon", seed=7,
                         craters=[(300, 600, 120, 14), (-500, 900, 80, 10)])
        moon.when = inside
        stars = fx.Stars(st, dist=50000)
        stars.when = inside
        terra = fx.Planet(st, "terra", ROOM + np.array([6000.0, 9000.0, 30000.0]), scale=9)
        terra.when = inside
        rm = st.ship(lib.imperial_room().voxels(), size=0.5, pivot=(0, 0, 0), sun=(0.2, 0.7, -0.6), ambient=0.3)
        rm.place(ROOM)
        rm.when = inside
        # 窓の外の街の光（低いビルの箱）
        city = lib.Vox()
        rng = np.random.default_rng(3)
        for k in range(70):
            x, z = int(rng.integers(-40, 40)), int(rng.integers(20, 90))
            hgt = int(rng.integers(2, 9))
            city.box(x, x + 1, 0, hgt, z, z + 1, "m_dark")
            for y in range(1, hgt, 2):
                city.put(x, y, z, "g_amber" if rng.uniform() < 0.6 else "m_black")
        cs = st.ship(city.voxels(), size=4.0, pivot=(0, 0, 0))
        cs.place(ROOM + np.array([0, -60, 0]))
        cs.when = inside
        kg = self.kingi = lib.actor(st, "kingi", ROOM + np.array([2.6, 0, -0.5]), -100.0)
        kg.poses = [(0, lib.pose("smoke")), (25.6, lib.pose("smoke")), (26.4, lib.pose("hands_pockets"))]
        kg.look = lambda t: HOLO if t < 13.5 or t > 24.5 else self.nemuri.head_pos(t=t)
        kg.expr = {"happy": anim.Track((23.0, 0.0), (24.0, 0.45))}
        kg.blinks = lib.blink_every(8, 27, 3.4)
        kg.visible = inside
        nm = self.nemuri = lib.actor(st, "nemuri", ROOM + np.array([0.6, 0, 5.0]), 0.0)
        turn = anim.Track((15.6, 0.0), (17.2, 205.0, "inout"))
        nm.place = lambda t: (ROOM + np.array([0.6, 0, 5.0]), float(turn(t)))
        nm.poses = [(0, lib.pose("hands_back"))]
        nm.look = lambda t: (ROOM + np.array([1.0, 2.0, 40.0]) if t < 14.4 else kg.head_pos(t=t))
        nm.expr = {"sad": anim.Track((16.5, 0.0), (18.5, 0.25)), "angry": anim.Track((18.5, 0.0), (19.5, 0.15))}
        nm.blinks = lib.blink_every(13, 27, 2.8)
        nm.visible = inside
        holo = self.holo_towa = lib.actor(st, "towa", HOLO, 200.0, scale=0.2, label="holo_towa")
        holo.holo_mode((0.35, 0.85, 1.0))
        holo.holo = lambda t: 0.9 if inside(t) else 0.0
        holo.look = lambda t: HOLO + np.array([0.0, 0.3, -2.0])
        for a in (kg, nm):
            a.light(key_dir=(0.2, 0.5, 1.0), key_col=(0.85, 0.9, 1.05), fill_col=(0.62, 0.5, 0.58), rim_col=(0.6, 0.12, 0.1))
        towa.light(key_dir=(-0.5, 0.45, 0.7), key_col=(1.0, 0.98, 0.96), fill_col=(0.8, 0.82, 0.9), rim_col=(0.25, 0.3, 0.4))
        glow = fx.Sprites(st, "glow", "add", order=12)
        smoke = fx.Sprites(st, "smoke", "alpha", order=8)
        lib.cigarette(glow, smoke, kg, 8.0, 99)
        glow.add(fx.Glow(lambda t: dr.world(dr.pivot + np.array([0.5, 0.5, 2.6])) if outside(t) else None,
                         lambda t: 0.9 + 0.3 * math.sin(t * 9), (1.0, 0.15, 0.1, 0.95)))
        glow.add(fx.Glow(HOLO - np.array([0, 0.1, 0]), 0.9, (0.3, 0.8, 1.0, 0.35), 8.0, 99))
        # 吐いた煙（最後）
        smoke.add(fx.Func(lambda t, pts, lines: [pts.append((kg.mouth_pos(t) + kg.face_dir(t) * (0.15 + 0.5 * (t - 24.6)) * (1 + i * 0.2)
                                                              + np.array([0, 0.1 * i, 0]), 0.15 + 0.3 * (t - 24.6) + i * 0.05,
                                                              (0.8, 0.8, 0.82, 0.35 * max(0.0, 1 - (t - 24.6) / 2.0)), i))
                                                 for i in range(5)] if 24.6 <= t < 26.6 else None))
        self.say("kingi", 9.0, 12.6, "……来たぜ、総統。財団の「三号」だ", kg)
        self.say("nemuri", 14.0, 17.2, "そうか。……やはり、送ってきたか", nm)
        self.say("nemuri", 19.0, 22.6, "目を離すな、キンギ。……殺す必要はない", nm)
        self.say("kingi", 23.4, 26.4, "了解。……ま、見るだけならタダだ", kg)

    # --- カメラ ------------------------------------------------------------
    def cam_base(self, t, u):
        pos = np.array([-26.0, 2.2, 38.0]) + np.array([4.0, 0.5, -3.0]) * u
        tgt = anim.lerp(np.array([0.0, 4.0, 4.0]), self.drone_path(t), 0.55 * anim.smooth(anim.remap(t, 1.0, 4.5)))
        return Cam(pos, tgt, fov=40)

    def cam_drone(self, t, u):
        d = self.drone_path(t)
        tw = self.towa.head_pos(t=t)
        f = anim.unit(tw - d)
        pos = d - f * 3.0 + np.array([0, 2.2, 0]) + np.cross([0, 1, 0], f) * 1.2
        return Cam(pos, anim.lerp(d, tw, 0.75), fov=34, aperture=6.0, focus=float(np.linalg.norm(d - pos)), roll=6)

    def cam_kingi(self, t, u):
        h = self.kingi.head_pos(t=t)
        pos = ROOM + np.array([-0.6, 1.45, 1.6]) + np.array([0.25, 0, -0.2]) * u
        return Cam(pos, anim.lerp(HOLO + np.array([0, 0.1, 0]), h, 0.55), fov=34, aperture=8.0,
                   focus=float(np.linalg.norm(h - pos)))

    def cam_nemuri_wide(self, t, u):
        h = self.nemuri.head_pos(t=t)
        pos = ROOM + np.array([-2.8, 1.5, 0.4]) + np.array([0.5, 0.0, 0.8]) * anim.smooth(u)
        return Cam(pos, h + np.array([0.6, -0.15, 1.0]) * (1 - anim.smooth(anim.remap(t, 15.0, 17.5))), fov=36,
                   aperture=5.0, focus=float(np.linalg.norm(h - pos)))

    def cam_nemuri(self, t, u):
        h = self.nemuri.head_pos(t=t)
        f = self.nemuri.face_dir(t)
        r = np.cross([0, 1, 0], f)
        pos = h + f * (1.15 - 0.15 * u) - r * 0.3 - np.array([0, 0.12, 0])
        return Cam(pos, h - np.array([0, 0.03, 0]), fov=28, aperture=12.0)

    def cam_kingi2(self, t, u):
        h = self.kingi.head_pos(t=t)
        f = self.kingi.face_dir(t)
        r = np.cross([0, 1, 0], f)
        pos = h + f * 1.4 + r * 0.45 + np.array([0, -0.15, 0])
        return Cam(pos, h - np.array([0, 0.05, 0]) + r * 0.1, fov=30, aperture=10.0)

    def grade(self, t):
        g = self.base_grade(t)
        if t < 8:
            g.tint = (0.97, 1.0, 1.05)
            g.saturation = 0.9
        else:
            g.tint = (1.06, 0.95, 0.95)
            g.lift = (0.015, 0.0, 0.01)
            g.vignette = 0.5
        if 5.0 <= t < 8.0:
            g.tint = (1.12, 0.92, 0.92)
            g.aberration = 1.5
        return g

    def audio(self, path):
        from cinema import sound as S
        mx = S.Mixer(self.duration)
        wind = S.bandpass(S.noise(8.2, 4), 200, 1400) * S.env(8.2, 0.5, 0, 1, 0.6) * 1.3
        mx.add(wind, 0, 0.2)
        buzz = (S.sine(180, 8.0) * 0.4 + S.sine(361, 8.0) * 0.2) * (1 + 0.3 * np.sin(S._t(8.0) * 40))
        mx.add(buzz * np.interp(S._t(8.0), [0, 1, 4.2, 7.6, 8.0], [0.05, 0.3, 1.0, 1.0, 0.0]), 0.0, 0.18)
        mx.add(S.pad(S.chord("E2 B2 E3 G3"), 8.0, a=1.4, r=1.0, bright=700), 0.2, 0.25)
        mx.add(S.drone(52, 19.0, 6) * S.env(19.0, 0.3, 0, 1, 1.2), 8.0, 0.22)
        mx.add(S.pad(S.chord("C2 G2 C3 D#3"), 10.0, a=1.0, r=1.5, bright=600), 8.0, 0.3)
        mx.add(S.hit(1.8, 42), 13.0, 0.5)
        mx.add(S.pad(S.chord("G#1 D#2 G#2 C3"), 9.0, a=1.5, r=2.0, bright=550), 17.0, 0.32)
        return mx.save(path)

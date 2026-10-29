"""01 序章「落下」（約 30 秒）

宇宙から惑星テラの高緯度の荒れ地へ、脱出ポッドが落ちていく。ミカガの無線が届く（なぜ落下地点を知っているのか）。
ポッドの中でトワが目を開ける（紫の目）。夕暮れの雪原にポッドが落ちて、題名。
"""
import math

import numpy as np

from cinema import anim, fx, shapes
from cinema.film import Cam, Shot
from scenes import lib
from scenes.base import Movie

CRASH = 23.2
SPACE = np.array([0.0, 0.0, -400000.0])            # 宇宙の場面の場所（地上とは離しておく）
INSIDE = np.array([3000.0, 0.0, 3000.0])           # ポッドの中の場面
T_SEP = 5.4                                        # 輸送船から切り離す


class FILM(Movie):
    name = "01_prologue"
    duration = 30.0
    near, far = 0.05, 90000.0
    sun = (-0.62, 0.22, 0.75)
    ambient = 0.32

    def __init__(self):
        super().__init__()
        self.titles = [(0.4, 3.8, "過去は消えない。", "それでも、進むことを選ぶ者たちの物語"),
                       (26.4, 29.6, "ALPEDIA", "それでも（dennoch）")]
        self.captions = [(4.4, 7.4, "惑星テラ　北の高緯度帯　上空")]
        self.shots = [
            Shot(0.0, 4.0, self.cam_black, "黒"),
            Shot(4.0, 10.0, self.cam_space, "宇宙"),
            Shot(10.0, 15.0, self.cam_entry, "大気圏"),
            Shot(15.0, 19.5, self.cam_inside, "ポッドの中"),
            Shot(19.5, 25.0, self.cam_crash, "落下"),
            Shot(25.0, 30.0, self.cam_after, "煙"),
        ]

    def build(self, st):
        is_space = lambda t: 4.0 <= t < 10.0  # noqa: E731
        is_ground = lambda t: t >= 10.0 and not (15.0 <= t < 19.5)  # noqa: E731
        is_land = lambda t: t >= 19.5  # noqa: E731
        stars = fx.Stars(st, dist=80000)
        stars.when = is_space
        terra = fx.Planet(st, "terra", SPACE + np.array([0.0, -26500.0, 9000.0]), scale=42, spin=0.05,
                          tilt=anim.axis_angle((1, 0, 0), -40))
        terra.when = is_space
        sky = fx.Sky(st, top=(0.1, 0.16, 0.34), horizon=(0.95, 0.58, 0.38), below=(0.55, 0.45, 0.48), sun_glow=0.7)
        sky.when = is_ground
        self.ground = g = fx.Ground(st, size=1600, n=240, style="tundra", seed=4, flat=[(0, 0, 18, 0.0)],
                                    fog=(0.78, 0.6, 0.52), craters=[(0, 0, 7, 1.2)])
        g.when = is_land
        # 遠くの観測基地
        h = g.height(140, 330)
        base = st.ship(lib.base_exterior().voxels(), size=1.0)
        base.place((140, h - 0.5, 330), anim.yaw_pitch_roll(200, 0, 0))
        base.when = is_land
        # 輸送船（印のない灰色の船）とポッド
        tr = st.ship(shapes.far_hull(length=30, w=8, h=5, mat="m_gray", band="m_dark", light="g_amber").voxels(),
                     size=1.5, name="transport")
        tr.motion = lambda t: (SPACE + np.array([0, 0, 18.0 * (t - 4)]), anim.yaw_pitch_roll(0, 0, 0)) \
            if is_space(t) else None
        self.tr = tr
        pv = lib.pod()
        pod = st.ship(pv.voxels(), size=0.5, name="pod")
        self.pod_path = anim.Track((10.0, (-1100.0, 2900.0, -1700.0), "linear"), (15.0, (-620.0, 1700.0, -1000.0), "linear"),
                                   (19.5, (-260.0, 760.0, -420.0), "linear"), (22.4, (-40.0, 110.0, -70.0), "in"),
                                   (CRASH, (0.0, 0.6, 0.0)))

        def pod_motion(t):
            if is_space(t):
                a = max(0.0, t - T_SEP)
                p = SPACE + np.array([0, 0, 18.0 * (t - 4)]) + np.array([0.0, -4.0 - 1.4 * a * a, -2.0 - 3 * a])
                return p, anim.yaw_pitch_roll(0, 20 * a, 15 * a)
            if 10.0 <= t < CRASH:
                d = anim.unit(self.pod_path(t + 0.05) - self.pod_path(t))
                return self.pod_path(t), anim.look_basis(-d, (0, 0, 1)).T @ anim.axis_angle((1, 0, 0), -90) \
                    @ anim.axis_angle((0, 1, 0), t * 40)
            return None
        pod.motion = pod_motion
        self.pod = pod
        burnt = st.ship(lib.pod_burnt().voxels(), size=0.5, name="pod2")
        burnt.motion = lambda t: ((0.0, -0.9, 0.0), anim.yaw_pitch_roll(30, 18, -12)) if t >= CRASH else None
        # ポッドの中
        from cinema.shapes import Vox
        cab = Vox()
        for x in range(-3, 4):
            for z in range(-3, 4):
                for y in range(-1, 6):
                    r = math.hypot(x, z)
                    if 2.6 <= r <= 3.6 or y in (-1, 5):
                        cab.put(x, y, z, "m_dark" if (y + x) % 4 else "m_panel")
        cab.box(-2, 2, 3, 3, 2, 2, "g_screen")
        cab.box(-1, 1, 0, 0, -3, -2, "m_wine")
        inside = st.ship(cab.voxels(), size=0.5, pivot=(0, 0, 0), sun=(0.0, 0.3, 1.0), ambient=0.4)
        inside.place(INSIDE)
        inside.when = lambda t: 15.0 <= t < 19.5
        towa = lib.actor(st, "towa", INSIDE + np.array([0, 0.5, -0.75]), 0.0)
        towa.poses = [(0, lib.pose("grip"))]
        towa.expr = {"blink": anim.Track((15.0, 1.0), (17.0, 1.0), (17.35, 0.0)),
                     "sad": anim.Track((15.0, 0.25), (18.0, 0.1))}
        towa.blinks = [18.6]
        towa.look = lambda t: INSIDE + np.array([0.4 * math.sin(t * 2), 1.45, 3.0])
        towa.visible = lambda t: 15.0 <= t < 19.5
        self.towa = towa
        # 光・煙・爆発
        glow = fx.Sprites(st, "glow", "add", order=12)
        beam = fx.Sprites(st, "beam", "add", order=13)
        smoke = fx.Sprites(st, "smoke", "alpha", order=8)
        for x in (-4, 4):
            glow.add(fx.Glow(lambda t, x=x: tr.world(tr.pivot + np.array([x, 0, -24.0])) if is_space(t) else None,
                             6.0, (1.0, 0.75, 0.4, 0.8)))

        def entry(t, pts, lines):
            """大気圏に入るときの炎（前に光、後ろに長い尾）。"""
            if not (10.0 <= t < CRASH):
                return
            p = self.pod_path(t)
            d = anim.unit(self.pod_path(t + 0.05) - p)
            heat = 1.0 if t < 19.0 else max(0.15, 1 - (t - 19.0) / 3.5)
            pts.append((p + d * 1.6, 7 * heat + 2, (1.0, 0.6, 0.3, 0.85 * heat), t * 4))
            pts.append((p + d * 1.0, 3.5, (1.0, 0.95, 0.85, 0.9 * heat), 0.0))
            lines.append(("line", p, p - d * 40 * heat, 3.5 * heat, (1.0, 0.5, 0.2, 0.6 * heat)))
            lines.append(("line", p, p - d * 18 * heat, 1.2 * heat, (1.0, 0.9, 0.7, 0.8 * heat)))
        glow.add(fx.Func(lambda t, pts, lines: entry(t, pts, [])))
        beam.add(fx.Func(lambda t, pts, lines: entry(t, [], lines)))
        smoke.add(fx.Trail(lambda te: self.pod_path(te) if 10.0 <= te < CRASH else None, 10.0, CRASH, rate=30,
                           life=4.0, size=(3, 18), color=(0.35, 0.33, 0.33, 0.45), drift=3.0, seed=2))
        cp = np.array([0.0, 0.8, 0.0])
        ex = fx.Explosion(CRASH, cp, scale=1.1, seed=5, n=30)
        glow.add(ex)
        glow.add(ex.shock())
        # 雪と土の煙（白っぽい）
        smoke.add(fx.Func(lambda t, pts, lines: [pts.append((cp + np.array([math.cos(i * 1.7) * (2 + 9 * anim.ease_out(min((t - CRASH) / 3, 1))),
                                                                               1 + i * 0.25 + (t - CRASH) * 0.6,
                                                                               math.sin(i * 2.3) * (2 + 9 * anim.ease_out(min((t - CRASH) / 3, 1)))]),
                                                              5 + 6 * anim.ease_out(min((t - CRASH) / 4, 1)),
                                                              (0.72, 0.7, 0.68, 0.55 * max(0.0, 1 - (t - CRASH) / 7)), i))
                                                 for i in range(26)] if t >= CRASH else None))
        smoke.add(fx.Trail(lambda te: cp + np.array([0.4, 1.0, 0.0]) if te >= CRASH else None, CRASH + 0.3, 40, rate=8,
                           life=5, size=(1.2, 7), color=(0.25, 0.24, 0.24, 0.5), drift=0.9, seed=9))
        fx.Debris(st, CRASH, cp, n=60, speed=16, size=(0.2, 0.7), cols=((0.85, 0.88, 0.92), (0.36, 0.35, 0.32),
                                                                      (0.92, 0.93, 0.95)), seed=3, life=12)
        glow.add(fx.Glow(lambda t: INSIDE + np.array([0.0, 1.6, 1.2]) if 15.0 <= t < 19.5 else None,
                         lambda t: 1.6 + 0.6 * math.sin(t * 23), (1.0, 0.45, 0.2, 0.35)))
        self.say("mikaga", 5.0, 9.6, "……聞こえる？ 聞こえなくてもいいや。落ちる場所は、わかってるから")
        self.say("mikaga", 15.6, 19.2, "北の荒れ地。古い観測基地の、すぐそば。……がんばって")
        st.add(self)

    def update(self, t, origin):
        a = 0.5 + 0.5 * math.sin(t * 19) * math.sin(t * 7)
        self.towa.light(key_dir=(0.0, 0.4, 1.0), key_col=(1.0, 0.76 + 0.1 * a, 0.62), fill_col=(0.5, 0.42, 0.5),
                        rim_col=(0.25, 0.3, 0.55))

    # --- カメラ ------------------------------------------------------------
    def cam_black(self, t, u):
        return Cam(SPACE + np.array([0, 0, -2000.0]), SPACE + np.array([0, 0, -3000.0]))

    def cam_space(self, t, u):
        c = SPACE + np.array([0, 0, 18.0 * (t - 4)])
        pos = c + anim.lerp(np.array([-70.0, 26.0, -80.0]), np.array([-52.0, 14.0, -45.0]), anim.ease_in_out(u))
        tgt = c + anim.lerp(np.array([0.0, -6.0, 0.0]), np.array([0.0, -30.0, 10.0]), anim.ease_in_out(u))
        return Cam(pos, tgt, fov=40, roll=-8)

    def cam_entry(self, t, u):
        p = self.pod_path(t)
        d = anim.unit(self.pod_path(t + 0.05) - p)
        side = anim.unit(np.cross(d, [0, 1, 0]))
        pos = p - d * 28 + side * 16 + np.array([0, 6.0, 0])
        return Cam(pos, p + d * 10, fov=42, roll=10, shake=0.25)

    def cam_inside(self, t, u):
        h = self.towa.head_pos(t=t)
        pos = h + np.array([0.24, 0.0, 1.0 - 0.15 * u])
        return Cam(pos, h + np.array([0, -0.02, 0]), fov=30, shake=0.012, aperture=9.0, max_blur=14)

    def cam_crash(self, t, u):
        g = self.ground
        x, z = 34.0, 38.0
        pos = np.array([x, g.height(x, z) + 2.2, z])
        k = anim.smooth(anim.remap(t, 22.6, 23.6))
        tgt = anim.lerp(self.pod_path(min(t, CRASH - 0.05)), np.array([0.0, 3.0, 0.0]), k)
        sh = 0.0 if t < CRASH else 0.5 * max(0.0, 1 - (t - CRASH) / 1.4)
        return Cam(pos, tgt, fov=38, shake=sh)

    def cam_after(self, t, u):
        g = self.ground
        x, z = anim.lerp(9.0, 6.5, u), anim.lerp(-10.0, -7.5, u)
        pos = np.array([x, g.height(x, z) + 1.3, z])
        return Cam(pos, np.array([0.0, 0.9, 0.0]), fov=38, aperture=4.0)

    # --- 仕上げ -------------------------------------------------------------
    def grade(self, t):
        g = self.base_grade(t, fade_in=0.01, fade_out=0.6)
        if t < 4.0:
            g.fade = 0.0
        elif t < 4.6:
            g.fade = (t - 4.0) / 0.6
        if 10 <= t < 15:
            g.tint = (1.05, 0.98, 0.92)
            g.bloom = 0.6
        if 15 <= t < 19.5:
            g.vignette = 0.6
            g.tint = (1.0, 0.95, 0.95)
        if 19.5 <= t:
            g.tint = (1.04, 0.97, 0.94)
            g.lift = (0.01, 0.0, 0.015)
        if CRASH <= t < CRASH + 0.4:
            g.flash = (1 - (t - CRASH) / 0.4) * 0.6
            g.aberration = 4 * (1 - (t - CRASH) / 0.4)
        if t > 26.0:
            g.exposure = 1 - 0.3 * anim.remap(t, 26.0, 26.8)
        return g

    # --- 音 ----------------------------------------------------------------
    def audio(self, path):
        from cinema import sound as S
        mx = S.Mixer(self.duration)
        mx.add(S.pad(S.chord("E2 B2 E3 G3"), 4.5, a=1.5, r=1.6, bright=700), 0.2, 0.35)
        mx.add(S.drone(40, 6.2, 1) * S.env(6.2, 1.0, 0, 1, 0.8), 4.0, 0.35)
        mx.add(S.pad(S.chord("C2 G2 C3 E3"), 6.5, a=1.2, r=1.5, bright=800), 4.0, 0.32)
        mx.add(S.launch(1.2, 4), T_SEP, 0.3)
        roar = S.sweep_lowpass(S.noise(9.2, 3), 300, 2500, 40) * S.env(9.2, 1.0, 0, 1, 1.5) * 1.6
        mx.add(roar, 10.0, 0.45)
        mx.add(S.drone(32, 4.6, 2) * 1.2, 15.0, 0.5)
        mx.add(S.pad(S.chord("A2 E3 A3 C4"), 5.0, a=0.8, r=1.2, bright=900), 15.0, 0.3)
        mx.add(S.riser(2.0, 200, 3000), 21.2, 0.45)
        mx.add(S.boom(4.0, 1.6, 7), CRASH, 1.0)
        mx.add(S.hit(2.2, 36), CRASH, 0.9)
        wind = S.bandpass(S.noise(7.0, 11), 200, 1400) * S.env(7.0, 1.5, 0, 1, 1.5)
        mx.add(wind, 23.5, 0.18)
        mx.add(S.pad(S.chord("E2 B2 E3 G#3 B3"), 4.0, a=0.6, r=1.6, bright=1800), 26.2, 0.42)
        mx.add(S.hit(2.0, 44), 26.4, 0.7)
        return mx.save(path)

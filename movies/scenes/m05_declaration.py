"""05 第10章「宣戦」（約 36 秒）

月の中立自由港ルナ・ポート。トワは広場で初めてミカガに会う（二人とも紫の目）。
広場の大きな立体映像に帝国総統ネムリが現れ、連盟への宣戦を布告する。その姿を見たトワの頭に、知らない記憶の
かけらがよぎる（暗い部屋のネムリとオミク、白い閃光）。記憶がすっかり戻るのは、このあとゲームの中で
脱出ポッドの記録を見たとき（第10章の目標）。
"""
import math

import numpy as np

from cinema import anim, fx
from cinema.film import Cam, Shot
from scenes import lib
from scenes.base import Movie

PYLON = np.array([0.0, 1.0, 22.0])
HOLO_ON = 14.3
MEM = np.array([20000.0, 0.0, 0.0])             # 記憶の部屋
MEM_SHOTS = ((26.0, 26.7), (27.4, 28.1))


def in_mem(t):
    return any(a <= t < b for a, b in MEM_SHOTS)


class FILM(Movie):
    name = "05_declaration"
    duration = 36.0
    near, far = 0.05, 70000.0
    sun = (-0.55, 0.55, 0.62)
    ambient = 0.3

    def __init__(self):
        super().__init__()
        self.captions = [(0.5, 3.8, "月　中立自由港ルナ・ポート")]
        self.titles = []
        self.shots = [
            Shot(0.0, 4.0, self.cam_est, "港"),
            Shot(4.0, 9.0, self.cam_meet, "出会い"),
            Shot(9.0, 12.0, self.cam_mikaga_eyes, "ミカガの目"),
            Shot(12.0, 14.0, self.cam_towa_eyes, "トワの目"),
            Shot(14.0, 20.0, self.cam_holo_wide, "立体映像"),
            Shot(20.0, 25.0, self.cam_holo_face, "ネムリ"),
            Shot(25.0, 26.0, self.cam_towa_mem, "トワ"),
            Shot(26.0, 26.7, self.cam_mem1, "記憶1"),
            Shot(26.7, 27.4, self.cam_towa_mem, "トワ"),
            Shot(27.4, 28.1, self.cam_mem2, "記憶2"),
            Shot(28.1, 36.0, self.cam_after, "そのあと"),
        ]

    def build(self, st):
        port = lambda t: not in_mem(t)  # noqa: E731
        stars = fx.Stars(st, dist=60000)
        stars.when = port
        terra = fx.Planet(st, "terra", (-9000.0, 16000.0, 32000.0), scale=14, tilt=anim.axis_angle((1, 0, 0), -30))
        terra.when = port
        g = fx.Ground(st, size=2400, n=200, style="moon", seed=21, flat=[(0, 0, 45, -0.6)],
                      craters=[(150, 220, 60, 8), (-260, 120, 90, 10), (90, -300, 70, 6)])
        g.when = port
        pl = st.ship(lib.plaza().voxels(), size=1.0, pivot=(0, 0, 0))
        pl.place((0, 0, 0))
        pl.when = port
        for k, (x, z, r) in enumerate(((-26, 16, 9), (24, 20, 11), (-30, -18, 7), (30, -14, 8), (0, 44, 13))):
            d = st.ship(lib.dome(r).voxels(), size=1.2, pivot=(0, 0, 0))
            d.place((x, -0.6, z), anim.yaw_pitch_roll(k * 20, 0, 0))
            d.when = port
        tw = self.towa = lib.actor(st, "towa")
        tw.place = lib.walk([(0.0, (-5.0, 0.0, -15.0), 15), (4.0, (-4.0, 0.0, -12.5), 15), (7.6, (-2.0, 0.0, -6.8), 20),
                             (40, (-2.0, 0.0, -6.8), 20)])
        mk = self.mikaga = lib.actor(st, "mikaga")
        mturn = anim.Track((5.4, 35.0), (6.4, 200.0, "inout"))
        mk.place = lambda t: (np.array([0.6, 0.0, -5.4]), float(mturn(t)) if t < 13.5 else float(mturn(t)) + 150 * anim.smooth(anim.remap(t, 14.0, 15.2)))
        tw.look = lambda t: (mk.head_pos(t=t) if 5.0 < t < 14.2 or t > 31.5 else self.holo_head(t))
        mk.look = lambda t: (tw.head_pos(t=t) if t < 14.4 or t > 30.0 else self.holo_head(t))
        mk.poses = [(0, lib.pose("hands_back")), (5.6, lib.pose("hands_back")), (6.6, lib.pose("hands_pockets")), (40, lib.pose("hands_pockets"))]
        mk.expr = {"happy": anim.Track((6.2, 0.0), (6.8, 0.55), (10.5, 0.4), (13.0, 0.1), (30.0, 0.0), (31.0, 0.1))}
        mk.blinks = lib.blink_every(0, 36, 3.6)
        tw.poses = [(0, {}), (24.8, {}), (25.3, lib.pose("hand_chest")), (28.0, lib.pose("head_hold")), (31.6, lib.pose("head_hold")),
                    (32.6, lib.pose("fist")), (36, lib.pose("fist"))]
        tw.expr = {"surprised": anim.Track((12.0, 0.0), (12.5, 0.2), (15.0, 0.4), (25.0, 0.7), (28.0, 0.2), (32.0, 0.0)),
                   "sad": anim.Track((28.0, 0.0), (28.6, 0.6), (32.0, 0.3)),
                   "angry": anim.Track((32.2, 0.0), (33.2, 0.25))}
        tw.blinks = [3.0, 8.0, 13.6, 19.0, 23.5, 33.8]
        for a in (tw, mk):
            a.light(key_dir=(-0.55, 0.55, 0.62), key_col=(1.0, 0.98, 0.96), fill_col=(0.62, 0.66, 0.85), rim_col=(0.75, 0.25, 0.2))
        # 大きな立体映像のネムリ
        nm = self.nemuri_holo = lib.actor(st, "nemuri", PYLON, 180.0, scale=9.0, label="holo_nemuri")
        nm.holo_mode((1.0, 0.36, 0.2))
        nm.holo = lambda t: 0.0 if (t < HOLO_ON or in_mem(t)) else min(1.0, (t - HOLO_ON) / 0.6) * (0.85 + 0.15 * math.sin(t * 37) * math.sin(t * 11))
        nm.poses = [(0, {}), (17.5, {}), (18.4, lib.pose("command")), (21.0, lib.pose("command")), (22.0, lib.pose("fist")), (36, lib.pose("fist"))]
        nm.look = lambda t: np.array([0.0, 1.6, -30.0])
        nm.expr = {"angry": anim.const(0.2)}
        nm.blinks = [19.5, 23.0, 33.0]
        # 記憶の部屋
        rm = st.ship(lib.imperial_room().voxels(), size=0.5, pivot=(0, 0, 0), sun=(0.2, 0.7, -0.6), ambient=0.25)
        rm.place(MEM)
        rm.when = in_mem
        nm2 = self.nemuri_mem = lib.actor(st, "nemuri", MEM + np.array([0.0, 0, 3.4]), 180.0)
        nm2.visible = in_mem
        nm2.poses = [(0, lib.pose("reach")), (27.4, lib.pose("point"))]
        nm2.expr = {"sad": anim.const(0.3)}
        om = self.omiku = lib.actor(st, "omiku", MEM + np.array([0.1, 0, 1.5]), 0.0)
        om.visible = in_mem
        om.poses = [(0, lib.pose("hands_back")), (27.4, lib.pose("hand_chest"))]
        om.expr = {"happy": anim.Track((26, 0.3), (27.4, 0.0)), "surprised": anim.Track((27.4, 0.0), (27.8, 0.7))}
        nm2.look = lambda t: om.head_pos(t=t)
        om.look = lambda t: nm2.head_pos(t=t)
        for a in (nm2, om):
            a.light(key_dir=(0.1, 0.5, 1.0), key_col=(0.7, 0.72, 0.85), fill_col=(0.35, 0.3, 0.35), rim_col=(0.9, 0.2, 0.15))
        glow = fx.Sprites(st, "glow", "add", order=12)
        glow.add(fx.Glow(PYLON + np.array([0, 0.5, 0]), lambda t: 12 * max(0.0, 1 - (t - HOLO_ON) / 0.5),
                         (1.0, 0.5, 0.35, 0.9), HOLO_ON, HOLO_ON + 0.5))
        glow.add(fx.Glow(PYLON + np.array([0, 0.2, 0]), 6.0, (1.0, 0.35, 0.2, 0.4), HOLO_ON, 99))
        self.say("mikaga", 6.4, 9.2, "やっと会えたね。……ようこそ、ルナ・ポートへ", mk)
        self.say("nemuri", 15.0, 18.8, "帝国臣民、そして地球圏のすべての人々に告げる", nm)
        self.say("nemuri", 20.4, 24.6, "本日をもって、帝国は連盟に宣戦を布告する", nm)
        self.say("mikaga", 29.6, 32.2, "……トワ？ あの人を、知ってるの？", mk)
        self.say("towa", 32.9, 35.4, "……わからない。でも、知ってる気がする", tw)

    def holo_head(self, t):
        return PYLON + np.array([0.0, 1.5 * 9.0, 0.0])

    # --- カメラ ------------------------------------------------------------
    def cam_est(self, t, u):
        pos = np.array([-36.0, 9.0, -44.0]) + np.array([8.0, -3.0, 6.0]) * anim.smooth(u)
        return Cam(pos, np.array([4.0, 6.0, 14.0]), fov=42)

    def cam_meet(self, t, u):
        a, b = self.towa.head_pos(t=t), self.mikaga.head_pos(t=t)
        mid = (a + b) / 2
        pos = mid + np.array([-4.2, -0.3, 2.6]) + np.array([0.8, 0, -0.4]) * u
        return Cam(pos, mid - np.array([0, 0.2, 0]), fov=32, aperture=4.0)

    def _eyes(self, a, t, d, side):
        h = a.head_pos(t=t) + np.array([0, 0.03, 0])
        f = a.face_dir(t)
        r = np.cross([0, 1, 0], f)
        return Cam(h + f * d + r * side, h, fov=24, aperture=14.0, max_blur=18)

    def cam_mikaga_eyes(self, t, u):
        return self._eyes(self.mikaga, t, 0.62 - 0.08 * u, 0.06)

    def cam_towa_eyes(self, t, u):
        return self._eyes(self.towa, t, 0.55 - 0.05 * u, -0.06)

    def cam_holo_wide(self, t, u):
        a, b = self.towa.head_pos(t=t), self.mikaga.head_pos(t=t)
        mid = (a + b) / 2
        pos = mid + np.array([0.6, -0.6, -3.6]) + np.array([0, -0.2, -0.8]) * u
        tgt = anim.lerp(PYLON + np.array([0, 4.0, 0]), PYLON + np.array([0, 7.5, 0]), anim.smooth(u))
        return Cam(pos, tgt, fov=40, aperture=3.0, focus=float(np.linalg.norm(PYLON - pos)))

    def cam_holo_face(self, t, u):
        h = self.holo_head(t)
        pos = np.array([2.0, 1.2, -4.0]) + np.array([0, 0.3, 2.0]) * u
        return Cam(pos, h - np.array([0, 1.5, 0]), fov=26)

    def cam_towa_mem(self, t, u):
        h = self.towa.head_pos(t=t)
        f = self.towa.face_dir(t)
        r = np.cross([0, 1, 0], f)
        return Cam(h + f * 0.9 + r * 0.2, h, fov=28, aperture=12.0, shake=0.02)

    def cam_mem1(self, t, u):
        p = MEM + np.array([-3.0, 1.45, 2.5])
        return Cam(p + np.array([0.2, 0, 0]) * u, MEM + np.array([0.05, 1.35, 2.45]), fov=34, roll=-8)

    def cam_mem2(self, t, u):
        h = self.omiku.head_pos(t=t)
        return Cam(h + np.array([0.35, -0.05, 0.95]), h, fov=28, roll=6, shake=0.03, aperture=10.0)

    def cam_after(self, t, u):
        h = self.towa.head_pos(t=t)
        f = self.towa.face_dir(t)
        r = np.cross([0, 1, 0], f)
        k = anim.smooth(anim.remap(t, 29.8, 31.5))
        pos = h + f * anim.lerp(1.0, 3.6, k) + r * anim.lerp(0.2, -1.6, k) - np.array([0, 0.1, 0])
        tgt = anim.lerp(h, (h + self.mikaga.head_pos(t=t)) / 2, k)
        return Cam(pos, tgt, fov=32, aperture=6.0)

    def grade(self, t):
        g = self.base_grade(t)
        g.tint = (0.98, 0.99, 1.04)
        if t >= HOLO_ON:
            g.tint = (1.05, 0.97, 0.97)
        if in_mem(t):
            g.exposure = 1.6
            g.saturation = 0.35
            g.contrast = 1.25
            g.tint = (1.0, 0.92, 0.9)
            g.grain = 0.04
        for a, b in MEM_SHOTS:
            if a - 0.12 <= t < a + 0.2:
                g.flash = 1.0 - abs(t - a) / 0.2
                g.flash_color = (1, 1, 1)
        if 28.1 <= t < 28.6:
            g.flash = 1 - (t - 28.1) / 0.5
        if HOLO_ON <= t < HOLO_ON + 0.3:
            g.flash = 0.3 * (1 - (t - HOLO_ON) / 0.3)
            g.flash_color = (1.0, 0.6, 0.5)
        return g

    def audio(self, path):
        from cinema import sound as S
        mx = S.Mixer(self.duration)
        mx.add(S.drone(70, 14.0, 3) * S.env(14.0, 1.0, 0, 1, 1.0), 0, 0.12)
        mx.add(S.pad(S.chord("F2 C3 F3 A3"), 8.0, a=2.0, r=1.5, bright=1200), 0.2, 0.25)
        mx.add(S.pad(S.chord("D3 A3 C4 F4"), 6.0, a=1.0, r=1.5, bright=1600), 8.0, 0.22)
        mx.add(S.shimmer(0.8, 4), 11.8, 0.2)
        mx.add(S.zap(0.6, 900, 100), HOLO_ON, 0.3)
        mx.add(S.hit(2.0, 40), HOLO_ON, 0.8)
        mx.add(S.pad(S.chord("C2 G2 C3 D#3 G3"), 11.0, a=0.8, r=1.5, bright=650), HOLO_ON, 0.35)
        for k in range(8):
            mx.add(S.hit(0.8, 50), 15.0 + k * 1.25, 0.3)
        for a, b in MEM_SHOTS:
            mx.add(S.highpass(S.noise(0.6, int(a * 10)), 2500) * S.decay(0.6, 5), a - 0.1, 0.4)
            mx.add(S.riser(0.5, 400, 4000), a - 0.5, 0.25)
        mx.add(S.hit(2.2, 36), 28.1, 0.9)
        mx.add(S.pad(S.chord("A2 E3 A3 C4"), 8.0, a=1.0, r=2.0, bright=900), 28.1, 0.32)
        return mx.save(path)

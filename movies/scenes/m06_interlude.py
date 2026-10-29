"""06 幕間「誘惑と証拠」（約 32 秒）

A. ルナ・ポートのラウンジ。名家の子息ヤマシナ（シヤマ）が懐中時計を手に「家の護衛にならないか」と誘う。
   トワは断り、旅を続ける。
B. 同じころ、旧資料室にもぐり込んだサクラが、軌道都市アマツの軌道が下がっている（落ちている）証拠を偶然つかむ。
"""
import math

import numpy as np

from cinema import anim, fx
from cinema.film import Cam, Shot
from scenes import lib
from scenes.base import Movie

ARC = np.array([8000.0, 0.0, 0.0])        # 資料室
SCREEN_Z = 4.45
FLASH = 27.0


def in_lounge(t):
    return t < 18.0


class FILM(Movie):
    name = "06_interlude"
    duration = 32.0
    near, far = 0.05, 60000.0
    sun = (-0.5, 0.55, 0.65)
    ambient = 0.36

    def __init__(self):
        super().__init__()
        self.captions = [(0.5, 3.6, "ルナ・ポート　ラウンジ"), (18.3, 21.3, "同じころ　旧資料室")]
        self.shots = [
            Shot(0.0, 5.4, self.cam_two, "二人"),
            Shot(5.4, 10.4, self.cam_siyama, "ヤマシナ"),
            Shot(10.4, 13.8, self.cam_towa, "トワ"),
            Shot(13.8, 18.0, self.cam_siyama2, "ヤマシナ"),
            Shot(18.0, 23.6, self.cam_sakura_back, "資料室"),
            Shot(23.6, 26.8, self.cam_screen, "画面"),
            Shot(26.8, 32.0, self.cam_sakura, "サクラ"),
        ]

    def build(self, st):
        lounge = in_lounge
        arc = lambda t: not in_lounge(t)  # noqa: E731
        fx.Stars(st, dist=50000).when = lounge
        fx.Planet(st, "terra", (12000.0, 9000.0, 36000.0), scale=10).when = lounge
        g = fx.Ground(st, center=(0, -30, 0), size=3000, n=160, style="moon", seed=31, craters=[(100, 500, 90, 9)])
        g.when = lounge
        rm = st.ship(lib.lounge().voxels(), size=0.5, pivot=(0, 0, 0), sun=(0.3, 0.8, -0.4), ambient=0.5)
        rm.place((0, 0, 0))
        rm.when = lounge
        ar = st.ship(lib.archive().voxels(), size=0.5, pivot=(0, 0, 0), sun=(0.0, 0.6, -0.8), ambient=0.3)
        ar.place(ARC)
        ar.when = arc
        sy = self.siyama = lib.actor(st, "siyama", (-1.0, 0.5, 2.45), 180.0)
        sy.poses = [(0, lib.pose("sit_watch")), (14.6, lib.pose("sit_watch")), (15.4, lib.pose("sit")), (18, lib.pose("sit"))]
        sy.visible = lounge
        tw = self.towa = lib.actor(st, "towa", (-0.6, 0.0, -0.9), lib.yaw_to((-0.6, 0, -0.9), (-1.0, 0, 2.45)))
        tw.poses = [(0, lib.pose("hands_pockets")), (11.4, lib.pose("hands_pockets")), (12.0, {}), (18, {})]
        tw.visible = lounge
        sy.look = lambda t: (sy.head_pos(t=t) + np.array([0.2, -0.6, -0.6]) if t < 2.3 or 14.7 < t < 15.5 else tw.head_pos(t=t))
        tw.look = lambda t: sy.head_pos(t=t) if t < 11.2 or t > 13.5 else np.array([-0.6, 1.4, -6.0])
        sy.expr = {"happy": anim.Track((0, 0.35), (10.0, 0.25), (11.0, 0.0), (14.4, 0.0), (15.2, 0.45))}
        sy.blinks = lib.blink_every(0, 18, 3.3)
        tw.expr = {"sad": anim.Track((10.5, 0.0), (11.5, 0.3), (13.5, 0.15))}
        tw.blinks = [3.6, 8.1, 11.6, 16.0]
        for a in (sy, tw):
            a.light(key_dir=(0.3, 0.6, -0.7), key_col=(1.05, 0.95, 0.82), fill_col=(0.78, 0.7, 0.7), rim_col=(0.35, 0.45, 0.7))
        sk = self.sakura = lib.actor(st, "sakura")
        sk.place = lib.walk([(18.0, ARC + np.array([0.25, 0, 2.6]), 0.0), (27.8, ARC + np.array([0.25, 0, 2.6]), 0.0),
                             (28.3, ARC + np.array([0.25, 0, 2.5]), 160.0), (31.0, ARC + np.array([-0.6, 0, -2.8]), 175.0),
                             (40, ARC + np.array([-0.6, 0, -6.0]), 175.0)])
        sk.visible = arc
        sk.poses = [(0, lib.pose("type")), (23.8, lib.pose("type")), (24.4, lib.pose("hand_chest")), (26.4, lib.pose("hand_chest")),
                    (26.8, lib.pose("point")), (27.6, lib.pose("point")), (28.2, {}), (40, {})]
        sk.look = lambda t: ARC + np.array([0.25, 2.4, 5.0]) if t < 28.0 else ARC + np.array([-0.6, 1.6, -9.0])
        sk.expr = {"surprised": anim.Track((22.0, 0.0), (24.0, 0.5), (24.4, 0.95), (28.0, 0.7)),
                   "happy": anim.Track((18.0, 0.2), (21.5, 0.2), (22.5, 0.0))}
        sk.blinks = [19.5, 22.3, 29.0]
        sk.light(key_dir=(0.0, 0.3, 1.0), key_col=(1.05, 0.55, 0.5), fill_col=(0.45, 0.5, 0.65), rim_col=(0.3, 0.6, 0.4))
        glow = fx.Sprites(st, "glow", "add", order=12)
        beam = fx.Sprites(st, "beam", "add", order=13)

        def screen(t, pts, lines):
            """画面の絵：アマツの高度のグラフ。途中から下がっていく（赤い警告）。"""
            if in_lounge(t):
                return
            z = ARC[2] + SCREEN_Z
            x0, x1, y0, y1 = ARC[0] - 1.3, ARC[0] + 1.8, 1.7, 3.3
            k = anim.remap(t, 19.0, 23.5)
            n = 40
            pts_l = []
            for i in range(n + 1):
                x = i / n
                y = 0.75 + 0.03 * math.sin(x * 20)
                if x > 0.55:
                    y -= (x - 0.55) ** 1.6 * 1.3
                pts_l.append(np.array([x0 + (x1 - x0) * x, y0 + (y1 - y0) * y, z]))
            m = int(n * k)
            for i in range(m):
                warn = i > n * 0.55
                c = (1.0, 0.25, 0.2, 0.9) if warn else (0.4, 1.0, 0.6, 0.8)
                lines.append(("line", pts_l[i], pts_l[i + 1], 0.04, c))
            for i in range(5):                         # 目盛り
                yy = y0 + (y1 - y0) * i / 4
                lines.append(("line", np.array([x0, yy, z]), np.array([x1, yy, z]), 0.012, (0.3, 0.5, 0.4, 0.4)))
            if t > 23.6:
                a = 0.6 + 0.4 * math.sin(t * 12)
                pts.append((np.array([ARC[0] + 0.25, 2.5, z - 0.02]), 3.2, (1.0, 0.15, 0.1, 0.35 * a), 0.0))
        beam.add(fx.Func(lambda t, pts, lines: screen(t, [], lines)))
        glow.add(fx.Func(lambda t, pts, lines: screen(t, pts, [])))
        glow.add(fx.Glow(lambda t: sk.head_pos(t=t) - np.array([0, 0.45, 0]) + sk.face_dir(t) * 0.3,
                         lambda t: 2.0 * max(0.0, 1 - (t - FLASH) / 0.3), (1, 1, 1, 1), FLASH, FLASH + 0.3))
        glow.add(fx.Glow(lambda t: ARC + np.array([0.25, 2.5, 4.3]), 4.0, (1.0, 0.3, 0.25, 0.25), 18.0, 99))
        self.say("siyama", 2.0, 5.2, "君が噂の「トワ」か。……率直に言おう", sy)
        self.say("siyama", 5.6, 10.0, "僕の家の護衛にならないか。任務も戦争も、忘れて暮らせばいい", sy)
        self.say("towa", 12.0, 13.4, "……行かなきゃ", tw)
        self.say("siyama", 14.2, 17.4, "そうか。……君も、逃げないんだな", sy)
        self.say("sakura", 19.6, 23.2, "アマツの軌道データ……あれ？ 高度が、下がってる？", sk)
        self.say("sakura", 24.4, 26.6, "……これ、落ちてるじゃん！", sk)
        self.say("sakura", 28.4, 30.8, "トワに知らせなきゃ！", sk)

    # --- カメラ ------------------------------------------------------------
    def cam_two(self, t, u):
        pos = np.array([2.6, 1.35, -2.4]) + np.array([-0.3, 0, 0.4]) * u
        return Cam(pos, np.array([-0.9, 1.0, 1.0]), fov=36, aperture=3.0)

    def _close(self, a, t, d, side, up=0.0, fov=30, ap=11.0):
        h = a.head_pos(t=t)
        f = a.face_dir(t)
        r = np.cross([0, 1, 0], f)
        return Cam(h + f * d + r * side + np.array([0, up, 0]), h - np.array([0, 0.05, 0]), fov=fov, aperture=ap)

    def cam_siyama(self, t, u):
        return self._close(self.siyama, t, 1.5 - 0.2 * u, -0.4, 0.15)

    def cam_towa(self, t, u):
        return self._close(self.towa, t, 1.1 - 0.1 * u, 0.25, -0.05)

    def cam_siyama2(self, t, u):
        return self._close(self.siyama, t, 1.25, 0.35, 0.1, ap=12.0)

    def cam_sakura_back(self, t, u):
        pos = ARC + np.array([-1.4, 1.9, -0.6]) + np.array([0.4, -0.1, 0.9]) * anim.smooth(u)
        return Cam(pos, ARC + np.array([0.3, 2.0, 4.2]), fov=36, aperture=4.0,
                   focus=float(np.linalg.norm(self.sakura.head_pos(t=t) - pos)))

    def cam_screen(self, t, u):
        pos = ARC + np.array([0.6, 2.3, 2.0]) + np.array([-0.1, 0.0, 0.5]) * u
        return Cam(pos, ARC + np.array([0.3, 2.5, 4.45]), fov=34, aperture=3.0)

    def cam_sakura(self, t, u):
        h = self.sakura.head_pos(t=t)
        if t < 28.0:
            return Cam(ARC + np.array([0.75, 1.85, 4.2]), h, fov=30, aperture=8.0)
        return Cam(ARC + np.array([1.8, 1.5, -4.5]), h, fov=36, aperture=4.0, shake=0.01)

    def grade(self, t):
        g = self.base_grade(t)
        if in_lounge(t):
            g.tint = (1.05, 0.99, 0.92)
        else:
            g.tint = (0.95, 1.0, 1.05)
            g.vignette = 0.55
            g.lift = (0.0, 0.01, 0.015)
        if 17.6 <= t < 18.4:
            g.fade = min(g.fade, abs(t - 18.0) / 0.4)
        if FLASH <= t < FLASH + 0.3:
            g.flash = 0.8 * (1 - (t - FLASH) / 0.3)
            g.flash_color = (1, 1, 1)
        return g

    def audio(self, path):
        from cinema import sound as S
        mx = S.Mixer(self.duration)
        # ラウンジ：やわらかいピアノのような和音
        for k, ch in enumerate(("F3 A3 C4 E4", "D3 F3 A3 C4", "A#2 D3 F3 A3", "C3 E3 G3 A#3")):
            for j, f in enumerate(S.chord(ch)):
                tone = (S.sine(f, 3.0) + 0.3 * S.sine(f * 2, 3.0)) * S.decay(3.0, 1.6)
                mx.add(tone, 0.3 + k * 4.2 + j * 0.06, 0.08)
        mx.add(S.pad(S.chord("D3 F3 A3"), 4.0, a=0.8, r=1.5, bright=900), 10.8, 0.2)
        # 資料室：機械の音と、だんだん上がる緊張
        mx.add(S.drone(60, 14.0, 9) * S.env(14.0, 0.5, 0, 1, 1.0), 18.0, 0.2)
        for k in range(14):
            mx.add(S.bandpass(S.noise(0.04, 60 + k), 1500, 5000) * 0.6, 18.4 + k * 0.33 + 0.05 * (k % 3), 0.15)
        mx.add(S.riser(4.0, 150, 1800), 20.0, 0.25)
        mx.add(S.alarm(5.0, (980, 740), 2.0) * S.env(5.0, 0.1, 0, 1, 1.0), 23.6, 0.12)
        mx.add(S.hit(1.8, 44), 24.3, 0.8)
        mx.add(S.highpass(S.noise(0.08, 5), 2000) * S.decay(0.08, 50), FLASH, 0.5)
        mx.add(S.pad(S.chord("C2 G2 C3 D#3"), 5.0, a=0.3, r=1.5, bright=900), 27.0, 0.3)
        for k in range(6):
            mx.add(S.lowpass(S.noise(0.1, 80 + k), 900) * S.decay(0.1, 25), 28.4 + k * 0.34, 0.25)
        return mx.save(path)

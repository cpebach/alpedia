"""02 第1章「名前」（約 26 秒）

墜落現場に、特ダネを追ってきた情報屋サクラがやって来る。ポッドの陰から立ち上がるトワ。
識別名「三号」を聞いたサクラが「トワ」と名付け、カメラのシャッターを切る。
"""

import numpy as np

from cinema import anim, fx
from cinema.film import Cam, Shot
from scenes import lib
from scenes.base import Movie

POD = np.array([0.0, 0.0, 0.0])
FLASH = 20.9


class FILM(Movie):
    name = "02_naming"
    duration = 26.0
    near, far = 0.05, 20000.0
    sun = (-0.45, 0.5, 0.75)
    ambient = 0.36

    def __init__(self):
        super().__init__()
        self.captions = [(0.6, 3.6, "惑星ガイア　北の荒れ地　墜落現場")]
        self.shots = [
            Shot(0.0, 5.0, self.cam_wide, "サクラが来る"),
            Shot(5.0, 8.5, self.cam_pod, "立ち上がる"),
            Shot(8.5, 12.5, self.cam_sakura, "サクラ寄り"),
            Shot(12.5, 15.5, self.cam_towa, "トワ寄り"),
            Shot(15.5, 21.5, self.cam_sakura2, "名付け"),
            Shot(21.5, 26.0, self.cam_two, "二人"),
        ]

    def build(self, st):
        fx.Sky(st, top=(0.3, 0.48, 0.78), horizon=(0.82, 0.86, 0.9), below=(0.7, 0.72, 0.75), sun_glow=0.3)
        g = self.ground = fx.Ground(st, size=1600, n=240, style="tundra", seed=4, flat=[(0, 0, 18, 0.0)],
                                    fog=(0.82, 0.86, 0.9), craters=[(0, 0, 7, 1.2)])
        h = g.height
        base = st.ship(lib.base_exterior().voxels(), size=1.0)
        base.place((140, h(140, 330) - 0.5, 330), anim.yaw_pitch_roll(200, 0, 0))
        burnt = st.ship(lib.pod_burnt().voxels(), size=0.5)
        burnt.place((0.0, -0.9, 0.0), anim.yaw_pitch_roll(30, 18, -12))
        smoke = fx.Sprites(st, "smoke", "alpha", order=8)
        smoke.add(fx.Trail(lambda te: np.array([0.4, 1.4, 0.0]), -6, 40, rate=6, life=6, size=(1.0, 6), color=(0.3, 0.29, 0.29, 0.4),
                           drift=0.8, seed=9))
        self.H = lambda x, z: h(x, z)
        sk = self.sakura = lib.actor(st, "sakura")
        sp = lambda x, z: np.array([x, h(x, z), z])  # noqa: E731
        self.sp = sp
        S = sp(-0.4, -3.9)
        self.S = S
        sk.place = lib.walk([(0.0, sp(-20, -24), 40), (4.6, sp(-2.2, -5.6), 45), (7.2, S, 60), (30, S, 60)])
        sk.poses = [(0, {}), (4.4, {}), (5.6, lib.pose("hand_chest")), (9.0, lib.pose("hand_chest")),
                    (11.0, lib.pose("reach")), (12.4, {}), (17.8, {}), (18.4, lib.pose("point")), (21.4, lib.pose("point")),
                    (22.2, lib.pose("wave")), (26, lib.pose("wave"))]
        self.towa_spot = np.array([2.7, h(2.7, -1.0), -1.0])
        tw = self.towa = lib.actor(st, "towa")
        rise = anim.Track((5.0, -1.5), (5.4, -1.5), (6.6, 0.0, "out"))
        tw.place = lambda t: (self.towa_spot + np.array([0, rise(t), 0]), lib.yaw_to(self.towa_spot, S))
        tw.poses = [(0, lib.pose("stand")), (22.6, {}), (23.1, {"head": (0.0, -0.4, 1.0)}), (23.8, {}), (26, {})]
        tw.look = lambda t: sk.head_pos(t=t)
        tw.expr = {"sad": anim.const(0.15)}
        tw.blinks = [7.3, 13.0, 16.4, 19.2, 24.4]
        sk.look = lambda t: (np.array([0.3, 1.5, 0.2]) if t < 5.6 else tw.head_pos(t=t))
        sk.expr = {"happy": anim.Track((0, 0.5), (4.6, 0.5), (5.6, 0.0), (17.5, 0.0), (18.4, 0.85), (26, 0.8)),
                   "surprised": anim.Track((5.2, 0), (5.7, 0.9), (9.0, 0.6), (12.0, 0.35), (15.8, 0.4), (17.6, 0.0))}
        sk.blinks = [3.0, 9.9, 14.2, 17.9, 23.0]
        glow = fx.Sprites(st, "glow", "add", order=12)
        glow.add(fx.Glow(lambda t: sk.head_pos(t=t) - np.array([0, 0.45, 0]) + sk.face_dir(t) * 0.3, lambda t: 2.5 * max(0.0, 1 - (t - FLASH) / 0.3),
                         (1.0, 1.0, 1.0, 1.0), FLASH, FLASH + 0.3))
        self.say("sakura", 1.0, 4.6, "特ダネ、特ダネ……うわ、ほんとに落ちてる！", sk)
        self.say("sakura", 8.8, 12.2, "生きてる！？ ちょ、ちょっと……名前は？", sk)
        self.say("towa", 13.2, 14.6, "……三号", tw)
        self.say("sakura", 15.7, 17.6, "サンゴウ？ それ、番号じゃん", sk)
        self.say("sakura", 18.0, 20.8, "……じゃあ、トワ！ 今日からトワね！", sk)
        self.say("towa", 22.3, 23.6, "……トワ", tw)

    # --- カメラ ------------------------------------------------------------
    def cam_wide(self, t, u):
        pos = self.sp(8.0, -12.5) + np.array([0, 1.3, 0]) + np.array([-0.6, 0.0, 0.8]) * u
        tgt = anim.lerp(self.sakura.head_pos(t=t), np.array([0.0, 1.0, 0.0]), 0.3)
        return Cam(pos, tgt, fov=38, aperture=3.0, focus=float(np.linalg.norm(self.sakura.head_pos(t=t) - pos)))

    def cam_pod(self, t, u):
        tgt = self.towa_spot + np.array([0, 1.2, 0])
        pos = self.sp(4.8, -5.6) + np.array([0, 1.1, 0])
        return Cam(pos, tgt, fov=36, aperture=4.0)

    def _ots(self, a, b, t, d=1.3, side=0.6, fov=30):
        """b の肩越しに a を撮る。"""
        ha, hb = a.head_pos(t=t), b.head_pos(t=t)
        f = anim.unit(ha - hb)
        f[1] = 0
        f = anim.unit(f)
        right = np.cross([0, 1, 0], f)
        pos = hb - f * 0.95 + right * side + np.array([0, 0.1, 0])
        return Cam(pos, ha - np.array([0, 0.05, 0]), fov=fov, aperture=11.0, focus=float(np.linalg.norm(ha - pos)))

    def _close(self, a, t, d=1.1, side=0.25, fov=30, up=0.0):
        ha = a.head_pos(t=t)
        f = a.face_dir(t)
        right = np.cross([0, 1, 0], f)
        pos = ha + f * d + right * side + np.array([0, up, 0])
        return Cam(pos, ha - np.array([0, 0.05, 0]), fov=fov, aperture=10.0, max_blur=14)

    def cam_sakura(self, t, u):
        return self._ots(self.sakura, self.towa, t, fov=22)

    def cam_towa(self, t, u):
        return self._close(self.towa, t, d=1.0 - 0.1 * u, side=-0.2)

    def cam_sakura2(self, t, u):
        return self._close(self.sakura, t, d=1.3 - 0.15 * u, side=0.3, up=-0.1)

    def cam_two(self, t, u):
        a, b = self.sakura.head_pos(t=t), self.towa.head_pos(t=t)
        mid = (a + b) / 2
        f = anim.unit(np.cross(b - a, [0, 1, 0]))
        if f[2] > 0:
            f = -f
        pos = mid + f * (5.2 - 0.6 * u) - np.array([0, 0.7, 0])
        return Cam(pos, mid - np.array([0, 0.3, 0]), fov=34, aperture=3.0)

    def grade(self, t):
        g = self.base_grade(t)
        g.tint = (1.0, 1.0, 1.04)
        if FLASH <= t < FLASH + 0.35:
            g.flash = (1 - (t - FLASH) / 0.35) * 0.9
            g.flash_color = (1, 1, 1)
        return g

    def audio(self, path):
        from cinema import sound as S
        mx = S.Mixer(self.duration)
        wind = S.bandpass(S.noise(self.duration, 21), 180, 1200) * 1.4
        mx.add(wind * (0.8 + 0.2 * np.sin(S._t(self.duration) * 0.7))[:len(wind)], 0, 0.16)
        for k in range(10):                               # 雪を踏む足音
            mx.add(S.bandpass(S.noise(0.12, 40 + k), 600, 3500) * S.decay(0.12, 30), 0.3 + k * 0.45, 0.12)
        mx.add(S.pad(S.chord("D3 F#3 A3"), 7.0, a=1.0, r=1.5, bright=1500), 0.2, 0.22)
        mx.add(S.hit(1.2, 70), 5.5, 0.35)
        mx.add(S.pad(S.chord("B2 D3 F#3"), 6.0, a=1.0, r=1.2, bright=1000), 7.5, 0.22)
        mx.add(S.pad(S.chord("G2 B2 D3 G3"), 6.5, a=1.0, r=1.2, bright=1200), 13.0, 0.24)
        shutter = S.highpass(S.noise(0.08, 5), 2000) * S.decay(0.08, 50)
        mx.add(shutter, FLASH, 0.5)
        mx.add(S.pad(S.chord("D3 F#3 A3 D4"), 6.0, a=0.3, r=2.0, bright=2400), FLASH, 0.3)
        mx.add(S.shimmer(1.5, 3), FLASH + 0.1, 0.18)
        return mx.save(path)



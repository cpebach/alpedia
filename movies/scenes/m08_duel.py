"""08 第12章「解体者」（約 30 秒）

軌道都市アマツの外壁。キンギの黒いロボットが立ちはだかる。トワの白いロボットが降り立ち、
キンギの肩のミサイルを横にかわして、二体が組み合う。
"""
import math

import numpy as np

from cinema import anim, fx, shapes
from cinema.film import Cam, Shot
from scenes import lib
from scenes.base import Movie
from scenes.pilot_fleet import Missile

K_POS = np.array([0.0, 0.0, 30.0])        # キンギのロボットの立つ所（足もと）
T_LAND = np.array([0.0, 0.0, -22.0])      # トワのロボットが降りる所
LEG = 7.5                                 # 腰から足までの長さ（m）
CAB_K = np.array([0.0, 0.0, 40000.0])
CAB_T = np.array([0.0, 0.0, -40000.0])
FIRE = 15.2
CLASH = 24.4


class FILM(Movie):
    name = "08_duel"
    duration = 30.0
    near, far = 0.1, 80000.0
    sun = (-0.5, 0.6, -0.55)
    ambient = 0.3
    subframes = 2

    def __init__(self):
        super().__init__()
        self.captions = [(0.5, 3.8, "軌道都市アマツ　外壁")]
        self.shots = [
            Shot(0.0, 5.0, self.cam_rise, "立ちはだかる"),
            Shot(5.0, 9.0, self.cam_kingi, "キンギ"),
            Shot(9.0, 12.0, self.cam_land, "降り立つ"),
            Shot(12.0, 14.5, self.cam_towa, "トワ"),
            Shot(14.5, 21.0, self.cam_dodge, "かわす"),
            Shot(21.0, 26.0, self.cam_clash, "組み合う"),
            Shot(26.0, 30.0, self.cam_end, "引き"),
        ]

    # --- ロボットの動き -----------------------------------------------------
    def k_root(self, t):
        rise = anim.Track((0.0, -16.0), (3.6, 0.0, "out"))
        x = K_POS + np.array([0, LEG + float(rise(t)), 0])
        if t > 21.0:                                     # 前へ出る
            x = x + np.array([0, 0, -1]) * 13.0 * anim.ease_in_out(anim.remap(t, 21.2, CLASH))
        return x, anim.yaw_pitch_roll(180, 0, 0)

    def k_pose(self, t):
        p = {"sh_l": 10, "sh_r": 10, "ab_l": 12, "ab_r": 12, "el_l": 30, "el_r": 30, "hip_l": 0, "hip_r": 0}
        if t > 21.0:
            p.update(lib.walk_cycle(t, 1.2, 28) if t < CLASH else {})
            k = anim.smooth(anim.remap(t, 23.4, CLASH))
            p["sh_r"] = anim.lerp(10, 95, k)
            p["el_r"] = anim.lerp(30, 5, k)
        if t >= CLASH:
            p.update({"sh_r": 95, "el_r": 5, "sh_l": 70, "el_l": 40, "hip_l": 20, "kn_l": 25, "hip_r": -15})
        return p

    def t_root(self, t):
        land = anim.Track((8.0, 40.0), (11.0, 0.0, "out"))
        x = T_LAND + np.array([0, LEG + float(land(t)), 0])
        side = 0.0
        if t > FIRE + 0.8:                               # 横へかわす
            side = 14.0 * anim.ease_in_out(anim.remap(t, FIRE + 0.9, FIRE + 2.6))
        x = x + np.array([side, 0, 0])
        if t > 21.0:
            x = x + np.array([0, 0, 1]) * 12.0 * anim.ease_in_out(anim.remap(t, 21.2, CLASH))
            x = x - np.array([side, 0, 0]) * anim.smooth(anim.remap(t, 21.0, 23.5))
        lean = 0.0
        if FIRE + 0.8 < t < FIRE + 3.2:
            lean = -18 * math.sin(math.pi * anim.remap(t, FIRE + 0.8, FIRE + 3.2))
        return x, anim.yaw_pitch_roll(0, 0, lean)

    def t_pose(self, t):
        p = {"sh_l": 15, "sh_r": 15, "ab_l": 10, "ab_r": 10, "el_l": 35, "el_r": 35}
        if t < 11.0:
            p.update({"hip_l": 25, "hip_r": 10, "kn_l": 30, "kn_r": 20})
        elif t < 11.6:
            k = anim.remap(t, 11.0, 11.6)
            p.update({"hip_l": 30 * (1 - k), "hip_r": 30 * (1 - k), "kn_l": 60 * math.sin(math.pi * k), "kn_r": 60 * math.sin(math.pi * k)})
        if FIRE + 0.8 < t < FIRE + 3.2:
            p.update({"hip_l": 25, "kn_l": 35, "hip_r": -15, "ab_l": 40, "ab_r": 30})
        if 21.0 < t < CLASH:
            p.update(lib.walk_cycle(t, 1.2, 30))
            k = anim.smooth(anim.remap(t, 23.4, CLASH))
            p["sh_l"] = anim.lerp(15, 95, k)
            p["el_l"] = anim.lerp(35, 5, k)
        if t >= CLASH:
            p.update({"sh_l": 95, "el_l": 5, "sh_r": 70, "el_r": 40, "hip_r": 20, "kn_r": 25, "hip_l": -15})
        return p

    def build(self, st):
        out = lambda t: not (5.0 <= t < 9.0 or 12.0 <= t < 14.5)  # noqa: E731
        lib.space(st, terra=((-6000, -16000, 22000), 22, anim.axis_angle((1, 0, 0), -55)))
        for it in st.items[-2:]:
            it.when = out
        hull = lib.Vox()
        for x in range(-30, 31):
            for z in range(-30, 31):
                hull.put(x, -1, z, "m_white" if (x // 5 + z // 5) % 2 else "m_gray")
                if x % 15 == 0 and z % 6 == 0:
                    hull.put(x, 0, z, "g_amber")
        hp = st.ship(hull.voxels(), size=2.0, pivot=(0, 0, 0))
        hp.place((0, 0, 0))
        hp.when = out
        city = st.ship(lib.amatsu().voxels(), size=lib.AMATSU_SIZE)
        city.place(np.array([-260.0, -160.0, 560.0]), anim.yaw_pitch_roll(-20, 6, 0))
        city.when = out
        kr = self.k_robot = lib.Robot(st, body="m_dark", trim="m_wine", eye="g_red")
        kr.root, kr.pose = self.k_root, self.k_pose
        tr = self.t_robot = lib.Robot(st, body="m_white", trim="m_navy", eye="g_cyan")
        tr.root, tr.pose = self.t_root, self.t_pose
        for part in [kr.torso, kr.head, kr.hips, *kr.parts.values(), tr.torso, tr.head, tr.hips, *tr.parts.values()]:
            part.when = out
        # 操縦席
        for C, col in ((CAB_K, "m_wine"), (CAB_T, "m_navy")):
            cab = lib.Vox()
            for x in range(-3, 4):
                for z in range(-3, 4):
                    for y in range(-1, 5):
                        if abs(x) == 3 or abs(z) == 3 or y in (-1, 4):
                            cab.put(x, y, z, "m_dark" if (x + y) % 3 else col)
            cab.box(-2, 2, 3, 3, 3, 3, "g_red" if col == "m_wine" else "g_screen")
            cb = st.ship(cab.voxels(), size=0.5, pivot=(0, 0, 0), sun=(0, 0.4, 1.0), ambient=0.4)
            cb.place(C - np.array([0, 0.5, 0]))
            cb.when = (lambda t, C=C: (5.0 <= t < 9.0) if C is CAB_K else (12.0 <= t < 14.5))
        kg = self.kingi = lib.actor(st, "kingi", CAB_K + np.array([0, 0.0, -0.6]), 0.0)
        kg.visible = lambda t: 5.0 <= t < 9.0
        kg.poses = [(0, lib.pose("grip"))]
        kg.expr = {"happy": anim.Track((5, 0.2), (8.0, 0.55)), "angry": anim.const(0.15)}
        kg.blinks = [6.2, 8.4]
        kg.look = lambda t: CAB_K + np.array([0, 1.0, 5.0])
        kg.light(key_dir=(0.0, 0.4, 1.0), key_col=(1.0, 0.55, 0.5), fill_col=(0.4, 0.3, 0.35), rim_col=(0.9, 0.2, 0.15))
        tw = self.towa = lib.actor(st, "towa", CAB_T + np.array([0, 0.0, -0.6]), 0.0)
        tw.visible = lambda t: 12.0 <= t < 14.5
        tw.poses = [(0, lib.pose("grip"))]
        tw.expr = {"angry": anim.const(0.3)}
        tw.blinks = [13.9]
        tw.look = lambda t: CAB_T + np.array([0, 1.0, 5.0])
        tw.light(key_dir=(0.0, 0.4, 1.0), key_col=(0.6, 0.85, 1.05), fill_col=(0.35, 0.4, 0.6), rim_col=(0.3, 0.7, 1.0))
        glow = fx.Sprites(st, "glow", "add", order=12)
        beam = fx.Sprites(st, "beam", "add", order=13)
        smoke = fx.Sprites(st, "smoke", "alpha", order=8)
        smoke.when = out
        lib.cigarette(glow, smoke, kg, 5.0, 9.0)
        # トワのロボットの着地の噴射
        for sx in (-1, 1):
            glow.add(fx.Glow(lambda t, sx=sx: tr.joints(t)["ll_" + ("l" if sx < 0 else "r")][0] + np.array([0, -3.6, 0])
                             if (8.0 <= t < 11.4 and out(t)) else None, lambda t: 3.5 + 0.6 * math.sin(t * 33), (0.5, 0.85, 1.0, 0.9)))
            beam.add(fx.Beam(lambda t, sx=sx: (tr.joints(t)["ll_" + ("l" if sx < 0 else "r")][0] + np.array([0, -3.6, 0]),
                                               tr.joints(t)["ll_" + ("l" if sx < 0 else "r")][0] + np.array([0, -9.0, 0]), 1.6,
                                               (0.6, 0.9, 1.0, 0.7)) if (8.0 <= t < 11.4 and out(t)) else None))
        # 目の光
        for r, c in ((kr, (1.0, 0.2, 0.15, 0.9)), (tr, (0.4, 0.9, 1.0, 0.9))):
            glow.add(fx.Glow(lambda t, r=r: (r.joints(t)["head"][0] + r.joints(t)["head"][1] @ np.array([0.0, 1.25, 1.6]))
                             if out(t) else None, 1.6, c))
        # ミサイル（キンギのロボットの肩から）
        self.missiles = []
        for i in range(8):
            sx = -1 if i % 2 else 1
            t0 = FIRE + 0.1 * i
            P0 = self.k_root(t0)[0] + np.array([sx * 3.5, 3.0, -1.0])
            tgt = self.t_root(FIRE + 0.8)[0] + np.array([sx * 2.0, 0.0, 0.0])
            m = Missile(t0, t0 + 2.6, P0, np.array([sx * 0.6, 1.0, -0.2]), tgt + np.array([0, 0, -40.0]), np.array([0.0, -0.2, -1.0]))
            m.p1 = m.p0 + np.array([sx * 6.0, 10.0, -6.0])
            m.p2 = tgt + np.array([sx * 3.0, 4.0, 18.0])
            m.stop = t0 + 2.6
            self.missiles.append(m)
            sh = st.ship(shapes.kmissile().voxels(), size=0.35)
            sh.motion = (lambda t, m=m: (m.pos(t), anim.look_basis(m.dir(t)).T) if m.t0 <= t < m.stop and out(t) else None)
            glow.add(fx.Glow(lambda t, m=m: m.pos(t) - m.dir(t) * 1.4 if m.t0 <= t < m.stop and out(t) else None, 1.6,
                             (1.0, 0.5, 0.2, 0.9)))
            smoke.add(fx.Trail(lambda te, m=m: m.pos(te) if m.t0 <= te < m.stop else None, t0, m.stop, rate=24, life=2.2,
                               size=(0.6, 4.0), color=(0.7, 0.68, 0.66, 0.45), drift=0.6, seed=i + 3))
            e = fx.Explosion(m.stop, m.pos(m.stop - 0.02), scale=0.8, seed=40 + i, n=18)
            glow.add(e)
            smoke.add(e.smoke())
        # 組み合った瞬間
        cp = (self.k_root(CLASH)[0] + self.t_root(CLASH)[0]) / 2 + np.array([0, 2.0, 0])
        ex = fx.Explosion(CLASH, cp, scale=0.9, seed=7, n=20)
        glow.add(ex)
        glow.add(ex.shock())
        fx.Debris(st, CLASH, cp, n=40, speed=14, size=(0.2, 0.7), cols=((0.9, 0.9, 0.92), (0.22, 0.23, 0.27), (0.42, 0.06, 0.08)),
                  seed=5, life=8)
        self.say("kingi", 5.4, 8.8, "ここから先は通さねえ。……解体してやるよ、三号", kg)
        self.say("towa", 12.6, 13.8, "……通る", tw)
        self.say("kingi", 21.4, 23.2, "上等だ！", kg, mouth=False)

    # --- カメラ ------------------------------------------------------------
    def cam_rise(self, t, u):
        pos = np.array([-26.0, 4.0, -6.0]) + np.array([4.0, -1.0, 4.0]) * u
        return Cam(pos, self.k_root(t)[0] + np.array([0, 2.0, 0]), fov=40, roll=-5)

    def cam_kingi(self, t, u):
        h = self.kingi.head_pos(t=t)
        return Cam(h + np.array([0.3, 0.02, 0.95 - 0.1 * u]), h, fov=30, aperture=10.0, shake=0.01)

    def cam_land(self, t, u):
        p = self.t_root(t)[0]
        pos = np.array([14.0, 1.5, -40.0])
        return Cam(pos, p + np.array([0, 1.0, 0]), fov=42, roll=6, shake=0.3 * max(0.0, 1 - abs(t - 11.0) / 0.5))

    def cam_towa(self, t, u):
        h = self.towa.head_pos(t=t)
        return Cam(h + np.array([-0.3, 0.02, 0.95 - 0.1 * u]), h, fov=30, aperture=10.0)

    def cam_dodge(self, t, u):
        k = self.k_root(t)[0]
        tw = self.t_root(t)[0]
        mid = (k + tw) / 2
        pos = mid + np.array([-38.0, 6.0, -8.0]) + np.array([0, 0, 6.0]) * u
        return Cam(pos, mid + np.array([4.0, 0, 0]), fov=46, shake=0.15 if t > FIRE + 2.5 else 0.03)

    def cam_clash(self, t, u):
        tw = self.t_root(t)[0]
        pos = tw + np.array([9.0, -3.0, -16.0]) + np.array([2.0, 0, 3.0]) * u
        tgt = (tw + self.k_root(t)[0]) / 2 + np.array([0, 2.0, 0])
        return Cam(pos, tgt, fov=44, roll=-8, shake=0.6 * max(0.0, 1 - abs(t - CLASH) / 0.8))

    def cam_end(self, t, u):
        mid = (self.t_root(t)[0] + self.k_root(t)[0]) / 2
        pos = mid + np.array([-60.0, 20.0, -50.0]) + np.array([-10.0, 5.0, -10.0]) * u
        return Cam(pos, mid, fov=40)

    def grade(self, t):
        g = self.base_grade(t)
        g.bloom = 0.5
        g.bloom_threshold = 0.75
        g.contrast = 1.1
        if 5.0 <= t < 9.0:
            g.tint = (1.06, 0.94, 0.94)
        if CLASH <= t < CLASH + 0.4:
            g.flash = 0.8 * (1 - (t - CLASH) / 0.4)
            g.aberration = 6 * (1 - (t - CLASH) / 0.4)
        return g

    def audio(self, path):
        from cinema import sound as S
        mx = S.Mixer(self.duration)
        mx.add(S.pad(S.chord("C2 G2 C3 D#3"), 5.5, a=1.0, r=1.0, bright=650), 0.0, 0.35)
        for k in range(4):
            mx.add(S.hit(1.2, 40), 0.6 + k * 0.9, 0.5)
        mx.add(S.drone(48, 4.0, 3), 5.0, 0.25)
        mx.add(S.pad(S.chord("D2 A2 D3 F3"), 6.0, a=0.6, r=1.0, bright=800), 9.0, 0.3)
        mx.add(S.lowpass(S.noise(3.2, 5), 1200) * S.env(3.2, 0.2, 0, 1, 0.6) * 1.6, 8.0, 0.4)
        mx.add(S.boom(2.0, 1.0, 4), 11.0, 0.7)
        for m in self.missiles:
            mx.add(S.launch(1.2, int(m.t0 * 100) % 999), m.t0, 0.25)
            mx.add(S.boom(1.6, 0.8, int(m.stop * 10) % 999), m.stop, 0.25)
        mx.add(S.whoosh(1.8, 200, 2500, 5), FIRE + 0.8, 0.5)
        for k in range(12):
            mx.add(S.hit(0.9, 46 if k % 2 == 0 else 58), 14.5 + 0.5 * k, 0.35)
        for k in range(5):
            mx.add(S.lowpass(S.noise(0.4, 90 + k), 300) * S.decay(0.4, 8) * 2, 21.3 + k * 0.42, 0.4)
        mx.add(S.riser(1.2, 150, 2000), CLASH - 1.2, 0.4)
        mx.add(S.boom(3.0, 1.4, 6), CLASH, 0.9)
        mx.add(S.hit(2.0, 36), CLASH, 0.9)
        mx.add(S.pad(S.chord("D2 A2 D3 F3 A3"), 5.0, a=0.3, r=2.0, bright=1400), CLASH, 0.35)
        return mx.save(path)

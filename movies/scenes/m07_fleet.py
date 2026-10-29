"""07 第11章「艦隊戦」（約 32 秒）

地球の高軌道、軌道都市アマツの宙域。連盟艦隊（白と紺）の旗艦アルテミスでは「艦王」ケンスが前進を命じ、
帝国艦隊（黒と赤）の旗艦ではシロツ提督がミサイル隊をキンギに任せる。
キンギのミサイルの雨の中を、トワの小さなルミナス号がすり抜け、アマツへ向かう。
"""
import math

import numpy as np

from cinema import anim, fx, shapes
from cinema.film import Cam, Shot
from scenes import lib
from scenes.base import Movie
from scenes.pilot_fleet import Missile

VU = np.array([0.0, 0.0, 10.0])
VE = np.array([0.0, 0.0, -8.0])
R_E = anim.yaw_pitch_roll(180, 0, 0)
UNION = {"artemis": (0, 0, 0), "U1": (-95, 14, -30), "U2": (100, -12, -25), "U3": (-190, -28, -120), "U4": (200, 22, -140)}
EMPIRE = {"E0": (70, 45, 1100), "E1": (-150, 12, 960), "E2": (210, -26, 1010), "E3": (-30, -40, 900)}
AMATSU = np.array([1100.0, 260.0, 1750.0])
BR_U = np.array([0.0, 0.0, -30000.0])          # 連盟の艦橋の部屋
BR_E = np.array([0.0, 0.0, 30000.0])           # 帝国の艦橋の部屋
LAUNCH = 16.4
L0 = 18.0                                       # ルミナス号が飛び出す


def lum_path():
    keys = [(L0 - 2.0, (40.0, 22.0, -60.0)), (L0, (40.0, 20.0, 60.0)), (21.0, (150.0, 60.0, 360.0)), (23.0, (240.0, 40.0, 560.0)),
            (25.0, (380.0, 110.0, 780.0)), (27.0, (560.0, 150.0, 1040.0)), (29.5, (800.0, 210.0, 1380.0)),
            (32.5, AMATSU + np.array([-140.0, 0.0, -260.0]))]
    return anim.Track(*[(t, p, "linear") for t, p in keys])


class FILM(Movie):
    name = "07_fleet"
    duration = 32.0
    near, far = 0.3, 90000.0
    sun = (-0.35, 0.85, -0.4)
    ambient = 0.28
    subframes = 2

    def __init__(self):
        super().__init__()
        self.captions = [(0.5, 4.0, "地球高軌道　軌道都市アマツ宙域")]
        self.shots = [
            Shot(0.0, 4.5, self.cam_wide, "両艦隊"),
            Shot(4.5, 9.0, self.cam_kensu, "ケンス"),
            Shot(9.0, 13.0, self.cam_sirotsu, "シロツ"),
            Shot(13.0, 16.0, self.cam_kingi, "キンギ"),
            Shot(16.0, 21.0, self.cam_salvo, "ミサイルの雨"),
            Shot(21.0, 24.0, self.cam_chase, "すり抜ける"),
            Shot(24.0, 25.6, self.cam_towa, "トワ"),
            Shot(25.6, 32.0, self.cam_break, "突破"),
        ]
        self.lum = lum_path()

    def build(self, st):
        sp = lambda t: not (4.5 <= t < 16.0 or 24.0 <= t < 25.6)  # noqa: E731
        lib.space(st, terra=((6000, -18000, 26000), 20, anim.axis_angle((1, 0, 0), -65)))
        for it in st.items[-2:]:
            it.when = sp
        self.s = {}
        for name, p in UNION.items():
            v = lib.fleet_ship("union_flag" if name == "artemis" else "union")
            sh = st.ship(v.voxels(), size=1.0, name=name)
            sh.motion = (lambda t, p=np.array(p, float): (p + VU * t, np.eye(3)) if sp(t) else None)
            self.s[name] = sh
        for name, p in EMPIRE.items():
            v = lib.fleet_ship("empire_flag" if name == "E0" else "empire")
            sh = st.ship(v.voxels(), size=1.0, name=name)
            sh.motion = (lambda t, p=np.array(p, float): (p + VE * t, R_E) if sp(t) else None)
            self.s[name] = sh
        city = st.ship(lib.amatsu().voxels(), size=lib.AMATSU_SIZE, name="amatsu")
        city.motion = lambda t: (AMATSU, anim.yaw_pitch_roll(-35, 8, 0) @ anim.axis_angle((0, 0, 1), 3 * t)) if sp(t) else None
        rng = np.random.default_rng(9)
        for k in range(8):
            un = k < 4
            p = np.array([rng.uniform(-700, 700), rng.uniform(-150, 120),
                          rng.uniform(-900, -400) if un else rng.uniform(1500, 2200)])
            v = shapes.far_hull(mat="m_white" if un else "m_dark", band="m_navy" if un else "m_wine",
                                light="g_blue" if un else "g_red")
            sh = st.ship(v.voxels(), size=1.6)
            sh.motion = (lambda t, p=p, un=un: (p + (VU if un else VE) * t, np.eye(3) if un else R_E) if sp(t) else None)
        lum = self.luminous = st.ship(lib.luminous_voxels(), size=0.5, name="luminous")

        def lum_motion(t):
            if not sp(t) or t < L0 - 2.0:
                return None
            p = self.lum(t) + np.array([math.sin(t * 2.3) * 6, math.sin(t * 3.1) * 4, 0])
            d = anim.unit(self.lum(t + 0.1) - self.lum(t))
            roll = 35 * math.sin(t * 2.3)
            return p, anim.look_basis(d, (0, 1, 0), roll).T
        lum.motion = lum_motion
        self._lum_motion = lum_motion
        # 艦橋（連盟・帝国）
        bu = st.ship(lib.bridge("union").voxels(), size=0.5, pivot=(0, 0, 0), sun=(0.25, 0.85, -0.45), ambient=0.5)
        bu.place(BR_U)
        bu.when = lambda t: 4.5 <= t < 9.0
        be = st.ship(lib.bridge("empire").voxels(), size=0.5, pivot=(0, 0, 0), sun=(0.25, 0.85, -0.45), ambient=0.42)
        be.place(BR_E)
        be.when = lambda t: 9.0 <= t < 16.0
        ks = self.kensu = lib.actor(st, "kensu", BR_U + np.array([0.8, 0.0, 3.6]), 0.0)
        ks.visible = lambda t: 4.5 <= t < 9.0
        ks.poses = [(0, lib.pose("hands_back")), (6.0, lib.pose("hands_back")), (7.0, lib.pose("command")), (9, lib.pose("command"))]
        ks.expr = {"happy": anim.Track((4.5, 0.6), (9, 0.7))}
        ks.blinks = [5.4, 8.2]
        ks.look = lambda t: BR_U + np.array([0.0, 1.6, 30.0])
        sr = self.sirotsu = lib.actor(st, "sirotsu", BR_E + np.array([0.4, 0.0, 3.6]), 0.0)
        sr.visible = lambda t: 9.0 <= t < 16.0
        sr.poses = [(0, lib.pose("crossed")), (10.8, lib.pose("crossed")), (11.6, lib.pose("point")), (16, lib.pose("point"))]
        sr.expr = {"angry": anim.Track((9, 0.2), (11.5, 0.35)), "happy": anim.Track((9, 0.2), (11.5, 0.0))}
        sr.blinks = [10.2, 12.9, 15.0]
        kg = self.kingi = lib.actor(st, "kingi", BR_E + np.array([-1.5, 0.0, 2.4]), 30.0)
        kg.visible = sr.visible
        kg.poses = [(0, lib.pose("smoke")), (14.6, lib.pose("smoke")), (15.2, lib.pose("hands_pockets"))]
        kg.expr = {"happy": anim.Track((13, 0.2), (14.6, 0.55))}
        kg.blinks = [11.0, 14.0]
        sr.look = lambda t: BR_E + np.array([0.0, 1.6, 30.0]) if t < 11.4 else kg.head_pos(t=t)
        kg.look = lambda t: sr.head_pos(t=t) if t < 14.0 else BR_E + np.array([0.0, 1.6, 30.0])
        tw = self.towa = lib.actor(st, "towa")
        tw.place = lambda t: (np.array([0.0, 0.0, 60000.0]), 0.0)
        tw.visible = lambda t: 24.0 <= t < 25.6
        tw.poses = [(0, lib.pose("grip"))]
        tw.expr = {"angry": anim.const(0.3)}
        tw.look = lambda t: np.array([0.0, 1.5, 60010.0])
        cab = lib.Vox()
        for x in range(-3, 4):
            for z in range(-3, 4):
                for y in range(-1, 5):
                    if abs(x) == 3 or abs(z) == 3 or y in (-1, 4):
                        cab.put(x, y, z, "m_dark" if (x + y) % 3 else "m_panel")
        cab.box(-2, 2, 3, 3, 3, 3, "g_screen")
        cb = st.ship(cab.voxels(), size=0.5, pivot=(0, 0, 0), sun=(0, 0.4, 1.0), ambient=0.4)
        cb.place(np.array([0.0, -0.5, 60000.0]))
        cb.when = tw.visible
        for a in (ks, sr, kg, tw):
            a.light(key_dir=(0.2, 0.6, 1.0), key_col=(1.0, 0.97, 0.95), fill_col=(0.6, 0.62, 0.78), rim_col=(0.4, 0.5, 0.8))
        sr.light(key_dir=(0.2, 0.6, 1.0), key_col=(1.0, 0.92, 0.9), fill_col=(0.6, 0.45, 0.48), rim_col=(0.9, 0.2, 0.15))
        kg.light(key_dir=(0.2, 0.6, 1.0), key_col=(1.0, 0.92, 0.9), fill_col=(0.6, 0.45, 0.48), rim_col=(0.9, 0.2, 0.15))
        tw.light(key_dir=(0.0, 0.4, 1.0), key_col=(1.0, 0.6, 0.5), fill_col=(0.4, 0.5, 0.8), rim_col=(0.3, 0.7, 1.0))
        self._missiles(st, sp)
        glow = self.glow = fx.Sprites(st, "glow", "add", order=12)
        beam = fx.Sprites(st, "beam", "add", order=13)
        smoke = fx.Sprites(st, "smoke", "alpha", order=8)
        smoke.when = sp
        for m in self.missiles:
            glow.add(fx.Glow(lambda t, m=m: m.pos(t) - m.dir(t) * 2.0 if (m.t0 <= t < m.stop and sp(t)) else None,
                             lambda t: 3.4, (1.0, 0.45, 0.2, 0.9)))
            beam.add(fx.Beam(lambda t, m=m: (m.pos(t) - m.dir(t) * 1.6, m.pos(t) - m.dir(t) * 11.0, 1.3, (1.0, 0.6, 0.3, 0.85))
                             if (m.t0 <= t < m.stop and sp(t)) else None))
            smoke.add(fx.Trail(lambda te, m=m: m.pos(te) - m.dir(te) * 3.0 if m.t0 <= te < m.stop else None, m.t0 + 0.05, m.stop,
                               rate=14, life=2.4, size=(1.4, 7.0), color=(0.6, 0.55, 0.55, 0.4), drift=1.2, seed=int(m.t0 * 100) % 999))
            e = fx.Explosion(m.stop, m.pos(m.stop - 0.02), scale=0.7, seed=int(m.stop * 77) % 999, n=16)
            glow.add(e)
            smoke.add(e.smoke())
        # ルミナス号の近くで爆発するもの（すり抜ける演出）
        for k, te in enumerate((21.6, 22.4, 23.1, 26.1, 26.9, 27.6, 28.3)):
            p = self.lum(te) + np.array([(-1) ** k * (18 + 6 * k % 11), 10 * math.sin(k * 2.0), 40 + 10 * k % 25])
            e = fx.Explosion(te, p, scale=1.2, seed=100 + k, n=22)
            glow.add(e)
            glow.add(e.shock())
            smoke.add(e.smoke())
        # 迎撃のビーム（連盟の艦から）
        for k in range(16):
            src = list(UNION)[k % 5]
            te = 18.5 + k * 0.45
            glow_t = te

            def pd(t, src=src, te=glow_t, k=k):
                if not (te <= t < te + 0.35) or not sp(t):
                    return None
                a = np.array(UNION[src], float) + VU * t + np.array([0, 8, 30])
                b = a + anim.unit(np.array([math.sin(k * 1.7) * 0.4, math.cos(k * 2.1) * 0.25, 1.0])) * 600
                return (a, b, 1.2, (0.5, 0.95, 1.0, 0.85))
            beam.add(fx.Beam(pd))
        # ルミナス号の噴射
        glow.add(fx.Glow(lambda t: lum.world(np.array([1.25, 1.25, -0.4])) if (sp(t) and t >= L0 - 2) else None,
                         lambda t: 3.0 + 0.5 * math.sin(t * 30), (0.5, 0.85, 1.0, 0.95)))
        smoke.add(fx.Trail(lambda te: self.lum(te) if te >= L0 - 2 else None, L0 - 2, 40, rate=24, life=1.2, size=(0.8, 3.0),
                           color=(0.7, 0.85, 1.0, 0.3), drift=0.3, seed=55))
        # 艦の噴射の光
        for name in UNION:
            sh = self.s[name]
            for x in (-3, 0, 3):
                glow.add(fx.Glow(lambda t, sh=sh, x=x: sh.world((x + 0.5, 3.5, -3.0)) if sp(t) else None, 5.0, (0.45, 0.85, 1.0, 0.85)))
        for name in EMPIRE:
            sh = self.s[name]
            for x in (-4, 0, 4):
                glow.add(fx.Glow(lambda t, sh=sh, x=x: sh.world((x + 0.5, 2.5, -4.0)) if sp(t) else None, 5.5, (1.0, 0.45, 0.15, 0.9)))
        lib.cigarette(glow, smoke, kg, 9.0, 16.0)
        self.say("kensu", 5.0, 8.6, "さーて、艦王さまのお仕事だ。全艦、前へ！", ks)
        self.say("sirotsu", 9.3, 12.8, "来たな、艦王……！ ミサイル隊、キンギに任せる", sr)
        self.say("kingi", 13.3, 15.6, "了解。雨を降らせてやるよ", kg)
        self.say("kensu", 19.0, 20.8, "迎撃！ 一発も通すな！")
        self.say("towa", 24.3, 25.3, "……抜ける", tw)

    def _missiles(self, st, sp):
        rng = np.random.default_rng(3)
        self.missiles = []
        launchers = ["E1", "E2", "E3", "E0"]
        targets = list(UNION)
        for i in range(48):
            src = launchers[i % 4]
            t0 = LAUNCH + 0.065 * i
            sp0 = np.array(EMPIRE[src], float) + VE * t0 + np.array([rng.uniform(-6, 6), 8.0, rng.uniform(-10, 10)])
            tgt = targets[i % 5]
            t1 = t0 + rng.uniform(5.5, 9.0)
            end = np.array(UNION[tgt], float) + VU * t1 + np.array([rng.uniform(-10, 10), rng.uniform(0, 8), rng.uniform(10, 40)])
            up = anim.unit(np.array([rng.uniform(-0.7, 0.7), 1.0, rng.uniform(-0.3, 0.3)]))
            m = Missile(t0, t1, sp0, up, end, anim.unit(end - sp0 + np.array([rng.uniform(-200, 200), rng.uniform(-80, 120), 0])))
            m.stop = t1 - rng.uniform(0.3, 2.0)
            self.missiles.append(m)
            sh = st.ship(shapes.kmissile().voxels(), size=0.55, name="missile")
            sh.motion = (lambda t, m=m: (m.pos(t), anim.look_basis(m.dir(t)).T) if (m.t0 <= t < m.stop and sp(t)) else None)

    # --- カメラ ------------------------------------------------------------
    def cam_wide(self, t, u):
        a = np.array(UNION["artemis"], float) + VU * t
        pos = a + anim.lerp(np.array([-140.0, 50.0, -240.0]), np.array([-110.0, 36.0, -150.0]), anim.ease_in_out(u))
        tgt = a + anim.lerp(np.array([200.0, 20.0, 600.0]), np.array([300.0, 40.0, 800.0]), anim.ease_in_out(u))
        return Cam(pos, tgt, fov=40, roll=-7)

    def _close(self, a, t, d, side, up=0.0, fov=30, ap=10.0, tgt_side=0.0):
        h = a.head_pos(t=t)
        f = a.face_dir(t)
        r = np.cross([0, 1, 0], f)
        return Cam(h + f * d + r * side + np.array([0, up, 0]), h - np.array([0, 0.05, 0]) + r * tgt_side, fov=fov, aperture=ap)

    def cam_kensu(self, t, u):
        return self._close(self.kensu, t, 2.6 - 0.6 * anim.smooth(u), 0.6, -0.2, fov=32, tgt_side=0.3)

    def cam_sirotsu(self, t, u):
        return self._close(self.sirotsu, t, 2.4 - 0.4 * u, -0.9, -0.1, fov=34, tgt_side=-0.6)

    def cam_kingi(self, t, u):
        return self._close(self.kingi, t, 1.3, 0.35, -0.1, fov=30)

    def cam_salvo(self, t, u):
        e = np.array(EMPIRE["E3"], float) + VE * t
        pos = e + np.array([-55.0, -14.0, -95.0]) + np.array([10.0, 6.0, -30.0]) * u
        tgt = e + np.array([-12.0, 14.0, -40.0])
        return Cam(pos, tgt, fov=46, roll=8, shake=0.1 if t > LAUNCH else 0.0)

    def cam_chase(self, t, u):
        p = self._lum_motion(t)[0]
        d = anim.unit(self.lum(t + 0.1) - self.lum(t))
        side = anim.unit(np.cross(d, [0, 1, 0]))
        pos = p - d * 11 + side * 4 + np.array([0, 2.2, 0])
        return Cam(pos, p + d * 25, fov=44, roll=anim.wobble(t, 3, 0.6) * 6, shake=0.08, aperture=2.0,
                   focus=float(np.linalg.norm(p - pos)))

    def cam_towa(self, t, u):
        h = self.towa.head_pos(t=t)
        return Cam(h + np.array([0.25, 0.05, 0.95]), h, fov=30, aperture=10.0, shake=0.03)

    def cam_break(self, t, u):
        p = self._lum_motion(t)[0]
        d = anim.unit(self.lum(t + 0.1) - self.lum(t))
        side = anim.unit(np.cross(d, [0, 1, 0]))
        k = anim.smooth(anim.remap(t, 29.0, 32.0))
        pos = p - d * anim.lerp(14, 30, k) - side * 6 + np.array([0, anim.lerp(3, 9, k), 0])
        tgt = anim.lerp(p + d * 30, AMATSU, k)
        return Cam(pos, tgt, fov=anim.lerp(44, 50, k), roll=anim.wobble(t, 5, 0.5) * 5, shake=0.06)

    def grade(self, t):
        g = self.base_grade(t)
        g.bloom = 0.55
        g.bloom_threshold = 0.72
        g.contrast = 1.1
        if 9.0 <= t < 16.0:
            g.tint = (1.06, 0.95, 0.95)
        return g

    def audio(self, path):
        from cinema import sound as S
        mx = S.Mixer(self.duration)
        mx.add(S.pad(S.chord("D2 A2 D3 F3"), 5.0, a=1.5, r=1.0, bright=800), 0, 0.35)
        mx.add(S.drone(36, 4.6, 1), 0, 0.3)
        mx.add(S.drone(58, 4.6, 2) * S.env(4.6, 0.3, 0, 1, 0.5), 4.5, 0.18)
        mx.add(S.pad(S.chord("D3 F#3 A3 D4"), 4.5, a=0.5, r=1.0, bright=1500), 4.5, 0.25)
        mx.add(S.alarm(7.0, (880, 660), 1.4) * S.env(7.0, 0.2, 0, 1, 1.0), 9.0, 0.08)
        mx.add(S.pad(S.chord("C2 G2 C3 D#3"), 7.2, a=0.6, r=1.0, bright=650), 9.0, 0.32)
        mx.add(S.hit(1.8, 40), 16.0, 0.8)
        for k in range(30):
            tt = 16.0 + 0.5 * k
            if tt > 31.5:
                break
            mx.add(S.hit(0.9, 48 if k % 2 == 0 else 62), tt, 0.4 if k % 4 == 0 else 0.22)
        for m in self.missiles[::2]:
            mx.add(S.launch(1.3, int(m.t0 * 100) % 999), m.t0, 0.18, pan=float(np.clip(m.p0[0] / 250, -0.8, 0.8)))
        for m in self.missiles:
            mx.add(S.boom(1.4, 0.6, int(m.stop * 10) % 999), m.stop, 0.12)
        for k in range(16):
            mx.add(S.zap(0.3, 2600 + 120 * (k % 4), 400), 18.5 + k * 0.45, 0.1, pan=0.4 * math.sin(k))
        for k, te in enumerate((21.6, 22.4, 23.1, 26.1, 26.9, 27.6, 28.3)):
            mx.add(S.boom(2.0, 1.0, 20 + k), te, 0.45, pan=0.5 * (-1) ** k)
        mx.add(S.whoosh(10.0, 300, 3000, 7), 21.0, 0.3)
        mx.add(S.pad(S.chord("D3 A3 D4 F#4 A4"), 4.5, a=1.0, r=1.5, bright=2400), 27.5, 0.32)
        mx.add(S.hit(2.0, 44), 29.0, 0.7)
        return mx.save(path)

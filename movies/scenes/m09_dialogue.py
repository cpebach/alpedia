"""09 終章A「対話」（約 45 秒）

地球へ落ちていく軌道都市アマツ。大気に触れた先端がプラズマの光をまとう。
中心の広間（非常灯の赤、大きな窓の外に近づく地球）で、トワはネムリと向き合う。
撃つことも、都市が落ちることも、どちらも選ばない。入口ではミカガが二人を見ている。
"""
import math

import numpy as np

from cinema import anim, fx
from cinema.film import Cam, Shot
from cinema.shapes import Vox
from scenes import lib
from scenes.base import Movie

EXT = np.array([0.0, 0.0, -90000.0])           # 外から見た都市
IN_T = 4.5                                       # ここから中
NEM = np.array([0.0, 0.0, 3.4])                  # ネムリの立つ所（窓の前）
GUN_UP, GUN_DOWN = 14.4, 20.6


def inside(t):
    return t >= IN_T


def outside(t):
    return t < IN_T


def gun_voxels():
    v = Vox()
    v.box(-1, 1, -1, 1, 0, 8, "m_black")          # 本体（前 = +Z）
    v.box(-1, 1, -6, -1, 0, 2, "m_dark")          # 握り
    v.put(0, 2, 7, "g_red")
    return v


class FILM(Movie):
    name = "09_dialogue"
    duration = 45.0
    near, far = 0.05, 120000.0
    sun = (0.35, 0.45, 0.82)
    ambient = 0.3

    def __init__(self):
        super().__init__()
        self.captions = [(0.6, 4.0, "終章　落ちていく都市アマツ"), (5.0, 8.0, "アマツ　中心区画")]
        self.shots = [
            Shot(0.0, IN_T, self.cam_ext, "落ちる都市"),
            Shot(IN_T, 9.2, self.cam_enter, "入る"),
            Shot(9.2, 13.6, self.cam_nemuri_turn, "ネムリ"),
            Shot(13.6, 19.4, self.cam_over_towa, "撃て"),
            Shot(19.4, 22.2, self.cam_towa, "撃たない"),
            Shot(22.2, 25.2, self.cam_nemuri, "落ちる"),
            Shot(25.2, 28.2, self.cam_towa_push, "落とさない"),
            Shot(28.2, 34.2, self.cam_side, "三つ目"),
            Shot(34.2, 39.4, self.cam_nemuri_soft, "答え"),
            Shot(39.4, 45.0, self.cam_mikaga, "ミカガ"),
        ]

    # --- 舞台 ----------------------------------------------------------------
    def build(self, st):
        fx.Stars(st, dist=100000)
        # 外：落ちる都市と地球
        e1 = fx.Planet(st, "terra", (0, 0, 0), scale=30, spin=0.02, tilt=anim.axis_angle((1, 0, 0), -40))
        e1.center = EXT + np.array([0.0, -e1.radius * 30 - 2600.0, 9000.0])
        e1.when = outside
        city = self.city = st.ship(lib.amatsu().voxels(), size=lib.AMATSU_SIZE)
        Rc = anim.axis_angle((1, 0, 0), 62) @ anim.axis_angle((0, 0, 1), 12)
        city.motion = lambda t: (EXT + np.array([0, -14.0 * t, 0]), Rc @ anim.axis_angle((0, 0, 1), 1.2 * t)) if outside(t) else None
        # 中：窓の外の地球（だんだん近づく）
        e2 = self.terra = fx.Planet(st, "terra", (0, 0, 0), scale=24, spin=0.02, tilt=anim.axis_angle((1, 0, 0), -30))
        R2 = e2.radius * 24
        dirn = anim.unit(np.array([0.15, -0.62, 1.0]))
        up0 = e2.update

        def upd(t, origin, up0=up0):
            d = R2 * anim.lerp(1.55, 1.32, anim.remap(t, IN_T, 45.0))
            e2.center = dirn * d
            up0(t, origin)
        e2.update = upd
        e2.when = inside
        rm = st.ship(lib.city_core().voxels(), size=0.5, pivot=(0, 0, 0), sun=(0.1, 0.35, -1.0), ambient=0.46)
        rm.place((0, 0, 0))
        rm.when = inside
        # 人
        nm = self.nemuri = lib.actor(st, "nemuri")
        turn = anim.Track((9.6, 0.0), (11.0, 180.0, "inout"))
        nm.place = lambda t: (NEM, float(turn(t)))
        nm.poses = [(0, lib.pose("hands_back")), (34.0, lib.pose("hands_back")), (35.0, {}), (45, {})]
        tw = self.towa = lib.actor(st, "towa")
        tw.place = lib.walk([(0.0, (0.7, 0.0, -5.6), 0.0), (IN_T, (0.7, 0.0, -5.4), 0.0), (8.6, (0.45, 0.0, -2.3), -4.0),
                             (45.0, (0.45, 0.0, -2.3), -4.0)])
        tw.poses = [(0, {}), (GUN_UP - 0.5, {}), (GUN_UP + 0.4, lib.pose("point")), (GUN_DOWN, lib.pose("point")),
                    (GUN_DOWN + 1.0, {}), (28.6, {}), (29.4, lib.pose("hand_chest")), (33.0, lib.pose("hand_chest")),
                    (34.0, {}), (45, {})]
        mk = self.mikaga = lib.actor(st, "mikaga", (-2.6, 0.0, -5.2), 30.0)
        mk.visible = lambda t: t > 26.0
        mk.poses = [(0, lib.pose("hands_back")), (45, lib.pose("hands_back"))]
        nm.look = lambda t: (NEM + np.array([0, 1.5, 10.0]) if t < 10.0 else
                             NEM + np.array([0.3, 0.6, -2.0]) if 36.2 < t < 38.6 else tw.head_pos(t=t))
        tw.look = lambda t: (mk.head_pos(t=t) if 42.6 < t else nm.head_pos(t=t))
        mk.look = lambda t: (tw.head_pos(t=t) if t > 42.4 else (tw.head_pos(t=t) + nm.head_pos(t=t)) / 2)
        nm.expr = {"angry": anim.Track((10.5, 0.0), (11.5, 0.15), (19.0, 0.15), (19.5, 0.0), (22.3, 0.0), (23.0, 0.2), (25.0, 0.2), (27.0, 0.0)),
                   "surprised": anim.Track((25.6, 0.0), (26.2, 0.35), (28.0, 0.1), (31.0, 0.15), (33.0, 0.4), (34.5, 0.0)),
                   "sad": anim.Track((34.0, 0.0), (35.0, 0.45), (40.0, 0.35)),
                   "happy": anim.Track((35.5, 0.0), (37.5, 0.3), (40.0, 0.25))}
        nm.blinks = [12.6, 16.2, 21.0, 24.4, 29.0, 32.6, 36.3, 37.0, 41.0]
        tw.expr = {"sad": anim.Track((14.0, 0.0), (15.0, 0.25), (19.5, 0.35), (21.5, 0.15), (35.0, 0.15), (38.0, 0.35)),
                   "angry": anim.Track((24.6, 0.0), (25.6, 0.3), (27.8, 0.2), (28.6, 0.0))}
        tw.blinks = [6.2, 11.0, 16.5, 19.9, 23.0, 27.9, 31.0, 36.0, 40.5, 43.5]
        mk.expr = {"happy": anim.Track((39.0, 0.0), (40.4, 0.4), (45, 0.45)), "sad": anim.Track((26, 0.2), (39.0, 0.2), (40.0, 0.0))}
        mk.blinks = lib.blink_every(26, 45, 3.4)
        for a in (nm, tw, mk):
            a.light(key_dir=(0.15, 0.35, 1.0), key_col=(1.15, 0.72, 0.5), fill_col=(0.55, 0.35, 0.4), rim_col=(1.0, 0.18, 0.12))
        # 銃（トワの右手）
        gun = st.ship(gun_voxels().voxels(), size=0.028, pivot=(0, -1, 1))

        def gun_motion(t):
            if not inside(t):
                return None
            h = tw.bone_world("rightHand", t)
            e = tw.bone_world("rightLowerArm", t)
            if h is None or e is None:
                return None
            f = anim.unit(h - e)
            side = np.cross([0.0, 1.0, 0.0], f)
            if np.linalg.norm(side) < 1e-3:
                side = np.cross([1.0, 0.0, 0.0], f)
            side = anim.unit(side)
            up = np.cross(f, side)
            R = np.stack([side, up, f], axis=1)
            return h + f * 0.07 + up * 0.02, R
        gun.motion = gun_motion
        # プラズマの光
        glow = fx.Sprites(st, "glow", "add", order=12)
        rng = np.random.default_rng(9)
        seeds = rng.uniform(0, 1, (90, 4))

        def plasma_in(t, pts, lines):
            if not inside(t):
                return
            k = anim.remap(t, IN_T, 45.0)
            for a, b, c, d in seeds:
                ph = (t * (0.9 + b) + a) % 1.0
                x = -9.0 + 18.0 * c
                z = 9.0 + 12.0 * d
                y0 = anim.lerp(-6.0, 12.0, ph)
                L = 2.0 + 4.0 * b
                al = 0.35 * math.sin(ph * math.pi) * (0.6 + 0.6 * k)
                lines.append(("line", np.array([x, y0, z]), np.array([x, y0 + L, z]), 0.25 + 0.4 * a,
                              (1.0, 0.45 + 0.25 * b, 0.25, al)))
            fl = 0.8 + 0.2 * math.sin(t * 23) * math.sin(t * 7.1)
            for x in np.linspace(-6, 6, 7):
                pts.append((np.array([x, -1.0, 10.0]), 4.0, (1.0, 0.42, 0.2, 0.1 * fl * (0.7 + 0.5 * k)), 0.0))
        glow.add(fx.Func(plasma_in))

        def plasma_out(t, pts, lines):
            if not outside(t):
                return
            P = city.pos
            f = city.R @ np.array([0, 0, 1.0])
            nose = city.world(city.pivot + f * 0 + np.array([0, 0, 0])) if False else None
            tip = min((P + f * 190.0, P - f * 190.0), key=lambda q: q[1])
            fl = 0.85 + 0.15 * math.sin(t * 31)
            pts.append((tip, 520.0 * fl, (1.0, 0.5, 0.25, 0.75), 0.0))
            pts.append((tip, 180.0, (1.0, 0.85, 0.6, 0.6), 0.0))
            for a, b, c, d in seeds[:60]:
                ph = (t * (0.6 + b) + a) % 1.0
                th = c * 2 * math.pi
                off = np.array([math.cos(th), 0, math.sin(th)]) * (60 + 70 * d)
                s = tip + off + np.array([0, ph * 420.0, 0])
                lines.append(("line", s, s + np.array([0, 90 + 80 * b, 0]), 14 + 18 * a,
                              (1.0, 0.5, 0.25, 0.45 * math.sin(ph * math.pi))))
        glow.add(fx.Func(plasma_out))
        # セリフ
        self.say("nemuri", 10.6, 13.4, "来たか、三号。……いや、今はトワだったか", nm)
        self.say("nemuri", 15.0, 19.2, "撃て。私が死ねば、この都市と一緒に、戦争を延ばした者たちも消える", nm)
        self.say("towa", 20.0, 21.8, "……撃たない", tw)
        self.say("nemuri", 22.6, 24.8, "なら、都市は落ちる", nm)
        self.say("towa", 25.8, 27.6, "落とさない", tw)
        self.say("towa", 28.8, 33.8, "どっちも、選ばない。……三つ目を、探す", tw)
        self.say("nemuri", 35.0, 39.0, "……それは、私が持てなかった答えだ", nm)
        self.say("mikaga", 40.4, 42.6, "今度は、止めないよ", mk)

    # --- カメラ ----------------------------------------------------------------
    def cam_ext(self, t, u):
        P = self.city.pos
        f = self.city.R @ np.array([0, 0, 1.0])
        tgt = P - np.array([0, 230.0, 0])
        pos = P + np.array([-760.0, 120.0, -420.0]) + np.array([90.0, -30.0, 60.0]) * u
        return Cam(pos, tgt, fov=52, shake=0.4)

    def cam_enter(self, t, u):
        h = self.towa.head_pos(t=t)
        pos = np.array([-1.8, 0.55, -5.9]) + np.array([0.4, 0.1, 0.7]) * anim.smooth(u)
        tgt = anim.lerp(h + np.array([0, 0, 4.0]), NEM + np.array([0, 1.5, 0]), anim.smooth(anim.remap(t, 5.0, 8.5)))
        return Cam(pos, tgt, fov=44, shake=0.012, aperture=2.0, focus=float(np.linalg.norm(NEM - pos)))

    def cam_nemuri_turn(self, t, u):
        h = self.nemuri.head_pos(t=t)
        pos = h + np.array([0.9, -0.15, -2.4]) + np.array([-0.2, 0, 0.35]) * u
        return Cam(pos, h - np.array([0, 0.12, 0]), fov=30, aperture=6.0, shake=0.008)

    def cam_over_towa(self, t, u):
        a, b = self.towa.head_pos(t=t), self.nemuri.head_pos(t=t)
        r = anim.unit(np.cross([0, 1, 0], b - a))
        pos = a - anim.unit(b - a) * 0.9 + r * 0.42 + np.array([0, 0.08, 0])
        return Cam(pos + anim.unit(b - a) * 0.3 * u, b - np.array([0, 0.25, 0]), fov=24, aperture=7.0, shake=0.01)

    def cam_towa(self, t, u):
        h = self.towa.head_pos(t=t)
        f = anim.unit(self.nemuri.head_pos(t=t) - h)
        r = np.cross([0, 1, 0], f)
        return Cam(h + f * 1.1 - r * 0.25 - np.array([0, 0.05, 0]), h, fov=28, aperture=9.0, shake=0.008)

    def cam_nemuri(self, t, u):
        h = self.nemuri.head_pos(t=t)
        f = anim.unit(self.towa.head_pos(t=t) - h)
        r = np.cross([0, 1, 0], f)
        return Cam(h + f * (1.5 - 0.3 * u) + r * 0.35 - np.array([0, 0.25, 0]), h, fov=30, aperture=7.0, shake=0.03)

    def cam_towa_push(self, t, u):
        h = self.towa.head_pos(t=t)
        f = anim.unit(self.nemuri.head_pos(t=t) - h)
        r = np.cross([0, 1, 0], f)
        d = anim.lerp(1.6, 0.75, anim.smooth(u))
        return Cam(h + f * d + r * 0.12, h, fov=28, aperture=10.0, shake=0.012)

    def cam_side(self, t, u):
        a, b = self.towa.head_pos(t=t), self.nemuri.head_pos(t=t)
        mid = (a + b) / 2
        pos = mid + np.array([-4.6, -0.4, 0.3]) + np.array([0.5, 0.0, -0.5]) * u
        return Cam(pos, mid - np.array([0, 0.25, 0]), fov=40, aperture=2.0, shake=0.01)

    def cam_nemuri_soft(self, t, u):
        h = self.nemuri.head_pos(t=t)
        f = anim.unit(self.towa.head_pos(t=t) - h)
        r = np.cross([0, 1, 0], f)
        return Cam(h + f * (1.0 - 0.15 * u) - r * 0.2 - np.array([0, 0.06, 0]), h, fov=28, aperture=10.0, shake=0.006)

    def cam_mikaga(self, t, u):
        m = self.mikaga.head_pos(t=t)
        k = anim.smooth(anim.remap(t, 42.8, 45.0))
        tw = self.towa.head_pos(t=t)
        pos = anim.lerp(m + np.array([0.9, -0.1, 1.6]), np.array([-4.6, 2.2, -6.4]), k)
        tgt = anim.lerp(m, (tw + self.nemuri.head_pos(t=t)) / 2 + np.array([0, -0.3, 0]), k)
        return Cam(pos, tgt, fov=anim.lerp(28, 46, k), aperture=anim.lerp(8.0, 2.0, k), shake=0.008)

    def grade(self, t):
        g = self.base_grade(t, fade_out=1.6)
        g.tint = (1.08, 0.92, 0.9)
        g.contrast = 1.08
        if outside(t):
            g.tint = (1.04, 0.97, 0.94)
        if IN_T <= t < IN_T + 0.25:
            g.flash = 0.35 * (1 - (t - IN_T) / 0.25)
            g.flash_color = (1.0, 0.5, 0.3)
        return g

    def audio(self, path):
        from cinema import sound as S
        mx = S.Mixer(self.duration)
        mx.add(S.drone(45, 45.0, 5) * S.env(45.0, 1.5, 0, 1, 2.0), 0, 0.3)
        mx.add(S.lowpass(S.noise(45.0, 4), 500) * S.env(45.0, 1.0, 0, 1, 2.0), 0, 0.22)   # 大気をこする音
        mx.add(S.hit(2.4, 34), 0.2, 0.7)
        mx.add(S.whoosh(3.5), 0.4, 0.5)
        for k in range(10):                                                          # 非常の警報（遠く）
            mx.add(S.lowpass(S.alarm(1.0), 1500), IN_T + 0.5 + k * 2.4, 0.06)
        mx.add(S.pad(S.chord("D2 A2 D3 F3"), 10.0, a=2.0, r=2.0, bright=700), IN_T, 0.28)
        mx.add(S.pad(S.chord("A#1 F2 A#2 D3"), 9.0, a=1.5, r=2.0, bright=650), 14.0, 0.3)
        mx.add(S.hit(1.8, 40), GUN_UP, 0.4)
        mx.add(S.pad(S.chord("G2 D3 G3 A#3"), 8.0, a=1.5, r=2.0, bright=800), 22.0, 0.3)
        mx.add(S.riser(3.0, 200, 1200), 25.2, 0.18)
        mx.add(S.pad(S.chord("F2 C3 F3 A3 C4"), 11.0, a=2.0, r=3.0, bright=1300), 28.4, 0.32)
        mx.add(S.shimmer(1.5, 6), 34.0, 0.18)
        mx.add(S.pad(S.chord("A#2 F3 A3 D4"), 11.0, a=2.0, r=4.0, bright=1500), 36.0, 0.3)
        return mx.save(path)

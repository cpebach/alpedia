"""パイロット版：艦隊戦のショートムービー「黒鉄の艦隊」（約 16 秒）

月の軌道で、ルナ艦隊（白と赤）と黒鉄艦隊（黒と赤い光）がぶつかる。
  1. 防衛線：惑星テラの上を進むルナ艦隊（上から大きく）
  2. 黒鉄艦隊：敵の巡洋艦の腹の下から。ミサイルの一斉発射
  3. 追う：ミサイルの後ろにつく。迎撃のビームと爆発の中を抜け、巡洋艦に命中
  4. 主砲：旗艦アルテミスの砲身の横。光がたまり、主砲が放たれる
  5. 撃沈：敵の巡洋艦が爆発して2つに折れる。手前を味方の艦が横切り、題名
船はゲームと同じブロックの構築物（完全な剛体として、決めた軌道で動かす）。遠くの船は箱だけの張りぼて。
"""
import math

import numpy as np

from cinema import anim, fx, shapes
from cinema.film import Cam, Film, Shot
from cinema.post import Grade

VL = np.array([0.0, 0.0, 12.0])         # ルナ艦隊の速度
VK = np.array([0.0, 0.0, -10.0])        # 黒鉄艦隊の速度
R_L = np.eye(3)
R_K = anim.yaw_pitch_roll(180, 0, 0)

LUNA = {"artemis": (0, 0, 0), "L1": (-95, 14, -30), "L2": (100, -12, -25), "L3": (-190, -28, -120),
        "L4": (200, 22, -140)}
KURO = {"K0": (70, 45, 1120), "K1": (-150, 12, 960), "K2": (210, -26, 1010), "K3": (-30, -40, 900)}

LAUNCH = 4.55
HIT = 9.05            # 命中（L1）
FIRE = 11.2           # 主砲
BOOM = 12.75          # 敵の巡洋艦が爆発


def bez(p0, p1, p2, p3, k):
    a = 1 - k
    return a ** 3 * p0 + 3 * a * a * k * p1 + 3 * a * k * k * p2 + k ** 3 * p3


def bez_d(p0, p1, p2, p3, k):
    a = 1 - k
    return 3 * a * a * (p1 - p0) + 6 * a * k * (p2 - p1) + 3 * k * k * (p3 - p2)


class Missile:
    """発射の時刻・場所から、目標の時刻・場所まで弧を描いて飛ぶ。"""

    def __init__(self, t0, t1, p0, up, p3, approach, hit=False):
        self.t0, self.t1, self.hit = t0, t1, hit
        self.p0 = np.asarray(p0, float)
        self.p3 = np.asarray(p3, float)
        self.p1 = self.p0 + np.asarray(up, float) * 70
        self.p2 = self.p3 - anim.unit(approach) * 260

    def k(self, t):
        return anim.clamp((t - self.t0) / (self.t1 - self.t0))

    def pos(self, t):
        return bez(self.p0, self.p1, self.p2, self.p3, self.k(t))

    def dir(self, t):
        return anim.unit(bez_d(self.p0, self.p1, self.p2, self.p3, max(self.k(t), 0.01)))

    def alive(self, t):
        return self.t0 <= t < self.t1


class FILM(Film):
    name = "pilot_fleet"
    duration = 16.5
    vertical_fov = False          # このころは画角を横で決めていた
    near, far = 0.5, 80000.0
    sun = (-0.35, 0.85, -0.4)
    ambient = 0.28
    subframes = 3
    shutter = 0.5
    lines = [
        (1.5, 3.4, "オペレーター", "前方に艦影多数！ 黒鉄艦隊です！"),
        (3.9, 4.6, "ヴァルガ", "沈めろ。"),
        (6.9, 7.9, "オペレーター", "ミサイル接近、迎撃！"),
        (9.0, 9.4, "", "――！"),
        (9.9, 11.0, "ミオ", "主砲、撃てーっ！"),
    ]
    captions = [(0.4, 3.3, "月軌道・防衛線")]
    titles = [(14.6, 16.4, "ALPEDIA", "黒鉄の艦隊（パイロット版）")]

    def __init__(self):
        super().__init__()
        self.shots = [
            Shot(0.0, 3.6, self.cam_wide, "防衛線"),
            Shot(3.6, 6.4, self.cam_enemy, "黒鉄艦隊"),
            Shot(6.4, 9.4, self.cam_chase, "ミサイルを追う"),
            Shot(9.4, 12.6, self.cam_cannon, "主砲"),
            Shot(12.6, 16.5, self.cam_sink, "撃沈"),
        ]

    # ------------------------------------------------------------------
    def build(self, st):
        fx.Stars(st, dist=70000)
        fx.Planet(st, "terra", (6000, -18000, 26000), scale=20, spin=0.1, tilt=anim.axis_angle((1, 0, 0), -65))
        fx.Planet(st, "luna", (-30000, 12000, -40000), scale=8, style="luna")
        self.s = {}
        # ルナ艦隊
        for name, p in LUNA.items():
            v = shapes.artemis() if name == "artemis" else shapes.luna_cruiser()
            sh = st.ship(v.voxels(), size=1.0, name=name)
            sh.motion = (lambda t, p=np.array(p, float): (p + VL * t, R_L))
            self.s[name] = sh
        # 黒鉄艦隊（K3 は爆発で2つに折れる）
        for name, p in KURO.items():
            v = shapes.kurogane_flagship() if name == "K0" else shapes.kurogane_cruiser()
            sh = st.ship(v.voxels(), size=1.0, name=name)
            if name == "K3":
                sh.motion = (lambda t, p=np.array(p, float): (p + VK * t, R_K) if t < BOOM else None)
                self.k3_vox = v
            else:
                sh.motion = (lambda t, p=np.array(p, float): (p + VK * t, R_K))
            self.s[name] = sh
        self._halves(st)
        # 遠くの張りぼて
        rng = np.random.default_rng(9)
        for k in range(10):
            luna = k < 5
            p = np.array([rng.uniform(-700, 700), rng.uniform(-150, 120),
                          rng.uniform(-900, -400) if luna else rng.uniform(1500, 2200)])
            v = shapes.far_hull(mat="m_white" if luna else "m_dark", band="m_red" if luna else "m_wine",
                                light="g_blue" if luna else "g_red")
            sh = st.ship(v.voxels(), size=1.6, name=f"far{k}")
            sh.motion = (lambda t, p=p, luna=luna: (p + (VL if luna else VK) * t, R_L if luna else R_K))
        # 横切る味方の艦（最後のカット）
        fly = st.ship(shapes.luna_cruiser().voxels(), size=1.0, name="flyby")
        self.fly = fly
        fly.motion = self.flyby
        self._missiles(st)
        self._effects(st)

    def _halves(self, st):
        L = 52
        front, back = shapes.cut(self.k3_vox, lambda x, y, z: z > L * 0.45 + (x * 0.4))
        full = self.s["K3"]
        p3 = np.array(KURO["K3"], float)
        self.halves = []
        for k, (part, sign) in enumerate(((front, 1), (back, -1))):
            sh = st.ship(part.voxels(), size=1.0, pivot=full.pivot, name=f"K3_{k}")

            def mot(t, sign=sign, k=k):
                if t < BOOM:
                    return None
                a = t - BOOM
                base = p3 + VK * t
                sep = np.array([3.0 * sign, 4.0 - 6.0 * k, -9.0 * sign]) * a
                R = R_K @ anim.axis_angle((0.3, 0.2, 1.0) if k else (1.0, 0.2, 0.3), (14 + 6 * k) * a * sign)
                return base + sep, R
            sh.motion = mot
            self.halves.append(sh)

    def flyby(self, t):
        if t < 12.6:
            return None
        cam = self.sink_cam_pos(12.6)
        a = t - 12.6
        # カメラの左後ろから、手前を右へ横切る
        p = cam + np.array([-150.0, -12.0, 55.0]) + np.array([46.0, 2.0, 6.0]) * a
        return p, anim.yaw_pitch_roll(78, 0, -12)

    # --- ミサイル -------------------------------------------------------------
    def _missiles(self, st):
        s = self.s
        self.missiles = []
        k1, k2, k3 = s["K1"], s["K2"], s["K3"]

        def hatch(ship, i, t):
            x = -3 if i % 2 else 3
            z = int(52 * 0.3) + 3 * (i // 2 % 3)
            p = np.array(KURO[ship.c.name], float) + VK * t
            return p + R_K @ (np.array([x + 0.5, 4.5, z + 0.5]) - ship.pivot)

        def tgt(name, t, local=(0.5, 3.0, 20.0)):
            sh = s[name]
            p = np.array(LUNA[name], float) + VL * t
            return p + (np.asarray(local, float) - sh.pivot)

        plan = [  # (発射する艦, 何番目, 発射時刻, 目標, 終わる時刻, 命中か)
            (k1, 0, LAUNCH, "L1", HIT, True),
            (k1, 1, LAUNCH + 0.12, "L1", 7.35, False),
            (k1, 2, LAUNCH + 0.25, "L1", 7.95, False),
            (k1, 3, LAUNCH + 0.36, "L1", 8.45, False),
            (k3, 0, LAUNCH + 0.08, "L1", 8.8, False),
            (k3, 1, LAUNCH + 0.2, "artemis", 8.6, False),
            (k3, 2, LAUNCH + 0.33, "artemis", 9.6, False),
            (k3, 3, LAUNCH + 0.45, "artemis", 10.1, False),
            (k2, 0, LAUNCH + 0.15, "L2", 8.2, False),
            (k2, 1, LAUNCH + 0.3, "L2", 8.9, False),
            (k2, 2, LAUNCH + 0.42, "L2", 9.4, False),
        ]
        rng = np.random.default_rng(2)
        for ship, i, t0, name, t1, hit in plan:
            p0 = hatch(ship, i, t0)
            # 目標の時刻の目標の位置（命中しないものは手前で撃ち落とされる）
            end = tgt(name, t1 if hit else t1 + 0.6, (rng.uniform(-3, 3), rng.uniform(2, 6), rng.uniform(12, 30)))
            up = anim.unit(np.array([rng.uniform(-0.6, 0.6), 1.0, rng.uniform(-0.2, 0.3)]))
            appr = anim.unit(end - p0 + np.array([rng.uniform(-150, 150), rng.uniform(-60, 100), 0]))
            m = Missile(t0, t1 if hit else t1 + 0.6, p0, up, end, appr, hit)
            m.stop = t1                                   # 撃ち落とされる（または命中する）時刻
            m.target = name
            self.missiles.append(m)
            sh = st.ship(shapes.kmissile().voxels(), size=0.55, name="missile")
            sh.motion = (lambda t, m=m: (m.pos(t), anim.look_basis(m.dir(t)).T) if m.t0 <= t < m.stop else None)
        self.hero = self.missiles[0]

    # --- 光・煙・爆発 ---------------------------------------------------------
    def _effects(self, st):
        s = self.s
        smoke = fx.Sprites(st, "smoke", "alpha", order=8)
        glow = fx.Sprites(st, "glow", "add", order=12)
        spark = fx.Sprites(st, "beam", "add", order=13)
        # 噴射口の光（味方：水色、敵：赤橙）
        for name in LUNA:
            sh = s[name]
            hw = 7 if name == "artemis" else 5
            for x in (-3, 0, 3):
                for y in (2, 4):
                    glow.add(fx.Glow(lambda t, sh=sh, x=x, y=y: sh.world((x + 0.5, y + 0.5, -3.0)),
                                     lambda t, x=x: 4.5 + 0.6 * math.sin(t * 37 + x), (0.45, 0.85, 1.0, 0.85)))
            del hw
        for name in KURO:
            sh = s[name]
            for x in (-4, 0, 4):
                glow.add(fx.Glow(lambda t, sh=sh, x=x: sh.world((x + 0.5, 2.5, -4.0)) if (sh.c.name != "K3" or t < BOOM)
                                 else None, lambda t, x=x: 5.0 + 0.8 * math.sin(t * 29 + x), (1.0, 0.45, 0.15, 0.9)))
        # ミサイル：炎・光・煙の尾
        for m in self.missiles:
            glow.add(fx.Glow(lambda t, m=m: m.pos(t) - m.dir(t) * 2.0 if m.t0 <= t < m.stop else None,
                             lambda t, m=m: 1.7 + 0.3 * math.sin(t * 61 + m.t0 * 9), (1.0, 0.55, 0.25, 0.85)))
            spark.add(fx.Beam(lambda t, m=m: (m.pos(t) - m.dir(t) * 1.6, m.pos(t) - m.dir(t) * 5.5, 0.8,
                                              (1.0, 0.7, 0.35, 0.8)) if m.t0 <= t < m.stop else None))
            smoke.add(fx.Trail(lambda te, m=m: m.pos(te) - m.dir(te) * 3.0 if m.t0 <= te < m.stop else None,
                               m.t0 + 0.05, m.stop, rate=26, life=2.6, size=(1.4, 7.0), color=(0.7, 0.68, 0.66, 0.45),
                               drift=1.2, seed=int(m.t0 * 100)))
            # 発射の火花
            glow.add(fx.Glow(lambda t, m=m: m.p0, lambda t, m=m: 9 * (1 - (t - m.t0) / 0.35),
                             (1.0, 0.7, 0.4, 0.9), m.t0, m.t0 + 0.35))
            if m.hit:
                continue
            # 迎撃：味方の艦からのビームと、小さな爆発
            src = s[m.target]
            for j, off in enumerate((0.0, 0.12, 0.24)):
                spark.add(fx.Beam(lambda t, m=m, src=src, j=j, off=off: (
                    src.world((2.5 * (1 - 2 * (j % 2)) + 0.5, 8.5, 26.0 + 6 * j)), m.pos(m.stop - 0.02 + 0 * t),
                    0.7, (0.5, 0.95, 1.0, 0.9)) if m.stop - 0.42 + off <= t < m.stop - 0.3 + off else None))
            e = fx.Explosion(m.stop, m.pos(m.stop - 0.02), scale=0.55, seed=int(m.stop * 77), n=16)
            glow.add(e)
            smoke.add(e.smoke())
        # 命中（L1）
        hero = self.hero
        hp = hero.pos(HIT - 0.01)
        e = fx.Explosion(HIT, hp, scale=1.6, seed=3, n=34, vel=VL)
        glow.add(e)
        glow.add(e.shock())
        smoke.add(e.smoke())
        fx.Debris(st, HIT, hp, n=70, speed=26, size=(0.4, 1.3), cols=((0.92, 0.93, 0.95), (0.82, 0.16, 0.14),
                                                                      (0.13, 0.2, 0.42)), seed=4, vel=VL)
        l1 = s["L1"]
        # 命中したあと燃え続ける所
        burn = l1.c.center_of_mass()
        local_hit = l1.R.T @ (hp - (np.array(LUNA["L1"], float) + VL * HIT)) + l1.pivot

        def fire(t, pts, lines):
            if t < HIT:
                return
            c = l1.world(local_hit)
            for i in range(6):
                ph = (t * 1.7 + i / 6) % 1.0
                pts.append((c + np.array([math.sin(i * 2.1) * 1.5, ph * 6, math.cos(i * 1.3) * 1.5]), 3 + ph * 4,
                            (1.0, 0.5 - ph * 0.3, 0.15, (1 - ph) * 0.8), i + t))
        glow.add(fx.Func(fire))
        smoke.add(fx.Trail(lambda te: l1.world(local_hit) + np.array([0, 2.0, 0]) if te >= HIT else None, HIT + 0.2,
                           30, rate=10, life=4.0, size=(3, 14), color=(0.25, 0.24, 0.24, 0.5), drift=2.0, seed=8))
        del burn
        # 主砲
        art, k3 = s["artemis"], s["K3"]

        def muzzle(t):
            return art.world((0.5, 4.5, 88.5))

        def k3_hit(t):
            return k3.world((0.5, 2.5, 24.0)) if t < BOOM else np.array(KURO["K3"], float) + VK * t

        def charge(t, pts, lines):
            a = t - (FIRE - 1.6)
            if a < 0 or t > FIRE + 1.4:
                return
            mz = muzzle(t)
            if t < FIRE:
                k = a / 1.6
                pts.append((mz, 3 + 10 * k ** 2, (0.6, 0.95, 1.0, 0.5 + 0.5 * k), t * 3))
                rng = np.random.default_rng(int(t * 30))
                for i in range(16):   # 光の粒が砲口に集まる
                    d = anim.unit(rng.standard_normal(3))
                    r = (1 - ((t * 2 + i / 16) % 1.0)) * 14
                    lines.append(("line", mz + d * r, mz + d * (r + 2.5), 0.25, (0.6, 0.95, 1.0, 0.7 * k)))
            else:
                b = t - FIRE
                w = 1.0 - anim.remap(b, 0.9, 1.4)
                kt = k3_hit(t)
                lines.append(("line", mz, kt, 9.0 * w, (0.3, 0.75, 1.0, 0.55 * w)))
                lines.append(("line", mz, kt, 3.6 * w, (0.75, 0.95, 1.0, 0.9 * w)))
                lines.append(("line", mz, kt, 1.2 * w, (1.0, 1.0, 1.0, 1.0 * w)))
                pts.append((mz, 16 * w + 4, (0.7, 0.95, 1.0, 0.9 * w), 0.0))
                pts.append((kt, 22 * w, (0.8, 0.95, 1.0, 0.9 * w), 0.0))
        glow.add(fx.Func(lambda t, pts, lines: charge(t, pts, [])))
        spark.add(fx.Func(lambda t, pts, lines: charge(t, [], lines)))
        # 主砲が当たった所の火花と、爆発
        hit_p = k3_hit(FIRE + 0.1)
        e1 = fx.Explosion(FIRE + 0.08, hit_p, scale=1.2, seed=12, n=20, vel=VK)
        glow.add(e1)
        smoke.add(e1.smoke())
        boom_p = np.array(KURO["K3"], float) + VK * BOOM
        for k, (off, sc, dt) in enumerate((((0, 0, 0), 4.2, 0.0), ((8, 3, 14), 2.4, 0.18), ((-6, -2, -12), 2.6, 0.32))):
            e = fx.Explosion(BOOM + dt, boom_p + np.array(off, float), scale=sc, seed=40 + k, n=36, vel=VK)
            glow.add(e)
            if k == 0:
                glow.add(e.shock())
            smoke.add(e.smoke())
        fx.Debris(st, BOOM, boom_p, n=160, speed=38, size=(0.5, 2.2), cols=((0.17, 0.18, 0.21), (0.08, 0.085, 0.1),
                                                                         (0.42, 0.06, 0.08)), seed=7, vel=VK)
        # 折れた所の炎
        for h in self.halves:
            glow.add(fx.Func(lambda t, pts, lines, h=h: [pts.append((h.world(h.pivot + np.array(
                [math.sin(i * 1.9) * 4, math.cos(i * 2.7) * 2, (i - 4) * 1.2])), 6 + 3 * math.sin(t * 9 + i),
                (1.0, 0.45, 0.12, 0.75), i + t)) for i in range(9)] if t >= BOOM else None))

    # --- カメラ --------------------------------------------------------------
    def cam_wide(self, t, u):
        a = np.array(LUNA["artemis"], float) + VL * t
        pos = a + anim.lerp(np.array([-125.0, 42.0, -215.0]), np.array([-95.0, 30.0, -130.0]), anim.ease_in_out(u))
        tgt = a + anim.lerp(np.array([30.0, -25.0, 180.0]), np.array([40.0, -28.0, 260.0]), anim.ease_in_out(u))
        return Cam(pos, tgt, fov=44, roll=anim.lerp(-9, -5, u))

    def cam_enemy(self, t, u):
        k1 = self.s["K1"]
        p = np.array(KURO["K1"], float) + VK * t
        pos = p + np.array([-24.0, -15.0, -85.0]) + np.array([4.0, 3.0, 14.0]) * anim.smooth(u)
        tgt = p + np.array([12.0, 12.0, 25.0])
        del k1
        sh = 0.15 if t > LAUNCH else 0.0
        return Cam(pos, tgt, fov=46, roll=anim.lerp(12, 8, u), shake=sh)

    def cam_chase(self, t, u):
        m = self.hero
        tc = min(t, HIT - 0.65)
        d = m.dir(tc)
        side = anim.unit(np.cross([0, 1, 0], d))
        pos = m.pos(tc) - d * 16 + np.array([0, 3.0, 0]) + side * 6.0
        tgt = m.pos(t) + m.dir(t) * 16 - side * 1.5
        sh = 0.05 + (0.9 if t > HIT else 0.0) * max(0.0, 1 - (t - HIT) / 0.4)
        return Cam(pos, tgt, fov=38, roll=anim.wobble(t, 2, 0.8) * 4 - 6, shake=sh, aperture=2.5,
                   focus=float(np.linalg.norm(m.pos(t) - pos)))

    def cam_cannon(self, t, u):
        art = self.s["artemis"]
        pos = art.world((-24.0, 13.0, 58.0 + 6.0 * u))
        tgt = art.world((2.0, 1.0, 112.0))
        sh = 0.0
        if t > FIRE:
            sh = 0.8 * max(0.0, 1 - (t - FIRE) / 0.9) + 0.05
        return Cam(pos, tgt, fov=40, roll=anim.lerp(-8, -5, u), shake=sh, aperture=3.0,
                   focus=float(np.linalg.norm(art.world((0.5, 4.5, 80.0)) - pos)))

    def sink_cam_pos(self, t):
        p = np.array(KURO["K3"], float) + VK * t
        return p + np.array([-150.0, 46.0, -120.0])

    def cam_sink(self, t, u):
        p = np.array(KURO["K3"], float) + VK * t
        pos = self.sink_cam_pos(t) + np.array([20.0, -8.0, 30.0]) * anim.ease_out(u)
        tgt = p + np.array([0.0, 4.0, 0.0])
        sh = 0.0
        if BOOM <= t < BOOM + 1.2:
            sh = 0.9 * (1 - (t - BOOM) / 1.2)
        return Cam(pos, tgt, fov=46, roll=anim.lerp(4, -2, u), shake=sh)

    # --- 仕上げ --------------------------------------------------------------
    def grade(self, t):
        g = Grade()
        g.fade = min(1.0, t / 0.5) * (1 - anim.remap(t, 15.9, 16.5))
        g.bloom = 0.65
        g.bloom_threshold = 0.7
        g.contrast = 1.12
        g.tint = (0.97, 1.0, 1.05)
        g.lift = (0.0, 0.004, 0.015)
        if HIT <= t < HIT + 0.35:
            g.flash = (1 - (t - HIT) / 0.35) * 0.7
            g.aberration = 6 * (1 - (t - HIT) / 0.35)
        if FIRE <= t < FIRE + 0.5:
            g.flash = (1 - (t - FIRE) / 0.5) * 0.6
            g.flash_color = (0.75, 0.95, 1.0)
            g.aberration = 8 * (1 - (t - FIRE) / 0.5)
        if BOOM <= t < BOOM + 0.6:
            g.flash = (1 - (t - BOOM) / 0.6) * 0.8
            g.aberration = 5 * (1 - (t - BOOM) / 0.6)
        if t > 14.4:                                  # 題名の前に少し暗く
            g.exposure = 1 - 0.35 * anim.remap(t, 14.4, 15.0)
        return g

    # --- 音 ------------------------------------------------------------------
    def audio(self, path):
        from cinema import sound as S
        mx = S.Mixer(self.duration)
        # 防衛線：低い和音と機関の音
        mx.add(S.pad(S.chord("D2 A2 D3 F3"), 4.2, a=1.8, r=0.8, bright=800), 0.0, 0.42)
        mx.add(S.drone(36, 4.0, seed=1), 0.0, 0.35)
        # 黒鉄艦隊：重い和音と太鼓
        mx.add(S.hit(2.0, 40), 3.6, 0.9)
        mx.add(S.pad(S.chord("C2 G2 C3 D#3 G3"), 9.2, a=0.6, r=1.0, bright=650), 3.6, 0.38)
        for k in range(18):
            tt = 3.6 + 0.5 * k
            if tt > 12.4:
                break
            mx.add(S.hit(0.9, 48 if k % 2 == 0 else 60), tt, 0.45 if k % 4 == 0 else 0.25)
        for m in self.missiles:
            mx.add(S.launch(1.5, int(m.t0 * 100)), m.t0, 0.35, pan=float(np.clip(m.p0[0] / 200, -0.8, 0.8)))
        # 追う：風を切る音・迎撃の光線・小さな爆発
        mx.add(S.whoosh(3.0, 400, 2600, 21), 6.4, 0.4)
        for m in self.missiles:
            if m.hit:
                continue
            for j, off in enumerate((0.0, 0.12, 0.24)):
                mx.add(S.zap(0.3, 2600 + 300 * j, 400), m.stop - 0.42 + off, 0.12, pan=0.3 * (j - 1))
            mx.add(S.boom(1.5, 0.5, int(m.stop * 10)), m.stop, 0.22)
        mx.add(S.boom(3.0, 1.4, 1), HIT, 0.95)
        mx.add(S.hit(1.8, 38), HIT, 0.8)
        # 主砲：力がたまる → 発射（低いうなりが続く）
        mx.add(S.riser(1.6, 120, 2400), FIRE - 1.6, 0.45)
        mx.add(S.boom(2.4, 1.6, 5), FIRE, 0.9)
        beam = (S.sine(62, 1.4) * 0.6 + S.lowpass(S.noise(1.4, 6), 300) * 1.5 + S.zap(1.4, 1600, 200) * 0.3)
        mx.add(beam * S.env(1.4, 0.02, 0.1, 0.9, 0.5), FIRE, 0.5)
        # 撃沈
        mx.add(S.boom(4.5, 2.4, 2), BOOM, 1.0)
        mx.add(S.hit(2.5, 32), BOOM, 0.9)
        mx.add(S.boom(3.0, 1.2, 3), BOOM + 0.18, 0.45, pan=0.3)
        mx.add(S.boom(3.0, 1.2, 8), BOOM + 0.32, 0.45, pan=-0.3)
        mx.add(S.whoosh(2.2, 120, 1400, 30), 13.3, 0.35, pan=-0.4)          # 味方の艦が横切る
        # 題名：明るい和音
        mx.add(S.hit(2.0, 44), 14.6, 0.8)
        mx.add(S.pad(S.chord("D3 A3 D4 F#4 A4"), 2.2, a=0.4, r=1.0, bright=2400), 14.4, 0.45)
        mx.add(S.pad(S.chord("D2 A2 D3"), 2.2, a=0.4, r=1.0, bright=600), 14.4, 0.4)
        return mx.save(path)

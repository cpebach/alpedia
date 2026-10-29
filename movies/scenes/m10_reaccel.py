"""10 終章B「再加速」（約 40 秒）

撃ち合いが止まる。帝国のシロツは「あの時」を思い出し、連盟のケンスは帝国艦隊に呼びかける。
計算室では先生・ルテロ・ダバイが、アマツを押し上げる 24 基の推進モジュールの配置を出す。
モジュールが点火し、両艦隊がアマツを下から押す。現場で指揮していたキンギが倒れ、ドナチが黙って引き継ぐ。
アマツは上昇に転じる。
"""
import math

import numpy as np

from cinema import anim, fx
from cinema.film import Cam, Shot
from scenes import lib
from scenes.base import Movie

BR_E = np.array([0.0, 0.0, 30000.0])            # 帝国の艦橋
BR_U = np.array([0.0, 0.0, -30000.0])           # 連盟の艦橋
CALC = np.array([30000.0, 0.0, 0.0])            # 計算室
HULL = np.array([-30000.0, 0.0, 0.0])           # アマツの外殻の上の作業場
IGN0, IGN1 = 22.4, 25.0                          # モジュールの点火
TURN = 31.0                                      # 落下 → 上昇
SHOTS_SPACE = ((0.0, 4.5), (22.0, 27.0), (33.0, 40.0))


def in_space(t):
    return any(a <= t < b for a, b in SHOTS_SPACE)


def in_be(t):
    return 4.5 <= t < 8.6


def in_bu(t):
    return 8.6 <= t < 13.0


def in_calc(t):
    return 13.0 <= t < 22.0


def in_hull(t):
    return 27.0 <= t < 33.0


def city_y(t):
    """アマツの高さ（落ちる → 止まる → 上がる）。"""
    v0, v1 = -16.0, 22.0
    if t < IGN0:
        return v0 * t
    y0 = v0 * IGN0
    k = t - IGN0
    T = TURN + 3.0 - IGN0
    if k < T:
        return y0 + v0 * k + (v1 - v0) * k * k / (2 * T)
    return y0 + v0 * T + (v1 - v0) * T / 2 + v1 * (k - T)


def ign(i):
    return IGN0 + (IGN1 - IGN0) * ((i * 7) % 24) / 23.0


def module_local(i):
    """24 基のモジュール（アマツのローカル、m。下側の半周に 3 列）。"""
    col, row = i % 8, i // 8
    th = math.radians(200 + col * 140 / 7)
    r = 18 * lib.AMATSU_SIZE
    return np.array([math.cos(th) * r, math.sin(th) * r, (8 + row * 22) * lib.AMATSU_SIZE]), np.array([math.cos(th), math.sin(th), 0.0])


class FILM(Movie):
    name = "10_reaccel"
    duration = 40.0
    near, far = 0.05, 140000.0
    sun = (0.4, 0.75, -0.5)
    ambient = 0.3
    subframes = 2

    def __init__(self):
        super().__init__()
        self.captions = [(0.5, 4.0, "終章　アマツ宙域")]
        self.titles = []
        self.shots = [
            Shot(0.0, 4.5, self.cam_wide, "止まる"),
            Shot(4.5, 8.6, self.cam_sirotsu, "シロツ"),
            Shot(8.6, 13.0, self.cam_kensu, "ケンス"),
            Shot(13.0, 16.0, self.cam_nekosa, "先生"),
            Shot(16.0, 19.3, self.cam_rutero, "ルテロ"),
            Shot(19.3, 22.0, self.cam_dabai, "ダバイ"),
            Shot(22.0, 27.0, self.cam_ignite, "点火"),
            Shot(27.0, 30.6, self.cam_kingi, "キンギ"),
            Shot(30.6, 33.0, self.cam_donachi, "ドナチ"),
            Shot(33.0, 40.0, self.cam_rise, "上昇"),
        ]

    # --- 舞台 ----------------------------------------------------------------
    def build(self, st):
        sky = lambda t: in_space(t) or in_hull(t)  # noqa: E731
        fx.Stars(st, dist=120000).when = sky
        e = self.terra = fx.Planet(st, "terra", (0, 0, 0), scale=30, spin=0.02, tilt=anim.axis_angle((1, 0, 0), -35))
        self.R_E = e.radius * 30
        e.center = np.array([0.0, -self.R_E - 2400.0, 3000.0])
        e.when = in_space
        e2 = fx.Planet(st, "terra", (0, 0, 0), scale=30, spin=0.02, tilt=anim.axis_angle((1, 0, 0), -35))
        e2.center = HULL + np.array([0.0, -self.R_E - 2400.0, 3000.0])
        e2.when = in_hull
        # アマツ（横向き、腹を地球へ）
        city = self.city = st.ship(lib.amatsu().voxels(), size=lib.AMATSU_SIZE, pivot=(0, 0, 30 * lib.AMATSU_SIZE), name="amatsu")
        Rc = anim.yaw_pitch_roll(70, 4, 0)
        city.motion = lambda t: (np.array([0.0, city_y(t), 0.0]), Rc @ anim.axis_angle((0, 0, 1), 0.6 * t)) if in_space(t) else None
        # 両艦隊（下から押す）
        rng = np.random.default_rng(4)
        self.fleet = []
        for k in range(10):
            kind = "union" if k % 2 == 0 else "empire"
            if k == 0:
                kind = "union_flag"
            if k == 1:
                kind = "empire_flag"
            sh = st.ship(lib.fleet_ship(kind).voxels(), size=1.0, name=f"f{k}")
            along = (k - 4.5) * 34.0
            off = np.array([rng.uniform(-60, 60), -200.0 - rng.uniform(0, 70), 0.0])

            def mot(t, along=along, off=off, k=k):
                if not in_space(t):
                    return None
                ax = city.R @ np.array([0, 0, 1.0])
                base = city.pos + ax * along + off
                push = anim.smooth(anim.remap(t, 4.0, 22.0))
                p = anim.lerp(base + np.array([(k % 2 * 2 - 1) * 900.0, 120.0, -300.0 + 60 * k]), base, push)
                fwd = anim.lerp(np.array([-(k % 2 * 2 - 1), 0.0, 0.2]), np.array([0.0, 1.0, 0.0]), push)
                return p, anim.look_basis(fwd, (0, 0, 1), 0).T
            sh.motion = mot
            self.fleet.append((sh, kind))
        glow = self.glow = fx.Sprites(st, "glow", "add", order=12)
        beam = fx.Sprites(st, "beam", "add", order=13)
        # 撃ち合い（すぐ止まる）
        for k in range(10):
            te = 0.15 + k * 0.17

            def shot(t, te=te, k=k):
                if not (te <= t < te + 0.3) or not in_space(t):
                    return None
                a = np.array([-800.0 + 160 * k, -40.0 + 20 * (k % 3), -500.0])
                b = a + np.array([500.0 * (1 if k % 2 else -1), 60.0, 900.0])
                c = (0.5, 0.95, 1.0, 0.85) if k % 2 else (1.0, 0.35, 0.25, 0.85)
                return (a, b, 3.0, c)
            beam.add(fx.Beam(shot))
        # 艦の噴射
        for sh, kind in self.fleet:
            col = (0.45, 0.85, 1.0, 0.85) if kind.startswith("union") else (1.0, 0.45, 0.15, 0.9)
            for x in (-3, 0, 3):
                glow.add(fx.Glow(lambda t, sh=sh, x=x: sh.world((x + 0.5, 3.5, -3.0)) if in_space(t) else None,
                                 lambda t: 6.0 + 6.0 * anim.remap(t, 22.0, 25.0), col))
        # モジュール
        for i in range(24):
            p, n = module_local(i)
            t0 = ign(i)
            on = (lambda t, t0=t0: anim.smooth(anim.remap(t, t0, t0 + 0.6)))
            glow.add(fx.Glow(lambda t, p=p: city.world(city.pivot + p) if in_space(t) else None,
                             lambda t, on=on, i=i: 4.0 + 38.0 * on(t) * (1 + 0.12 * math.sin(t * 37 + i)),
                             lambda t, on=on: (0.55, 0.85, 1.0, 0.3 + 0.65 * on(t))))
            glow.add(fx.Glow(lambda t, p=p, t0=t0: city.world(city.pivot + p) if in_space(t) and t0 <= t < t0 + 0.4 else None,
                             lambda t, t0=t0: 160.0 * (1 - (t - t0) / 0.4), (0.8, 0.95, 1.0, 0.8)))
            beam.add(fx.Beam(lambda t, p=p, n=n, on=on: (city.world(city.pivot + p), city.world(city.pivot + p) + np.array([0, -150.0, 0]) * on(t),
                                                       22.0 * on(t), (0.5, 0.8, 1.0, 0.7 * on(t)))
                             if in_space(t) and on(t) > 0.02 else None))

        # プラズマ（腹の下。上昇で消える）
        def plasma(t, pts, lines):
            if not in_space(t):
                return
            k = 1.0 - anim.smooth(anim.remap(t, TURN - 2.0, TURN + 5.0))
            if k <= 0.01:
                return
            fl = 0.85 + 0.15 * math.sin(t * 29)
            for z in np.linspace(0, 360, 9):
                q = city.world(city.pivot + np.array([0.0, 0.0, z]))
                q = q + np.array([0, -110.0, 0])
                pts.append((q, 260.0 * fl, (1.0, 0.45, 0.2, 0.5 * k), 0.0))
        glow.add(fx.Func(plasma))
        # 艦橋
        be = st.ship(lib.bridge("empire").voxels(), size=0.5, pivot=(0, 0, 0), sun=(0.25, 0.85, -0.45), ambient=0.42)
        be.place(BR_E)
        be.when = in_be
        bu = st.ship(lib.bridge("union").voxels(), size=0.5, pivot=(0, 0, 0), sun=(0.25, 0.85, -0.45), ambient=0.5)
        bu.place(BR_U)
        bu.when = in_bu
        sr = self.sirotsu = lib.actor(st, "sirotsu", BR_E + np.array([0.4, 0.0, 3.6]), 0.0)
        sr.visible = in_be
        sr.poses = [(0, lib.pose("point")), (5.2, lib.pose("point")), (6.0, {}), (7.0, lib.pose("hand_chest")), (9, lib.pose("hand_chest"))]
        sr.expr = {"surprised": anim.Track((4.5, 0.2), (5.4, 0.6), (6.8, 0.3)), "sad": anim.Track((6.5, 0.0), (7.5, 0.35))}
        sr.blinks = [5.9, 7.6]
        sr.look = lambda t: BR_E + np.array([0.0, 1.6, 30.0]) if t < 6.4 else BR_E + np.array([2.0, 0.6, 20.0])
        ks = self.kensu = lib.actor(st, "kensu", BR_U + np.array([0.8, 0.0, 3.6]), 0.0)
        ks.visible = in_bu
        ks.poses = [(0, lib.pose("hands_back")), (9.6, lib.pose("hands_back")), (10.4, lib.pose("reach")), (13, lib.pose("reach"))]
        ks.expr = {"happy": anim.Track((8.6, 0.2), (11.0, 0.55))}
        ks.blinks = [9.4, 11.8]
        ks.look = lambda t: BR_U + np.array([0.0, 1.6, 30.0])
        # 計算室
        cr = st.ship(lib.calc_room().voxels(), size=0.5, pivot=(0, 0, 0), sun=(0.2, 0.8, -0.5), ambient=0.5)
        cr.place(CALC)
        cr.when = in_calc
        nk = self.nekosa = lib.actor(st, "nekosa", CALC + np.array([-3.7, 0.0, 1.6]), 30.0)
        nk.place = lib.walk([(0.0, CALC + np.array([-3.7, 0.0, 1.6]), 20.0), (13.6, CALC + np.array([-3.7, 0.0, 1.6]), 20.0),
                             (14.6, CALC + np.array([-3.5, 0.0, 1.4]), 130.0), (40, CALC + np.array([-3.5, 0.0, 1.4]), 130.0)])
        nk.poses = [(0, lib.pose("type")), (13.8, lib.pose("type")), (14.4, lib.pose("cheer")), (16.0, lib.pose("cheer")), (16.6, {}), (40, {})]
        nk.expr = {"happy": anim.Track((13.8, 0.0), (14.4, 0.8), (17.0, 0.5))}
        nk.blinks = [16.5, 19.6]
        rt = self.rutero = lib.actor(st, "rutero", CALC + np.array([1.2, 0.0, -1.4]), -40.0)
        rt.poses = [(0, lib.pose("crossed")), (16.0, lib.pose("crossed")), (16.6, lib.pose("hand_chest")), (17.8, lib.pose("hand_chest")),
                    (18.2, lib.pose("fist")), (40, lib.pose("fist"))]
        rt.expr = {"happy": anim.Track((16.0, 0.1), (17.0, 0.3)), "angry": anim.Track((17.6, 0.0), (18.3, 0.35))}
        rt.blinks = [14.6, 17.3, 20.4]
        db = self.dabai = lib.actor(st, "dabai", CALC + np.array([-1.0, 0.0, -1.6]), 30.0)
        db.poses = [(0, lib.pose("watch")), (19.4, lib.pose("watch")), (20.0, lib.pose("point")), (22, lib.pose("point"))]
        db.expr = {"happy": anim.Track((20.6, 0.0), (21.4, 0.3))}
        db.blinks = [15.2, 18.4, 21.0]
        HOLO = CALC + np.array([0.0, 1.5, 0.0])
        nk.look = lambda t: CALC + np.array([-3.0, 1.6, 4.0]) if t < 14.2 else HOLO
        rt.look = lambda t: HOLO if t < 16.2 else nk.head_pos(t=t) if t < 17.6 else db.head_pos(t=t)
        db.look = lambda t: HOLO if t < 20.8 else rt.head_pos(t=t)
        for a in (nk, rt, db):
            a.visible = in_calc
            a.light(key_dir=(0.2, 0.8, -0.5), key_col=(1.0, 0.98, 0.96), fill_col=(0.6, 0.75, 0.9), rim_col=(0.3, 0.8, 1.0))
        for a in (sr,):
            a.light(key_dir=(0.2, 0.6, 1.0), key_col=(1.0, 0.92, 0.9), fill_col=(0.6, 0.45, 0.48), rim_col=(0.9, 0.2, 0.15))
        ks.light(key_dir=(0.2, 0.6, 1.0), key_col=(1.0, 0.97, 0.95), fill_col=(0.6, 0.62, 0.78), rim_col=(0.4, 0.5, 0.8))

        # 立体映像のアマツと軌道（光の線）
        def holo(t, pts, lines):
            if not in_calc(t):
                return
            c = HOLO
            yaw = t * 0.5
            ax = np.array([math.cos(yaw), 0.0, math.sin(yaw)])
            sd = np.array([-math.sin(yaw), 0.0, math.cos(yaw)])
            up = np.array([0.0, 1.0, 0.0])
            r, L = 0.18, 0.8
            for k in range(12):
                a = k / 12 * 2 * math.pi
                o = (sd * math.cos(a) + up * math.sin(a)) * r
                lines.append(("line", c - ax * L / 2 + o, c + ax * L / 2 + o, 0.012, (0.35, 0.85, 1.0, 0.6)))
            # 落ちる軌道（赤）→ 計算後の軌道（水色）
            done = anim.smooth(anim.remap(t, 13.8, 15.0))
            for k in range(24):
                a0, a1 = k / 24 * 2 * math.pi, (k + 1) / 24 * 2 * math.pi
                for rad, col, al in ((1.0 - 0.012 * k, (1.0, 0.3, 0.2), 0.7 * (1 - 0.6 * done)), (1.05, (0.4, 1.0, 0.8), 0.8 * done)):
                    p0 = c + np.array([math.cos(a0) * rad, -0.6, math.sin(a0) * rad])
                    p1 = c + np.array([math.cos(a1) * rad, -0.6, math.sin(a1) * rad])
                    if al > 0.01:
                        lines.append(("line", p0, p1, 0.015, col + (al,)))
            if t > 19.6:
                for i in range(24):
                    q = c + ax * (-0.3 + 0.6 * (i // 8) / 2) + (sd * math.cos(i * 0.8) + up * math.sin(i * 0.8)) * r
                    if t > 19.6 + i * 0.05:
                        pts.append((q, 0.06, (0.6, 0.95, 1.0, 0.9), 0.0))
            pts.append((CALC + np.array([0.0, 0.3, 0.0]), 2.2, (0.3, 0.75, 1.0, 0.25), 0.0))
        glow.add(fx.Func(holo))
        # 外殻の作業場
        v = lib.Vox()
        for x in range(-16, 17):
            for z in range(-16, 17):
                v.put(x, -1, z, "m_white" if (x // 4 + z // 4) % 2 else "m_gray")
        v.box(-2, 2, 0, 1, 3, 4, "m_dark")              # 操作台
        v.box(-2, 2, 2, 2, 4, 4, "g_screen")
        v.box(5, 9, 0, 3, 6, 10, "m_dark")              # モジュール
        v.box(6, 8, 0, 2, 11, 11, "g_cyan")
        hl = st.ship(v.voxels(), size=0.5, pivot=(0, 0, 0), sun=(0.55, 0.45, -0.7), ambient=0.35)
        hl.place(HULL)
        hl.when = in_hull
        kg = self.kingi = lib.actor(st, "kingi")
        kg.place = lib.walk([(0.0, HULL + np.array([0.1, 0.0, 1.0]), 0.0), (28.4, HULL + np.array([0.1, 0.0, 1.0]), 0.0),
                             (29.4, HULL + np.array([-0.4, 0.0, 0.6]), 20.0), (40.0, HULL + np.array([-0.4, 0.0, 0.6]), 20.0)])
        kg.poses = [(0, lib.pose("type")), (28.2, lib.pose("type")), (28.9, lib.pose("slump")), (40, lib.pose("slump", bob=-0.6))]
        kg.expr = {"sad": anim.Track((28.4, 0.0), (29.0, 0.5)), "happy": anim.Track((29.4, 0.0), (30.0, 0.25))}
        kg.blinks = [27.6, 29.0, 30.4]
        kg.look = lambda t: HULL + np.array([0.1, 1.0, 2.2]) if t < 28.6 else dn.head_pos(t=t)
        dn = self.donachi = lib.actor(st, "donachi")
        dn.place = lib.walk([(0.0, HULL + np.array([2.6, 0.0, -2.4]), -40.0), (28.6, HULL + np.array([2.6, 0.0, -2.4]), -40.0),
                             (30.4, HULL + np.array([0.35, 0.0, 1.05]), 0.0), (40.0, HULL + np.array([0.35, 0.0, 1.05]), 0.0)])
        dn.poses = [(0, {}), (30.3, {}), (30.9, lib.pose("type")), (40, lib.pose("type"))]
        dn.expr = {"angry": anim.const(0.12)}
        dn.blinks = [31.4]
        dn.look = lambda t: kg.head_pos(t=t) if t < 30.4 else HULL + np.array([0.35, 1.0, 2.2])
        for a in (kg, dn):
            a.visible = in_hull
            a.light(key_dir=(0.55, 0.45, -0.7), key_col=(1.05, 1.0, 0.95), fill_col=(0.4, 0.55, 0.85), rim_col=(0.5, 0.8, 1.0))
        glow.add(fx.Glow(lambda t: HULL + np.array([3.5, 0.6, 5.6]) if in_hull(t) else None,
                         lambda t: 5.0 + 0.4 * math.sin(t * 40), (0.5, 0.85, 1.0, 0.9)))
        beam.add(fx.Beam(lambda t: (HULL + np.array([3.5, 0.6, 5.6]), HULL + np.array([3.5, -8.0, 5.6]), 2.4, (0.5, 0.85, 1.0, 0.7))
                         if in_hull(t) else None))
        # セリフ
        self.say("sirotsu", 5.0, 8.3, "攻撃が……止まった？ ――あの時と同じだ", sr)
        self.say("kensu", 9.1, 12.7, "帝国艦隊へ。……一緒に、でかい荷物を押さないか", ks)
        self.say("nekosa", 13.9, 15.8, "軌道計算、出ました！", nk)
        self.say("rutero", 16.3, 19.1, "構造はあたしが知ってる。……あたしの都市だもの！", rt)
        self.say("dabai", 19.6, 21.8, "モジュール配置、24基。……いける", db)
        self.say("kingi", 29.4, 30.5, "……悪ぃ", kg)
        self.say("オペレーター", 34.2, 36.2, "上昇、確認！")

    # --- カメラ ----------------------------------------------------------------
    def cam_wide(self, t, u):
        c = self.city.pos
        pos = c + np.array([-900.0, 160.0, -1300.0]) + np.array([120.0, -20.0, 80.0]) * u
        return Cam(pos, c - np.array([0, 160.0, 0]), fov=50, shake=0.3)

    def _bridge_cam(self, a, t, u):
        h = a.head_pos(t=t)
        f = a.face_dir(t)
        r = np.cross([0, 1, 0], f)
        return Cam(h + f * (1.5 - 0.2 * u) + r * 0.45 - np.array([0, 0.1, 0]), h, fov=30, aperture=6.0)

    def cam_sirotsu(self, t, u):
        return self._bridge_cam(self.sirotsu, t, u)

    def cam_kensu(self, t, u):
        return self._bridge_cam(self.kensu, t, u)

    def cam_nekosa(self, t, u):
        h = self.nekosa.head_pos(t=t)
        pos = CALC + np.array([-1.4, 1.5, 3.4]) + np.array([-0.3, 0.0, -0.3]) * u
        return Cam(pos, h - np.array([0, 0.15, 0]), fov=32, aperture=5.0)

    def cam_rutero(self, t, u):
        h = self.rutero.head_pos(t=t)
        f = self.rutero.face_dir(t)
        r = np.cross([0, 1, 0], f)
        return Cam(h + f * 1.3 - r * 0.4 + np.array([0, -0.05, 0]) - f * 0.2 * u, h, fov=30, aperture=6.0)

    def cam_dabai(self, t, u):
        h = self.dabai.head_pos(t=t)
        hc = CALC + np.array([0.0, 1.4, 0.0])
        pos = CALC + np.array([1.6, 1.7, 1.4]) + np.array([-0.2, 0.0, 0.3]) * u
        return Cam(pos, (h + hc) / 2, fov=40, aperture=3.0, focus=float(np.linalg.norm(h - pos)))

    def cam_ignite(self, t, u):
        c = self.city.pos
        ax = self.city.R @ np.array([0, 0, 1.0])
        side = anim.unit(np.cross(ax, [0, 1.0, 0]))
        pos = c + side * 820.0 + ax * 250.0 + np.array([0, -140.0, 0]) + ax * -200.0 * anim.smooth(u)
        return Cam(pos, c + np.array([0, -200.0, 0]), fov=50, shake=0.6 * anim.remap(t, IGN0, IGN0 + 0.5) * (1 - anim.remap(t, 25.0, 27.0)) + 0.15)

    def cam_kingi(self, t, u):
        h = self.kingi.head_pos(t=t)
        pos = HULL + np.array([-2.4, 1.35, -0.6]) + np.array([0.2, -0.1, 0.2]) * u
        return Cam(pos, h - np.array([0, 0.2, 0]), fov=34, aperture=4.0)

    def cam_donachi(self, t, u):
        h = self.donachi.head_pos(t=t)
        pos = HULL + np.array([0.9, 2.6, 3.9])
        return Cam(pos, h - np.array([0, 0.1, 0]), fov=34, aperture=4.0)

    def cam_rise(self, t, u):
        c = self.city.pos
        ax = self.city.R @ np.array([0, 0, 1.0])
        side = anim.unit(np.cross(ax, [0, 1.0, 0]))
        pos = side * -950.0 + ax * -300.0 + np.array([0, city_y(33.0) - 80.0, 0]) + ax * 200.0 * u
        return Cam(pos, c + np.array([0, -150.0, 0]), fov=50, shake=0.1)

    def grade(self, t):
        g = self.base_grade(t, fade_out=1.4)
        if in_be(t):
            g.tint = (1.04, 0.96, 0.96)
        if in_calc(t):
            g.tint = (0.97, 1.0, 1.04)
        if IGN0 <= t < IGN0 + 0.3:
            g.flash = 0.25 * (1 - (t - IGN0) / 0.3)
            g.flash_color = (0.7, 0.9, 1.0)
        return g

    def audio(self, path):
        from cinema import sound as S
        mx = S.Mixer(self.duration)
        mx.add(S.lowpass(S.noise(4.5, 3), 600) * S.env(4.5, 0.3, 0, 1, 1.0), 0, 0.25)
        for k in range(10):
            mx.add(S.zap(0.3, 1200 if k % 2 else 700, 200), 0.15 + k * 0.17, 0.12 * (1 - k / 12))
        mx.add(S.pad(S.chord("D3 A3 D4 F4"), 8.0, a=1.0, r=2.0, bright=1000), 3.6, 0.25)
        mx.add(S.pad(S.chord("A#2 F3 A#3 D4"), 6.0, a=1.0, r=2.0, bright=1100), 8.6, 0.25)
        mx.add(S.pad(S.chord("G2 D3 G3 B3"), 9.0, a=0.5, r=1.5, bright=1600), 13.0, 0.28)
        mx.add(S.shimmer(1.0, 3), 13.9, 0.2)
        mx.add(S.riser(3.0, 150, 2000), IGN0 - 3.0, 0.25)
        for i in range(24):
            mx.add(S.launch(1.4), ign(i), 0.1)
        mx.add(S.hit(2.5, 36), IGN0, 0.8)
        mx.add(S.lowpass(S.noise(15.0, 8), 400) * S.env(15.0, 1.0, 0, 1, 3.0), IGN0, 0.35)
        mx.add(S.pad(S.chord("A2 E3 A3 C#4 E4"), 7.0, a=1.0, r=2.0, bright=1500), 22.0, 0.3)
        mx.add(S.pad(S.chord("F#2 C#3 F#3 A3"), 6.0, a=1.0, r=1.5, bright=700), 27.0, 0.25)
        mx.add(S.pad(S.chord("D2 A2 D3 F#3 A3 D4"), 8.0, a=1.0, r=3.0, bright=2000), 33.0, 0.35)
        mx.add(S.hit(2.0, 45), 34.0, 0.5)
        return mx.save(path)

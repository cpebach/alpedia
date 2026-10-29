"""11 エピローグ「それでも」（約 46 秒）

停戦から三か月。法廷でネムリは生きて裁かれることを選ぶ。
どこかの暗い部屋では、N財団の残党がまだ終わっていないとつぶやく。
発射場では、みんながトワを見送る。サクラ、火のつかない煙草のキンギ、ミカガ。
「……それでも、行く」。ロケットが空へ上がっていく。
"""
import math

import numpy as np

from cinema import anim, fx
from cinema.film import Cam, Shot
from scenes import lib
from scenes.base import Movie

COURT = np.array([30000.0, 0.0, 0.0])
NOC = np.array([-30000.0, 0.0, 0.0])
PAD = np.array([0.0, 0.0, 0.0])               # 発射台（観覧の丘は手前 70 m）
LIFT = 37.0
SEE = np.array([0.0, 0.0, -66.0])              # 見送りの場所の真ん中


def in_court(t):
    return t < 10.0


def in_noc(t):
    return 10.0 <= t < 15.5


def in_pad(t):
    return t >= 15.5


class FILM(Movie):
    name = "11_epilogue"
    duration = 46.0
    near, far = 0.05, 60000.0
    sun = (0.45, 0.55, 0.7)
    ambient = 0.36
    subframes = 1

    def __init__(self):
        super().__init__()
        self.captions = [(0.6, 3.8, "――停戦から三か月"), (16.0, 19.0, "旅立ちの日")]
        self.titles = [(42.2, 45.6, "ALPEDIA", "それでも")]
        self.shots = [
            Shot(0.0, 4.4, self.cam_court, "法廷"),
            Shot(4.4, 10.0, self.cam_nemuri, "ネムリ"),
            Shot(10.0, 15.5, self.cam_noc, "夜会"),
            Shot(15.5, 20.0, self.cam_pad_wide, "発射場"),
            Shot(20.0, 24.4, self.cam_sakura, "サクラ"),
            Shot(24.4, 28.4, self.cam_kingi, "キンギ"),
            Shot(28.4, 32.6, self.cam_mikaga, "ミカガ"),
            Shot(32.6, 36.4, self.cam_towa, "トワ"),
            Shot(36.4, 41.6, self.cam_launch, "発射"),
            Shot(41.6, 46.0, self.cam_sky, "空"),
        ]

    def rocket_y(self, t):
        a = max(0.0, t - LIFT)
        return 2.5 * a * a + 0.6 * a ** 3

    # --- 舞台 ----------------------------------------------------------------
    def build(self, st):
        sky = fx.Sky(st, top=(0.24, 0.42, 0.74), horizon=(0.86, 0.84, 0.8), below=(0.7, 0.7, 0.7), sun_glow=0.4)
        sky.when = in_pad
        g = fx.Ground(st, size=2400, n=220, style="tundra", seed=8, flat=[(0, 0, 40, 0.0), (0, -66, 16, 0.0)],
                      fog=(0.86, 0.85, 0.82))
        g.when = in_pad
        lp = st.ship(lib.launch_pad().voxels(), size=1.0, pivot=(0, 0, 0))
        lp.place(PAD)
        lp.when = in_pad
        rk = self.rocket = st.ship(lib.rocket().voxels(), size=1.0, pivot=(0, 0, 0), name="rocket")
        rk.motion = lambda t: (PAD + np.array([0.0, 1.0 + self.rocket_y(t), 0.0]), np.eye(3)) if in_pad(t) else None
        # 法廷
        cr = st.ship(lib.courtroom().voxels(), size=0.5, pivot=(0, 0, 0), sun=(0.3, 0.8, -0.5), ambient=0.5)
        cr.place(COURT)
        cr.when = in_court
        nm = self.nemuri = lib.actor(st, "nemuri", COURT + np.array([0.0, 0.0, -0.1]), 0.0)
        nm.visible = in_court
        nm.poses = [(0, lib.pose("hands_back")), (10, lib.pose("hands_back"))]
        nm.look = lambda t: COURT + np.array([0.0, 2.8, 5.0]) if t < 8.6 else COURT + np.array([0.0, 1.0, 5.0])
        nm.expr = {"sad": anim.Track((0, 0.15), (8.6, 0.25)), "happy": anim.Track((7.0, 0.0), (8.4, 0.25))}
        nm.blinks = [2.0, 5.4, 8.8]
        nm.light(key_dir=(0.3, 0.8, 0.5), key_col=(1.05, 0.97, 0.88), fill_col=(0.7, 0.68, 0.7), rim_col=(0.8, 0.7, 0.5))
        # 夜会
        nc = st.ship(lib.nocturne().voxels(), size=0.5, pivot=(0, 0, 0), sun=(0.0, 0.6, -0.8), ambient=0.22)
        nc.place(NOC)
        nc.when = in_noc
        glow = fx.Sprites(st, "glow", "add", order=12)
        beam = fx.Sprites(st, "beam", "add", order=13)
        smoke = fx.Sprites(st, "smoke", "alpha", order=8)
        for k, z in enumerate((-2.4, -0.8, 0.8, 2.4)):
            for x in (-0.3, 0.3):
                p = NOC + np.array([x, 1.35, z])
                glow.add(fx.Glow(lambda t, p=p, k=k: p if in_noc(t) else None,
                                 lambda t, k=k: 0.22 + 0.04 * math.sin(t * 9 + k * 2.1), (1.0, 0.6, 0.25, 0.9)))
                glow.add(fx.Glow(lambda t, p=p: p if in_noc(t) else None, 1.4, (1.0, 0.5, 0.2, 0.12)))
        # 見送り
        self.cast = {}
        spots = {"towa": ((0.0, -63.6), 180.0), "sakura": ((-1.3, -66.0), None), "kingi": ((1.5, -66.3), None),
                 "mikaga": ((0.3, -67.0), None), "kensu": ((-3.0, -67.2), None), "sirotsu": ((3.3, -67.4), None),
                 "nekosa": ((-2.0, -68.2), None), "dabai": ((-4.4, -68.0), None), "rutero": ((2.3, -68.4), None),
                 "donachi": ((4.6, -68.2), None), "siyama": ((-5.4, -66.8), None)}
        for key, ((x, z), yaw) in spots.items():
            pos = np.array([x, 0.0, z])
            if yaw is None:
                yaw = lib.yaw_to(pos, (0.0, 0.0, -63.6))
            a = lib.actor(st, key, pos, yaw)
            a.visible = in_pad
            a.blinks = lib.blink_every(15, 46, 3.3, off=(x * 0.37) % 1.0)
            a.light(key_dir=(0.45, 0.55, 0.7), key_col=(1.05, 1.0, 0.95), fill_col=(0.68, 0.72, 0.85), rim_col=(0.9, 0.85, 0.7))
            a.expr = {"happy": anim.Track((15, 0.35), (46, 0.45))}
            self.cast[key] = a
        tw, sk, kg, mk = (self.cast[k] for k in ("towa", "sakura", "kingi", "mikaga"))
        self.towa, self.sakura, self.kingi, self.mikaga = tw, sk, kg, mk
        rocket_head = lambda t: PAD + np.array([0.0, 30.0 + self.rocket_y(t), 0.0])  # noqa: E731
        for key, a in self.cast.items():
            if key == "towa":
                continue
            a.look = (lambda t, a=a: tw.head_pos(t=t) if t < LIFT - 0.4 else rocket_head(t))
            a.poses = [(0, {}), (LIFT + 1.2, {}), (LIFT + 2.0, lib.pose("wave_big") if key in ("sakura", "nekosa", "kensu") else
                                                     lib.pose("wave") if key in ("dabai", "rutero") else {}), (46, {})]
        a = self.cast["siyama"]
        a.poses = [(0, lib.pose("watch")), (46, lib.pose("watch"))]
        a = self.cast["sirotsu"]
        a.poses = [(0, lib.pose("crossed")), (LIFT + 1.0, lib.pose("crossed")), (LIFT + 1.8, lib.pose("salute")), (46, lib.pose("salute"))]
        self.cast["donachi"].poses = [(0, lib.pose("hands_back")), (46, lib.pose("hands_back"))]
        sk.poses = [(0, {}), (20.4, {}), (21.0, lib.pose("hand_chest")), (24.0, lib.pose("hand_chest")), (24.6, {}),
                    (LIFT + 1.2, {}), (LIFT + 2.0, lib.pose("wave_big")), (46, lib.pose("wave_big"))]
        sk.expr = {"happy": anim.Track((15, 0.45), (20.5, 0.6), (22.4, 0.3), (24.0, 0.55)), "sad": anim.Track((21.6, 0.0), (22.4, 0.3), (24.0, 0.0))}
        kg.poses = [(0, lib.pose("hands_pockets")), (24.6, lib.pose("hands_pockets")), (25.2, lib.pose("smoke")), (27.6, lib.pose("smoke")),
                    (28.2, lib.pose("hands_pockets")), (46, lib.pose("hands_pockets"))]
        kg.expr = {"happy": anim.Track((15, 0.2), (26.0, 0.45))}
        mk.poses = [(0, lib.pose("hands_back")), (46, lib.pose("hands_back"))]
        mk.expr = {"happy": anim.Track((15, 0.3), (28.6, 0.6), (30.0, 0.4), (31.0, 0.6))}
        tw.place = lib.walk([(0.0, (0.0, 0.0, -63.6), 180.0), (34.4, (0.0, 0.0, -63.6), 180.0), (35.4, (0.0, 0.0, -63.4), 0.0),
                             (46, (0.0, 0.0, -63.4), 0.0)])
        tw.look = lambda t: (sk.head_pos(t=t) if t < 24.4 else kg.head_pos(t=t) if t < 28.4 else mk.head_pos(t=t) if t < 34.8
                             else rocket_head(t))
        tw.poses = [(0, {}), (33.0, {}), (33.6, lib.pose("fist")), (46, lib.pose("fist"))]
        tw.expr = {"happy": anim.Track((15, 0.2), (31.0, 0.35), (33.0, 0.15)), "sad": anim.Track((29.0, 0.0), (30.0, 0.2), (32.6, 0.0))}
        # 火のつかない煙草（白い線）
        beam.add(fx.Beam(lambda t: (kg.mouth_pos(t) + kg.face_dir(t) * 0.02, kg.mouth_pos(t) + kg.face_dir(t) * 0.09 - np.array([0, 0.015, 0]),
                                    0.012, (0.9, 0.88, 0.85, 0.9)) if in_pad(t) else None))
        # ロケットの炎と煙
        on = lambda t: anim.smooth(anim.remap(t, LIFT - 1.6, LIFT - 0.6))  # noqa: E731
        for dx, dz in ((0, 0), (3, 0), (-3, 0), (0, 3), (0, -3)):
            base = lambda t, dx=dx, dz=dz: rk.pos + np.array([dx, -0.4, dz])  # noqa: E731
            glow.add(fx.Glow(lambda t, base=base: base(t) if in_pad(t) and on(t) > 0.02 else None,
                             lambda t: 4.0 * on(t) * (1 + 0.15 * math.sin(t * 43)), (1.0, 0.75, 0.4, 0.95)))
            beam.add(fx.Beam(lambda t, base=base: (base(t), base(t) - np.array([0, 9.0 + 4.0 * on(t), 0]), 2.2 * on(t), (1.0, 0.7, 0.35, 0.85 * on(t)))
                             if in_pad(t) and on(t) > 0.02 else None))
        glow.add(fx.Glow(lambda t: rk.pos + np.array([0, -3.0, 0]) if in_pad(t) and on(t) > 0.02 else None,
                         lambda t: 26.0 * on(t), (1.0, 0.6, 0.3, 0.5)))
        rng = np.random.default_rng(12)
        ang = rng.uniform(0, 2 * math.pi, 60)
        spd = rng.uniform(5, 14, 60)

        def billow(t, pts, lines):
            if not in_pad(t) or t < LIFT - 1.4:
                return
            for k in range(60):
                te = LIFT - 1.4 + (k % 20) * 0.25
                age = t - te
                if age < 0:
                    continue
                r = spd[k] * (1 - math.exp(-age * 0.5)) * 3.0
                p = PAD + np.array([math.cos(ang[k]) * r, 2.0 + age * 0.8 + (k % 3), math.sin(ang[k]) * r])
                a = 0.55 * min(1.0, age * 2) * max(0.0, 1 - age / 9.0)
                pts.append((p, 8.0 + age * 3.0, (0.86, 0.85, 0.84, a), ang[k] + age * 0.2))
        smoke.add(fx.Func(billow))
        smoke.add(fx.Trail(lambda te: rk.pos + np.array([0, -6.0, 0]) if te > LIFT else None, LIFT, 46.0, rate=18, life=4.0,
                           size=(4.0, 14.0), color=(0.9, 0.89, 0.88, 0.55), drift=2.0, seed=7))
        # セリフ
        self.say("nemuri", 4.9, 9.6, "私は、生きて裁かれる。それが、私の選んだ『それでも』だ", nm)
        self.say("？？？", 11.4, 14.4, "……まだ、終わってはいない")
        self.say("sakura", 20.5, 24.2, "その名前、あたしがつけたんだからね。……ちゃんと、帰ってきてよ", sk)
        self.say("kingi", 25.2, 27.8, "……火、つけらんねえんだよな", kg)
        self.say("mikaga", 28.8, 32.2, "行っておいで。……今度は、見届けるから", mk)
        self.say("towa", 33.2, 35.4, "……それでも、行く", tw)

    # --- カメラ ----------------------------------------------------------------
    def cam_court(self, t, u):
        pos = COURT + np.array([-4.5, 4.2, -6.2]) + np.array([1.2, -0.6, 0.6]) * anim.smooth(u)
        return Cam(pos, COURT + np.array([0.0, 1.4, 1.0]), fov=48)

    def cam_nemuri(self, t, u):
        h = self.nemuri.head_pos(t=t)
        pos = COURT + np.array([1.1, 1.45, 1.9]) + np.array([-0.2, 0, -0.3]) * u
        return Cam(pos, h, fov=26, aperture=7.0)

    def cam_noc(self, t, u):
        k = anim.smooth(u)
        pos = NOC + np.array([1.6, 1.9, -3.4]) + np.array([-0.6, 0.0, 3.6]) * k
        tgt = anim.lerp(NOC + np.array([0.0, 1.2, 0.0]), NOC + np.array([0.0, 2.6, 4.5]), k)
        return Cam(pos, tgt, fov=40, aperture=4.0)

    def cam_pad_wide(self, t, u):
        pos = SEE + np.array([-14.0, 2.4, -14.0]) + np.array([3.0, 0.4, 0.0]) * u
        return Cam(pos, PAD + np.array([0.0, 18.0, 0.0]), fov=46, aperture=1.5, focus=float(np.linalg.norm(SEE - pos)))

    def _over(self, speaker, listener, t, d=1.3, side=0.4, fov=30):
        h = speaker.head_pos(t=t)
        f = anim.unit(listener.head_pos(t=t) - h)
        r = np.cross([0, 1, 0], f)
        return Cam(h + f * d + r * side - np.array([0, 0.06, 0]), h, fov=fov, aperture=7.0)

    def cam_sakura(self, t, u):
        return self._over(self.sakura, self.towa, t, 1.35 - 0.2 * u, 0.35)

    def cam_kingi(self, t, u):
        return self._over(self.kingi, self.towa, t, 1.2 - 0.15 * u, -0.35)

    def cam_mikaga(self, t, u):
        return self._over(self.mikaga, self.towa, t, 1.5 - 0.2 * u, 0.25)

    def cam_towa(self, t, u):
        h = self.towa.head_pos(t=t)
        k = anim.smooth(anim.remap(t, 34.2, 36.2))
        f = anim.unit(self.mikaga.head_pos(t=t) - h)
        r = np.cross([0, 1, 0], f)
        pos = anim.lerp(h + f * 1.2 + r * 0.3, h + np.array([0.6, -0.1, -1.4]), k)
        tgt = anim.lerp(h, PAD + np.array([0, 20.0, 0]), k)
        return Cam(pos, tgt, fov=anim.lerp(28, 40, k), aperture=anim.lerp(7.0, 2.0, k), focus=float(np.linalg.norm(h - pos)))

    def cam_launch(self, t, u):
        pos = SEE + np.array([3.5, 2.3, -10.5])
        tgt = self.rocket.pos + np.array([0, 18.0, 0])
        return Cam(pos, tgt, fov=44, shake=0.03 * anim.remap(t, LIFT, LIFT + 1.0))

    def cam_sky(self, t, u):
        pos = SEE + np.array([-3.0, 1.2, -4.0])
        tgt = self.rocket.pos * np.array([1, 0.55, 1]) + np.array([0, 10.0, 0])
        return Cam(pos, tgt, fov=34)

    def grade(self, t):
        g = self.base_grade(t, fade_out=1.4)
        if in_court(t):
            g.tint = (1.03, 1.0, 0.95)
        if in_noc(t):
            g.tint = (1.05, 0.94, 0.86)
            g.exposure = 0.95
        if in_pad(t):
            g.tint = (1.02, 1.0, 0.98)
        return g

    def audio(self, path):
        from cinema import sound as S
        mx = S.Mixer(self.duration)
        mx.add(S.pad(S.chord("C3 G3 C4 E4"), 10.0, a=2.0, r=2.0, bright=900), 0.0, 0.25)
        mx.add(S.drone(55, 6.0, 2) * S.env(6.0, 1.0, 0, 1, 1.5), 10.0, 0.3)
        mx.add(S.pad(S.chord("C2 G2 C#3"), 5.5, a=0.8, r=1.5, bright=400), 10.0, 0.28)
        mx.add(S.pad(S.chord("F3 A3 C4 F4"), 9.0, a=1.5, r=2.0, bright=1400), 15.5, 0.25)
        mx.add(S.pad(S.chord("D3 A3 D4 F4"), 8.0, a=1.0, r=2.0, bright=1300), 24.4, 0.25)
        mx.add(S.pad(S.chord("A#2 F3 A#3 D4"), 6.0, a=1.0, r=2.0, bright=1500), 30.0, 0.25)
        mx.add(S.riser(3.0, 100, 900), LIFT - 3.0, 0.2)
        mx.add(S.launch(3.5), LIFT - 1.2, 0.6)
        mx.add(S.lowpass(S.noise(9.0, 5), 350) * S.env(9.0, 0.8, 0, 1, 3.0), LIFT - 1.0, 0.6)
        mx.add(S.boom(3.0), LIFT - 0.8, 0.5)
        mx.add(S.pad(S.chord("C3 G3 C4 E4 G4"), 9.0, a=1.0, r=4.0, bright=2200), LIFT, 0.35)
        mx.add(S.shimmer(2.0, 8), 42.0, 0.25)
        mx.add(S.pad(S.chord("F2 C3 F3 A3 C4 E4"), 4.5, a=0.5, r=3.0, bright=1800), 41.6, 0.3)
        return mx.save(path)

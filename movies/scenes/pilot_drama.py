"""パイロット版：ドラマパート「通信」（約 17 秒）

月の艦隊の旗艦アルテミス。艦橋で地球型の惑星テラを見ていた管制官ミオのもとに、警報とともに
黒鉄艦隊の総督ヴァルガから立体映像の通信が入る。

  1. 外：旗艦アルテミスが惑星テラの前を進む（低い位置から船体に沿って寄る）
  2. 艦橋・引き：窓ぎわのミオの後ろ姿。警報で振り向く
  3. ミオの寄り：奥の窓にピント → ミオにピントを移す（ピント送り）。「この信号……」
  4. ミオの肩越し：立体映像の台にヴァルガが現れる（手前の肩はぼける）
  5. ミオの寄り（あおり）：決意の表情で号令。暗転
役者はどちらも今ある VRM（alpedia_player1）。ヴァルガは赤い立体映像として描く（あとで本番のモデルに差し替える）。
"""
import math

import numpy as np

from cinema import anim, fx, shapes
from cinema.actor import Actor
from cinema.film import Cam, Film, Shot
from cinema.post import Grade
from scenes import sets

ALARM = 5.9
MIO_POS = np.array([1.0, 0.0, 5.6])
VAL_POS = np.array([0.0, 0.5, 0.0])
VAL_SCALE = 1.15
TURN = (6.15, 7.3)            # ミオが振り向く
HOLO_ON = 10.45
EXT = np.array([400.0, 0.0, -2500.0])     # 外の場面の旗艦の位置（はじめ）


def mio_yaw(t):
    k = anim.smoother(anim.remap(t, *TURN))
    return 0.0 + 192.0 * k


class FILM(Film):
    name = "pilot_drama"
    duration = 17.5
    vertical_fov = False          # このころは画角を横で決めていた
    near, far = 0.05, 60000.0
    sun = (-0.6, 0.5, 0.35)
    ambient = 0.32
    lines = [
        (7.55, 9.95, "ミオ", "この信号……黒鉄艦隊！？"),
        (11.25, 13.7, "ヴァルガ", "久しいな、管制官ミオ。ルナ・ポートは我ら黒鉄艦隊がもらい受ける"),
        (14.05, 14.85, "ミオ", "……させない。"),
        (15.0, 16.6, "ミオ", "全艦、第一種戦闘配置！"),
    ]
    captions = [(0.5, 3.2, "月軌道　ルナ艦隊旗艦アルテミス"), (3.7, 6.2, "艦橋")]

    def __init__(self):
        super().__init__()
        self.shots = [
            Shot(0.0, 3.4, self.cam_exterior, "外"),
            Shot(3.4, 7.2, self.cam_wide, "艦橋・引き"),
            Shot(7.2, 10.4, self.cam_mio, "ミオ寄り"),
            Shot(10.4, 13.8, self.cam_ots, "肩越し"),
            Shot(13.8, 17.5, self.cam_close, "ミオ寄り（あおり）"),
        ]

    # ------------------------------------------------------------------
    def build(self, st):
        fx.Stars(st, dist=52000)
        fx.Planet(st, "terra", (3000, -4000, 40000), scale=15, spin=0.15)
        fx.Planet(st, "luna", (-26000, 9000, 47000), scale=6, style="luna")
        # 艦橋
        br = st.ship(sets.bridge().voxels(), size=0.5, pivot=(0, 0, 0), name="bridge", sun=(0.25, 0.85, -0.45),
                     ambient=0.5)
        br.place((0, 0, 0))
        # 窓の外を進む護衛の巡洋艦
        for k, (p, v) in enumerate((((-70, -14, 260), 7.0), ((120, 10, 470), 5.0), ((-260, 30, 900), 6.0))):
            s = st.ship(shapes.luna_cruiser(seed=k).voxels(), size=1.0, name=f"escort{k}")
            R = anim.yaw_pitch_roll(70 + 10 * k, 0, 0)
            s.motion = (lambda t, p=np.array(p, float), R=R, v=v: (p + R[:, 2] * v * t, R))
        # 外の場面の旗艦
        art = st.ship(shapes.artemis().voxels(), size=1.0, name="artemis")
        art.motion = lambda t: (EXT + np.array([0, 0, 9.0 * t]), anim.yaw_pitch_roll(0, -2, 0)) if t < 3.5 else None
        self.artemis = art
        # 噴射の光（外の場面）
        glow = fx.Sprites(st, "glow", "add", order=12)
        for x in (-3, 0, 3):
            for y in (2, 4):
                glow.add(fx.Glow(lambda t, x=x, y=y: art.world((x + 0.5, y + 0.5, -2.5)) if t < 3.5 else None,
                                 lambda t: 3.2 + 0.4 * math.sin(t * 31), (0.45, 0.85, 1.0, 0.9), 0, 3.5))
        # 警報の赤い回転灯（後ろの扉の上）
        glow.add(fx.Glow((0.0, 3.2, -7.2), lambda t: 1.6 if t > ALARM else 0.0,
                         lambda t: (1.0, 0.15, 0.1, self.alarm(t) * 0.9), 3.4, 99))
        # 立体映像の台：光の柱と、上る光の粒
        holo = fx.Sprites(st, "glow", "add", order=14)

        def beams(t, pts, lines):
            h = self.holo(t)
            if h <= 0:
                return
            for i in range(14):
                a = i / 14 * 2 * math.pi + t * 0.4
                base = np.array([math.cos(a) * 1.9, 0.52, math.sin(a) * 1.9])
                top = np.array([math.cos(a) * 0.9, 2.9, math.sin(a) * 0.9])
                lines.append(("line", base, top, 0.05, (1.0, 0.35, 0.15, 0.22 * h)))
            pts.append(((0, 0.6, 0), 4.4, (1.0, 0.4, 0.2, 0.55 * h), 0.0))
            rng = np.random.default_rng(4)
            for i in range(40):
                ph = (t * 0.35 + rng.uniform()) % 1.0
                a = rng.uniform(0, 2 * math.pi)
                r = rng.uniform(0.4, 1.8) * (1 - ph * 0.4)
                pts.append(((math.cos(a) * r, 0.6 + ph * 2.6, math.sin(a) * r), 0.09,
                            (1.0, 0.6, 0.35, h * (1 - ph) * 0.9), 0.0))
        holo.add(fx.Func(beams))
        # 役者
        mio = Actor(st, sets.vrm(), "mio")
        mio.place = lambda t: (MIO_POS, mio_yaw(t))
        mio.light(key_dir=(-0.35, 0.55, 0.75), key_col=(1.0, 0.97, 0.94), fill_col=(0.78, 0.84, 1.0),
                  rim_col=(0.3, 0.45, 0.7))
        hands_back = {"arm_l": (0.3, -1.0, -0.35), "arm_r": (0.3, -1.0, -0.35), "fore_l": (-0.75, -0.45, -0.5),
                      "fore_r": (-0.75, -0.45, -0.5), "head": (0.0, 0.08, 1.0)}
        startle = {"arm_r": (0.25, -0.55, 0.6), "fore_r": (-0.75, 0.55, 0.35), "arm_l": (0.3, -1.0, 0.05),
                   "fore_l": (0.2, -1.0, 0.2), "chest": (0.0, 1.0, -0.06)}
        brace = {"arm_r": (0.35, -0.8, 0.35), "fore_r": (0.2, -0.6, 0.8), "arm_l": (0.3, -1.0, 0.0),
                 "fore_l": (0.2, -1.0, 0.25), "chest": (0.0, 1.0, 0.05)}
        command = {"arm_r": (0.9, 0.05, 0.55), "fore_r": (0.85, 0.2, 0.7), "arm_l": (0.3, -1.0, 0.0),
                   "fore_l": (0.25, -1.0, 0.25), "chest": (0.0, 1.0, 0.07)}
        mio.poses = [(0.0, hands_back), (5.9, hands_back), (6.6, {}), (7.4, startle), (9.6, startle),
                     (10.6, brace), (14.6, brace), (15.25, command), (17.5, command)]

        def mio_look(t):
            if t < ALARM + 0.25:
                return np.array([3.0, 0.0, 40.0])              # 窓の外のテラ
            if t < 10.2:
                return np.array([0.2, 1.75, -1.0]) if t > 7.6 else np.array([-1.0, 1.9, -3.0])
            return VAL_POS + np.array([0, 1.75 * VAL_SCALE, 0])
        mio.look = mio_look
        mio.expr = {"surprised": anim.Track((6.0, 0), (6.35, 0.75), (8.0, 0.55), (10.4, 0.15), (11.6, 0.35), (13.5, 0.1),
                                            (14.0, 0.0)),
                    "angry": anim.Track((13.6, 0), (14.3, 0.28), (17.5, 0.32)),
                    "sad": anim.Track((13.6, 0), (14.2, 0.18), (15.0, 0.0))}
        mio.talk = [(7.55, 8.35), (8.75, 9.85), (14.1, 14.75), (15.05, 16.45)]
        mio.blinks = [4.2, 6.05, 9.2, 11.9, 13.9, 16.7]
        self.mio = mio
        val = Actor(st, sets.vrm(), "valga", scale=VAL_SCALE)
        val.place = lambda t: (VAL_POS, 10.0)
        val.holo_mode((1.0, 0.38, 0.18))
        val.holo = self.holo
        crossed = {"arm_l": (0.3, -0.5, 0.8), "fore_l": (-0.95, 0.15, 0.25), "arm_r": (0.3, -0.5, 0.8),
                   "fore_r": (-0.95, 0.25, 0.15), "chest": (0.0, 1.0, -0.1), "head": (0.0, 0.12, 1.0)}
        offer = {"arm_r": (0.45, -0.15, 0.9), "fore_r": (0.35, 0.05, 1.0), "arm_l": (0.3, -0.5, 0.8),
                 "fore_l": (-0.95, 0.15, 0.25), "chest": (0.0, 1.0, -0.05), "head": (0.0, 0.05, 1.0)}
        val.poses = [(0.0, crossed), (12.2, crossed), (12.8, offer), (17.5, offer)]
        val.look = lambda t: MIO_POS + np.array([0, 1.5, 0])
        val.expr = {"happy": anim.const(0.35), "angry": anim.const(0.3)}
        val.talk = [(11.3, 12.4), (12.6, 13.6)]
        val.blinks = [12.5, 15.0]
        self.val = val
        # 警報で人物にかかる赤い光
        self._lights = [(mio, (-0.35, 0.55, 0.75)), (val, (0.3, 0.6, 0.8))]
        st.add(self)

    def update(self, t, origin):
        """警報の赤い光を人物に当てる（毎コマ）。"""
        a = self.alarm(t)
        m = self.mio
        kd = (-0.35, 0.55, 0.75) if t < TURN[0] + 0.6 else (-0.55, 0.5, -0.65)   # 振り向いたら部屋の側から
        if t >= 13.8:
            a *= 0.4
        m.light(key_dir=kd, key_col=(1.0 + 0.06 * a, 0.97 - 0.1 * a, 0.94 - 0.12 * a),
                fill_col=(0.74 + 0.12 * a, 0.8 - 0.08 * a, 0.98 - 0.12 * a),
                rim_col=(0.3 + 0.25 * a, 0.42 - 0.12 * a, 0.65 - 0.25 * a))

    # --- 時刻 → 状態 -------------------------------------------------------
    def alarm(self, t):
        if t < ALARM:
            return 0.0
        return (0.5 + 0.5 * math.sin((t - ALARM) * 2 * math.pi * 1.1 - 1.4)) * min(1.0, (t - ALARM) * 3)

    def holo(self, t):
        if t < HOLO_ON:
            return 0.0
        k = anim.remap(t, HOLO_ON, HOLO_ON + 0.8)
        flick = 0.75 + 0.25 * math.sin(t * 53.0) * math.sin(t * 17.0)
        if k < 1:
            flick *= 0.4 + 0.6 * (math.sin(t * 90) > -0.2)
        return k * flick

    # --- カメラ ------------------------------------------------------------
    def cam_exterior(self, t, u):
        c = EXT + np.array([0, 0, 9.0 * t])
        pos = c + anim.lerp(np.array([-30.0, -9.0, -78.0]), np.array([-36.0, -8.0, -42.0]), anim.ease_in_out(u))
        tgt = c + anim.lerp(np.array([8.0, 2.0, 60.0]), np.array([10.0, 4.0, 90.0]), anim.ease_in_out(u))
        return Cam(pos, tgt, fov=42, roll=anim.lerp(7, 3, u), aperture=0.0)

    def head(self):
        return MIO_POS + np.array([0, 1.47, 0])

    def cam_wide(self, t, u):
        pos = anim.lerp(np.array([-1.9, 1.55, 0.2]), np.array([-1.35, 1.5, 1.5]), anim.smooth(u))
        tgt = np.array([0.9, 1.45, 7.0])
        shake = 0.012 if t > ALARM else 0.0
        return Cam(pos, tgt, fov=36, aperture=5.0, focus=float(np.linalg.norm(self.head() - pos)), shake=shake)

    def _face_dir(self, t):
        y = math.radians(mio_yaw(t))
        return np.array([math.sin(y), 0.0, math.cos(y)])

    def cam_mio(self, t, u):
        h = self.head()
        f = self._face_dir(9.0)
        right = np.cross([0, 1, 0], f)
        d = anim.lerp(1.55, 1.2, anim.smooth(u))
        pos = h + f * d - right * 0.32 - np.array([0, 0.05, 0])
        tgt = h + np.array([0, -0.04, 0])
        rack = anim.smoother(anim.remap(t, 7.3, 7.95))
        focus = anim.lerp(30.0, float(np.linalg.norm(tgt - pos)), rack)
        return Cam(pos, tgt, fov=30, aperture=11.0, focus=focus, max_blur=16)

    def cam_ots(self, t, u):
        h = self.head()
        vh = VAL_POS + np.array([0, 1.55 * VAL_SCALE, 0])
        f = anim.unit(vh - h)
        f[1] = 0
        f = anim.unit(f)
        right = np.cross([0, 1, 0], f)
        pos = h - f * anim.lerp(1.0, 0.85, u) + right * 0.5 + np.array([0, 0.16, 0])
        tgt = anim.lerp(vh - np.array([0, 0.35, 0]), vh - np.array([0, 0.15, 0]), anim.smooth(u)) - right * 0.45
        return Cam(pos, tgt, fov=34, aperture=12.0, focus=float(np.linalg.norm(vh - pos)), max_blur=18)

    def cam_close(self, t, u):
        h = self.head()
        vh = VAL_POS + np.array([0, 1.55 * VAL_SCALE, 0])
        f = anim.unit(vh - h)
        f[1] = 0
        f = anim.unit(f)
        right = np.cross([0, 1, 0], f)
        d = anim.lerp(1.45, 1.15, anim.ease_out(u))
        pos = h + f * d + right * 0.25 - np.array([0, 0.3, 0])
        tgt = h + np.array([0, -0.06, 0])
        return Cam(pos, tgt, fov=33, roll=-3.0, aperture=12.0, max_blur=16)

    # --- 仕上げ ------------------------------------------------------------
    def grade(self, t):
        g = Grade()
        g.fade = min(1.0, t / 0.6) * (1 - anim.remap(t, 16.85, 17.5))
        if t < 3.4:                                    # 外：冷たく深い色
            g.tint = (0.95, 1.0, 1.08)
            g.lift = (0.0, 0.006, 0.02)
            g.bloom = 0.55
            g.contrast = 1.1
        else:                                          # 艦橋：青緑の照明 → 警報で赤く脈打つ
            a = self.alarm(t)
            g.tint = (0.94 + 0.14 * a, 1.0 - 0.07 * a, 1.06 - 0.1 * a)
            g.lift = (0.006 + 0.02 * a, 0.012, 0.025)
            g.bloom_threshold = 0.86
            g.bloom = 0.5
            g.vignette = 0.45
            if 13.8 <= t:                              # 寄り：赤は弱めて、顔は自然な色に
                g.tint = (1.0 + 0.05 * a, 0.99, 1.02)
                g.contrast = 1.1
                g.bloom = 0.3
        if HOLO_ON <= t < HOLO_ON + 0.25:              # 立体映像が現れる瞬間の閃光
            g.flash = (1 - (t - HOLO_ON) / 0.25) * 0.35
            g.flash_color = (1.0, 0.55, 0.4)
        return g

    # --- 音 ------------------------------------------------------------------
    def audio(self, path):
        from cinema import sound as S
        mx = S.Mixer(self.duration)
        # 外：機関のうなりと、静かな和音
        mx.add(S.drone(38, 3.8, seed=1) * S.env(3.8, 0.8, 0, 1, 0.6), 0.0, 0.5)
        mx.add(S.pad(S.chord("A2 E3 A3 C4"), 6.6, a=2.2, r=1.6, bright=900), 0.0, 0.4)
        # 艦橋：空調の低い音
        mx.add(S.drone(58, 14.1, seed=2) * S.env(14.1, 0.4, 0, 1, 1.0), 3.4, 0.22)
        mx.add(S.whoosh(0.8, 200, 1800, 3), 3.0, 0.12)
        # 警報（始めは強く、セリフの間は小さく）
        al = S.alarm(11.6)
        g = np.interp(S._t(11.6), [0, 0.3, 2.2, 3.0, 11.6], [0.0, 1.0, 1.0, 0.35, 0.3])
        mx.add(al * g, ALARM, 0.22, pan=-0.25)
        mx.add(S.hit(2.2, 42), ALARM, 0.8)
        # 緊張の和音
        mx.add(S.pad(S.chord("D2 A2 D3 F3"), 7.2, a=1.5, r=1.5, bright=700), 6.6, 0.38)
        # 立体映像が現れる
        mx.add(S.shimmer(1.4, 5), HOLO_ON - 0.1, 0.45)
        mx.add(S.zap(0.5, 900, 120), HOLO_ON, 0.25)
        mx.add(S.drone(46, 3.4, seed=9) * S.env(3.4, 0.5, 0, 1, 0.5), HOLO_ON + 0.2, 0.3)
        # ミオの決意：上がっていく音 → ドン → 和音
        mx.add(S.riser(1.0, 150, 1400), 12.85, 0.35)
        mx.add(S.hit(1.8, 50), 13.8, 0.9)
        mx.add(S.pad(S.chord("D2 A2 D3 F3 A3"), 4.0, a=0.8, r=1.4, bright=1500), 13.8, 0.4)
        mx.add(S.hit(2.0, 40), 15.05, 1.0)
        mx.add(S.boom(2.5, 1.2, 4) * 0.5, 15.05, 0.5)
        mx.add(S.pad(S.chord("D3 A3 D4 F4"), 2.6, a=0.15, r=1.6, bright=2600), 15.05, 0.35)
        return mx.save(path)

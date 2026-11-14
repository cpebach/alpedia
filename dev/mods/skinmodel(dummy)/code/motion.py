"""プレイヤーの簡単なモーション（手で書いた動き。アニメーションのファイルは使わない）。

状態ごとの姿勢（立つ・歩く・走る・跳ぶ／落ちる・ジェットパック・泳ぐ・座る）を数値で作り、
なめらかに切り替える。骨の回転は「初期姿勢（T ポーズ）からの回転」をモデル座標で表して VRM.pose に渡す。
角度の向きは「骨の向きを、ある方向へ倒す」形で決める（右手系・左手系の取り違えが起きない）。
画面のライブラリは使わない（numpy だけ）。
"""
import math
import random

import numpy as np

from .vrm import qaxis, qinv, qmul, qrot

PARAMS = ("leg_l", "leg_r", "knee_l", "knee_r", "arm_l", "arm_r", "swing_l", "swing_r", "elbow_l", "elbow_r",
          "lean", "twist", "bob", "head", "seat", "spread", "grip")

FINGERS = ("Thumb", "Index", "Middle", "Ring", "Little")
# 手の形：指ごとの (曲げる角度 rad, 横に開く角度 rad)。曲げは付け根・中・先の3つの関節に 1.0・1.1・0.8 倍で分ける。
# 親指は (手のひらへ寄せる角度, 曲げる角度)
HANDS = {
    "relax": {"Thumb": (0.35, 0.25), "Index": (0.3, 0.03), "Middle": (0.38, 0.0), "Ring": (0.46, -0.03), "Little": (0.54, -0.06)},
    "open": {"Thumb": (0.1, 0.05), "Index": (0.04, 0.08), "Middle": (0.04, 0.0), "Ring": (0.05, -0.08), "Little": (0.06, -0.16)},
    "flat": {"Thumb": (0.25, 0.05), "Index": (0.02, 0.0), "Middle": (0.02, 0.0), "Ring": (0.02, 0.0), "Little": (0.02, 0.0)},
    "fist": {"Thumb": (0.75, 0.7), "Index": (1.45, 0.0), "Middle": (1.5, 0.0), "Ring": (1.5, 0.0), "Little": (1.45, 0.0)},
    "grip": {"Thumb": (0.6, 0.45), "Index": (1.05, 0.0), "Middle": (1.1, 0.0), "Ring": (1.1, 0.0), "Little": (1.05, 0.0)},
    "point": {"Thumb": (0.7, 0.6), "Index": (0.02, 0.0), "Middle": (1.5, 0.0), "Ring": (1.5, 0.0), "Little": (1.45, 0.0)},
    "peace": {"Thumb": (0.75, 0.65), "Index": (0.02, 0.18), "Middle": (0.02, -0.12), "Ring": (1.5, 0.0), "Little": (1.45, 0.0)},
    "paw": {"Thumb": (0.5, 0.35), "Index": (0.9, 0.0), "Middle": (0.95, 0.0), "Ring": (1.0, 0.0), "Little": (1.0, 0.0)},
    "pinch": {"Thumb": (0.65, 0.35), "Index": (0.75, 0.0), "Middle": (0.55, 0.0), "Ring": (0.6, 0.0), "Little": (0.7, 0.0)},
}
JOINT_SHARE = (1.0, 1.1, 0.8)


class Motion:
    def __init__(self, vrm):
        self.v = vrm
        self.R, self.U, self.F = vrm.basis()
        h = vrm.human
        self.node = h
        P = vrm.bone_pos

        def d(a, b, default):
            if a in h and b in h:
                x = P(b) - P(a)
                n = np.linalg.norm(x)
                if n > 1e-6:
                    return x / n
            return default
        down = -self.U
        self.dir = {
            "leftUpperLeg": d("leftUpperLeg", "leftLowerLeg", down), "rightUpperLeg": d("rightUpperLeg", "rightLowerLeg", down),
            "leftLowerLeg": d("leftLowerLeg", "leftFoot", down), "rightLowerLeg": d("rightLowerLeg", "rightFoot", down),
            "leftUpperArm": d("leftUpperArm", "leftLowerArm", -self.R), "rightUpperArm": d("rightUpperArm", "rightLowerArm", self.R),
            "leftLowerArm": d("leftLowerArm", "leftHand", -self.R), "rightLowerArm": d("rightLowerArm", "rightHand", self.R),
        }
        self._setup_hands()
        self.leg_len = float(np.linalg.norm(P("leftUpperLeg") - P("leftLowerLeg"))
                             + np.linalg.norm(P("leftLowerLeg") - (P("leftFoot") if "leftFoot" in h else P("leftLowerLeg") - self.U * 0.4)))
        self.hips_h = float((P("hips") - vrm.bounds()[0]) @ self.U)
        self.p = dict.fromkeys(PARAMS, 0.0)
        self.p["arm_l"] = self.p["arm_r"] = 1.25
        self.phase = 0.0
        self.t = 0.0
        self.blink_t = random.uniform(1.0, 4.0)
        self.blink = 0.0

    # --- 手と指 ------------------------------------------------------------------
    def _setup_hands(self):
        """指の骨の並び・向きと、手のひらの向き（初期姿勢）を調べておく。指の骨が無いモデルでは何もしない。"""
        h, P = self.node, self.v.bone_pos
        self.hands = {}
        for s in ("left", "right"):
            if f"{s}Hand" not in h or f"{s}MiddleProximal" not in h:
                continue
            hand = P(f"{s}Hand")
            d_hand = P(f"{s}MiddleProximal") - hand
            d_hand /= np.linalg.norm(d_hand)
            across = P(f"{s}IndexProximal") - P(f"{s}LittleProximal") if f"{s}LittleProximal" in h else self.F
            palm = np.cross(d_hand, across)                       # 手のひらの向き（甲の反対）
            palm /= np.linalg.norm(palm)
            if palm @ self.U > 0:                                 # 初期姿勢（T ポーズ）では手のひらは下向き
                palm = -palm
            fingers = {}
            for f in FINGERS:
                names = ([f"{s}ThumbMetacarpal", f"{s}ThumbProximal", f"{s}ThumbDistal"] if f == "Thumb"
                         and f"{s}ThumbMetacarpal" in h else
                         [f"{s}{f}Proximal", f"{s}{f}Intermediate", f"{s}{f}Distal"])
                names = [n for n in names if n in h]
                if not names:
                    continue
                chain = []
                for k, n in enumerate(names):
                    nxt = P(names[k + 1]) if k + 1 < len(names) else None
                    d = (nxt - P(n)) if nxt is not None else (chain[-1][1] if chain else d_hand)
                    d = d / max(np.linalg.norm(d), 1e-9)
                    chain.append((h[n], d))
                fingers[f] = chain
            across = across - d_hand * (across @ d_hand)
            across /= max(np.linalg.norm(across), 1e-9)               # 小指 → 人さし指（親指の側）
            self.hands[s] = {"dir": d_hand, "palm": palm, "across": across, "fingers": fingers, "node": h[f"{s}Hand"]}

    def hand_quats(self, side, shape="relax", amount=1.0):
        """手の形 shape（HANDS の名前か、{指: (曲げ, 開き)} の辞書）→ {指の骨のノード: 四元数}。amount で弱める。"""
        H = self.hands.get(side)
        if H is None:
            return {}
        key = (side, shape, amount) if isinstance(shape, str) else None
        cache = self.__dict__.setdefault("_hand_cache", {})
        if key is not None and key in cache:                      # 名前の形はいつも同じなので使い回す（毎フレーム・人数分）
            return cache[key]
        out = self._hand_quats(H, side, shape, amount)
        if key is not None:
            cache[key] = out
        return out

    def _hand_quats(self, H, side, shape, amount):
        sh = HANDS.get(shape, HANDS["relax"]) if not isinstance(shape, dict) else {**HANDS["relax"], **shape}
        palm, d_hand, across = H["palm"], H["dir"], H["across"]
        out = {}
        for f, chain in H["fingers"].items():
            curl, spread = (float(x) * amount for x in sh.get(f, (0.0, 0.0)))
            if f == "Thumb":
                # 親指：付け根（中手骨）を、手に沿う・手のひらの側へ・小指の側へ の向きにする（curl＝寄せる量 0〜1 くらい）。
                # 先の2つは手のひらの側へ曲げる（spread＝曲げる角度）
                opp = curl
                want = d_hand * (0.85 - 0.3 * opp) + palm * (0.2 + 0.55 * opp) + across * (0.55 - 1.05 * opp)
                q0 = _arc(chain[0][1], want)
                out[chain[0][0]] = q0
                toward = qrot(qinv(q0), palm - across * 0.4)          # 付け根を回した後の、手のひらの側（初期姿勢の向きで）
                for k, (node, d) in enumerate(chain[1:], 1):
                    ax = np.cross(d, toward)
                    n = np.linalg.norm(ax)
                    out[node] = qaxis(ax / n, spread * JOINT_SHARE[k]) if n > 1e-6 else np.array([0.0, 0.0, 0.0, 1.0])
                continue
            for k, (node, d) in enumerate(chain):
                toward = palm
                ang = curl * JOINT_SHARE[min(k, 2)]
                ax = np.cross(d, toward)
                n = np.linalg.norm(ax)
                q = qaxis(ax / n, ang) if n > 1e-6 and abs(ang) > 1e-4 else np.array([0.0, 0.0, 0.0, 1.0])
                if k == 0 and f != "Thumb" and abs(spread) > 1e-4:
                    # 付け根で横に開く（人さし指側へ +。左右とも親指側が +）
                    q = qmul(q, qaxis(palm, -spread if side == "right" else spread))
                out[node] = q
        return out

    def palm_quat(self, side, arm_rot, want):
        """手首をひねって、手のひらを want（モデル座標の向き）へ向ける回転（手の骨、初期姿勢の向きで）。
        arm_rot：上腕と前腕の回転を合わせたもの（初期姿勢 → 今）。指の向きは前腕に沿ったまま、ひねりだけ"""
        H = self.hands.get(side)
        if H is None:
            return None
        d, p0 = H["dir"], H["palm"]
        t = qrot(qinv(arm_rot), np.asarray(want, float))          # 前腕の向きに戻して考える
        t = t - d * (t @ d)
        p = p0 - d * (p0 @ d)
        if np.linalg.norm(t) < 1e-6 or np.linalg.norm(p) < 1e-6:
            return None
        t, p = t / np.linalg.norm(t), p / np.linalg.norm(p)
        ang = math.atan2(float(np.cross(p, t) @ d), float(p @ t))
        ang = max(-2.0, min(2.0, ang))                            # 手首がねじれすぎないように
        return qaxis(d, ang)

    # --- 状態 → 目標の姿勢 ------------------------------------------------------
    def target(self, st, dt):
        """st: speed（地面に沿った速さ m/s）, on_ground, flying（ジェットパック）, water（0-1）, seated, pitch（視線の上下 rad）"""
        speed = float(st.get("speed", 0.0))
        t = self.t
        p = dict.fromkeys(PARAMS, 0.0)
        p["arm_l"] = p["arm_r"] = 1.25                       # 腕を下ろす（T ポーズから約 72°）
        p["elbow_l"] = p["elbow_r"] = 0.15
        p["head"] = float(np.clip(st.get("pitch", 0.0), -1.0, 1.0)) * 0.5
        breath = math.sin(t * 2 * math.pi * 0.25)
        if st.get("seated"):
            p.update(leg_l=1.45, leg_r=1.45, knee_l=1.45, knee_r=1.45, arm_l=1.3, arm_r=1.3,
                     swing_l=0.45, swing_r=0.45, elbow_l=0.7, elbow_r=0.7, seat=1.0, lean=0.05 + 0.01 * breath)
            return p
        if st.get("water", 0.0) > 0.5 and not st.get("on_ground"):
            k = math.sin(t * 2 * math.pi * 1.6)
            p.update(leg_l=0.25 * k, leg_r=-0.25 * k, knee_l=0.35 + 0.2 * k, knee_r=0.35 - 0.2 * k,
                     arm_l=0.85 + 0.3 * math.sin(t * 2 * math.pi * 0.8), arm_r=0.85 + 0.3 * math.sin(t * 2 * math.pi * 0.8),
                     swing_l=0.5, swing_r=0.5, elbow_l=0.6, elbow_r=0.6, lean=0.25)
            return p
        if st.get("flying"):
            w = math.sin(t * 2 * math.pi * 0.7) * 0.04
            p.update(leg_l=-0.15 + w, leg_r=-0.05 - w, knee_l=0.35, knee_r=0.25, arm_l=1.05, arm_r=1.05,
                     swing_l=-0.35, swing_r=-0.35, elbow_l=0.25, elbow_r=0.25, lean=0.12)
            return p
        if not st.get("on_ground"):
            up = float(st.get("vup", 0.0))
            k = float(np.clip(-up / 6.0, -1, 1))             # 上がる時は脚を曲げ、落ちる時は伸ばして腕を広げる
            p.update(leg_l=0.45 - 0.2 * k, leg_r=-0.05, knee_l=0.8 - 0.4 * k, knee_r=0.45 - 0.2 * k,
                     arm_l=0.75 - 0.2 * k, arm_r=0.75 - 0.2 * k, swing_l=0.15, swing_r=0.15,
                     elbow_l=0.4, elbow_r=0.4, lean=0.05)
            return p
        # 地面の上：立つ・歩く・走る
        if speed < 0.25:
            p["lean"] = 0.01 * breath
            p["bob"] = 0.004 * breath
            p["arm_l"] = p["arm_r"] = 1.25 + 0.02 * breath
            return p
        run = float(np.clip((speed - 2.0) / 3.0, 0.0, 1.0))
        walk = float(np.clip(speed / 1.2, 0.2, 1.0))
        s, c = math.sin(self.phase), math.cos(self.phase)
        amp = (0.32 + 0.3 * run) * walk
        p["leg_l"], p["leg_r"] = amp * s, -amp * s
        kb = (0.55 + 0.8 * run) * walk
        p["knee_l"] = 0.08 + kb * max(0.0, c) + 0.15 * run
        p["knee_r"] = 0.08 + kb * max(0.0, -c) + 0.15 * run
        arm = (0.3 + 0.35 * run) * walk
        p["swing_l"], p["swing_r"] = -arm * s, arm * s
        p["elbow_l"] = p["elbow_r"] = 0.25 + 1.0 * run
        p["arm_l"] = p["arm_r"] = 1.25 - 0.1 * run
        p["lean"] = 0.04 + 0.16 * run
        p["twist"] = 0.08 * s * walk
        p["bob"] = -(0.02 + 0.04 * run) * (1 - abs(c)) - 0.03 * run
        return p

    def update(self, st, dt, emote=None, et=0.0):
        """1フレーム進めて、(骨の回転 {ノード: 四元数}, hips のずれ, まばたき 0-1, 座り具合 0-1, 体の向きのずれ rad) を返す。
        emote：MOD のモーション（関数 pose(t) か {"pose": 関数}）。et はモーションを始めてからの秒"""
        rot, hips, blink, seat = self._update(st, dt)
        return self._emote(rot, hips, blink, seat, emote, et, dt)

    def _update(self, st, dt):
        dt = float(min(max(dt, 0.0), 0.1))
        self.t += dt
        speed = float(st.get("speed", 0.0))
        if st.get("on_ground") and not st.get("seated") and speed >= 0.25:
            run = min(max((speed - 2.0) / 3.0, 0.0), 1.0)
            stride = self.leg_len * (0.9 + 0.5 * run)
            self.phase = (self.phase + dt * math.pi * speed / max(stride, 0.2)) % (2 * math.pi)
        tgt = self.target(st, dt)
        a = 1.0 - math.exp(-dt * 14.0)
        for k in PARAMS:
            self.p[k] += (tgt[k] - self.p[k]) * a
        # まばたき
        self.blink_t -= dt
        if self.blink_t <= 0:
            self.blink_t = random.uniform(2.0, 5.5)
        x = self.blink_t
        self.blink = max(0.0, 1.0 - abs(x - 0.07) / 0.07) if x < 0.14 else 0.0
        return self.rotations(), self.U * self.p["bob"] * (self.hips_h / 0.95), self.blink, self.p["seat"]

    # --- 姿勢 → 骨の回転 ----------------------------------------------------------
    def rotations(self):
        p, R, U, F, h = self.p, self.R, self.U, self.F, self.node
        out = {}

        def tilt(bone, toward, ang, base=None):
            """骨（bone）の向き base を toward の方へ ang だけ倒す回転"""
            b = self.dir.get(bone) if base is None else base
            ax = np.cross(b, toward)
            if np.linalg.norm(ax) < 1e-6:
                ax = R
            return qaxis(ax, ang)

        def put(bone, q):
            if bone in h:
                n = h[bone]
                out[n] = qmul(q, out[n]) if n in out else q
        side = np.cross(-U, F)                                 # 脚を前後に振る軸
        for s_, leg, knee in (("left", "leg_l", "knee_l"), ("right", "leg_r", "knee_r")):
            th, kn = p[leg], max(p[knee], 0.0)
            put(f"{s_}UpperLeg", qaxis(side, th))
            put(f"{s_}LowerLeg", qaxis(side, -kn))
            put(f"{s_}Foot", qaxis(side, -(th - kn) * 0.8 if p["seat"] < 0.5 else 0.0))
        for s_, arm, sw, el in (("left", "arm_l", "swing_l", "elbow_l"), ("right", "arm_r", "swing_r", "elbow_r")):
            down = tilt(f"{s_}UpperArm", -U, p[arm])
            put(f"{s_}UpperArm", qmul(qaxis(side, p[sw]), down))
            put(f"{s_}LowerArm", tilt(f"{s_}LowerArm", F, p[el]))
        put("hips", qaxis(U, p["twist"]))
        lean = qaxis(np.cross(U, F), p["lean"])
        put("spine", qmul(lean, qaxis(U, -p["twist"] * 1.6)))
        ch = "upperChest" if "upperChest" in h else "chest"
        put(ch, qaxis(np.cross(U, F), p["lean"] * 0.4))
        put("neck", qaxis(np.cross(U, -F), p["head"] * 0.4 - p["lean"] * 0.5))
        put("head", qaxis(np.cross(U, -F), p["head"] * 0.6 - p["lean"] * 0.3))
        # 指：ふだんは力を抜いて軽く曲げる（T ポーズのまっすぐ開いた手にしない）。座っている間は握る
        for s_ in ("left", "right"):
            shape = "grip" if p["seat"] > 0.5 else "relax"
            out.update(self.hand_quats(s_, shape))
        return out

    # --- MOD のモーション（カメラモードなどで選ぶ） ------------------------------------
    # pose(t) が返す辞書（向きは体の向きで表す。腕・脚は (外, 上, 前)：左右とも「外」が体の外側）
    #   "arm_l"/"arm_r"：上腕の向き  "fore_l"/"fore_r"：前腕の向き
    #   "thigh_l"/"thigh_r"：太ももの向き  "shin_l"/"shin_r"：すねの向き（足先は地面と平行に保つ）
    #   "spine"/"chest"：胴の上の向き (右, 上, 前)  "head"：顔の向き (右, 上, 前)  "tilt"：首をかしげる角度（右へ +）
    #   "twist"：腰のひねり（rad）  "bob"：腰の上下（m）  "face"：体全体の向きのずれ（rad、左回り +）
    #   "blink"：目を閉じる 0-1（無ければ自動のまばたき）
    #   "hand_l"/"hand_r"：手の形（"relax" 力を抜く・"open" 開く・"flat" そろえて伸ばす・"fist" にぎる・"grip" 物を持つ・
    #                     "point" 指さす・"peace" ピース・"paw" うさぎの手・"pinch" つまむ。{指: (曲げ, 開き)} の辞書でもよい）
    #   "palm_l"/"palm_r"：手のひらを向ける向き（体の向きで (外, 上, 前)。腕を書いたときだけ。手首と前腕をひねる）
    # 書いていない骨は、ふだんの動き（立つ・歩くなど）のまま。始めと終わりはなめらかにつなぐ。
    def _emote(self, rot, hips, blink, seat, emote, et, dt):
        a = 1.0 - math.exp(-dt * 8.0)
        if emote is not None:
            fn = emote.get("pose") if isinstance(emote, dict) else emote
            try:
                pose = fn(float(et)) or {}
            except Exception:  # noqa: BLE001
                pose = {}
            self._emote_last = (self._emote_quats(pose), pose)
            self.emote_w = getattr(self, "emote_w", 0.0) + (1.0 - getattr(self, "emote_w", 0.0)) * a
        else:
            self.emote_w = getattr(self, "emote_w", 0.0) * (1.0 - a)
        w = getattr(self, "emote_w", 0.0)
        last = getattr(self, "_emote_last", None)
        if w < 1e-3 or last is None:
            return rot, hips, blink, seat, 0.0
        quats, pose = last
        out = dict(rot)
        ident = np.array([0.0, 0.0, 0.0, 1.0])
        for n, q in quats.items():
            out[n] = _slerp(rot.get(n, ident), q, w)
        hips = hips * (1 - w) + self.U * float(pose.get("bob", 0.0)) * w
        if "blink" in pose:
            blink = blink * (1 - w) + float(pose["blink"]) * w
        return out, hips, blink, seat, float(pose.get("face", 0.0)) * w

    def _body(self, v, side=None):
        """体の向きの (外または右, 上, 前) → モデル座標。"""
        x, y, z = (float(c) for c in v)
        if side == "left":
            x = -x
        d = x * self.R + y * self.U + z * self.F
        n = np.linalg.norm(d)
        return d / n if n > 1e-9 else self.U.copy()

    def _emote_quats(self, pose):
        h = self.node
        out = {}
        ident = np.array([0.0, 0.0, 0.0, 1.0])
        for s_, sl in (("left", "l"), ("right", "r")):
            for upper, lower, ku, kl in ((f"{s_}UpperArm", f"{s_}LowerArm", f"arm_{sl}", f"fore_{sl}"),
                                         (f"{s_}UpperLeg", f"{s_}LowerLeg", f"thigh_{sl}", f"shin_{sl}")):
                if upper not in h:
                    continue
                qu = None
                ql = None
                if ku in pose:
                    qu = _arc(self.dir[upper], self._body(pose[ku], s_))
                    out[h[upper]] = qu
                if kl in pose and lower in h:
                    t = self._body(pose[kl], s_)
                    if qu is not None:
                        t = qrot(qinv(qu), t)
                    ql = _arc(self.dir[lower], t)
                    out[h[lower]] = ql
                    if "Leg" in upper and f"{s_}Foot" in h:
                        tot = qmul(qu, ql) if qu is not None else ql
                        out[h[f"{s_}Foot"]] = qinv(tot)       # 足先は地面と平行のまま
                if "Arm" in upper:
                    if f"hand_{sl}" in pose:
                        out.update(self.hand_quats(s_, pose[f"hand_{sl}"]))
                    if f"palm_{sl}" in pose and qu is not None and s_ in self.hands:
                        arm = qmul(qu, ql if ql is not None else ident)
                        tw = self.palm_quat(s_, arm, self._body(pose[f"palm_{sl}"], s_))
                        if tw is not None:
                            # ひねりは前腕に 6 割、手首に 4 割（手首だけをひねると、肌がしぼったようにねじれる）
                            d = self.hands[s_]["dir"]
                            ang = 2.0 * math.atan2(float(np.linalg.norm(tw[:3])), float(tw[3])) * (1 if tw[:3] @ d >= 0 else -1)
                            if lower in h:
                                out[h[lower]] = qmul(ql if ql is not None else ident, qaxis(d, ang * 0.6))
                            out[self.hands[s_]["node"]] = qaxis(d, ang * 0.4)
        for key, bone in (("spine", "spine"), ("chest", "upperChest" if "upperChest" in h else "chest")):
            if key in pose and bone in h:
                out[h[bone]] = _arc(self.U, self._body(pose[key]))
        if "twist" in pose and "hips" in h:
            out[h["hips"]] = qaxis(self.U, float(pose["twist"]))
        if ("head" in pose or "tilt" in pose) and "head" in h:
            f = self._body(pose.get("head", (0, 0, 1)))
            q = _arc(self.F, f)
            out[h["head"]] = qmul(qaxis(f, float(pose.get("tilt", 0.0))), q)
        return out


def _arc(a, b):
    """向き a を向き b に回す、いちばん小さい回転。"""
    a = np.asarray(a, dtype=np.float64)
    b = np.asarray(b, dtype=np.float64)
    a = a / max(np.linalg.norm(a), 1e-12)
    b = b / max(np.linalg.norm(b), 1e-12)
    c = float(a @ b)
    if c < -0.9999:
        ax = np.cross(a, [1.0, 0, 0])
        if np.linalg.norm(ax) < 1e-6:
            ax = np.cross(a, [0, 1.0, 0])
        return qaxis(ax, math.pi)
    v = np.cross(a, b)
    q = np.array([v[0], v[1], v[2], 1.0 + c])
    return q / np.linalg.norm(q)


def _slerp(q0, q1, t):
    q0 = np.asarray(q0, dtype=np.float64)
    q1 = np.asarray(q1, dtype=np.float64)
    d = float(q0 @ q1)
    if d < 0:
        q1, d = -q1, -d
    if d > 0.9995:
        q = q0 + (q1 - q0) * t
        return q / np.linalg.norm(q)
    th = math.acos(d)
    return (math.sin((1 - t) * th) * q0 + math.sin(t * th) * q1) / math.sin(th)

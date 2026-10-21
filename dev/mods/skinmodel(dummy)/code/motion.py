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
          "lean", "twist", "bob", "head", "seat", "spread")


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
        self.leg_len = float(np.linalg.norm(P("leftUpperLeg") - P("leftLowerLeg"))
                             + np.linalg.norm(P("leftLowerLeg") - (P("leftFoot") if "leftFoot" in h else P("leftLowerLeg") - self.U * 0.4)))
        self.hips_h = float((P("hips") - vrm.bounds()[0]) @ self.U)
        self.p = dict.fromkeys(PARAMS, 0.0)
        self.p["arm_l"] = self.p["arm_r"] = 1.25
        self.phase = 0.0
        self.t = 0.0
        self.blink_t = random.uniform(1.0, 4.0)
        self.blink = 0.0

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
        return out

    # --- MOD のモーション（カメラモードなどで選ぶ） ------------------------------------
    # pose(t) が返す辞書（向きは体の向きで表す。腕・脚は (外, 上, 前)：左右とも「外」が体の外側）
    #   "arm_l"/"arm_r"：上腕の向き  "fore_l"/"fore_r"：前腕の向き
    #   "thigh_l"/"thigh_r"：太ももの向き  "shin_l"/"shin_r"：すねの向き（足先は地面と平行に保つ）
    #   "spine"/"chest"：胴の上の向き (右, 上, 前)  "head"：顔の向き (右, 上, 前)  "tilt"：首をかしげる角度（右へ +）
    #   "twist"：腰のひねり（rad）  "bob"：腰の上下（m）  "face"：体全体の向きのずれ（rad、左回り +）
    #   "blink"：目を閉じる 0-1（無ければ自動のまばたき）
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
        for s_, sl in (("left", "l"), ("right", "r")):
            for upper, lower, ku, kl in ((f"{s_}UpperArm", f"{s_}LowerArm", f"arm_{sl}", f"fore_{sl}"),
                                         (f"{s_}UpperLeg", f"{s_}LowerLeg", f"thigh_{sl}", f"shin_{sl}")):
                if upper not in h:
                    continue
                qu = None
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

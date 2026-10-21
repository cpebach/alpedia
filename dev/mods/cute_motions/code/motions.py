"""かわいいモーション。各関数は秒 t を受け取り、骨の向きの辞書を返す。

向きは体の向きで表す：腕・脚は (外, 上, 前)（左右とも「外」が体の外側）、胴・顔は (右, 上, 前)。
長さはそろえなくてよい（向きだけ使う）。書いていない骨は、ふだんの立ち姿のまま。
    arm_l / arm_r     上腕の向き        fore_l / fore_r   前腕の向き
    thigh_l / thigh_r 太ももの向き      shin_l / shin_r   すねの向き
    spine / chest     胴の上の向き      head              顔の向き      tilt  首をかしげる（右へ +、rad）
    twist             腰のひねり（rad） bob               腰の上下（m） face  体全体の向き（左回り +、rad）
    blink             目を閉じる（0〜1）
"""
import math

TAU = 2 * math.pi
DOWN = (0.25, -1.0, 0.05)           # 腕を下ろした向き（ふだん）
FORE_DOWN = (0.15, -1.0, 0.15)


def _s(t, hz, phase=0.0):
    return math.sin(TAU * hz * t + phase)


def _ease(x):
    x = min(max(x, 0.0), 1.0)
    return x * x * (3 - 2 * x)


def wave(t):
    """手をふる"""
    k = _s(t, 1.8)
    return {"arm_r": (0.85, 0.55, 0.2), "fore_r": (0.45 * k + 0.1, 1.0, 0.15),
            "arm_l": DOWN, "fore_l": FORE_DOWN, "tilt": 0.12 + 0.04 * _s(t, 0.6), "head": (0.0, 0.05, 1.0),
            "bob": 0.01 * _s(t, 0.9)}


def peace(t):
    """ピース（右手は顔の横、左手は腰）"""
    return {"arm_r": (0.45, -0.25, 0.6), "fore_r": (0.05, 1.0, 0.4),
            "arm_l": (0.7, -0.75, -0.15), "fore_l": (-0.75, -0.2, 0.35),
            "chest": (0.06, 1.0, 0.0), "tilt": 0.28, "head": (0.05, 0.03, 1.0), "bob": 0.006 * _s(t, 0.5)}


def cheeks(t):
    """ほっぺに手"""
    sway = 0.18 * _s(t, 0.55)
    arm = (0.35, -0.6, 0.6)
    fore = (-0.5, 0.85, 0.25)
    return {"arm_l": arm, "arm_r": arm, "fore_l": fore, "fore_r": fore,
            "spine": (0.0, 1.0, 0.08), "tilt": sway, "head": (sway * 0.3, 0.05, 1.0), "twist": -sway * 0.3}


def banzai(t):
    """ばんざい（ぴょんと跳ねる）"""
    hop = max(0.0, _s(t, 1.1))
    crouch = max(0.0, -_s(t, 1.1))
    arm = (0.3, 1.0, 0.05 + 0.1 * crouch)
    knee = 0.45 * crouch
    return {"arm_l": arm, "arm_r": arm, "fore_l": (0.15, 1.0, 0.0), "fore_r": (0.15, 1.0, 0.0),
            "thigh_l": (0.05, -1.0, knee), "thigh_r": (0.05, -1.0, knee),
            "shin_l": (0.05, -1.0, -knee), "shin_r": (0.05, -1.0, -knee),
            "bob": 0.14 * hop - 0.07 * crouch, "head": (0.0, 0.2, 1.0)}


def heart(t):
    """頭の上でハート"""
    sway = 0.12 * _s(t, 0.5)
    return {"arm_l": (0.7, 0.8, 0.08), "arm_r": (0.7, 0.8, 0.08),
            "fore_l": (-0.9, 0.45, 0.0), "fore_r": (-0.9, 0.45, 0.0),
            "tilt": sway, "twist": sway * 0.5, "bob": 0.008 * _s(t, 1.0)}


def bunny(t):
    """うさぎジャンプ（手は胸の前でうさぎの手）"""
    ph = (t * 1.6) % 1.0
    hop = math.sin(math.pi * ph)                        # 0 → 1 → 0
    crouch = 1.0 - min(1.0, hop * 3.0)
    paw_a = (0.2, -0.55, 0.8)
    paw_f = (-0.1, -0.95, 0.35)
    knee = 0.6 * crouch
    return {"arm_l": paw_a, "arm_r": paw_a, "fore_l": paw_f, "fore_r": paw_f,
            "thigh_l": (0.08, -1.0, knee), "thigh_r": (0.08, -1.0, knee),
            "shin_l": (0.05, -1.0, -knee * 1.2), "shin_r": (0.05, -1.0, -knee * 1.2),
            "spine": (0.0, 1.0, 0.12 * crouch), "bob": 0.22 * hop - 0.1 * crouch, "head": (0.0, 0.1, 1.0),
            "tilt": 0.1 * math.sin(TAU * 0.8 * t)}


def spin(t):
    """くるっと回る（回ったあと、ちょっと決めポーズ）"""
    cyc = t % 3.0
    turn = _ease(cyc / 1.0) * TAU if cyc < 1.0 else 0.0
    if cyc < 1.0:
        arm = (1.0, 0.15, -0.1)
        return {"face": turn, "arm_l": arm, "arm_r": arm, "fore_l": (1.0, 0.25, 0.0), "fore_r": (1.0, 0.25, 0.0),
                "bob": 0.04 * math.sin(math.pi * cyc), "head": (0.0, 0.15, 1.0)}
    return peace(t) | {"blink": 1.0 if 1.2 < cyc < 1.7 else 0.0}     # ウインクの代わりに目を閉じる


def bow(t):
    """おじぎ"""
    k = _ease(min(1.0, (t % 2.6) / 0.7)) * (1.0 - _ease(max(0.0, ((t % 2.6) - 1.5) / 0.7)))
    hands = (0.1, -1.0, 0.2 + 0.3 * k)
    return {"spine": (0.0, 1.0, 0.75 * k), "chest": (0.0, 1.0, 0.3 * k), "head": (0.0, -0.25 * k, 1.0),
            "arm_l": hands, "arm_r": hands, "fore_l": (-0.45, -0.9, 0.35), "fore_r": (-0.45, -0.9, 0.35)}


def wonder(t):
    """首をかしげる（手は後ろで組む）"""
    tilt = 0.32 * math.sin(TAU * 0.35 * t)
    return {"arm_l": (0.25, -0.9, -0.45), "arm_r": (0.25, -0.9, -0.45),
            "fore_l": (-0.7, -0.3, -0.55), "fore_r": (-0.7, -0.3, -0.55),
            "tilt": tilt, "head": (tilt * 0.4, 0.12, 1.0), "chest": (-tilt * 0.08, 1.0, -0.04),
            "bob": 0.006 * _s(t, 0.7)}


MOTIONS = {
    "手をふる": wave,
    "ピース": peace,
    "ほっぺに手": cheeks,
    "ばんざい": banzai,
    "ハート": heart,
    "うさぎジャンプ": bunny,
    "くるっと回る": spin,
    "おじぎ": bow,
    "首をかしげる": wonder,
}

"""人物のポーズ（VRM の見た目 MOD のモーションと同じ書き方：秒 t → 骨の向き）。"""
import math

TAU = 2 * math.pi
DOWN = (0.25, -1.0, 0.05)
FORE_DOWN = (0.15, -1.0, 0.15)


def _s(t, hz, phase=0.0):
    return math.sin(TAU * hz * t + phase)


def wave(t):
    k = _s(t, 1.6)
    return {"arm_r": (0.85, 0.75, 0.2), "fore_r": (0.45 * k + 0.1, 1.0, 0.15), "arm_l": DOWN, "fore_l": FORE_DOWN,
            "head": (0.0, 0.15, 1.0), "hand_r": "open", "palm_r": (0.0, 0.1, 1.0)}


def crossed(t):
    return {"arm_l": (0.3, -0.5, 0.8), "fore_l": (-0.95, 0.15, 0.25), "arm_r": (0.3, -0.5, 0.8),
            "fore_r": (-0.95, 0.25, 0.15), "chest": (0.0, 1.0, -0.05), "bob": 0.005 * _s(t, 0.25)}


def salute(t):
    # ひじを肩の高さで横へ張り、指先をこめかみ（まゆの横）へ（前は手が頭の上に行っていた）
    return {"arm_r": (0.85, 0.0, 0.5), "fore_r": (-0.75, 0.6, -0.15), "arm_l": DOWN, "fore_l": FORE_DOWN,
            "hand_r": "flat", "palm_r": (0.15, -0.4, 1.0), "hand_l": "flat", "palm_l": (-1.0, 0.0, 0.0)}


def think(t):
    return {"arm_r": (0.25, -0.55, 0.6), "fore_r": (-0.6, 0.75, 0.3), "arm_l": (0.3, -0.5, 0.8),
            "fore_l": (-0.95, 0.15, 0.25), "head": (0.1, -0.15, 1.0), "tilt": 0.12,
            "hand_r": "grip", "palm_r": (-0.6, 0.0, -0.8)}


def slump(t):
    # 片ひざをつく：左は立てひざ（太ももが前・すねが下）、右はひざを地面につける（太ももが下・すねが後ろ）。
    # 前は両脚をそろえてしゃがんだ形で、腰が宙に浮いて見えた
    return {"chest": (0.0, 0.8, 0.55), "spine": (0.0, 0.92, 0.35), "head": (0.0, -0.55, 1.0),
            "arm_l": (0.15, -0.6, 0.8), "fore_l": (-0.1, -0.85, 0.5), "arm_r": (0.3, -1.0, 0.25),
            "fore_r": (0.1, -1.0, 0.2), "thigh_l": (0.1, -0.15, 1.0), "shin_l": (0.0, -1.0, 0.05),
            "thigh_r": (0.05, -1.0, -0.2), "shin_r": (0.0, -0.15, -1.0), "bob": -0.42}


def point(t):
    return {"arm_r": (0.35, 0.15, 0.95), "fore_r": (0.2, 0.15, 1.0), "arm_l": DOWN, "fore_l": FORE_DOWN,
            "hand_r": "point", "palm_r": (-0.6, -1.0, 0.0)}


def smoke(t):
    return {"arm_r": (0.25, -0.5, 0.55), "fore_r": (-0.6, 0.75, 0.3), "arm_l": (0.3, -0.5, 0.8),
            "fore_l": (-0.95, 0.15, 0.25), "hand_r": "pinch", "palm_r": (-0.3, 0.0, -1.0)}


def bow(t):
    return {"spine": (0.0, 0.85, 0.5), "chest": (0.0, 0.9, 0.3), "head": (0.0, -0.4, 1.0), "arm_l": DOWN,
            "arm_r": DOWN}


def cheer(t):
    # 片手のばんざい（名前どおり右手だけ。前は両手を上げていた）。こぶしをにぎる
    return {"arm_r": (0.4, 1.0, 0.1), "fore_r": (0.0, 1.0, 0.1), "arm_l": DOWN, "fore_l": FORE_DOWN,
            "bob": 0.03 * abs(_s(t, 1.2)), "hand_r": "fist", "palm_r": (-1.0, 0.0, 0.3)}


POSES = {"wave": wave, "crossed": crossed, "salute": salute, "think": think, "slump": slump, "point": point,
         "smoke": smoke, "bow": bow, "cheer": cheer}
NAMES = {"wave": "見送る（手をふる）", "crossed": "腕を組む", "salute": "敬礼", "think": "考える", "slump": "ひざをつく",
         "point": "指さす", "smoke": "煙草", "bow": "おじぎ", "cheer": "ばんざい（片手）"}

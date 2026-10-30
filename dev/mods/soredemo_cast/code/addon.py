"""登場人物（ワールド側）：人物（NPC）の置き場所・向き・ポーズ・歩く先、話しかけたときの処理。

人物は保存しない（置いた MOD がワールドを読み込むたびに置き直す）。マルチプレイではホストが送る。

    cast = world.addon("cast")
    cast.place("sakura", 足元の位置, up=上向き, face=向いている方向, pose="wave", owner="story", label="サクラ")
    cast.walk_to("donachi", 目的地の足元, speed=1.4)
    cast.remove("sakura") / cast.clear(owner="story")
ポーズ：None（立つ）・"wave"（手をふる）・"crossed"（腕を組む）・"salute"（敬礼）・"think"（考える）・
"slump"（ひざをつく）・"point"（指さす）・"smoke"（煙草）・"bow"（おじぎ）
"""
import numpy as np

from main import log
from main.hooks import Addon

L = log.get("cast")

POSES = ("wave", "crossed", "salute", "think", "slump", "point", "smoke", "bow", "cheer")
LINES = {
    "towa": "トワ「……」",
    "mikaga": "ミカガ「なあに？　ふふ、聞きたいことがあるなら、自分で答えを探してごらん」",
    "nemuri": "ネムリ「……私に話すことは、もう無いはずだ」",
    "sakura": "サクラ「特ダネのにおいがする……！ ね、何か面白いことあった？」",
    "siyama": "ヤマシナ「おっと、婚約の話ならお断りだよ。……君か。元気そうでなにより」",
    "sirotsu": "シロツ「よう！ 元気にしてるか？」",
    "kensu": "ケンス「艦王さまに何か用？ 伝説の話ならサインつきで聞かせてやるよ」",
    "kingi": "キンギ「……火、持ってねえか。……冗談だよ」",
    "donachi": "ドナチ「…………」",
    "nekosa": "先生「あっ、すみません、いま計算の見直しを……！ え、わたしに用ですか？」",
    "dabai": "ダバイ「……運が悪いのには慣れてるんで。何か手伝います？」",
    "rutero": "ルテロ「あたしの設計の話？ いいわよ、三日三晩でも聞かせてあげる！」",
    "omiku": "オミク「…………」",
}


def _unit(v):
    v = np.asarray(v, float)
    n = np.linalg.norm(v)
    return v / n if n > 1e-9 else v


class NPC:
    """人物1人。VRM の体（skinmodel の VRMBody）がプレイヤーと同じように読めるように、プレイヤーと同じ名前の項目を持つ。"""

    def __init__(self, key, defn, feet, up, face, pose, owner, label, talk):
        self.key, self.defn, self.owner, self.label, self.talkable = key, defn, owner, label, talk
        self.radius = float(defn.get("radius", 0.3))
        self.height = float(defn.get("height", 1.7))
        self.up = _unit(up)
        self.pos = np.asarray(feet, float) + self.up * self.radius
        self.vel = np.zeros(3)
        self.face_dir = _unit(np.asarray(face, float) - self.up * float(np.dot(face, self.up)))
        self.look_fwd = self.face_dir.copy()
        self.pitch = 0.0
        self.on_ground, self.jetpack, self.seated, self.in_water = True, False, None, 0.0
        self.emote = pose
        self.emote_t0 = 0.0
        self.target = None              # 歩く先（足元）
        self.speed = 1.3

    def feet(self):
        return self.pos - self.up * self.radius

    def to_dict(self):
        return {"key": self.key, "def": self.defn["id"], "feet": self.feet().tolist(), "up": self.up.tolist(),
                "face": self.face_dir.tolist(), "pose": self.emote, "label": self.label, "talk": self.talkable,
                "vel": self.vel.tolist()}


class CastAddon(Addon):
    name = "cast"

    def __init__(self):
        self.npcs = {}
        self.world = None

    def on_load(self, world):
        self.world = world

    # --- 置く・動かす ---------------------------------------------------
    def place(self, key, feet, up, face, pose=None, owner=None, label=None, talk=True, def_id=None):
        defn = self.world.registry.get("players", def_id or key)
        if defn is None:
            L.warning("人物 '%s' の見た目の定義がありません", def_id or key)
            return None
        n = NPC(key, defn, feet, up, face, pose if pose in POSES else None, owner, label or defn.get("name", key), talk)
        self.npcs[key] = n
        return n

    def set_pose(self, key, pose):
        n = self.npcs.get(key)
        if n is not None:
            n.emote = pose if pose in POSES else None

    def face(self, key, point):
        n = self.npcs.get(key)
        if n is not None:
            d = np.asarray(point, float) - n.pos
            d = d - n.up * float(d @ n.up)
            if np.linalg.norm(d) > 1e-6:
                n.face_dir = _unit(d)
                n.look_fwd = n.face_dir.copy()

    def walk_to(self, key, feet, speed=1.3):
        n = self.npcs.get(key)
        if n is not None:
            n.target = np.asarray(feet, float) + n.up * n.radius
            n.speed = float(speed)

    def remove(self, key):
        self.npcs.pop(key, None)

    def clear(self, owner=None):
        for k in [k for k, n in self.npcs.items() if owner is None or n.owner == owner]:
            del self.npcs[k]

    def near(self, pos, radius):
        """pos から radius m 以内の人物 [(距離, 人物)]（近い順）。"""
        out = []
        for n in self.npcs.values():
            d = float(np.linalg.norm(n.pos - np.asarray(pos, float)))
            if d <= radius:
                out.append((d, n))
        return sorted(out, key=lambda x: x[0])

    # --- 毎ティック：歩く -------------------------------------------------
    def before_physics(self, world, dt):
        for n in self.npcs.values():
            if n.target is None:
                n.vel = np.zeros(3)
                continue
            d = n.target - n.pos
            d = d - n.up * float(d @ n.up)
            L_ = float(np.linalg.norm(d))
            if L_ < 0.05:
                n.target, n.vel = None, np.zeros(3)
                continue
            step = min(L_, n.speed * dt)
            n.vel = d / L_ * n.speed
            n.pos = n.pos + d / L_ * step
            n.face_dir = d / L_
            n.look_fwd = n.face_dir.copy()
        return set()

    # --- 話しかける -------------------------------------------------------
    def talk(self, key):
        n = self.npcs.get(key)
        if n is None or not n.talkable:
            return None
        for a in self.world.addons:
            if a is self:
                continue
            try:
                msg = a.on_npc_talk(self.world, key)
            except Exception:  # noqa: BLE001
                L.exception("会話の処理に失敗")
                msg = None
            if msg:
                return msg
        return LINES.get(key, f"{n.label}「……」")

    def raycast(self, world, o, dvec, max_t):
        """視線が人物（縦長のカプセル）に当たるか。"""
        best = None
        for n in self.npcs.values():
            a = n.feet() + n.up * 0.3
            b = n.feet() + n.up * (n.height - 0.25)
            t = _ray_capsule(o, dvec, a, b, 0.35, max_t)
            if t is not None and (best is None or t < best["t"]):
                best = {"kind": "npc", "t": t, "key": n.key, "point": o + dvec * t, "normal": -dvec,
                        "placeable": False}
        return best

    # --- マルチプレイ -------------------------------------------------------
    def net_state(self, world):
        return {"npcs": [n.to_dict() for n in self.npcs.values()]}

    def net_apply(self, world, state):
        if not isinstance(state, dict):
            return
        keep = {}
        for d in state.get("npcs", []):
            n = self.npcs.get(d["key"])
            if n is None or n.defn["id"] != d["def"]:
                defn = world.registry.get("players", d["def"])
                if defn is None:
                    continue
                n = NPC(d["key"], defn, d["feet"], d["up"], d["face"], d.get("pose"), None, d.get("label"),
                        d.get("talk", True))
            else:
                n.up = _unit(d["up"])
                n.pos = np.asarray(d["feet"], float) + n.up * n.radius
                n.face_dir = _unit(d["face"])
                n.look_fwd = n.face_dir.copy()
                n.emote = d.get("pose")
            n.vel = np.asarray(d.get("vel", [0, 0, 0]), float)
            keep[n.key] = n
        self.npcs = keep


def _ray_capsule(o, d, a, b, r, max_t):
    """光線 o + t d（d は単位）とカプセル（線分 a-b・半径 r）の最初の交わり。"""
    best = None
    for k in range(9):                              # 線分を点で近似（人物の大きさなら十分）
        c = a + (b - a) * (k / 8)
        oc = o - c
        bq = float(oc @ d)
        cq = float(oc @ oc) - r * r
        disc = bq * bq - cq
        if disc < 0:
            continue
        t = -bq - np.sqrt(disc)
        if 0 <= t <= max_t and (best is None or t < best):
            best = t
    return best

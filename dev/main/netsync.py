"""マルチプレイで、ホストと参加者が同じ形で使う「状態の送り方・受け方」。

構築物：
    形（ボクセル・結合）が変わったら丸ごと（to_dict）、剛体として動いている間は (R, x, v, w)、
    ブロックごとに動いている間は頂点の位置、止まったら最後の位置を1回だけ送る（止まっている間は何も送らない）
機器・資源・電力などの中身（メタ）：変わったときだけ
プログラムのファイル（scripts/）：変わったファイルだけ（どちらの PC で書き換えても相手に届く）
"""
import hashlib
import json

import numpy as np

from .construct import Construct, link_key

F32 = np.float32
PRIVATE = ("memory",)           # 機器の状態のうち送らないもの（"_" で始まるものも送らない）


# ---------------------------------------------------------------------------
# 構築物
# ---------------------------------------------------------------------------
class ConsTracker:
    """ホスト：前に送った構築物の状態を覚えておき、変わった分だけの送り物を作る。"""

    def __init__(self):
        self.sent = {}

    def diff(self, world):
        out = []
        for c in list(world.constructs.values()):
            if not c.n_vox:
                continue
            s = self.sent.get(c.id)
            if s is None or s["ver"] != c.version:
                e = {"id": c.id, "full": c.to_dict(), "spawn": c.spawn_dict()}
                s = self.sent[c.id] = {"ver": c.version, "sleep": None, "rb": None}
                if c.asleep:
                    e["sleep"] = True
                    s["sleep"] = c.sleep_epoch
                out.append(e)
                if c.asleep:
                    continue
            if c.asleep:
                if s["sleep"] != c.sleep_epoch:
                    out.append({"id": c.id, "sleep": c.pos.astype(F32)})
                    s["sleep"], s["rb"] = c.sleep_epoch, None
                continue
            s["sleep"] = None
            if c.mode in ("rigid", "rails") and c.rb is not None:
                rb = c.rb
                e = {"id": c.id, "R": np.asarray(rb["R"], float), "x": np.asarray(rb["x"], float),
                     "v": np.asarray(rb["v"], float), "w": np.asarray(rb["w"], float)}
                if s["rb"] != c.rb_epoch:
                    e["local"] = np.asarray(rb["local"], F32)
                    s["rb"] = c.rb_epoch
                out.append(e)
            else:
                s["rb"] = None
                out.append({"id": c.id, "pos": c.pos.astype(F32)})
        gone = [cid for cid in self.sent if cid not in world.constructs or not world.constructs[cid].n_vox]
        for cid in gone:
            del self.sent[cid]
        return out, gone


EXTRAPOLATE = 0.08      # 更新が来ない間、速度のまま進めてよい時間（これより長く来なければ止めて待つ）


def apply_construct(world, e, dt_since):
    """参加者：構築物の更新を1つ当てる。作り直したら新しい構築物を返す。"""
    cid = e["id"]
    c0 = world.constructs.get(cid)
    if c0 is not None:
        c0._net_age = 0.0
    if "full" in e:
        c = Construct.from_dict(world.registry, e["full"], e.get("spawn") or {})
        old = world.constructs.get(cid)
        if old is not None:
            c.anchor = old.anchor if c.anchor is None else c.anchor
        world.constructs[cid] = c
        c._net_age = 0.0
        if e.get("sleep"):
            _sleep(c)
        return c
    c = world.constructs.get(cid)
    if c is None:
        return None
    if "sleep" in e:
        p = np.asarray(e["sleep"], dtype=np.float64)
        if p.shape == c.pos.shape:
            c.pos = p
        c.mode, c.rb = "soft", None
        _sleep(c)
    elif "R" in e:
        if "local" in e or c.rb is None or c.mode != "rails":
            loc = e.get("local")
            if loc is None:
                loc = (c.pos - np.asarray(e["x"])) @ np.asarray(e["R"])   # 届く前に作り直したとき（次の版で揃う）
            c.rb = {"local": np.asarray(loc, dtype=np.float64), "R": np.eye(3), "x": np.zeros(3),
                    "v": np.zeros(3), "w": np.zeros(3), "Ib": np.eye(3), "ext": 1.0}
            c.rb_epoch += 1
        c.mode = "rails"
        for k in ("R", "x", "v", "w"):
            c.rb[k] = np.asarray(e[k], dtype=np.float64)
        c._rails_sync()
        c.asleep = False
    elif "pos" in e:
        p = np.asarray(e["pos"], dtype=np.float64)
        if p.shape != c.pos.shape:
            return None
        if c.mode == "rails":
            c.mode, c.rb = "soft", None
            c.vel = np.zeros_like(p)
        elif dt_since > 1e-3 and not c.asleep:
            c.vel = (p - c.pos) / dt_since
        else:
            c.vel = np.zeros_like(p)
        c.pos = p
        c.asleep = False
    return None


def _sleep(c):
    c.vel = np.zeros_like(c.pos)
    c.asleep = True
    c.sleep_epoch += 1


def advance(world, dt):
    """参加者：次の更新が届くまでの間、構築物を速度のまま進める（動きをなめらかにする）。"""
    for c in world.constructs.values():
        if c.asleep or not c.n_vox:
            continue
        age = getattr(c, "_net_age", 0.0)
        c._net_age = age + dt
        if age >= EXTRAPOLATE:
            continue
        if c.mode == "rails" and c.rb is not None:
            rb = c.rb
            rb["x"] = rb["x"] + rb["v"] * dt
            wn = float(np.linalg.norm(rb["w"]))
            if wn > 1e-9:
                rb["R"] = _rot(rb["w"] / wn, wn * dt) @ rb["R"]
            c._rails_sync()
        else:
            c.pos = c.pos + c.vel * dt


def _rot(axis, ang):
    x, y, z = axis
    K = np.array([[0, -z, y], [z, 0, -x], [-y, x, 0]])
    return np.eye(3) + np.sin(ang) * K + (1 - np.cos(ang)) * (K @ K)


# ---------------------------------------------------------------------------
# 機器・資源など（メタ）
# ---------------------------------------------------------------------------
def dev_public(d):
    """機器の、相手に見せる部分（名前と状態。"_" で始まるものと memory は除く）。"""
    return d["name"], {k: v for k, v in d["state"].items() if not k.startswith("_") and k not in PRIVATE}


def meta_of(c):
    devs = [[i, *dev_public(d)] for i, d in enumerate(c.devs) if d]
    return {"name": c.name, "devs": devs, "status": c.status, "storage": c.storage, "powered": bool(c.powered),
            "anchor": c.anchor, "cav": [[list(k), v] for k, v in c.cavity_air.items()]}


def dumps(x):
    return json.dumps(x, sort_keys=True, ensure_ascii=False, default=_default)


def _default(x):
    if isinstance(x, np.ndarray):
        return x.tolist()
    if isinstance(x, (np.integer,)):
        return int(x)
    if isinstance(x, (np.floating,)):
        return float(x)
    if isinstance(x, (set, tuple)):
        return list(x)
    return str(x)


def jsonable(x):
    return json.loads(dumps(x))


class MetaTracker:
    def __init__(self):
        self.sent = {}

    def diff(self, world):
        out = []
        for c in list(world.constructs.values()):
            m = jsonable(meta_of(c))
            key = dumps(m)
            if self.sent.get(c.id) != key:
                self.sent[c.id] = key
                out.append({"id": c.id, **m})
        for cid in [k for k in self.sent if k not in world.constructs]:
            del self.sent[cid]
        return out


def apply_meta(c, m, devices=True):
    c.name = m.get("name", c.name)
    c.status = m.get("status") or {}
    c.storage = m.get("storage") or {}
    c.powered = bool(m.get("powered"))
    c.anchor = m.get("anchor")
    c.cavity_air = {tuple(k): v for k, v in m.get("cav", [])}
    if not devices:
        return
    for i, name, st in m.get("devs", []):
        if i >= len(c.devs) or not c.devs[i]:
            continue
        d = c.devs[i]
        d["name"] = name
        keep = {k: v for k, v in d["state"].items() if k.startswith("_") or k in PRIVATE}
        d["state"] = {**st, **keep}


def dev_patch(c, base):
    """参加者：機器の画面で変えた分（base は開いたときの dev_public の一覧）。[(セル, 名前, 変えた値, 消したキー)]"""
    out = []
    for i, d in enumerate(c.devs):
        if not d or i not in base:
            continue
        name, st = jsonable(dev_public(d))
        bname, bst = base[i]
        chg = {k: v for k, v in st.items() if bst.get(k, object()) != v}
        rm = [k for k in bst if k not in st]
        if name != bname or chg or rm:
            out.append({"cell": list(c.cells[i]), "name": name, "set": chg, "del": rm})
    return out


def dev_base(c):
    return {i: tuple(jsonable(dev_public(d))) for i, d in enumerate(c.devs) if d}


def apply_dev_patch(world, cid, patches):
    """ホスト：参加者が機器の画面で変えた分を当てる。"""
    c = world.constructs.get(cid)
    if c is None:
        return
    for p in patches:
        i = c.index.get(tuple(p["cell"]))
        if i is None or not c.devs[i]:
            continue
        d = c.devs[i]
        if p.get("name"):
            d["name"] = p["name"]
        st = d["state"]
        st.update(p.get("set") or {})
        for k in p.get("del") or []:
            st.pop(k, None)
        if any(k in (p.get("set") or {}) for k in ("running", "script", "blocks")):
            for k in ("_env",):
                st.pop(k, None)             # プログラムを変えた・実行し直した：最初から


# ---------------------------------------------------------------------------
# プログラムのファイル
# ---------------------------------------------------------------------------
TEXT_SUFFIXES = (".py", ".json", ".txt", ".md")


class FileSync:
    """フォルダの中のファイル（プログラム）の変化を見張る。相手から受け取ったものは送り返さない。"""

    def __init__(self, folder):
        self.dir = folder
        self.known = {}             # 名前 -> ハッシュ（最後に送った・受け取った中身）
        self.stat = {}

    def all(self):
        out = {}
        if self.dir.is_dir():
            for f in sorted(self.dir.iterdir()):
                if f.is_file() and f.name.endswith(TEXT_SUFFIXES) and f.stat().st_size < 4_000_000:
                    out[f.name] = f.read_bytes()
        return out

    def mark_all(self):
        for name, data in self.all().items():
            self.known[name] = _hash(data)

    def changes(self):
        """前に見たときから変わったファイル {名前: 中身}。"""
        out = {}
        if not self.dir.is_dir():
            return out
        for f in self.dir.iterdir():
            if not (f.is_file() and f.name.endswith(TEXT_SUFFIXES)):
                continue
            try:
                st = f.stat()
            except OSError:
                continue
            key = (st.st_mtime_ns, st.st_size)
            if self.stat.get(f.name) == key:
                continue
            self.stat[f.name] = key
            try:
                data = f.read_bytes()
            except OSError:
                continue
            h = _hash(data)
            if self.known.get(f.name) != h:
                self.known[f.name] = h
                out[f.name] = data
        return out

    def put(self, name, data):
        """相手から受け取ったファイルを書く（名前にフォルダは含めない）。"""
        if "/" in name or "\\" in name or ".." in name or not name.endswith(TEXT_SUFFIXES):
            return False
        self.dir.mkdir(parents=True, exist_ok=True)
        p = self.dir / name
        p.write_bytes(data)
        self.known[name] = _hash(data)
        st = p.stat()
        self.stat[name] = (st.st_mtime_ns, st.st_size)
        return True


def _hash(data):
    return hashlib.sha1(data).hexdigest()


# ---------------------------------------------------------------------------
# OBJ 読み込みの結果（参加者 → ホスト）
# ---------------------------------------------------------------------------
def pack_import(data):
    out = dict(data)
    out["voxels"] = [[list(cell), mat, dev] for cell, mat, dev in data["voxels"]]
    if data.get("links") is not None:
        out["links"] = [[list(a), list(b)] for a, b in data["links"]]
    return jsonable(out)


def unpack_import(d):
    out = dict(d)
    out["voxels"] = [(tuple(cell), mat, dev) for cell, mat, dev in d["voxels"]]
    if d.get("links") is not None:
        out["links"] = {link_key(tuple(a), tuple(b)) for a, b in d["links"]}
    return out


# ---------------------------------------------------------------------------
# プレイヤー
# ---------------------------------------------------------------------------
def player_public(p, pid, name):
    """ほかの人に見せるプレイヤーの状態（描画用）。"""
    return {"pid": pid, "name": name, "def": p.defn["id"], "pos": p.pos.tolist(), "up": p.up.tolist(),
            "vel": p.vel.tolist(), "gv": np.asarray(p.ground_vel, float).tolist(), "og": bool(p.on_ground),
            "jp": bool(p.jetpack), "fl": bool(getattr(p, "flying", False)), "wt": float(p.in_water),
            "seat": [p.seated[0], list(p.seated[1])] if p.seated else None,
            "fwd": np.asarray(getattr(p, "look_fwd", np.array([0, 0, 1.0])), float).tolist(),
            "pitch": float(getattr(p, "pitch", 0.0)), "emote": getattr(p, "emote", None),
            "face": None if getattr(p, "face_dir", None) is None else np.asarray(p.face_dir, float).tolist()}


def apply_player_public(world, p, d):
    if p.defn.get("id") != d["def"]:
        p.defn = world.player_def(d["def"])
    p.pos = np.asarray(d["pos"], float)
    p.up = np.asarray(d["up"], float)
    p.vel = np.asarray(d["vel"], float)
    p.ground_vel = np.asarray(d["gv"], float)
    p.on_ground = d["og"]
    p.jetpack = d["jp"]
    p.flying = d["fl"]
    p.in_water = d["wt"]
    s = d.get("seat")
    p.seated = (s[0], tuple(s[1])) if s else None
    p.look_fwd = np.asarray(d["fwd"], float)
    p.pitch = d["pitch"]
    p.name = d["name"]
    p.pid = d["pid"]
    em = d.get("emote")
    if em != getattr(p, "emote", None):
        import time as _t
        p.emote, p.emote_t0 = em, _t.monotonic()
    p.face_dir = None if d.get("face") is None else np.asarray(d["face"], float)

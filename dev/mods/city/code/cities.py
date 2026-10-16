"""サイバーパンクの街：惑星の地表と軌道上に、一定間隔でランダムに生成する。

- 街は箱（ビル・連絡通路・看板など）の集まり。ボクセルの構築物ではないので軽い。
- 惑星の seed から毎回同じ場所・同じ形に作るので、セーブには「壊れた建物」と「軌道上の街の位置・速度」だけを保存する。
- 当たり判定：構築物・プレイヤーは建物にぶつかり、上に乗れる。
- 損傷：一定以上の速さでぶつかった建物は壊れた見た目（低く・暗く・ネオンが消える）になり、さらに強い衝撃で消える。
- 軌道上の街（ステーション）は街全体を1つの剛体として軌道を計算する。構築物で押したり、
  ステーションに直接置いた構築物の推進機を噴かしたりすると軌道が変わり、落とすこともできる。
  地面に落ちると、ぶつかった所の建物が壊れ、止まった所で「墜落した街」になる。
"""
import hashlib

import numpy as np

from main import log
from main.geom import box_sdf, ray_boxes

L = log.get("cities")

NEON = np.array([[1.0, 0.18, 0.75], [0.1, 0.95, 1.0], [1.0, 0.82, 0.15], [0.62, 0.3, 1.0],
                 [0.3, 1.0, 0.5], [1.0, 0.35, 0.2]])
SYL = ["カブキ", "シンジュク", "アキバ", "ミナト", "クロガネ", "ヨミ", "ネオン", "ハガネ", "シグマ", "ゼロ",
       "クーロン", "アマツ", "シノノメ", "ラスト", "オボロ", "ヒカリ", "ムラサキ", "テツ", "コード", "ノヴァ"]
PRE = ["ネオ", "メガ", "ハイパー", "ニュー", "サイバー"]

DAMAGE_SPEED = 7.0       # この速さ (m/s) 以上でぶつかると建物が壊れた見た目に
DESTROY_SPEED = 18.0     # この速さ以上（または壊れた建物にもう一度強く当たる）で消える
STATION_DENSITY = 40.0   # ステーションの質量 = 箱の体積 × これ (kg/m³)


def _ground(pl, dirs):
    """(地表の半径, 水面の半径) をノイズから直接計算（キャッシュのタイルを大量に作らないように）。"""
    v = pl._tile_values(np.asarray(dirs, float))
    return v[:, 0], v[:, 1]


def _rng(*keys):
    h = hashlib.sha256("|".join(str(k) for k in keys).encode()).digest()
    return np.random.default_rng(int.from_bytes(h[:8], "little"))


def _frame(up, ref=None):
    """up を y 軸にした直交フレーム（行が x, y, z 軸）。"""
    up = up / np.linalg.norm(up)
    ref = np.array([0.0, 0.0, 1.0]) if ref is None else ref
    if abs(float(up @ ref)) > 0.95:
        ref = np.array([1.0, 0.0, 0.0])
    z = ref - up * (ref @ up)
    z /= np.linalg.norm(z)
    x = np.cross(up, z)
    return np.stack([x, up, z])


def _rot(axis, ang):
    axis = axis / np.linalg.norm(axis)
    K = np.array([[0, -axis[2], axis[1]], [axis[2], 0, -axis[0]], [-axis[1], axis[0], 0]])
    return np.eye(3) + np.sin(ang) * K + (1 - np.cos(ang)) * (K @ K)


# ---------------------------------------------------------------------------
class City:
    """街1つ。建物は街のローカル座標の箱。世界座標 = x + R @ local。"""

    def __init__(self, cid, name, kind, host):
        self.id = cid
        self.name = name
        self.kind = kind          # surface / orbit / wreck（墜落して止まったステーション）
        self.host = host          # 生成元の天体（Planet）
        self.x = np.zeros(3)
        self.R = np.eye(3)
        self.v = np.zeros(3)
        self.w = np.zeros(3)
        self.parts = []           # dict(c, A, e, color, neon, collide, bld, decor)
        self.npcs = []            # 住民・商人など（citylife.add_npc）
        self.homes = []           # 住宅の部屋（citylife.build_housing）
        self.region = "plain"     # 地域（値段が変わる）：plain / coast / volcano / orbit / frontier
        self.state = None         # 建物ごとの状態 0=無事 1=壊れた 2=消えた
        self.version = 0
        self.force = np.zeros(3)
        self.torque = np.zeros(3)
        self.impulse = np.zeros(3)
        self.ang_impulse = np.zeros(3)

    # --- 作る ---------------------------------------------------------------
    def add(self, c, A, e, color, neon=None, collide=True, bld=-1, kind="body"):
        self.parts.append({"c": np.asarray(c, float), "A": np.asarray(A, float), "e": np.asarray(e, float),
                           "color": np.asarray(color, float), "neon": None if neon is None else np.asarray(neon, float),
                           "collide": collide, "bld": bld, "kind": kind})
        return len(self.parts) - 1

    def finish(self):
        n_b = max([p["bld"] for p in self.parts] + [-1]) + 1
        if self.state is None:
            self.state = np.zeros(n_b, np.int64)
        self.C0 = np.array([p["c"] for p in self.parts])
        self.A0 = np.array([p["A"] for p in self.parts])
        self.E0 = np.array([p["e"] for p in self.parts])
        self.bld = np.array([p["bld"] for p in self.parts])
        self.col = np.array([p["collide"] for p in self.parts])
        self.radius = float(np.max(np.linalg.norm(self.C0, axis=1) + np.linalg.norm(self.E0, axis=1))) + 1.0
        vol = (8 * self.E0.prod(1))[self.col]
        self.mass = max(float(vol.sum()) * STATION_DENSITY, 1e4)
        r2 = (self.C0[self.col] ** 2).sum(1)
        self.I = np.eye(3) * max(float((vol * STATION_DENSITY * r2).sum()), 1e4)
        self._geom_cache = None

    def n_buildings(self):
        return len(self.state)

    def alive(self):
        a = getattr(self, "_alive", None)
        if a is None or a[0] != self.version:
            a = self._alive = (self.version, int((self.state < 2).sum()))
        return a[1]

    def _geom(self):
        """壊れ具合を反映した、街のローカル座標の箱（全部品）と、有効な部品のマスク。"""
        key = self.version
        if self._geom_cache is not None and self._geom_cache[0] == key:
            return self._geom_cache[1]
        C, E = self.C0.copy(), self.E0.copy()
        live = np.ones(len(C), bool)
        st = np.where(self.bld >= 0, self.state[np.maximum(self.bld, 0)], 0)
        live &= st < 2
        dmg = st == 1
        if dmg.any():
            # 壊れた建物：上の方が崩れて低くなる（足元は同じ）、飾りは落ちる
            up = self.A0[dmg][:, 1]
            h = E[dmg][:, 1]
            base = C[dmg] - up * h[:, None]
            nh = h * 0.55
            C[dmg] = base + up * nh[:, None]
            E[dmg, 1] = nh
            decor = np.array([p["kind"] not in ("body", "wall", "furn") for p in self.parts])
            live &= ~(dmg & decor)
        g = (C, E, live, st)
        self._geom_cache = (key, g)
        return g

    def world_boxes(self, collide_only=True):
        key = (self.version, collide_only)
        wc = getattr(self, "_wb_cache", None)
        if not self.is_moving() and wc is not None and wc[0] == key and np.array_equal(wc[1], self.x):
            return wc[2]
        res = self._world_boxes(collide_only)
        if not self.is_moving():
            self._wb_cache = (key, self.x.copy(), res)
        return res

    def _world_boxes(self, collide_only=True):
        C, E, live, _ = self._geom()
        m = live & (self.col if collide_only else True)
        Cw = self.x + C[m] @ self.R.T
        Aw = np.einsum("bij,kj->bik", self.A0[m], self.R)
        return Cw, Aw, E[m], np.nonzero(m)[0]

    def point_velocity(self, p):
        return self.v + np.cross(self.w, np.asarray(p) - self.x)

    def is_moving(self):
        return self.kind == "orbit"

    # --- 損傷 ---------------------------------------------------------------
    def hit(self, part, speed):
        b = int(self.bld[part])
        if b < 0 or speed < DAMAGE_SPEED:
            return False
        s = int(self.state[b])
        new = 2 if (speed >= DESTROY_SPEED or s == 1) else 1
        if b == getattr(self, "core", None):
            new = 1        # ステーションの台座は消えない（上の建物が宙に浮かないように）
        if new > s:
            self.state[b] = new
            self.version += 1
            return True
        return False

    # --- 保存 ---------------------------------------------------------------
    def to_state(self):
        d = {"b": {str(k): int(v) for k, v in enumerate(self.state) if v}}
        if self.kind != "surface":
            d.update(kind=self.kind, x=self.x.tolist(), v=self.v.tolist(), R=self.R.tolist(), w=self.w.tolist())
        return d

    def load_state(self, d):
        for k, v in (d.get("b") or {}).items():
            if int(k) < len(self.state):
                self.state[int(k)] = int(v)
        if "x" in d:
            self.kind = d.get("kind", self.kind)
            self.x = np.asarray(d["x"], float)
            self.v = np.asarray(d["v"], float)
            self.R = np.asarray(d["R"], float)
            self.w = np.asarray(d["w"], float)
        self.version += 1


# ---------------------------------------------------------------------------
# 街の形
# ---------------------------------------------------------------------------
def _tower(city, rng, base, frame, w, d, h, b):
    """1棟のビル：本体＋ネオンの帯＋屋上の看板・アンテナ。base は足元（街ローカル）、frame は行が軸。"""
    up = frame[1]
    body = np.array([0.08, 0.09, 0.13]) + rng.uniform(0, 0.08) * np.array([1, 1, 1.4])
    neon = NEON[rng.integers(len(NEON))]
    city.add(base + up * (h / 2), frame, (w / 2, h / 2, d / 2), body, None, True, b, "body")
    # ネオンの帯（何階かおき）
    step = rng.choice([3.0, 4.0, 5.0])
    y = step
    while y < h - 1.0:
        if rng.random() < 0.7:
            city.add(base + up * y, frame, (w / 2 + 0.06, 0.12, d / 2 + 0.06), neon * 0.9, neon, False, b, "neon")
        y += step
    # 縦の光の筋
    if rng.random() < 0.6:
        side = frame[0] * (w / 2 + 0.05)
        city.add(base + up * (h / 2) + side, frame, (0.1, h * 0.45, 0.25), neon, neon, False, b, "neon")
    # 屋上の看板
    if rng.random() < 0.55:
        sw = min(w, d) * rng.uniform(0.5, 0.9)
        sh = rng.uniform(1.5, 4.0)
        c2 = NEON[rng.integers(len(NEON))]
        city.add(base + up * (h + sh / 2 + 0.3) + frame[2] * (d / 2 - 0.2), frame, (sw / 2, sh / 2, 0.15),
                 c2, c2, False, b, "sign")
    # アンテナ
    if h > 25 and rng.random() < 0.6:
        ah = rng.uniform(4, 12)
        city.add(base + up * (h + ah / 2), frame, (0.15, ah / 2, 0.15), (0.5, 0.5, 0.55), None, False, b, "decor")
        city.add(base + up * (h + ah), frame, (0.3, 0.3, 0.3), (1, 0.15, 0.15), (1, 0.15, 0.15), False, b, "decor")


PAVE = np.array([0.34, 0.35, 0.4])         # 舗装
PIER = np.array([0.3, 0.33, 0.42])         # 水の上の桟橋
STEP = np.array([0.46, 0.47, 0.52])        # 階段
STRIP = 2.4                                # 区画の縁の、階段のための帯の幅 (m)
RISE = 0.22                                # 階段1段の高さの上限 (m)
MAX_STAIR = 8.5                            # 隣の区画との高さの差がこれ以上なら崖（階段なし）
MAX_OUT_STAIR = 16.0                       # 街の外の地面へ下りる階段の高さの上限 (m)
STEP_UP = 0.55                             # プレイヤーが歩くだけで上れる段差（これ以下なら階段は作らない）
PIER_DEPTH = 20.0                          # これより深い水の上には区画を作らない


def _stairs_along(city, base_c, fr, e, nrm, lo_r, hi_r, run, bottom_r, pc):
    """区画の縁の帯に、縁に沿って上る階段を置く。base_c：帯の中心線の中点（惑星中心からの方向つき位置）、
    e：上っていく向き、nrm：帯の幅の向き。lo_r/hi_r：下と上の高さ（惑星中心からの半径）。"""
    dh = hi_r - lo_r
    n = max(1, int(np.ceil(dh / RISE)))
    depth = run / n
    up = fr[1]
    rise = dh / n
    start = -run / 2
    sfr = np.stack([e, up, nrm])
    if hasattr(city, "stairs"):
        city.stairs.append({"kind": "along", "start": base_c + e * start, "e": e, "nrm": nrm, "lo": lo_r, "hi": hi_r,
                            "run": run})
    for k in range(n):
        top = lo_r + (k + 1) * rise
        s = start + (k + 0.5) * depth
        center_dir = base_c + e * s
        cd = (center_dir - pc) / np.linalg.norm(center_dir - pc)
        mid = (top + bottom_r) / 2
        # 帯の幅いっぱい＋上の区画の台に少し食い込ませる（隣の台との間にすき間ができて落ちないように）
        city.add(pc + cd * mid - city.x + nrm * 0.15, sfr, (depth / 2 + 0.01, (top - bottom_r) / 2, STRIP / 2 + 0.15),
                 STEP, None, True, -1, "stair")
    # 上りきった所の踊り場（行き過ぎて下に落ちないように）
    land = 1.6
    center_dir = base_c + e * (run / 2 + land / 2)
    cd = (center_dir - pc) / np.linalg.norm(center_dir - pc)
    city.add(pc + cd * ((hi_r + bottom_r) / 2) - city.x + nrm * 0.15, sfr,
             (land / 2, (hi_r - bottom_r) / 2, STRIP / 2 + 0.15), STEP, None, True, -1, "stair")


def build_surface_city(city, planet, center_dir, rng, radius, scale=1.0):
    """地表の街。区画（block × block）ごとに地面を舗装した台（段々畑のように高さをそろえる）を作り、
    その上にビルを立てる。隣の区画と高さが違う所には階段、街の外側の地面へも階段で下りられる。
    水の上の区画は桟橋（水面より少し高い台）にする。"""
    pc = planet.center
    F = _frame(center_dir)
    ang = rng.uniform(0, np.pi)
    F = np.stack([np.cos(ang) * F[0] + np.sin(ang) * F[2], F[1], -np.sin(ang) * F[0] + np.cos(ang) * F[2]])
    r0 = float(_ground(planet, center_dir[None])[0][0])
    city.x = pc + center_dir * r0
    city.R = np.eye(3)
    city.links = []
    city.stairs = []
    block = 15.0 * scale
    half = block / 2
    n = int((radius + half) // block) + 1
    # 区画の中心と、地面の高さを調べる点（区画の中に 3×3）
    cells = {}
    for i in range(-n, n + 1):
        for j in range(-n, n + 1):
            dist = np.hypot(i * block, j * block)
            if dist > radius + half * 0.5:
                continue
            cells[(i, j)] = dist
    keys = list(cells)
    grid = (-0.5, -0.25, 0.0, 0.25, 0.5)
    offs = np.array([(a, b) for a in grid for b in grid]) * block
    NS = len(offs)
    P = []
    for (i, j) in keys:
        c = city.x + i * block * F[0] + j * block * F[2]
        for a, b in offs:
            P.append(c + a * F[0] + b * F[2])
    P = np.array(P)
    dirs = (P - pc) / np.linalg.norm(P - pc, axis=1)[:, None]
    gr, wr = _ground(planet, dirs)
    gr, wr = gr.reshape(len(keys), NS), wr.reshape(len(keys), NS)
    wet = (wr > gr) if planet.has_water else np.zeros_like(gr, bool)
    cell = {}
    for k, (i, j) in enumerate(keys):
        g, w_ = gr[k], wr[k]
        if wet[k].all() and float((w_ - g).max()) > PIER_DEPTH:
            continue
        top = float(g.max()) + 0.08 * float(g.max() - g.min())   # でこぼこな所は少し余裕を持って地面を覆う
        pier = False
        if wet[k].any():
            top = max(top, float(w_[wet[k]].max()) + 1.2)
            pier = wet[k].mean() > 0.5
        top = np.ceil((top + 0.15) / 0.25) * 0.25          # 高さを 25 cm 刻みにそろえる（平らな所は同じ高さに）
        c = city.x + i * block * F[0] + j * block * F[2]
        d = (c - pc) / np.linalg.norm(c - pc)
        cell[(i, j)] = {"key": (i, j), "c": c, "d": d, "fr": _frame(d, F[2]), "top": top, "bot": float(g.min()) - 0.8,
                        "pier": pier, "dist": cells[(i, j)], "ground": g}
    # 舗装した台
    for (i, j), ce in cell.items():
        h = ce["top"] - ce["bot"]
        mid = (ce["top"] + ce["bot"]) / 2
        city.add(pc + ce["d"] * mid - city.x, ce["fr"], (half + 0.15, h / 2, half + 0.15),
                 PIER if ce["pier"] else PAVE, None, True, -1, "ground")
    def edge_light(ce, out):
        """段差の上の縁に、光る線（落ちないための目印）。"""
        fr = ce["fr"]
        side = np.cross(fr[1], out)
        ctr = ce["c"] + out * (half - 0.1)
        cd = (ctr - pc) / np.linalg.norm(ctr - pc)
        nc = NEON[(abs(hash((round(float(ce["top"]), 2), int(ce["key"][0]), int(ce["key"][1])))) % len(NEON))]
        city.add(pc + cd * (ce["top"] + 0.03) - city.x, np.stack([side, fr[1], out]), (half, 0.04, 0.08),
                 nc, nc, False, -1, "neon")

    # 隣の区画との段差に階段（縁に沿って上る。曲がり角は避けて帯の真ん中の部分だけ使う）
    run = block - 2 * STRIP - 0.4
    for (i, j), ce in cell.items():
        for di, dj in ((1, 0), (0, 1)):
            nb = cell.get((i + di, j + dj))
            if nb is None:
                continue
            lo, hi = (ce, nb) if ce["top"] < nb["top"] else (nb, ce)
            dh = hi["top"] - lo["top"]
            if dh > 1.0:
                edge_light(hi, (lo["c"] - hi["c"]) / np.linalg.norm(lo["c"] - hi["c"]))
            if dh <= STEP_UP or dh > MAX_STAIR or (run / max(1, np.ceil(dh / RISE))) < 0.28:
                continue
            nrm = (hi["c"] - lo["c"]) / np.linalg.norm(hi["c"] - lo["c"])      # 低い区画 → 高い区画
            e = np.cross(lo["fr"][1], nrm)
            e /= np.linalg.norm(e)
            base_c = lo["c"] + nrm * (half - STRIP / 2)
            if rng.random() < 0.5:
                e = -e
            _stairs_along(city, base_c, lo["fr"], e, nrm, lo["top"], hi["top"], run, lo["top"] - 0.4, pc)
            city.links.append(((i, j), (i + di, j + dj), "stairs"))
    # 街の外の地面へ下りる階段（外向きに下る。外側の縁の真ん中）
    outs = []
    for (i, j), ce in cell.items():
        for di, dj in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            if (i + di, j + dj) not in cell:
                out = di * F[0] + dj * F[2]
                outs.append((ce, out, ce["c"] + out * half))
    depth = 0.36
    kmax = int(np.ceil(MAX_OUT_STAIR / RISE))
    if outs:
        # 階段が通る所の地面（と水面）の高さを、まとめて調べる
        ks = (np.arange(kmax) + 0.5) * depth
        P2 = np.array([e + o * k for _, o, e in outs for k in ks])
        G2, W2 = _ground(planet, (P2 - pc) / np.linalg.norm(P2 - pc, axis=1)[:, None])
        G2, W2 = G2.reshape(len(outs), kmax), W2.reshape(len(outs), kmax)
        if planet.has_water:
            wet2 = W2 > G2
            G2 = np.where(wet2, W2 - 0.6, G2)              # 水の上は水の中まで下りる（泳いで桟橋に上がれる）
    for n_, (ce, out, edge) in enumerate(outs):
        g_line = G2[n_]
        dh0 = ce["top"] - float(g_line[0])
        if dh0 > 1.0:
            edge_light(ce, out)
        if dh0 <= STEP_UP:
            city.links.append((ce["key"], None, "out"))     # そのまま歩いて上れる
            continue
        # 1段ずつ下りていき、地面に届いた所で止める（地面が階段より急に下がっていく崖なら作らない）
        steps = []
        top = ce["top"]
        reached = False
        for k in range(kmax):
            top -= RISE
            if top <= float(g_line[k]) + 0.05:
                reached = True
                break
            steps.append((k, top))
        if not reached or not steps:
            continue
        sfr = np.stack([np.cross(ce["fr"][1], out), ce["fr"][1], out])
        city.stairs.append({"kind": "out", "start": edge, "e": out, "lo": float(g_line[len(steps)]), "hi": ce["top"],
                            "run": len(steps) * depth})
        city.links.append((ce["key"], None, "out"))
        for k, top in steps:
            pos = edge + out * (k + 0.5) * depth
            pd = (pos - pc) / np.linalg.norm(pos - pc)
            bot = min(float(g_line[k]), ce["bot"]) - 1.0
            city.add(pc + pd * (top + bot) / 2 - city.x, sfr, (1.5, (top - bot) / 2, depth / 2 + 0.01),
                     STEP, None, True, -1, "stair")
    city.cells = {k: (v["top"], v["pier"]) for k, v in cell.items()}
    # ビル（区画の真ん中。縁の帯は階段・歩道のために空ける）
    from . import citylife
    b = 0
    tops = []
    maxw = 2 * (half - STRIP - 0.2)
    # 内装のある建物：中心に近い区画から 店・不動産屋・住宅（大きい街は住宅2棟）。前哨基地は店だけ
    specials = ["trader"] if scale < 0.9 else ["trader", "estate", "housing"] + (["housing"] if radius > 55 else [])
    for (i, j), ce in sorted(cell.items(), key=lambda kv: kv[1]["dist"]):
        dist = ce["dist"]
        if specials and dist <= radius * 0.8 and dist > 0:
            role = specials.pop(0)
            fr = ce["fr"]
            # 扉は街の中心の向き（区画の向きの4方向のうち近いもの）
            to_c = -(ce["c"] - city.x)
            to_c = to_c - fr[1] * (to_c @ fr[1])
            k_ = int(np.argmax([to_c @ fr[2], to_c @ fr[0], -(to_c @ fr[2]), -(to_c @ fr[0])]))
            fz = [fr[2], fr[0], -fr[2], -fr[0]][k_]
            fx = np.cross(fr[1], fz)
            bfr = np.stack([fx, fr[1], fz])
            base = pc + ce["d"] * (ce["top"] - 0.03) - city.x
            w_ = d_ = maxw
            if role == "housing":
                top = citylife.build_housing(city, rng, base, bfr, w_, d_, b)
            else:
                top = citylife.build_shop(city, rng, base, bfr, w_, d_, b, role)
            # 上にビルを重ねる（地面の階が店・部屋のビル）
            t = 1.0 - dist / radius
            h = (4 + 35 * t ** 1.6 * rng.uniform(0.3, 1.0)) * scale
            if h > 6:
                _tower(city, rng, top, bfr, w_ * 0.9, d_ * 0.9, h, b)
            b += 1
            continue
        if dist > radius * 0.95 or rng.random() < 0.15:
            # 空き地：住民が歩いている広場（ときどき）
            if rng.random() < 0.45 and scale >= 0.9:
                fr = ce["fr"]
                base = pc + ce["d"] * ce["top"] - city.x
                citylife.add_npc(city, rng, "citizen", base, fr[2], fr[1], -1, wander=float(rng.uniform(2.0, 4.0)))
            # 空き地：街灯
            if rng.random() < 0.6:
                fr = ce["fr"]
                lp = ce["c"] + fr[0] * rng.uniform(-half + 1, half - 1) + fr[2] * rng.uniform(-half + 1, half - 1)
                ld = (lp - pc) / np.linalg.norm(lp - pc)
                base = pc + ld * ce["top"] - city.x
                nc = NEON[rng.integers(len(NEON))]
                city.add(base + fr[1] * 2.5, fr, (0.08, 2.5, 0.08), (0.35, 0.35, 0.4), None, False, -1, "decor")
                city.add(base + fr[1] * 5.1, fr, (0.25, 0.12, 0.25), nc, nc, False, -1, "decor")
            continue
        w_ = min(rng.uniform(5, 11) * scale, maxw)
        d_ = min(rng.uniform(5, 11) * scale, maxw)
        t = 1.0 - dist / radius
        h = (6 + 55 * t ** 1.6 * rng.uniform(0.4, 1.0) + rng.uniform(0, 8)) * scale
        if ce["pier"]:
            h *= 0.6
        base = pc + ce["d"] * (ce["top"] - 0.05) - city.x
        _tower(city, rng, base, ce["fr"], w_, d_, h, b)
        tops.append((b, base, ce["fr"], h, min(w_, d_)))
        b += 1
    # 高いビルどうしを結ぶ連絡通路
    for k in range(len(tops)):
        for m in range(k + 1, len(tops)):
            b1, p1, f1, h1, _ = tops[k]
            b2, p2, f2, h2, _ = tops[m]
            L_ = np.linalg.norm(p2 - p1)
            if L_ > block * 1.6 or min(h1, h2) < max(18 * scale, 15.0) or rng.random() > 0.35:
                continue
            y = rng.uniform(10, min(h1, h2) - 4)
            a = p1 + f1[1] * y
            c2 = p2 + f2[1] * y
            mid = (a + c2) / 2
            ax = (c2 - a) / np.linalg.norm(c2 - a)
            fr = _frame(f1[1], ax)
            fr = np.stack([fr[0], fr[1], ax])
            nc = NEON[rng.integers(len(NEON))]
            city.add(mid, fr, (0.9, 0.35, np.linalg.norm(c2 - a) / 2), (0.15, 0.15, 0.2), None, True, b1, "walk")
            city.add(mid + fr[1] * 0.4, fr, (0.95, 0.05, np.linalg.norm(c2 - a) / 2), nc, nc, False, b1, "neon")
    city.finish()


def build_station(city, rng, scale=1.0):
    """軌道上の街（ステーション）：台座の上下にビル、まわりにネオンの輪と太陽電池の翼。"""
    I3 = np.eye(3)
    S = 18.0 * scale
    deck = np.array([0.12, 0.13, 0.18])
    city.core = 0
    city.add((0, 0, 0), I3, (S, 1.5, S), deck, None, True, 0, "body")
    city.add((0, 1.6, 0), I3, (S + 0.1, 0.1, S + 0.1), NEON[1], NEON[1], False, 0, "neon")
    from . import citylife
    b = 1
    # 内装のある建物（甲板の上）：店・不動産屋・住宅
    specials = {(0, -1): "trader", (-1, 0): "estate", (1, 0): "housing"}
    for (i, j), role in specials.items():
        p = np.array([i * S * 0.36, 1.5, j * S * 0.36])
        fz = -np.array([i, 0.0, j]) / max(np.hypot(i, j), 1e-9)       # 扉は中央を向く
        fz = fz if np.linalg.norm(fz) > 0 else np.array([0, 0, 1.0])
        bfr = np.stack([np.cross([0, 1.0, 0], fz), np.array([0, 1.0, 0]), fz])
        wd = 8.0 * scale + 1.0
        if role == "housing":
            top = citylife.build_housing(city, rng, p, bfr, wd, max(wd * 0.75, 5.5), b)
        else:
            top = citylife.build_shop(city, rng, p, bfr, max(wd * 0.8, 6.0), max(wd * 0.8, 6.0), b, role)
        _tower(city, rng, top, bfr, wd * 0.7, wd * 0.6, (10 + 20 * rng.random()) * scale, b)
        b += 1
    for k in range(3):                 # 甲板の住民
        a = rng.uniform(0, 2 * np.pi)
        citylife.add_npc(city, rng, "citizen", np.array([np.cos(a) * 3.0, 1.5, np.sin(a) * 3.0]),
                         np.array([0, 0, 1.0]), np.array([0, 1.0, 0]), 0, wander=float(rng.uniform(1.5, 2.5)))
    for i in range(-2, 3):
        for j in range(-2, 3):
            if rng.random() < 0.25 or (i, j) in specials or (i, j) == (0, 0):
                continue
            p = np.array([i * S * 0.36, 0, j * S * 0.36]) + rng.uniform(-1.5, 1.5, 3) * [1, 0, 1]
            w_, d_ = rng.uniform(4, 8) * scale, rng.uniform(4, 8) * scale
            t = 1 - np.hypot(i, j) / 3.0
            h = (8 + 40 * t * rng.uniform(0.5, 1.0)) * scale
            _tower(city, rng, p + [0, 1.5, 0], I3, w_, d_, h, b)
            b += 1
            if rng.random() < 0.5:   # 下向きのビル
                Fd = np.array([[1, 0, 0], [0, -1, 0], [0, 0, -1.0]])
                _tower(city, rng, p - [0, 1.5, 0], Fd, w_ * 0.8, d_ * 0.8, h * 0.5, b)
                b += 1
    for sgn in (-1, 1):                # 太陽電池の翼
        city.add((sgn * (S + 14 * scale), 0, 0), I3, (12 * scale, 0.2, 5 * scale), (0.12, 0.2, 0.45), None, True, b, "body")
        city.add((sgn * (S + 1.5), 0, 0), I3, (1.5, 0.6, 0.6), (0.4, 0.4, 0.45), None, True, b, "body")
        b += 1
    city.finish()


def build_outpost(city, planet, center_dir, rng):
    build_surface_city(city, planet, center_dir, rng, 16.0, scale=0.75)


# ---------------------------------------------------------------------------
class NoCities:
    """街なし（読み込み前・設定で OFF）。"""
    enabled = False
    cities = []
    by_id = {}

    def nearest(self, p, cands=None):
        return None, 1e300

    def marker_target(self, p):
        return None, 1e300, False


class CityManager:
    def __init__(self, world):
        self.world = world
        self.cities = []
        self.by_id = {}
        self.enabled = True          # この MOD が入っているワールドでは街を作る
        try:
            self._generate()
        except Exception:  # noqa: BLE001
            L.exception("街の生成に失敗しました（街なしで続けます）")
            self.cities = []
        st = world.settings.get("city_state", {}) or {}
        for c in self.cities:
            if c.id in st:
                c.load_state(st[c.id])
        self.by_id = {c.id: c for c in self.cities}
        L.info("街 %d（地表 %d・軌道 %d）", len(self.cities), sum(c.kind == "surface" for c in self.cities),
               sum(c.kind != "surface" for c in self.cities))

    # --- 生成 ---------------------------------------------------------------
    def _generate(self):
        w = self.world
        spawn = w._default_spawn()
        used_names = set()
        for pi, pl in enumerate(w.planets):
            seed = (pl.id, pl.seed, round(pl.radius), round(float(pl.center[0])), round(float(pl.center[2])))
            rng = _rng("cities", *seed)
            if pl.radius >= 300:
                spacing, rad, n_orbit = 520.0, (40.0, 70.0), 3
            elif pl.radius >= 100:
                spacing, rad, n_orbit = 300.0, (25.0, 40.0), 1
            else:
                spacing, rad, n_orbit = None, None, 0
            n_surf = 1 if spacing is None else max(2, int(round(4 * np.pi * pl.radius ** 2 / spacing ** 2)))
            # 地表：ほぼ等間隔（フィボナッチ球面）の候補地。作れない場所なら、まわりの別の場所を何か所か試す
            k = np.arange(n_surf) + 0.5
            phi = np.arccos(1 - 2 * k / n_surf)
            th = np.pi * (1 + 5 ** 0.5) * k + rng.uniform(0, 2 * np.pi)
            dirs = np.stack([np.cos(th) * np.sin(phi), np.cos(phi), np.sin(th) * np.sin(phi)], 1)
            placed = []
            for si, d0 in enumerate(dirs):
                srng = _rng("city", *seed, si)
                r_city = 14.0 if rad is None else float(srng.uniform(*rad))
                min_sep = (spacing or 0) * 0.62
                found = None
                for attempt in range(10):
                    if attempt == 0:
                        d = d0.copy()
                    else:
                        f = _frame(d0)
                        a = srng.uniform(0, 2 * np.pi)
                        off = srng.uniform(0.1, 0.35) * (spacing or 0) / pl.radius
                        d = d0 + (f[0] * np.cos(a) + f[2] * np.sin(a)) * off
                        d /= np.linalg.norm(d)
                    if any(np.arccos(np.clip(d @ q, -1, 1)) * pl.radius < min_sep for q in placed):
                        continue
                    if self._site_ok(pl, d, spawn, r_city):
                        found = d
                        break
                if found is None:
                    continue
                placed.append(found)
                name = self._name(rng, used_names, outpost=rad is None)
                c = City(f"{pl.id}{pi}-t{si}", name, "surface", pl)
                if rad is None:
                    build_outpost(c, pl, found, srng)
                else:
                    build_surface_city(c, pl, found, srng, r_city)
                c.region = self._region(c, pl)
                c.home = pi
                self.cities.append(c)
            # 軌道上
            for oi in range(n_orbit):
                orng = _rng("station", *seed, oi)
                c = City(f"{pl.id}{pi}-o{oi}", "軌道都市" + self._name(orng, used_names), "orbit", pl)
                c.homes, c.npcs = [], []
                build_station(c, orng, 1.0 if pl.radius >= 300 else 0.6)
                c.region = "orbit"
                alt = pl.radius * orng.uniform(0.45, 0.8) if pl.radius >= 300 else pl.radius * orng.uniform(0.6, 0.9)
                rr = pl.bound + alt
                n = orng.normal(size=3)
                n /= np.linalg.norm(n)
                pos_dir = _frame(n)[0]
                c.x = pl.center + pos_dir * rr
                vdir = np.cross(n, pos_dir)
                c.v = vdir * np.sqrt(pl.gm * float(w.settings.get("gravity_scale", 1.0)) / rr)
                c.R = _frame(pos_dir, vdir)[[0, 1, 2]].T
                c.w = n * np.linalg.norm(c.v) / rr       # いつも下面を惑星に向ける
                c.home = pi
                self.cities.append(c)

    @staticmethod
    def _region(c, pl):
        """地域：辺境（月・小惑星）／海辺（桟橋がある）／火山（硫黄の採れる高い山）／平地。"""
        if pl.radius < 300:
            return "frontier"
        if any(pier for _, pier in getattr(c, "cells", {}).values()):
            return "coast"
        h = float(np.linalg.norm(c.x - pl.center)) - pl.radius
        tops = [t for t, _ in getattr(c, "cells", {}).values()] or [0]
        relief = max(tops) - min(tops)
        if pl.resource_at(c.x + (c.x - pl.center) / np.linalg.norm(c.x - pl.center) * 0.1) == "sulfur" \
                or h > max(pl.amp * 0.3, 3.0) or relief > 8.0:
            return "volcano"
        return "plain"

    def _site_ok(self, pl, d, spawn, radius):
        """街を作れる場所か。山あい（段々にして階段でつなぐ）や海沿い（水の上は桟橋）も作れる。
        作らないのは：出現地点のまわり、ほとんど水の上、深い海、高低差が大きすぎる所（崖ばかりになる）。"""
        up_s = (spawn - pl.center)
        if np.linalg.norm(up_s) < pl.radius * 3:
            ang = np.arccos(np.clip(d @ (up_s / np.linalg.norm(up_s)), -1, 1)) * pl.radius
            if ang < radius + 140.0:
                return False
        f = _frame(d)
        pts = [d]
        for rr_, nn in ((0.45, 8), (0.9, 12)):
            for a in np.linspace(0, 2 * np.pi, nn, endpoint=False):
                pts.append(d + (f[0] * np.cos(a) + f[2] * np.sin(a)) * (radius * rr_ / pl.radius))
        pts = np.array(pts)
        pts /= np.linalg.norm(pts, axis=1)[:, None]
        r, wr = _ground(pl, pts)
        land = r > wr if pl.has_water else np.ones(len(r), bool)
        if land.mean() < 0.45:
            return False                       # ほとんど海・湖の上
        if pl.has_water and float((wr - r)[~land].max(initial=0.0)) > PIER_DEPTH * 1.5:
            return False                       # 深い海に面しすぎ
        rl = r[land]
        if float(rl.max() - rl.min()) > max(18.0, radius * 0.75):
            return False                       # 急すぎる山腹
        return True

    def _name(self, rng, used, outpost=False):
        for _ in range(50):
            n = (PRE[rng.integers(len(PRE))] + "・" if rng.random() < 0.6 else "") + SYL[rng.integers(len(SYL))]
            if outpost:
                n += "前哨基地"
            elif rng.random() < 0.4:
                n += f"{rng.integers(2, 99)}区"
            if n not in used:
                used.add(n)
                return n
        return n

    def clear_overlaps(self, constructs):
        """読み込んだ時点で構築物と重なっている建物は、最初から無かったことにする（既存のワールド向け）。
        構築物が街の地面（舗装・階段）と重なる場合は、その街ごと作らない。"""
        n = 0
        drop = set()
        for c in constructs:
            if not c.n_vox:
                continue
            P = c.voxel_centers()
            for city, C, A, E, idx in self.boxes_for(P.min(0), P.max(0), 1.0):
                if city.is_moving():
                    continue
                d, _, k = box_sdf(P, C, A, E)
                hit_parts = np.unique(idx[k[d < c.size]])
                if any(int(city.bld[part]) < 0 for part in hit_parts):
                    drop.add(city.id)          # 地面（舗装・階段）と重なる：その場所には街を作らない
                    continue
                for part in hit_parts:
                    b = int(city.bld[part])
                    if b >= 0 and city.state[b] < 2:
                        city.state[b] = 2
                        city.version += 1
                        n += 1
        if n:
            L.info("構築物と重なっていた建物 %d 棟を取り除きました", n)
        if drop:
            self.cities = [c for c in self.cities if c.id not in drop]
            self.by_id = {c.id: c for c in self.cities}
            L.info("構築物と重なるので作らなかった街: %d", len(drop))

    # --- 保存 ---------------------------------------------------------------
    def to_state(self):
        out = {}
        for c in self.cities:
            d = c.to_state()
            if d.get("b") or "x" in d:
                out[c.id] = d
        return out

    # --- 問い合わせ ---------------------------------------------------------
    def near(self, p, margin):
        p = np.asarray(p, float)
        return [c for c in self.cities if np.linalg.norm(p - c.x) < c.radius + margin and c.alive()]

    def nearest(self, p, cands=None):
        best, bd = None, 1e300
        for c in (self.cities if cands is None else cands):
            if not c.alive() or getattr(c, "fallen", False):
                continue
            d = float(np.linalg.norm(c.x - p))
            if d < bd:
                best, bd = c, d
        return best, bd

    SURFACE_ALT = 60.0        # 地表から（これ と 半径の 15% の大きい方）以内なら「地表近く」

    def surface_planet(self, p):
        """地表近くにいるなら、その惑星（いなければ None）。"""
        pl = self.world.nearest_planet(p)
        if pl is None:
            return None
        alt = float(pl.sdf(np.asarray(p, float)[None])[0])
        return pl if alt < max(self.SURFACE_ALT, pl.radius * 0.15) else None

    def on_planet(self, c, pl):
        """街 c が惑星 pl の地表にあるか（地表の街・墜落した街）。"""
        if c.kind == "orbit":
            return False
        if c.kind == "surface":
            return c.host is pl
        return self.world.nearest_planet(c.x) is pl      # 墜落した街は落ちた惑星

    def marker_target(self, p):
        """目印を出す街：地表近くにいるときはその惑星の地表の街を優先し、無ければいちばん近い街。
        戻り値 (街, 距離, 地表優先か)"""
        pl = self.surface_planet(p)
        if pl is not None:
            c, d = self.nearest(p, [c for c in self.cities if self.on_planet(c, pl)])
            if c is not None:
                return c, d, True
        c, d = self.nearest(p)
        return c, d, False

    def raycast(self, o, dvec, max_t):
        best = None
        for c in self.near(o, max_t):
            C, A, E, idx = c.world_boxes()
            if not len(C):
                continue
            r = ray_boxes(o, dvec, C, A, E, max_t)
            if r and (best is None or r[0] < best["t"]):
                best = {"kind": "city", "t": r[0], "city": c, "part": int(idx[r[1]]), "point": o + dvec * r[0],
                        "normal": r[2]}
            # NPC
            if c.npcs:
                from . import citylife
                live = [n for n in c.npcs if citylife.npc_alive(c, n)]
                if live:
                    boxes = [citylife.npc_box(c, n, self.world.time) for n in live]
                    Cn = np.array([b_[0] for b_ in boxes])
                    An = np.array([b_[1] for b_ in boxes])
                    En = np.array([b_[2] for b_ in boxes])
                    lim = best["t"] if best is not None else max_t
                    rn = ray_boxes(o, dvec, Cn, An, En, lim)
                    if rn:
                        best = {"kind": "npc", "t": rn[0], "city": c, "npc": live[rn[1]], "point": o + dvec * rn[0],
                                "normal": rn[2]}
        return best

    def boxes_for(self, lo, hi, margin):
        """AABB (lo,hi) の近くの街の当たり判定用の箱：[(city, C, A, E, idx)]"""
        out = []
        ctr = (lo + hi) / 2
        ext = float(np.linalg.norm(hi - lo)) / 2
        for c in self.near(ctr, ext + margin):
            C, A, E, idx = c.world_boxes()
            if not len(C):
                continue
            rb = np.linalg.norm(E, axis=1)
            m = np.all(C + rb[:, None] + margin > lo, axis=1) & np.all(C - rb[:, None] - margin < hi, axis=1)
            if m.any():
                out.append((c, C[m], A[m], E[m], idx[m]))
        return out

    def push_sphere(self, center, r):
        """プレイヤーなどの球を建物の外へ押し出す：[(押し出し, 法線, 街)]"""
        out = []
        for c, C, A, E, idx in self.boxes_for(center - r, center + r, 1.0):
            d, n, k = box_sdf(center[None], C, A, E)
            if d[0] < r:
                out.append((n[0] * (r - d[0]), n[0], c))
        return out

    # --- 軌道上の街の運動 ---------------------------------------------------
    def step(self, dt, anchored):
        w = self.world
        for c in self.cities:
            if c.kind != "orbit":
                c.force[:] = 0
                c.torque[:] = 0
                c.impulse[:] = 0
                c.ang_impulse[:] = 0
                continue
            M = c.mass + sum(a.total_mass() for a in anchored.get(c.id, []))
            Iinv = np.linalg.inv(c.R @ c.I @ c.R.T)
            g = w.gravity(c.x[None])[0]
            # 押された分（構築物との接触）と、くっついた構築物の推進機の力
            c.v = c.v + c.impulse / M + (g + c.force / M) * dt
            c.w = c.w + Iinv @ (c.ang_impulse + c.torque * dt)
            c.x = c.x + c.v * dt
            wn = float(np.linalg.norm(c.w))
            if wn > 1e-12:
                c.R = _rot(c.w, wn * dt) @ c.R
                u, _, vt = np.linalg.svd(c.R)
                c.R = u @ vt
            c.force[:] = 0
            c.torque[:] = 0
            c.impulse[:] = 0
            c.ang_impulse[:] = 0
            self._ground(c)

    def _ground(self, c):
        """ステーションが地面に落ちた：ぶつかった建物が壊れ、止まったら墜落した街になる。"""
        for pl in self.world.planets:
            if np.linalg.norm(c.x - pl.center) > pl.bound + c.radius:
                continue
            Cw, Aw, E, idx = c.world_boxes()
            corners = []
            owner = []
            for s in np.array([[x, y, z] for x in (-1, 1) for y in (-1, 1) for z in (-1, 1)]):
                corners.append(Cw + np.einsum("bi,bij->bj", E * s, Aw))
                owner.append(idx)
            P = np.concatenate(corners)
            own = np.concatenate(owner)
            d, n = pl.sdf_grad(P)
            hit = d < 0
            if not hit.any():
                continue
            k = int(np.argmin(d))
            nrm = n[k]
            vn = float(c.v @ nrm)
            speed = max(-vn, 0.5 * float(np.linalg.norm(c.v)))   # 滑りながら落ちても壊れる
            for part in np.unique(own[hit]):
                c.hit(int(part), speed + 3.0)      # 地面との衝突は強め
            c.x = c.x - nrm * float(d[k])
            if vn < 0:
                c.v = c.v - nrm * vn
            c.v *= 0.85
            c.w *= 0.85
            if float(np.linalg.norm(c.v)) < 0.5:
                c.kind = "wreck"
                c.v[:] = 0
                c.w[:] = 0
                self.world.message(f"{c.name} が {pl.name} に墜落しました")

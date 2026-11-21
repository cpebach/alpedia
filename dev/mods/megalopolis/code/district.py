"""A-10・A-11：大都市圏の街の暮らし。場所ごとに街 MOD（city）の街（City）を 1 つ作り、大都市圏の建物の中に店・部屋・仕事場を置く。

- 大きな街（東京・ニューヨークのような街）：都心の高層ビルと郊外の建物に、百貨店 2・不動産屋 2・商人 2 が点在する。
  駅近の部屋（駅にいちばん近い集合住宅など 2 棟）・ふつうの部屋（郊外の集合住宅 2 棟）。
  高層階の部屋は街でいちばん高いビルの上の方の階（3 フロア。1 階のロビーのエレベーターで上がる）。
  店の前には住民（半分は情報通）。
- 工業地帯：駅にいちばん近い工場の建物の中に仕事場（班長・置き場・装置・作業員）、倉庫の中に商人。
- 建物はそのまま（ブロックの殻）。中の家具・NPC・明かり・看板・入口の階段は街の箱。部屋の壁は街の壁（ブロック）。
  建物の中の床は、殻の 1 段目の上（地面から セルの大きさ だけ上）なので、入口の前に階段を置く。
中身（百貨店・値下げ・物件・仕事）は街 MOD の metro.py。街 MOD が無いワールドでは作らない。
"""
import importlib

import numpy as np

from . import build as BLD
from . import plan as P

CONC = np.array([0.5, 0.5, 0.52])
FRONT_CLEAR = 10.0      # 入口の前にこれだけ空いている建物だけ使う（道に面している）


def _stations(mplan, site):
    """場所の駅の真ん中（単位ベクトル）"""
    out = []
    for ln in mplan["lines"]:
        if site["id"] not in (ln["a"], ln["b"]):
            continue
        U = P.unit(ln["P"])
        S = np.concatenate([[0.0], np.cumsum(np.linalg.norm(np.diff(ln["P"], axis=0), axis=1))])
        from .addon import STOP
        s = STOP if ln["a"] == site["id"] else S[-1] - STOP
        i = int(np.clip(np.searchsorted(S, s) - 1, 0, len(S) - 2))
        out.append(U[i])
    return out


class Host:
    """大都市圏の建物 1 つの中：置き場所・中の広さ・入口（街ローカルの点と軸を返す）"""

    def __init__(self, b, site, pl, city):
        self.b, self.city = b, city
        self.pp = BLD.place(b, site, pl)
        self.s, self.nx, self.ny, self.nz = BLD.dims(b)
        s = self.s
        x0, z0 = -(self.nx // 2), -(self.nz // 2)
        self.x_lo, self.x_hi = (x0 + 1) * s - s / 2, (x0 + self.nx - 2) * s + s / 2
        self.z_lo, self.z_hi = (z0 + 1) * s - s / 2, (z0 + self.nz - 2) * s + s / 2
        self.front_out = (z0 + self.nz - 1) * s + s / 2
        self.ceil = (self.ny - 1) * s - s / 2
        self.dw, self.dh = BLD.door_cells(b["style"], s, self.ny)

    @property
    def ok(self):
        return self.pp is not None

    @property
    def inner(self):
        return self.x_hi - self.x_lo, self.z_hi - self.z_lo, self.ceil - self.s

    def fr(self):
        return np.stack([self.pp["right"], self.pp["up"], self.pp["front"]])

    def at(self, x, z, h):
        """建物の足もと（真ん中）から 横 x・前 z・上 h m の点（街ローカル）"""
        p = self.pp
        return p["foot"] + p["right"] * x + p["front"] * z + p["up"] * h - self.city.x

    def entrance(self, pl, b, sign_col=None):
        """入口の前の階段（地面から建物の床まで）と、扉の上の看板"""
        c, fr, s = self.city, self.fr(), self.s
        up = self.pp["up"]
        pc = np.asarray(pl.center, float)
        q = self.pp["foot"] + self.pp["front"] * (self.front_out + 1.5) - pc
        g, _ = P.ground(pl, P.unit(q)[None])
        drop = float(self.pp["top"] - g[0])            # 足もとの地面から入口の前の地面まで（下が正）
        rise = s + drop
        width = (2 * self.dw + 1) * s / 2
        if rise > 0.3:
            n = int(np.ceil(rise / 0.22))
            for k in range(n):
                st = s - (k + 1) * rise / (n + 1)       # 段の上面（足もとの地面から）
                bot = -drop - 0.4
                c.add(self.at(0.0, self.front_out + (k + 0.5) * 0.35, (st + bot) / 2), fr, (width, (st - bot) / 2, 0.18),
                      CONC, None, True, b, "stair")
        if sign_col is not None:
            col = np.asarray(sign_col, float)
            h = s * (self.dh + 1) + 0.6
            c.add(self.at(0.0, self.front_out + 0.1, h), fr, (min(width + 1.0, 4.0), 0.4, 0.06), col, col, False, b, "sign")
            c.add(self.at(0.0, self.front_out + 0.6, s + 0.03 - 0.0), fr, (width, 0.02, 0.5), col * 0.6, col * 0.6, False, b, "neon")

    def area(self, w_max, d_max, back=False):
        """中の床の上の、入口側（back なら奥）に寄せた売り場の真ん中（街ローカル）と、その広さ"""
        iw, id_, _ = self.inner
        w, d = min(iw - 1.0, w_max), min(id_ - 1.0, d_max)
        xc = (self.x_lo + self.x_hi) / 2
        zc = (self.z_lo + d / 2 + 0.3) if back else (self.z_hi - d / 2 - 0.3)
        return self.at(xc, zc, self.s), w, d


def _mods(ca):
    pkg = type(ca).__module__.rsplit(".", 1)[0]
    return tuple(importlib.import_module(pkg + "." + m) for m in ("cities", "citylife", "metro", "blockify"))


def build(ca, pl, pi, site, mplan, seed):
    """場所 site の街（City）を作る。建てられる建物が無ければ None"""
    cities, citylife, metro, blockify = _mods(ca)
    rng = P.rng("mega-district", *seed, site["id"])
    works = site["kind"] == "industrial"
    c = cities.City(f"{pl.id}{pi}-metro-{site['id']}", site["name"], "surface", pl)
    c.stype = "works" if works else "metro"
    c.region = c.stype
    c.home = pi
    g, _ = P.ground(pl, site["d"][None])
    c.x = np.asarray(pl.center, float) + site["d"] * float(g[0])
    R = pl.radius
    st_dirs = _stations(mplan, site)
    mine = [b for b in mplan["buildings"] if b["site"] == site["id"]]
    for b in mine:
        b["_r"] = float(np.hypot(b["x"], b["z"]))
        bd = np.asarray(b["dir"], float)
        b["_st"] = min((P.angdist(bd, q, R) for q in st_dirs), default=b["_r"])

    hosts = {}

    def host(b):
        h = hosts.get(b["id"])
        if h is None:
            h = hosts[b["id"]] = Host(b, site, pl, c)
        return h

    # 入口の前（建物の前 FRONT_CLEAR m）にほかの建物が無いか（道に面しているか）。建物はみな場所の +z を向いている
    rects = np.array([(b["x"] - b["w"] / 2, b["x"] + b["w"] / 2, b["z"] - b["d"] / 2, b["z"] + b["d"] / 2) for b in mine]) \
        if mine else np.zeros((0, 4))
    idx = {b["id"]: k for k, b in enumerate(mine)}

    def faces_street(b):
        x0, x1, z0 = b["x"] - b["w"] / 2 + 1.0, b["x"] + b["w"] / 2 - 1.0, b["z"] + b["d"] / 2
        z1 = z0 + FRONT_CLEAR
        hit = (rects[:, 0] < x1) & (rects[:, 1] > x0) & (rects[:, 2] < z1) & (rects[:, 3] > z0)
        hit[idx[b["id"]]] = False
        return not hit.any()

    def cands(styles, r0=0.0, r1=1e9, min_w=8.0, min_d=8.0, min_h=4.2):
        out = []
        for b in mine:
            if b["style"] not in styles or not (r0 * site["R"] <= b["_r"] <= r1 * site["R"]):
                continue
            if not faces_street(b):
                continue
            s, nx, ny, nz = BLD.dims(b)
            if (nx - 2) * s < min_w or (nz - 2) * s < min_d or (ny - 2) * s < min_h:
                continue
            out.append(b)
        return out

    used, spots = set(), []

    def choose(bs, key, sep=150.0):
        """ちょうどいい建物を 1 つ（置けない所・使った建物・近すぎる所は飛ばす）"""
        for b in sorted(bs, key=key):
            if b["id"] in used or any(np.hypot(b["x"] - x, b["z"] - z) < sep for x, z in spots):
                continue
            h = host(b)
            if not h.ok:
                continue
            used.add(b["id"])
            spots.append((b["x"], b["z"]))
            return b, h
        return None

    bi = [0]

    def nb():
        bi[0] += 1
        return bi[0] - 1

    def citizen(h, k, talk):
        p = h.at(rng.uniform(-3, 3), h.front_out + 5.0, 0.0)
        n = citylife.add_npc(c, rng, "citizen", p, h.pp["front"], h.pp["up"], -1, wander=float(rng.uniform(2.0, 3.5)))
        n["talk_set"] = talk
        n["gossip"] = (k % 2 == 0) and talk == "metro"

    def shop(h, role, k):
        b = nb()
        base, w, d = h.area(14.0, 12.0)
        if role == "dept":
            metro.build_dept(c, rng, base, h.fr(), w, d, b, walls=False)
        else:
            citylife.build_shop(c, rng, base, h.fr(), w, d, b, role, walls=False)
        c.npcs[-1]["inside"] = h.b["id"]
        h.entrance(pl, b, citylife.SIGN[role])
        citizen(h, k, "worker" if works else "metro")

    def rooms(h, grade, label):
        b = nb()
        base, w, d = h.area(12.0, 7.0, back=True)
        n0 = len(c.homes)
        metro.housing(c, rng, base, h.fr(), w, d, b, grade, label)
        for hm in c.homes[n0:]:
            hm["inside"] = h.b["id"]
        h.entrance(pl, b, citylife.SIGN["home"])

    if works:
        fac = choose(cands(("factory",), min_w=28.0, min_d=20.0, min_h=6.0), key=lambda b: b["_st"], sep=0.0)
        if fac is None:
            return None
        h = fac[1]
        b = nb()
        base, w, d = h.area(26.0, 18.0)
        metro.build_factory_hall(c, rng, base, h.fr(), w, d, b, walls=False)
        h.entrance(pl, b, citylife.SIGN["factory"])
        for k in range(2):
            citizen(h, k, "worker")
        tr = choose(cands(("warehouse", "factory")), key=lambda b: b["_st"], sep=60.0)
        if tr is not None:
            shop(tr[1], "trader", 1)
    else:
        office = ("office", "tower", "apartment")
        # 高層階：街でいちばん高いビル
        towers = sorted((b for b in mine if b["style"] == "tower"), key=lambda b: -b["h"])
        top = None
        for b in [t for t in towers if faces_street(t)][:6] or towers[:6]:
            h = host(b)
            if h.ok and h.inner[0] >= 10 and h.inner[1] >= 10 and h.ceil > 40:
                top = (b, h)
                used.add(b["id"])
                break
        if top is not None:
            b, h = top
            bt = nb()
            lobby = h.at((h.x_lo + h.x_hi) / 2, h.z_hi - 2.5, h.s)
            floors = [h.ceil - 4.5 - 14.0 * k for k in range(3) if h.ceil - 4.5 - 14.0 * k > 30.0]
            for k, hf in enumerate(floors):
                fb = nb()
                metro.build_view_floor(c, rng, h.at((h.x_lo + h.x_hi) / 2, (h.z_lo + h.z_hi) / 2, hf), h.fr(),
                                       h.inner[0], h.inner[1], fb, lobby, (k - (len(floors) - 1) / 2) * 2.2,
                                       f"{site['name']} タワー {int(round(hf))} m の階", h.b["id"])
            h.entrance(pl, bt, (0.4, 0.9, 1.0))
        R_ = site["R"]
        picks = [
            ("dept", cands(("tower", "office"), 0.0, 0.35, 12, 10), lambda b: b["_r"]),                      # 都心のビル
            ("estate", cands(office, 0.0, 0.5), lambda b: b["_st"]),                                          # 駅のそば
            ("trader", cands(office, 0.0, 0.4), lambda b: abs(b["_r"] - 0.25 * R_)),
            ("dept", cands(("office", "apartment"), 0.55, 0.9, 12, 10), lambda b: rng.random()),              # 郊外
            ("estate", cands(office, 0.5, 0.9), lambda b: rng.random()),
            ("trader", cands(office, 0.45, 0.9), lambda b: rng.random()),
        ]
        k = 0
        for role, bs, key in picks:
            ch = choose(bs, key)
            if ch is not None:
                shop(ch[1], role, k)
                k += 1
        # 部屋：駅近（駅にいちばん近い集合住宅など）2 棟・郊外のふつうの部屋 2 棟
        for grade, label, bs, key in (("station", "駅前", cands(("apartment", "office"), 0.0, 0.6, 9, 10.5), lambda b: b["_st"]),
                                      ("normal", "", cands(("apartment",), 0.4, 0.95, 9, 10.5), lambda b: rng.random())):
            for _ in range(2):
                ch = choose(bs, key, sep=60.0)
                if ch is not None:
                    rooms(ch[1], grade, label)
    if not c.parts:
        return None
    blockify.extract(c)
    c.finish()
    c.radius = max(c.radius, blockify.extent(c) + 1.0)
    return c

"""A-10：大都市圏の街の暮らし。場所ごとに、街 MOD（city）の街（City）を 1 つ作り、NPC・店・部屋・仕事場を置く。

- 大きな街（東京・ニューヨークのような街）：駅にいちばん近い区画を広場にして、百貨店・不動産屋・商人・駅前の部屋・
  スカイタワー（最上階の部屋）を置く。駅から少し離れた区画に、ふつうの部屋の建物を 2 棟。住民（半分は情報通）。
- 工業地帯：駅に近い区画に工場（班長・置き場・装置・作業員）と商人。
- 使う区画にあった大都市圏の建物は建てない（plan["buildings"] から外す）。
建物の中身・値下げ・物件・仕事は街 MOD の metro.py。街 MOD が無いワールドでは作らない。
"""
import importlib

import numpy as np

from . import plan as P

CONC = np.array([0.5, 0.5, 0.52])
PLAZA = {"tokyo": 30.0, "newyork": 30.0, "industrial": 34.0}     # 使う区画の範囲（区画の中心から ±m）
RES_HALF = 16.0


def _site_lines(mplan, site):
    """場所のまわりの線路の点（単位ベクトル）と、駅の真ん中（単位ベクトル）"""
    pts, stations = [], []
    for ln in mplan["lines"]:
        U = P.unit(ln["P"])
        pts.append(U)
        if site["id"] in (ln["a"], ln["b"]):
            S = np.concatenate([[0.0], np.cumsum(np.linalg.norm(np.diff(ln["P"], axis=0), axis=1))])
            from .addon import STOP
            s = STOP if ln["a"] == site["id"] else S[-1] - STOP
            i = int(np.clip(np.searchsorted(S, s) - 1, 0, len(S) - 2))
            stations.append(U[i])
    return (np.concatenate(pts) if pts else np.zeros((0, 3))), stations


def choose(pl, mplan):
    """場所ごとに使う区画：{場所 ID: {"plaza": (x, z, hx, hz), "res": (x, z, hx, hz) か None}}（毎回同じ）"""
    R = pl.radius
    out = {}
    for s in mplan["sites"]:
        LP, stations = _site_lines(mplan, s)
        want = "yard" if s["kind"] == "industrial" else "lot"
        cand = []
        for (x, z, w, d, kind, _st) in s.get("blocks", []):
            if kind != want or np.hypot(x, z) > s["R"] * 0.5:
                continue
            hx, hz = min(w / 2, PLAZA[s["kind"]]), min(d / 2, PLAZA[s["kind"]])
            dv = P.along(s["d"], s["F"], x, z, R)
            if len(LP):
                near = float(np.arccos(np.clip(LP @ dv, -1, 1)).min()) * R
                if near < np.hypot(hx, hz) + P.CLEAR + P.STEP / 2:
                    continue                                 # 線路・駅にかかる
            st = min((P.angdist(dv, q, R) for q in stations), default=float(np.hypot(x, z)))
            cand.append((st, x, z, hx, hz))
        if not cand:
            continue
        cand.sort()
        plaza = cand[0][1:]
        res = None
        if s["kind"] != "industrial":
            far = [c for c in cand[1:] if 150.0 <= c[0] <= 450.0 and np.hypot(c[1] - plaza[0], c[2] - plaza[1]) > 100.0]
            if far:
                c = min(far, key=lambda c: abs(c[0] - 260.0))
                res = (c[1], c[2], min(c[3], RES_HALF + 4), min(c[4], RES_HALF + 4))
        out[s["id"]] = {"plaza": plaza, "res": res}
    return out


def clear_buildings(mplan, chosen):
    """使う区画にかかる大都市圏の建物を外す"""
    rects = {}
    for sid, ch in chosen.items():
        rects[sid] = [r for r in (ch["plaza"], ch["res"]) if r is not None]
    keep = []
    for b in mplan["buildings"]:
        hit = False
        for (x, z, hx, hz) in rects.get(b["site"], []):
            if abs(b["x"] - x) < hx + b["w"] / 2 and abs(b["z"] - z) < hz + b["d"] / 2:
                hit = True
                break
        if not hit:
            keep.append(b)
    mplan["buildings"] = keep


# ---------------------------------------------------------------------------
# 街を作る
# ---------------------------------------------------------------------------
class _Builder:
    def __init__(self, city, pl, site):
        self.city, self.pl, self.site = city, pl, site
        self.pc = np.asarray(pl.center, float)
        self.R = pl.radius

    def frame(self, x, z, door):
        """場所の座標 (x, z) の上向きと、扉が場所の向き door=(dx, dz) を向く建物の軸（行が 横・上・前）"""
        s = self.site
        up = P.along(s["d"], s["F"], x, z, self.R)
        fz = s["F"][0] * door[0] + s["F"][2] * door[1]
        fz = P.unit(fz - up * (fz @ up))
        return np.stack([np.cross(up, fz), up, fz])

    def ground_at(self, fr, x, z, offs):
        s = self.site
        c = P.along(s["d"], s["F"], x, z, self.R) * self.R
        D = P.unit(np.array([c + fr[0] * ox + fr[2] * oz for ox, oz in offs]))
        g, _ = P.ground(self.pl, D)
        return g

    def place(self, x, z, door, w, d, b):
        """建物の足もと（街ローカル）と軸。坂なら土台と、扉の前の階段を置く"""
        fr = self.frame(x, z, door)
        offs = [(ox, oz) for ox in (-w / 2, 0, w / 2) for oz in (-d / 2, 0, d / 2)]
        g = self.ground_at(fr, x, z, offs + [(0.0, d / 2 + 1.6)])
        top, low, front = float(g[:-1].max()), float(g[:-1].min()), float(g[-1])
        up = fr[1]
        base = self.pc + up * top - self.city.x
        hh = (top - min(low, front) + 0.6) / 2
        self.city.add(self.pc + up * (top - hh) - self.city.x, fr, (w / 2 + 0.2, hh, d / 2 + 0.2), CONC, None, True, b, "found")
        rise = top - front
        if rise > 0.4:
            n = int(np.ceil(rise / 0.22))
            for k in range(n):
                st_top = top - (k + 1) * rise / (n + 1)
                h2 = (st_top - front + 0.6) / 2
                zc = d / 2 + 0.2 + (k + 0.5) * 0.35
                self.city.add(self.pc + up * (st_top - h2) + fr[2] * zc - self.city.x, fr, (1.3, h2, 0.18), CONC, None, True, b,
                              "stair")
        return base, fr


def _mods(ca):
    pkg = type(ca).__module__.rsplit(".", 1)[0]
    return (importlib.import_module(pkg + ".cities"), importlib.import_module(pkg + ".citylife"),
            importlib.import_module(pkg + ".metro"))


def build(ca, pl, pi, site, ch, seed):
    """場所 site の街（City）を作る。ch：choose の結果"""
    cities, citylife, metro = _mods(ca)
    rng = P.rng("mega-district", *seed, site["id"])
    works = site["kind"] == "industrial"
    c = cities.City(f"{pl.id}{pi}-mega-{site['id']}", site["name"], "surface", pl)
    c.stype = "works" if works else "metro"
    c.region = c.stype
    c.home = pi
    px, pz = ch["plaza"][0], ch["plaza"][1]
    dp = P.along(site["d"], site["F"], px, pz, pl.radius)
    g, _ = P.ground(pl, dp[None])
    c.x = np.asarray(pl.center, float) + dp * float(g[0])
    B = _Builder(c, pl, site)
    b = 0

    def npc(x, z, role, wander, talk=None, gossip=False):
        fr = B.frame(x, z, (0.0, 1.0))
        gg = B.ground_at(fr, x, z, [(0.0, 0.0)])
        base = B.pc + fr[1] * float(gg[0]) - c.x
        n = citylife.add_npc(c, rng, role, base, fr[2], fr[1], -1, wander=wander)
        n["talk_set"] = talk or role
        n["gossip"] = gossip
        return n

    if works:
        base, fr = B.place(px, pz - 8, (0.0, 1.0), 26.0, 18.0, b)
        metro.build_factory_hall(c, rng, base, fr, 26.0, 18.0, b)
        b += 1
        base, fr = B.place(px + 20, pz + 18, (0.0, -1.0), 8.0, 7.0, b)
        citylife.build_shop(c, rng, base, fr, 8.0, 7.0, b, "trader")
        b += 1
        for k in range(2):
            npc(px + rng.uniform(-14, 6), pz + rng.uniform(6, 16), "citizen", float(rng.uniform(2.0, 3.5)), talk="worker")
    else:
        base, fr = B.place(px - 6, pz - 17, (0.0, 1.0), 24.0, 12.0, b)
        metro.build_dept(c, rng, base, fr, 24.0, 12.0, b)
        b += 1
        h = 75.0 if site["kind"] == "tokyo" else 95.0
        base, fr = B.place(px + 17, pz - 17, (0.0, 1.0), 12.0, 12.0, b)
        metro.build_sky_tower(c, rng, base, fr, 12.0, 12.0, h, b, b + 1, (0.0, 7.0))
        b += 2
        base, fr = B.place(px - 20, pz + 8, (1.0, 0.0), 8.0, 7.0, b)
        citylife.build_shop(c, rng, base, fr, 8.0, 7.0, b, "estate")
        b += 1
        base, fr = B.place(px + 20, pz + 8, (-1.0, 0.0), 8.0, 7.0, b)
        citylife.build_shop(c, rng, base, fr, 8.0, 7.0, b, "trader")
        b += 1
        base, fr = B.place(px, pz + 21, (0.0, -1.0), 12.0, 7.0, b)
        metro.housing(c, rng, base, fr, 12.0, 7.0, b, "station", "駅前")
        b += 1
        for k in range(4):
            npc(px + rng.uniform(-10, 10), pz + rng.uniform(-5, 8), "citizen", float(rng.uniform(2.0, 4.0)), talk="metro",
                gossip=(k % 2 == 0))
        if ch["res"] is not None:
            rx, rz = ch["res"][0], ch["res"][1]
            for door, zz in (((0.0, 1.0), -8.0), ((0.0, -1.0), 8.0)):
                base, fr = B.place(rx, rz + zz, door, 12.0, 7.0, b)
                metro.housing(c, rng, base, fr, 12.0, 7.0, b, "normal", "")
                b += 1
            for k in range(2):
                npc(rx + rng.uniform(-6, 6), rz + rng.uniform(-2, 2), "citizen", float(rng.uniform(1.5, 3.0)), talk="metro",
                    gossip=(k == 0))
    from importlib import import_module
    blockify = import_module(type(ca).__module__.rsplit(".", 1)[0] + ".blockify")
    blockify.extract(c)
    c.finish()
    c.radius = max(c.radius, blockify.extent(c) + 1.0)
    return c

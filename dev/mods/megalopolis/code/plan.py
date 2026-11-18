"""大都市圏の計画：場所・街区と建物・線路（複線）・駅。惑星と seed から毎回同じ結果になる（セーブには入れない）。

座標：惑星ローカル（惑星の中心が原点）。場所（site）は中心の向き d と、地表に沿った座標の軸 F（行が 横・上・前）。
建物は場所の座標 (x, z) m に置き、建てるとき（build.py）にその点の上向きと地面の高さを調べる。
"""
import hashlib

import numpy as np

# 種類ごと：半径（m。ny は長い方の半分）
KINDS = {
    "tokyo": {"R": 1100.0},
    "newyork": {"R": 1300.0, "half": (1300.0, 360.0)},
    "industrial": {"R": 620.0},
}
ORDER = ["tokyo", "newyork", "industrial", "industrial"]
NAMES = {"tokyo": ["東都", "新京", "大江市"], "newyork": ["摩天楼市", "ハーバーシティ", "グランドアイル"],
         "industrial": ["臨海工業地帯", "北部コンビナート", "東部製鉄所", "南部工業団地"]}
NEAR_SPAWN = 1500.0     # 出現地点から場所の縁まで、これより離す
SEP = 1500.0            # 場所どうしの縁の間
DIST = (2500.0, 12000.0)  # 出現地点から場所の中心まで
STEP = 24.0             # 線路の点の間隔（m）
TRACK_GAP = 2.25        # 複線：真ん中から左右の線路まで（m）
CLEAR = 22.0            # 線路のまわりの建物を建てない幅（m。建物の半分の大きさに足す）
GRADE = 0.035           # 線路の坂の上限
STATION_OFF = 45.0      # 駅：場所の中心から線路の始まりまで


def rng(*keys):
    h = hashlib.sha256("|".join(str(k) for k in keys).encode()).digest()
    return np.random.default_rng(int.from_bytes(h[:8], "little"))


def unit(v):
    v = np.asarray(v, float)
    n = np.linalg.norm(v, axis=-1, keepdims=True)
    return v / np.maximum(n, 1e-12)


def frame(up, ref=None):
    """up を y 軸にした直交フレーム（行が x, y, z 軸）"""
    up = unit(up)
    ref = np.array([0.0, 0.0, 1.0]) if ref is None else np.asarray(ref, float)
    if abs(float(up @ ref)) > 0.95:
        ref = np.array([1.0, 0.0, 0.0])
    z = unit(ref - up * (ref @ up))
    x = np.cross(up, z)
    return np.stack([x, up, z])


def ground(pl, dirs):
    """(地表の半径, 水面の半径)"""
    v = np.asarray(pl._tile_values(np.asarray(dirs, float)))
    return v[:, 0], v[:, 1]


def along(d, F, x, z, R):
    """場所（中心の向き d・軸 F）の (x, z) m の向き。x, z は配列でもよい"""
    x = np.asarray(x, float)[..., None]
    z = np.asarray(z, float)[..., None]
    return unit(d * R + F[0] * x + F[2] * z)


def angdist(a, b, R):
    return float(np.arccos(np.clip(np.asarray(a) @ np.asarray(b), -1, 1))) * R


# ---------------------------------------------------------------------------
# 場所
# ---------------------------------------------------------------------------
def _probe(pl, dirs, Rs):
    """候補地ごとの (起伏 m, 水の割合, 真ん中の高さ m)"""
    n = len(dirs)
    pts = []
    for d, Rk in zip(dirs, Rs):
        F = frame(d)
        pts.append(d)
        for rr in (0.35, 0.75):
            for a in np.linspace(0, 2 * np.pi, 12, endpoint=False):
                pts.append(along(d, F, np.cos(a) * Rk * rr, np.sin(a) * Rk * rr, pl.radius))
    g, w = ground(pl, np.array(pts))
    g, w = g.reshape(n, 25), w.reshape(n, 25)
    wet = (w > g + 0.2) if getattr(pl, "has_water", True) else np.zeros_like(g, bool)
    return g.max(1) - g.min(1), wet.mean(1), g[:, 0] - pl.radius


def choose_sites(pl, seed, avoid=()):
    """[{id, kind, name, d, F, R}]。avoid：[(向き, 半径 m)]（ほかの MOD の街など）"""
    r = rng("mega-sites", *seed)
    spawn = np.array([0.0, 1.0, 0.0])
    Fs = frame(spawn)
    R = pl.radius
    n = 420
    u = r.uniform(0, 1, n)
    dist = np.sqrt(u * (DIST[1] ** 2 - DIST[0] ** 2) + DIST[0] ** 2)
    ang = r.uniform(0, 2 * np.pi, n)
    cand = unit(spawn * np.cos(dist / R)[:, None]
                + (Fs[0] * np.cos(ang)[:, None] + Fs[2] * np.sin(ang)[:, None]) * np.sin(dist / R)[:, None])
    rel, wet, h = _probe(pl, cand, np.full(n, 900.0))
    used = {k: 0 for k in NAMES}
    out = []
    for kind in ORDER:
        Rk = KINDS[kind]["R"]
        best = None
        for i in range(n):
            if wet[i] > 0.12 or h[i] < 3.0 or h[i] > 380.0 or rel[i] > 90.0:
                continue
            if angdist(cand[i], spawn, R) < Rk + NEAR_SPAWN:
                continue
            if any(angdist(cand[i], s["d"], R) < Rk + s["R"] + SEP for s in out):
                continue
            if any(angdist(cand[i], ad, R) < Rk + ar + 300.0 for ad, ar in avoid):
                continue
            score = rel[i] + 300.0 * wet[i] + 0.004 * dist[i]
            if best is None or score < best[0]:
                best = (score, i)
        if best is None:
            continue
        i = best[1]
        d = cand[i]
        yaw = r.uniform(0, np.pi)
        F0 = frame(d)
        F = np.stack([np.cos(yaw) * F0[0] + np.sin(yaw) * F0[2], F0[1], -np.sin(yaw) * F0[0] + np.cos(yaw) * F0[2]])
        name = NAMES[kind][used[kind] % len(NAMES[kind])]
        used[kind] += 1
        out.append({"id": f"m{len(out)}", "kind": kind, "name": name, "d": d, "F": F, "R": Rk})
    return out


# ---------------------------------------------------------------------------
# 建物の並び（場所の座標 x, z m・幅 w・奥行き d・高さ h・様式）
# ---------------------------------------------------------------------------
def _tokyo(r, Rk):
    out = []
    pitch, street = 70.0, 14.0
    centers = [(0.0, 0.0, 1.0)]
    for _ in range(2):
        a, rr = r.uniform(0, 2 * np.pi), r.uniform(0.35, 0.65) * Rk
        centers.append((np.cos(a) * rr, np.sin(a) * rr, r.uniform(0.6, 0.85)))
    n = int(Rk / pitch) + 1
    blk = pitch - street
    for i in range(-n, n + 1):
        for j in range(-n, n + 1):
            bx, bz = i * pitch, j * pitch
            dc = float(np.hypot(bx, bz))
            if dc > Rk - 40.0 or r.random() < 0.04:
                continue                                    # 街の外・小さな公園
            m = r.random()
            if m < 0.3:
                lots = [(bx, bz, blk, blk)]
            elif m < 0.7:
                lots = [(bx - blk / 4, bz, blk / 2, blk), (bx + blk / 4, bz, blk / 2, blk)]
            else:
                lots = [(bx + sx * blk / 4, bz + sz * blk / 4, blk / 2, blk / 2) for sx in (-1, 1) for sz in (-1, 1)]
            for (x, z, lw, ld) in lots:
                w = lw - r.uniform(3.0, 7.0)
                d = ld - r.uniform(3.0, 7.0)
                h = 9.0 + 34.0 * r.random() ** 2
                for cx, cz, k in centers:
                    f = np.exp(-(np.hypot(x - cx, z - cz) / (240.0 * k)) ** 2)
                    h = max(h, (70.0 + 190.0 * k) * f * r.uniform(0.45, 1.05))
                h *= 0.55 + 0.45 * (1 - dc / Rk)
                if h > 70.0:
                    style = "tower"
                    w, d = min(w, 44.0), min(d, 44.0)
                else:
                    style = "office" if r.random() < 0.5 else "apartment"
                out.append({"x": x, "z": z, "w": max(w, 8.0), "d": max(d, 8.0), "h": max(h, 8.0), "style": style})
    return out


def _newyork(r, Rk):
    out = []
    Lh, Wh = KINDS["newyork"]["half"]
    px, pz = 80.0, 280.0                                    # 通り（x 方向に 80 m ごと）・大通り（z 方向に 280 m ごと）
    bx_, bz_ = 60.0, 250.0
    clusters = [(-0.72 * Lh, 1.0), (0.08 * Lh, 0.9)]        # ダウンタウン・ミッドタウン
    park = (0.3 * Lh, 0.68 * Lh, 0.55 * Wh)
    for i in range(int(-Lh / px), int(Lh / px) + 1):
        for j in range(int(-Wh / pz) - 1, int(Wh / pz) + 2):
            cx, cz = i * px, j * pz
            if abs(cx) > Lh - 40 or abs(cz) > Wh:
                continue
            if park[0] < cx < park[1] and abs(cz) < park[2]:
                continue                                    # 大きな公園
            # 区画（60 × 250）の中：x 方向に 2 つ、z 方向に 40〜60 m ごと
            z0 = cz - bz_ / 2
            while z0 < cz + bz_ / 2 - 20:
                lz = min(r.uniform(38, 62), cz + bz_ / 2 - z0)
                for sx in (-1, 1):
                    x = cx + sx * bx_ / 4
                    z = z0 + lz / 2
                    if abs(z) > Wh - 10:
                        continue
                    w, d = bx_ / 2 - r.uniform(1.5, 4.0), lz - r.uniform(2.0, 5.0)
                    h = 20.0 + 45.0 * r.random() ** 1.5
                    for ccx, k in clusters:
                        f = np.exp(-((x - ccx) / (260.0 * k)) ** 2)
                        h = max(h, (110.0 + 270.0 * k) * f * r.uniform(0.4, 1.0))
                    style = "tower" if h > 80 else ("office" if r.random() < 0.7 else "apartment")
                    out.append({"x": x, "z": z, "w": max(w, 10.0), "d": max(d, 10.0), "h": h, "style": style})
                z0 += lz
    return out


def _industrial(r, Rk):
    out = []
    px, pz = 120.0, 90.0
    n = int(Rk / px) + 1
    for i in range(-n, n + 1):
        for j in range(-n - 1, n + 2):
            x, z = i * px, j * pz
            if np.hypot(x, z) > Rk - 50:
                continue
            m = r.random()
            if m < 0.35:
                out.append({"x": x, "z": z, "w": r.uniform(60, 90), "d": r.uniform(40, 60), "h": r.uniform(12, 20), "style": "factory"})
            elif m < 0.6:
                out.append({"x": x, "z": z, "w": r.uniform(50, 80), "d": r.uniform(30, 45), "h": r.uniform(9, 13), "style": "warehouse"})
            elif m < 0.85:
                for sx in (-1, 1):
                    for sz in (-1, 1):
                        if r.random() < 0.8:
                            rad = r.uniform(8, 13)
                            out.append({"x": x + sx * 24, "z": z + sz * 20, "w": 2 * rad, "d": 2 * rad,
                                        "h": r.uniform(10, 18), "style": "tank"})
            else:
                out.append({"x": x, "z": z, "w": 9.0, "d": 9.0, "h": r.uniform(60, 120), "style": "chimney"})
                out.append({"x": x + 30, "z": z, "w": 40.0, "d": 30.0, "h": 16.0, "style": "factory"})
    return out


LAYOUT = {"tokyo": _tokyo, "newyork": _newyork, "industrial": _industrial}


# ---------------------------------------------------------------------------
# 線路
# ---------------------------------------------------------------------------
def _edges(sites, R):
    """最小全域木 ＋ 大きな街どうし"""
    n = len(sites)
    if n < 2:
        return []
    D = np.array([[angdist(a["d"], b["d"], R) for b in sites] for a in sites])
    inside, edges = {0}, []
    while len(inside) < n:
        best = min((D[i, j], i, j) for i in inside for j in range(n) if j not in inside)
        edges.append((best[1], best[2]))
        inside.add(best[2])
    big = [i for i, s in enumerate(sites) if s["kind"] in ("tokyo", "newyork")]
    for a in big:
        for b in big:
            if a < b and (a, b) not in edges and (b, a) not in edges:
                edges.append((a, b))
    return edges


def _endpoint(site, other_d, R):
    """駅：場所の中心から相手の方へ STATION_OFF m の所の向き"""
    d = site["d"]
    t = unit(other_d - d * (other_d @ d))
    return unit(d * R + t * STATION_OFF)


def _line(pl, a, b, lid):
    """場所 a → b の線路の真ん中の点（惑星ローカル、レールの上面）と上向き。山が多すぎれば None"""
    R = pl.radius
    p0, p1 = _endpoint(a, b["d"], R), _endpoint(b, a["d"], R)
    ang = float(np.arccos(np.clip(p0 @ p1, -1, 1)))
    n = max(int(ang * R / STEP), 2)
    t = np.linspace(0, 1, n + 1)
    so = np.sin(ang)
    D = unit((np.sin((1 - t) * ang)[:, None] * p0 + np.sin(t * ang)[:, None] * p1) / so)
    g, w = ground(pl, D)
    h = g - R
    wl = w - R
    tgt = np.where(wl > h + 0.2, wl + 6.0, h + 0.6)
    k = 7
    pad = np.concatenate([np.full(k, tgt[0]), tgt, np.full(k, tgt[-1])])
    sm = np.convolve(pad, np.ones(2 * k + 1) / (2 * k + 1), mode="valid")
    sm = np.maximum(sm, np.where(wl > h + 0.2, wl + 6.0, -1e9))
    ds = ang * R / n
    for _ in range(2):
        for i in range(1, len(sm)):
            sm[i] = np.clip(sm[i], sm[i - 1] - GRADE * ds, sm[i - 1] + GRADE * ds)
        for i in range(len(sm) - 2, -1, -1):
            sm[i] = np.clip(sm[i], sm[i + 1] - GRADE * ds, sm[i + 1] + GRADE * ds)
    if float(np.max(h - sm)) > 60.0:
        return None                                     # 深すぎる切り通し（山）
    return D * (R + sm)[:, None], D, g


def offset_track(P, U, side):
    """真ん中の点 P の左右 side m の線路（P・U は (n,3)）"""
    T = np.gradient(P, axis=0)
    T = unit(T)
    right = unit(np.cross(U, T))
    return P + right * side


def generate(pl, seed, avoid=()):
    """{"sites", "buildings", "lines"}"""
    R = pl.radius
    sites = choose_sites(pl, seed, avoid)
    buildings = []
    for s in sites:
        r = rng("mega-layout", *seed, s["id"])
        for k, b in enumerate(LAYOUT[s["kind"]](r, s["R"])):
            b["id"] = f"mb-{s['id']}-{k}"
            b["site"] = s["id"]
            b["seed"] = int(r.integers(1 << 30))
            buildings.append(b)
    lines = []
    by = {s["id"]: s for s in sites}
    for ia, ib in _edges(sites, R):
        a, b = sites[ia], sites[ib]
        res = _line(pl, a, b, len(lines))
        if res is None:
            continue
        P, D, g = res
        lid = f"L{len(lines)}"
        U = D
        lines.append({"id": lid, "a": a["id"], "b": b["id"], "name": f"{a['name']}・{b['name']}線",
                      "P": P, "U": U, "ground": g,
                      "tracks": [offset_track(P, U, TRACK_GAP), offset_track(P, U, -TRACK_GAP)]})
    # 建物の向き・地面
    if buildings:
        bd = np.array([along(by[b["site"]]["d"], by[b["site"]]["F"], b["x"], b["z"], R) for b in buildings])
        g, w = ground(pl, bd)
        keep = (w <= g + 0.3)
        # 線路のまわりは空ける
        if lines:
            LP = unit(np.concatenate([ln["P"] for ln in lines]))
            half = np.array([0.5 * float(np.hypot(b["w"], b["d"])) for b in buildings])
            near = np.zeros(len(buildings))
            for k0 in range(0, len(bd), 2000):
                cs = np.clip(bd[k0:k0 + 2000] @ LP.T, -1, 1)
                near[k0:k0 + 2000] = np.arccos(cs.max(1)) * R
            keep &= near > half + CLEAR + STEP / 2
        out = []
        for b, d, gg, k in zip(buildings, bd, g, keep):
            if k:
                b["dir"] = d
                b["g"] = float(gg)
                out.append(b)
        buildings = out
    return {"sites": sites, "buildings": buildings, "lines": lines}

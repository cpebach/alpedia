"""A-03：街のほかの集落（村・住宅地・港・工業地帯・畑・鉱山）。地形と気候に合わせて自動で置く。

集落は街（City）の一種（kind "surface"、stype が種類）。建物はブロック（buildkit.Kit で組み立て、blockify が近くに来たら置く）。
畑の畝・道・桟橋は軽い箱（街の箱と同じ。描くのは CityView）。村・港・鉱山・工業地帯には商人の店（citylife.build_shop）。

場所の選び方（惑星の表面の候補地ごとに、高さ・水・傾き・気候・資源を調べる）：
  港：水辺（まわり 150 m に水）の低い所／鉱山：山（高い・急）か硫黄の出る所／畑：平らで温かく、ほどよく湿った所／
  村：住める所（寒すぎ・乾きすぎでない）／工業地帯：硫黄の出る所か大きな街のそば／住宅地：大きな街のそば
"""
import numpy as np

from main import buildkit

from . import blockify, citylife

# 種類ごと：(名前の後ろ, 1 つの惑星での上限)
KINDS = {
    "village": ("村", 30),
    "farm": ("農場", 24),
    "port": ("港", 12),
    "mine": ("鉱山", 14),
    "industrial": ("工業地帯", 10),
    "residential": ("住宅地", 16),
}
MIN_SEP = 700.0          # 集落どうしの間（m）
GROUND = np.array([0.40, 0.36, 0.30])
ROAD = np.array([0.30, 0.30, 0.32])
CROPS = [np.array(c) for c in ((0.42, 0.62, 0.22), (0.78, 0.68, 0.30), (0.30, 0.50, 0.20), (0.62, 0.55, 0.25))]
PIER_C = np.array([0.45, 0.38, 0.30])


# ---------------------------------------------------------------------------
# 建物（Kit。セル 1 m、+Y が上、+Z が前。足もとの真ん中が (0, 0, 0)）
# ---------------------------------------------------------------------------
def _roof(k, x0, x1, y, z0, z1, mat):
    """切妻の屋根（x 方向に下がる）"""
    i = 0
    while x0 + i <= x1 - i:
        k.fill(x0 + i, y + i, z0 - 1, x1 - i, y + i, z1 + 1, mat)
        i += 1


def house(rng):
    k = buildkit.Kit()
    w, d = int(rng.integers(3, 5)), int(rng.integers(3, 5))
    wall = ["clay", "wood", "concrete", "hull_cream"][int(rng.integers(4))]
    roof = ["hull_red", "clay", "hull_dark", "hull_navy"][int(rng.integers(4))]
    k.shell(-w, 0, -d, w, 3, d, wall, "wood", None)
    k.clear(0, 1, d, 0, 2, d)                                       # 扉
    for x in (-w + 1, w - 1):
        k.paint("glass", x, 2, d, x, 2, d)
        k.paint("glass", x, 2, -d, x, 2, -d)
    _roof(k, -w - 1, w + 1, 4, -d, d, roof)
    k.light(0, 3, 0, None, None, None, None, "家の明かり")
    return k.voxels(), 1.0, "家", max(w, d) + 2


def barn(rng):
    k = buildkit.Kit()
    k.shell(-5, 0, -7, 5, 5, 7, "bar_red", "wood", None)
    k.clear(-2, 1, 7, 2, 4, 7)
    _roof(k, -6, 6, 6, -7, 7, "hull_dark")
    return k.voxels(), 1.0, "納屋", 9


def silo(rng):
    k = buildkit.Kit()
    k.cylinder(0, 0, 0, 2, 10, "aluminum", True)
    k.dome(0, 10, 0, 2, "aluminum")
    return k.voxels(), 1.0, "サイロ", 3


def apartment(rng):
    k = buildkit.Kit()
    w, d, fl = int(rng.integers(4, 7)), int(rng.integers(3, 5)), int(rng.integers(3, 7))
    h = fl * 3
    k.shell(-w, 0, -d, w, h, d, "concrete", "concrete", "concrete")
    for y in range(2, h, 3):
        for x in range(-w + 1, w, 2):
            k.paint("glass", x, y, d, x, y, d)
            k.paint("glass", x, y, -d, x, y, -d)
    k.clear(0, 1, d, 0, 2, d)
    return k.voxels(), 1.0, "集合住宅", max(w, d) + 1


def warehouse(rng):
    k = buildkit.Kit()
    k.shell(-8, 0, -11, 8, 6, 11, "hull_dark", "concrete", "steel")
    k.clear(-3, 1, 11, 3, 5, 11)
    k.crates(-6, 1, -9, 4)
    return k.voxels(), 1.0, "倉庫", 12


def factory(rng):
    k = buildkit.Kit()
    k.shell(-9, 0, -7, 9, 7, 7, "steel", "concrete", "hull_dark")
    for x in range(-8, 9, 3):
        k.paint("glass", x, 5, 7, x, 5, 7)
    k.cylinder(6, 8, -4, 1, 14, "concrete", True)                   # 煙突
    k.cylinder(-6, 8, -4, 1, 10, "concrete", True)
    return k.voxels(), 1.0, "工場", 11


def tanks(rng):
    k = buildkit.Kit()
    for x in (-4, 4):
        k.cylinder(x, 0, 0, 3, 6, "hull_white", True)
        k.dome(x, 6, 0, 3, "hull_white")
    return k.voxels(), 1.0, "タンク", 8


def crane(rng):
    """港のクレーン（柱と腕）"""
    k = buildkit.Kit()
    for x in (-2, 2):
        for z in (-2, 2):
            k.fill(x, 0, z, x, 14, z, "hull_yellow")
    k.fill(-2, 14, -2, 2, 15, 2, "hull_yellow")
    k.fill(-1, 15, -10, 1, 15, 10, "hull_yellow")
    k.fill(0, 9, 9, 0, 14, 9, "steel")
    return k.voxels(), 1.0, "クレーン", 10


def headframe(rng):
    """鉱山の立て坑の櫓と小屋・鉱石の山"""
    k = buildkit.Kit()
    for x in (-2, 2):
        for z in (-2, 2):
            k.fill(x, 0, z, x, 12, z, "steel")
    for y in (4, 8, 12):
        k.fill(-2, y, -2, 2, y, -2, "steel")
        k.fill(-2, y, 2, 2, y, 2, "steel")
    k.cylinder(0, 12, 0, 2, 1, "hull_red", False)
    k.shell(4, 0, -3, 9, 4, 3, "wood", "wood", "hull_dark")
    k.clear(6, 1, 3, 7, 2, 3)
    k.ellipsoid(-6, 0, 3, 3, 2, 3, "rock", 1.2, True)
    return k.voxels(), 1.0, "立て坑", 10


def mine_entrance(rng):
    """山肌のトンネルの入り口（岩のアーチ）"""
    k = buildkit.Kit()
    k.fill(-3, 0, 0, 3, 5, 2, "rock")
    k.clear(-1, 0, 0, 1, 3, 2)
    k.fill(-1, 0, 3, 1, 0, 8, "steel")                              # 線路
    k.crates(2, 0, 5, 3)
    return k.voxels(), 1.0, "坑道の入り口", 6


BUILD = {"house": house, "barn": barn, "silo": silo, "apartment": apartment, "warehouse": warehouse,
         "factory": factory, "tanks": tanks, "crane": crane, "headframe": headframe, "mine_entrance": mine_entrance}


# ---------------------------------------------------------------------------
# 置き方
# ---------------------------------------------------------------------------
def _layout(stype, rng):
    """[(建物の種類, x, z, 向き（ラジアン）)] と、畑・道の範囲 [(x, z, 半幅 x, 半幅 z, 種類)]。街の中心から m"""
    out, areas = [], []
    if stype == "village":
        n = int(rng.integers(5, 10))
        for i in range(n):
            a = i / n * 2 * np.pi + rng.uniform(-0.2, 0.2)
            r = rng.uniform(18, 40)
            out.append(("house", np.cos(a) * r, np.sin(a) * r, -a + np.pi / 2))
        areas.append((0, 0, 4, 45, "road"))
        areas.append((0, 0, 45, 4, "road"))
    elif stype == "farm":
        out += [("barn", 0, 0, 0.0), ("silo", 14, -4, 0.0), ("house", -16, 4, 0.0)]
        for k in range(int(rng.integers(2, 5))):
            fx, fz = (k % 2) * 70 - 35 + rng.uniform(-5, 5), (k // 2) * 60 + 45
            areas.append((fx, fz, 30, 25, "field"))
    elif stype == "port":
        out += [("warehouse", -20, -18, 0.0), ("crane", 18, 12, 0.0), ("house", -30, 20, 0.3), ("house", -8, 28, -0.2),
                ("tanks", 30, -20, 0.0)]
        areas.append((0, 30, 40, 6, "pier"))
    elif stype == "mine":
        out += [("headframe", 0, 0, 0.0), ("mine_entrance", 25, -10, rng.uniform(0, 6.28)), ("house", -18, 12, 0.5),
                ("warehouse", -25, -20, 0.0)]
        areas.append((0, 0, 30, 4, "road"))
    elif stype == "industrial":
        out += [("factory", 0, 0, 0.0), ("factory", 0, 30, 0.0), ("warehouse", 35, 10, 0.0), ("tanks", -32, 8, 0.0)]
        areas.append((0, -20, 50, 4, "road"))
    elif stype == "residential":
        for i in range(-1, 2):
            for j in range(-1, 2):
                if (i, j) != (0, 0) or rng.random() < 0.5:
                    out.append(("apartment" if rng.random() < 0.6 else "house", i * 26 + rng.uniform(-3, 3),
                                j * 26 + rng.uniform(-3, 3), 0.0))
        areas.append((0, 13, 40, 3, "road"))
        areas.append((0, -13, 40, 3, "road"))
    return out, areas


def build(city, planet, d0, rng, stype, ground_fn):
    """集落を組み立てる：建物（city.blocks）・畑と道と桟橋（箱）・店（村・港・鉱山・工業地帯）。city.x は地表の真ん中"""
    from .cities import _frame
    pc = np.asarray(planet.center, float)
    F = _frame(d0)
    ang = rng.uniform(0, np.pi)
    F = np.stack([np.cos(ang) * F[0] + np.sin(ang) * F[2], F[1], -np.sin(ang) * F[0] + np.cos(ang) * F[2]])
    gr, _ = ground_fn(planet, d0[None])
    city.x = pc + d0 * float(gr[0])
    city.R = np.eye(3)
    city.F = F
    city.blocks = []
    city.links, city.stairs = [], []

    def at(x, z):
        """街の中心から (x, z) m の地表の点と上向き"""
        p = city.x + F[0] * x + F[2] * z
        d = (p - pc) / np.linalg.norm(p - pc)
        r, w = ground_fn(planet, d[None])
        return pc + d * float(r[0]), d, float(w[0]) - float(r[0])

    items, areas = _layout(stype, rng)
    # 建物：形（ボクセル）は近くに来たときに作る（realize）。ここでは置き場所だけ
    for kind, x, z, yaw in items:
        half = {"barn": 9, "warehouse": 12, "factory": 11, "crane": 10, "headframe": 10, "tanks": 8}.get(kind, 6)
        city.blocks.append({"bld": -1, "lazy": (kind, float(x), float(z), float(yaw), int(rng.integers(1 << 30))),
                            "origin": F[0] * x + F[2] * z, "radius": half * 1.5 + 12})
    # 畑・道・桟橋：4〜6 m の小さな箱を地面に沿って並べる（地面の高さはまとめて調べる）
    tiles = []
    for ax, az, hx, hz, kind in areas:
        tile = 6.0 if kind == "field" else 4.0
        nx, nz = max(1, int(hx * 2 // tile)), max(1, int(hz * 2 // tile))
        col0 = CROPS[int(rng.integers(len(CROPS)))]
        for i in range(nx):
            for j in range(nz):
                tiles.append((ax - hx + (i + 0.5) * tile, az - hz + (j + 0.5) * tile, tile, kind, col0 * (0.9 + 0.2 * ((i + j) % 2))))
    if tiles:
        P = np.array([city.x + F[0] * x + F[2] * z for x, z, *_ in tiles])
        D = (P - pc) / np.linalg.norm(P - pc, axis=1)[:, None]
        gr, wr = ground_fn(planet, D)
        for (x, z, tile, kind, col), d, g, wv in zip(tiles, D, gr, wr):
            p = pc + d * float(g)
            wet = float(wv) - float(g)
            fr = _frame(d, F[2])
            if kind == "pier":
                if wet > 0:
                    city.add(p + d * (wet + 0.6) - city.x, fr, (tile / 2, 0.25, tile / 2), PIER_C, None, True, -1, "walk")
            elif wet <= 0:
                if kind == "field":
                    city.add(p + d * 0.06 - city.x, fr, (tile / 2, 0.12, tile / 2 * 0.8), col, None, False, -1, "ground")
                else:
                    city.add(p + d * 0.05 - city.x, fr, (tile / 2, 0.08, tile / 2), ROAD, None, False, -1, "ground")
    # 店（商人）
    if stype in ("village", "port", "mine", "industrial"):
        p, up, wet = at(-6.0, -30.0)
        if wet <= 0:
            fr = _frame(up, F[2])
            citylife.build_shop(city, rng, p - city.x, fr, 7.0, 6.0, 0, "trader")
    if not city.parts:
        city.add(np.zeros(3), np.eye(3), (1.0, 0.05, 1.0), GROUND, None, False, -1, "ground")
    blockify.extract(city)
    city.finish()
    city.radius = max(city.radius, blockify.extent(city) + 1.0)


def realize(city, entry, ground_fn):
    """遅れて作る建物：entry["lazy"] から (ボクセル, 原点（街ローカル）, 回転, セルの大きさ, 名前)。水の中なら None"""
    from .cities import _frame
    kind, x, z, yaw, seed = entry["lazy"]
    planet = city.host
    pc = np.asarray(planet.center, float)
    F = city.F
    rng = np.random.default_rng(seed)
    V, s, name, half = BUILD[kind](rng)
    offs = [(dx, dz) for dx in (-half, 0, half) for dz in (-half, 0, half)]
    P = np.array([city.x + F[0] * (x + dx) + F[2] * (z + dz) for dx, dz in offs])
    D = (P - pc) / np.linalg.norm(P - pc, axis=1)[:, None]
    gr, wr = ground_fn(planet, D)
    k0 = offs.index((0, 0))
    if float(wr[k0]) - float(gr[k0]) > 0.3 and kind != "crane":
        return None                                                     # 水の中には建てない
    up = D[k0]
    top, low = float(gr.max()), float(gr.min())
    fr = _frame(up, F[2])
    c, s_ = np.cos(yaw), np.sin(yaw)
    right = fr[0] * c + fr[2] * s_
    front = np.cross(right, up)
    rot = np.stack([right, up, front], axis=1)
    foot = pc + up * top
    # 土台：足もとの範囲の下を坂の低い所まで埋める（コンクリート）
    depth = int(np.ceil((top - low) / s)) + 1
    cells = {v[0] for v in V}
    xs = [c_[0] for c_ in cells]
    zs = [c_[2] for c_ in cells]
    found = [((i, -j, k), "concrete", None) for i in range(min(xs), max(xs) + 1)
             for k in range(min(zs), max(zs) + 1) for j in range(1, depth + 1)]
    V = buildkit.anchored(found + list(V), 3)
    org = foot - right * (0.5 * s) - front * (0.5 * s)
    return V, org - city.x, rot, s, name


# ---------------------------------------------------------------------------
# 場所を選ぶ
# ---------------------------------------------------------------------------
def _probe(pl, dirs, ground_fn):
    """候補地ごとの (高さ (n,), 起伏 (n,), まわりに水 (n,), 真ん中が水 (n,), 気温 (n,), 湿り気 (n,))。まとめて計算"""
    from .cities import _frame
    n = len(dirs)
    pts = []
    for d in dirs:
        f = _frame(d)
        pts.append(d)
        for a in np.linspace(0, 2 * np.pi, 8, endpoint=False):
            for rr in (60.0, 150.0):
                pts.append(d + (f[0] * np.cos(a) + f[2] * np.sin(a)) * (rr / pl.radius))
    pts = np.array(pts)
    pts /= np.linalg.norm(pts, axis=1)[:, None]
    r, w = ground_fn(pl, pts)
    r, w = r.reshape(n, 17), w.reshape(n, 17)
    wet = (w > r) if pl.has_water else np.zeros_like(r, bool)
    h = r[:, 0] - pl.radius
    near = r[:, [0] + list(range(1, 17, 2))]                    # 真ん中と 60 m の輪
    relief = near.max(1) - near.min(1)
    try:
        cl = np.asarray(pl._nat().climate(np.asarray(dirs, float), h))
        t, m = cl[:, 0], cl[:, 1]
    except Exception:  # noqa: BLE001  地球型でない惑星
        t, m = np.full(n, 0.5), np.full(n, 0.5)
    return h, relief, wet.any(1), wet[:, [0] + list(range(1, 17, 2))].mean(1), t, m


def classify(pl, d, h, relief, wet_any, wet_mid, t, m, near_city):
    """候補地の種類（無理なら None）"""
    if wet_mid > 0.3 or t < -0.05:
        return None                                       # 水の上・雪と氷
    air = float(pl.defn.get("atmosphere_pressure", 101.3) or 0.0) > 0
    if not air:
        return "mine" if relief < 25 else None            # 空気の無い月・小惑星：鉱山だけ
    if wet_any and h < 20 and relief < 12:
        return "port"
    try:
        res = pl.resource_at(pl.center + d * (pl.radius + h - 0.5))
    except Exception:  # noqa: BLE001
        res = None
    if relief > 22 or h > 350 or res == "sulfur":
        if near_city and res == "sulfur" and relief < 22:
            return "industrial"
        return "mine" if relief < 45 else None
    if near_city:
        return "residential" if relief < 10 else None
    arid = (0.45 < t) and (m < 0.25)
    if relief < 6 and 0.3 < t < 0.85 and 0.3 < m < 0.8:
        return "farm"
    if relief < 14 and not arid:
        return "village"
    return None


def generate(manager, pl, pi, seed, spawn, used_names, ground_fn, rng_fn, City):
    """惑星 pl の集落（City のリスト）"""
    out = []
    if pl.radius < 3000:
        return out
    air = float(pl.defn.get("atmosphere_pressure", 101.3) or 0.0) > 0
    count = {k: 0 for k in KINDS}
    limit = {k: (v[1] if air else (4 if k == "mine" else 0)) for k, v in KINDS.items()}
    cities = [c for c in manager.cities if c.host is pl and c.kind == "surface"]
    placed = [((c.x - pl.center) / np.linalg.norm(c.x - pl.center)) for c in cities]
    up_s = (spawn - pl.center)
    up_s = up_s / max(np.linalg.norm(up_s), 1e-9)
    rng = rng_fn("settle", *seed)
    # 候補地：ほぼ等間隔（フィボナッチ球面）と、大きな街のまわり
    n = 900
    k = np.arange(n) + 0.5
    phi = np.arccos(1 - 2 * k / n)
    th = np.pi * (1 + 5 ** 0.5) * k + rng.uniform(0, 2 * np.pi)
    cand = [(dd, False) for dd in np.stack([np.cos(th) * np.sin(phi), np.cos(phi), np.sin(th) * np.sin(phi)], 1)]
    from .cities import _frame
    for c in cities:
        dc = (c.x - pl.center) / np.linalg.norm(c.x - pl.center)
        f = _frame(dc)
        for a in np.linspace(0, 2 * np.pi, 5, endpoint=False):
            rr = (c.radius + 260.0) / pl.radius
            v = dc + (f[0] * np.cos(a) + f[2] * np.sin(a)) * rr
            cand.append((v / np.linalg.norm(v), True))
    order = rng.permutation(len(cand))
    H, REL, WA, WM, T, M = _probe(pl, np.array([c_[0] for c_ in cand]), ground_fn)
    for oi in order:
        d, near = cand[oi]
        if np.arccos(np.clip(d @ up_s, -1, 1)) * pl.radius < 400.0:
            continue                                      # 出現地点のそばには作らない
        sep = MIN_SEP * (0.45 if near else 1.0)
        if len(placed) > len(cities):
            Q = np.array(placed[len(cities):])
            if float((Q @ d).max()) > np.cos(sep / pl.radius):
                continue
        if not near and cities and float((np.array(placed[:len(cities)]) @ d).max()) > np.cos(500.0 / pl.radius):
            continue
        st = classify(pl, d, H[oi], REL[oi], WA[oi], WM[oi], T[oi], M[oi], near)
        if st is None or count[st] >= limit[st]:
            continue
        srng = rng_fn("settle", *seed, int(oi))
        name = manager._name(srng, used_names, outpost=False).split("・")[-1].rstrip("0123456789区") + KINDS[st][0]
        if name in used_names:
            name += str(int(srng.integers(2, 99)))
        used_names.add(name)
        c = City(f"{pl.id}{pi}-s{int(oi)}", name, "surface", pl)
        c.stype = st
        try:
            build(c, pl, d, srng, st, ground_fn)
        except Exception:  # noqa: BLE001
            manager_log(f"集落 {name} を作れませんでした")
            continue
        c.region = manager._region(c, pl)
        c.home = pi
        out.append(c)
        placed.append(d)
        count[st] += 1
    return out


def manager_log(msg):
    from main import log
    log.get("city").warning(msg)

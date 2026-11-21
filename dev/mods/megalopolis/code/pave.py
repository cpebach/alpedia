"""大都市圏の舗装：街の道（アスファルト）・建物の区画（コンクリート）・場所どうしを結ぶ道路（線路の横）。

地表の色を塗り替えるだけ（地形の形は変えない）。塗った所には草木が生えない。惑星の地形（PlanetShape）の
set_pave_grid / set_pave_strip に渡す。計画（plan）から毎回同じに作るので、セーブには入れない。
"""
import numpy as np

from . import plan

CELL = 2.0                  # 街の舗装のマス（m）
PATCH = 4                   # 色むらの大きさ（マスの数）
ROAD_SIDE = 15.0            # 道路：線路の真ん中から道路の真ん中まで（m）
ROAD_HALF = 5.5             # 道路の幅の半分（m。片側 2 車線）
ROAD_EDGE = 2.0
ASPHALT = [(0.19, 0.19, 0.205), (0.215, 0.215, 0.23), (0.17, 0.17, 0.185)]
CONCRETE = [(0.50, 0.50, 0.49), (0.46, 0.46, 0.45)]
YARD = [(0.43, 0.42, 0.39), (0.39, 0.38, 0.355)]
PAL = ASPHALT + CONCRETE + YARD
CODE = {"asphalt": 1, "lot": 1 + len(ASPHALT), "yard": 1 + len(ASPHALT) + len(CONCRETE)}
NVAR = {"asphalt": len(ASPHALT), "lot": len(CONCRETE), "yard": len(YARD)}


def site_grid(site, seed):
    """場所の舗装のマス目：(x0, z0, codes (nx, nz) uint8)"""
    bl = site.get("blocks") or []
    if not bl:
        return None
    B = np.array([(b[0], b[1], b[2], b[3], b[5][0], b[5][1]) for b in bl], float)
    # 道（区画のまわり。道の幅の半分ずつ広げる）
    lo = np.stack([B[:, 0] - B[:, 2] / 2 - B[:, 4] / 2, B[:, 1] - B[:, 3] / 2 - B[:, 5] / 2], 1)
    hi = np.stack([B[:, 0] + B[:, 2] / 2 + B[:, 4] / 2, B[:, 1] + B[:, 3] / 2 + B[:, 5] / 2], 1)
    x0, z0 = np.floor(lo.min(0) / CELL) * CELL - CELL
    nx = int(np.ceil((hi[:, 0].max() - x0) / CELL)) + 2
    nz = int(np.ceil((hi[:, 1].max() - z0) / CELL)) + 2
    r = plan.rng("mega-pave", *seed, site["id"])
    # 色むら（PATCH マスごと）
    px, pz = nx // PATCH + 1, nz // PATCH + 1
    var = {k: (CODE[k] + np.kron(r.integers(0, n, (px, pz)), np.ones((PATCH, PATCH), int))[:nx, :nz]).astype(np.uint8)
           for k, n in NVAR.items()}
    codes = np.zeros((nx, nz), np.uint8)

    def rect(l, h):
        i0, k0 = int(round((l[0] - x0) / CELL)), int(round((l[1] - z0) / CELL))
        i1, k1 = int(round((h[0] - x0) / CELL)), int(round((h[1] - z0) / CELL))
        return slice(max(i0, 0), max(i1, 0)), slice(max(k0, 0), max(k1, 0))

    road = np.zeros((nx, nz), bool)
    for l, h in zip(lo, hi):
        road[rect(l, h)] = True
    codes[road] = var["asphalt"][road]
    for b in bl:
        x, z, w, d, kind = b[:5]
        sl = rect((x - w / 2, z - d / 2), (x + w / 2, z + d / 2))
        if kind == "park":
            codes[sl] = 0
        else:
            codes[sl] = var[kind][sl]
    return x0, z0, codes


def apply(pl, mplan, seed):
    """惑星の地形に舗装を置く（前の物は外す）"""
    nat = pl._nat()
    if not hasattr(nat, "set_pave_grid"):
        return False
    nat.clear_pave("mega:")
    for s in mplan["sites"]:
        g = site_grid(s, seed)
        if g is None:
            continue
        x0, z0, codes = g
        F = s["F"]
        nat.set_pave_grid(f"mega:site:{s['id']}", [float(v) for v in s["d"]], [float(v) for v in F[0]], [float(v) for v in F[2]],
                          float(x0), float(z0), CELL, int(codes.shape[0]), int(codes.shape[1]), codes.tobytes(), PAL)
    for ln in mplan["lines"]:
        Pr = plan.offset_track(ln["P"], ln["U"], ROAD_SIDE)
        nat.set_pave_strip(f"mega:road:{ln['id']}", plan.unit(Pr), ROAD_HALF, ROAD_EDGE, ASPHALT[0])
    pl.version += 1                                    # 草木・地表を描き直させる
    return True

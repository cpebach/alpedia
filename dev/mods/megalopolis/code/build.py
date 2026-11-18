"""大都市圏のブロックの形：建物・線路の区間・駅・電車の車両。

どれも (セル, 素材, 機器) のリスト。建物のセルは足もとの真ん中が (0, 0, 0)（x：横、y：上、z：前）。
"""
import numpy as np

from main import buildkit

# 様式ごと：(壁, 窓, 屋根)
TOWER_SKINS = [("glass", "steel", "hull_dark"), ("glass", "hull_navy", "hull_black"), ("glass", "aluminum", "hull_dark"),
               ("glass", "hull_teal", "steel")]
OFFICE_WALLS = ["concrete", "hull_cream", "aluminum", "hull_white", "clay"]
APT_WALLS = ["hull_cream", "concrete", "clay", "hull_white", "hull_orange"]
LINE_COLORS = ["hull_teal", "hull_red", "hull_yellow", "hull_green", "hull_blue", "hull_orange"]


def _cell(style, h):
    if style == "tower":
        return 2.0 if h <= 80 else (3.0 if h <= 200 else 4.0)
    if style in ("tank", "chimney"):
        return 1.0 if h < 80 else 2.0
    return 2.0


def _shell(nx, ny, nz):
    """箱の殻のセル：(x, y, z) の配列（足もとの真ん中が 0）と、面の種類（側面か・角か・床か・屋根か）"""
    x0, z0 = -(nx // 2), -(nz // 2)
    X, Y, Z = np.meshgrid(np.arange(nx), np.arange(ny), np.arange(nz), indexing="ij")
    side_x = (X == 0) | (X == nx - 1)
    side_z = (Z == 0) | (Z == nz - 1)
    floor, roof = Y == 0, Y == ny - 1
    keep = side_x | side_z | floor | roof
    X, Y, Z = X[keep], Y[keep], Z[keep]
    sx, sz = side_x[keep], side_z[keep]
    return X + x0, Y, Z + z0, sx, sz, Y == 0, Y == ny - 1, X, Z


def building(b):
    """建物 b → (ボクセル, セルの大きさ, 名前)"""
    r = np.random.default_rng(b["seed"])
    st, h = b["style"], float(b["h"])
    s = _cell(st, h)
    nx, nz, ny = max(3, int(round(b["w"] / s))), max(3, int(round(b["d"] / s))), max(2, int(round(h / s)))
    if st in ("tank", "chimney"):
        k = buildkit.Kit()
        if st == "tank":
            rad = max(2, nx // 2)
            k.cylinder(0, 0, 0, rad, ny, "hull_white", True)
            k.dome(0, ny, 0, rad, "hull_white")
            return k.voxels(), s, "タンク"
        rad = max(1, nx // 2)
        k.cylinder(0, 0, 0, rad, ny, "concrete", True)
        k.paint("hull_red", -rad - 1, ny - 6, -rad - 1, rad + 1, ny - 4, rad + 1)
        return k.voxels(), s, "煙突"
    x, y, z, sx, sz, floor, roof, ix, iz = _shell(nx, ny, nz)
    wall_ = sx | sz
    corner = sx & sz
    mat = np.empty(len(x), object)
    if st == "tower":
        wall, frame_, rf = TOWER_SKINS[int(r.integers(len(TOWER_SKINS)))]
        mat[:] = wall
        col = (sz & (ix % 4 == 0)) | (sx & (iz % 4 == 0)) | corner
        mat[wall_ & (col | (y % 4 == 0))] = frame_
        name = "高層ビル"
    elif st in ("office", "apartment"):
        wall = (OFFICE_WALLS if st == "office" else APT_WALLS)[int(r.integers(5))]
        rf = "concrete" if st == "office" else "hull_dark"
        mat[:] = wall
        if st == "office":
            win = wall_ & ~corner & (y % 2 == 1) & (y < ny - 1)
        else:
            win = wall_ & ~corner & (y % 2 == 1) & (y < ny - 1) & (((ix % 2) == 1) & sz | ((iz % 2) == 1) & sx)
        mat[win] = "glass"
        name = "ビル" if st == "office" else "集合住宅"
    elif st == "factory":
        mat[:] = "steel"
        rf = "hull_dark"
        mat[wall_ & ~corner & (y == ny - 2) & (((ix % 3) == 1) & sz | ((iz % 3) == 1) & sx)] = "glass"
        name = "工場"
    else:
        mat[:] = ["hull_navy", "hull_dark", "aluminum"][int(r.integers(3))]
        rf = "hull_dark"
        name = "倉庫"
    mat[floor] = "concrete"
    mat[roof] = rf
    # 入り口・搬入口（前の面の真ん中）
    door_h = 1 if s >= 2 else 2
    if st in ("factory", "warehouse"):
        door_h = min(2, ny - 2)
    hole = (z == z.max()) & (np.abs(x) <= (1 if st != "warehouse" else 3)) & (y >= 1) & (y <= door_h) & ~roof
    V = [((int(a), int(b_), int(c)), m, None) for a, b_, c, m, hh in zip(x, y, z, mat, hole) if not hh]
    if st == "tower" and nx > 6 and nz > 6:
        V += [((i, j, k_), "steel", None) for i in (-1, 0, 1) for j in (ny, ny + 1) for k_ in (-1, 0, 1)]
    if st == "factory":
        for cx in (x.min() + 3, x.max() - 3):
            hgt = int(r.integers(4, 10))
            V += [((int(cx), ny + j, int(z.min()) + 3), "concrete", None) for j in range(hgt)]
    return V, s, name


def realize(b, site, pl):
    """建物 b を建てる：(ボクセル, 原点（世界）, 回転, セルの大きさ, 名前)。水の中・急すぎる坂なら None"""
    from .plan import frame, ground, unit
    V, s, name = building(b)
    up = np.asarray(b["dir"], float)
    F = frame(up, site["F"][2])
    right, front = F[0], F[2]
    hw, hd = b["w"] / 2, b["d"] / 2
    pc = np.asarray(pl.center, float)
    R = pl.radius
    offs = [(dx, dz) for dx in (-hw, 0, hw) for dz in (-hd, 0, hd)]
    D = unit(np.array([up * R + right * dx + front * dz for dx, dz in offs]))
    g, w = ground(pl, D)
    if float(w[4]) > float(g[4]) + 0.3:
        return None
    top, low = float(g.max()), float(g.min())
    if top - low > 30.0:
        return None
    # 土台：足もとの外周の下を、坂の低い所まで埋める（中は空ける）
    depth = int(np.ceil((top - low) / s)) + 1
    cells = {v[0] for v in V}
    xs = [c[0] for c in cells]
    zs = [c[2] for c in cells]
    xa, xb, za, zb = min(xs), max(xs), min(zs), max(zs)
    found = []
    for i in range(xa, xb + 1):
        for k in range(za, zb + 1):
            if i in (xa, xb) or k in (za, zb):
                found += [((i, -j, k), "concrete", None) for j in range(1, depth + 1)]
    V = buildkit.anchored(found + list(V), 3)
    rot = np.stack([right, up, front], axis=1)
    foot = pc + up * top
    org = foot - right * (0.5 * s) - front * (0.5 * s)
    return V, org, rot, s, name


# ---------------------------------------------------------------------------
# 線路の区間（複線。セル 0.5 m）
# ---------------------------------------------------------------------------
SEG_S = 0.5
SEG_W = 18                 # 幅 9 m
RAIL_X = (3, 7, 10, 14)    # レールのセル（真ん中から ±2.25 m の線路の左右 ±1 m）
RAIL_TOP = 1.5             # 区間の原点からレールの上面まで（m）


def segment(n_len, pier_depth=0.0, ballast="rock"):
    """長さ n_len セルの区間。pier_depth > 0 なら始まりに橋脚（m）"""
    V = {}
    for i in range(n_len):
        for x in range(SEG_W):
            V[(x, 0, i)] = ballast
        if i % 2 == 0:
            for x in range(1, SEG_W - 1):
                V[(x, 1, i)] = "wood"
        for x in RAIL_X:
            V[(x, 2, i)] = "steel"
    if pier_depth > 0.5:
        n = int(np.ceil(pier_depth / SEG_S))
        for j in range(1, n + 1):
            for x in range(6, 12):
                for i in range(0, 3):
                    V[(x, -j, i)] = "concrete"
    return [(c, m, None) for c, m in V.items()]


def segment_place(P0, P1, U0):
    """世界の点 P0 → P1（真ん中・レールの上面）の区間の (原点, 回転, 長さのセル数)"""
    from .plan import unit
    fwd = unit(P1 - P0)
    up = unit(U0 - fwd * (U0 @ fwd))
    right = np.cross(up, fwd)
    n = int(np.ceil(np.linalg.norm(P1 - P0) / SEG_S)) + 1
    org = P0 - up * RAIL_TOP - right * (SEG_W * SEG_S / 2)
    return org, np.stack([right, up, fwd], axis=1), n


# ---------------------------------------------------------------------------
# 駅（線路の両側のホームと屋根。セル 1 m）
# ---------------------------------------------------------------------------
STATION_LEN = 120


def station():
    """ホームの上面がレールの上面より 1 m 上（車両の床の高さ）"""
    k = buildkit.Kit()
    L = STATION_LEN
    for side in (-1, 1):
        xa, xb = (4, 8) if side > 0 else (-8, -4)
        edge = xa if side > 0 else xb
        k.fill(xa, 0, 0, xb, 0, L - 1, "concrete")                        # ホーム
        k.paint("hull_yellow", edge, 0, 0, edge, 0, L - 1)                 # 点字ブロックの線
        x = (xb - 1) if side > 0 else (xa + 1)
        for z in range(4, L, 12):
            k.fill(x, 1, z, x, 4, z, "steel")                                 # 柱
        k.fill(xa, 5, 0, xb, 5, L - 1, "hull_dark")                           # 屋根
    # 線路の上をまたぐ通路
    m = L // 2
    k.fill(-8, 7, m - 2, 8, 7, m + 2, "concrete")
    k.fill(-8, 8, m - 2, 8, 8, m - 2, "glass")
    k.fill(-8, 8, m + 2, 8, 8, m + 2, "glass")
    for x in (-7, 7):
        k.fill(x, 1, m + 3, x, 6, m + 3, "steel")
    return k.voxels()


def station_place(P, U, T):
    """駅の真ん中の点 P（線路の真ん中・レールの上面）・上 U・線路の向き T → (原点, 回転)。セル (0,0,0) の下面がレールの上面"""
    from .plan import unit
    up = unit(U - T * (U @ T))
    right = np.cross(up, T)
    org = P - T * (STATION_LEN / 2) - right * 0.5 + up * 0.0
    return org, np.stack([right, up, T], axis=1)


# ---------------------------------------------------------------------------
# 電車の車両（セル 0.5 m。長さ 19 m・幅 3 m・高さ 4 m）
# ---------------------------------------------------------------------------
CAR_S = 0.5
CAR_N = (6, 9, 38)          # 幅・高さ（車輪の段を含む）・長さのセル数
CAR_LEN = CAR_N[2] * CAR_S
CAR_GAP = 1.0


def car(color, lead=False, tail=False):
    """車両。セル (0, 0, 0) の下面がレールの上面（車輪の段が y=0、床が y=1）"""
    nx, ny, nz = CAR_N
    V = {}
    doors = set()
    for zc in (8, nz - 10):
        for dz in range(2):
            doors.add(zc + dz)
    for z in range(nz):
        for x in range(nx):
            V[(x, 1, z)] = "steel"                      # 床
            V[(x, ny - 1, z)] = "hull_white"            # 屋根
        for y in range(2, ny - 1):
            for x in (0, nx - 1):
                if z in doors and y < ny - 2:
                    continue                            # 扉（あいている）
                if 4 <= y <= 5:
                    m = "glass"
                elif y <= 3:
                    m = color
                else:
                    m = "hull_white"
                V[(x, y, z)] = m
    for z in (0, nz - 1):
        for x in range(1, nx - 1):
            for y in range(2, ny - 1):
                front = (z == 0 and tail) or (z == nz - 1 and lead)
                V[(x, y, z)] = "glass" if (front and 4 <= y <= 6) else ("aluminum" if 1 < x < nx - 2 or y > 3 else color)
    # 台車と車輪
    for zc in (3, nz - 4):
        for z in (zc - 1, zc, zc + 1):
            for x in (0, nx - 1):
                V[(x, 0, z)] = "hull_black"
    # 座席（壁ぎわ。座れる）
    out = [(c, m, None) for c, m in V.items()]
    for z in range(12, nz - 12, 3):
        for x, d in ((1, 0), (nx - 2, 1)):
            out = [v for v in out if v[0] != (x, 2, z)]
            out.append(((x, 2, z), "steel", {"def": "seat", "dir": 4}))
    return out


def car_origin_local():
    """車両の原点（セル (0,0,0) の角）の、車両の真ん中・レールの上面からのずれ（右・上・前）"""
    nx, _, nz = CAR_N
    return np.array([-nx * CAR_S / 2, 0.0, -nz * CAR_S / 2])

"""ストーリーの舞台：地図の座標、景色の箱（建物・崖・街）、構築物（ボクセル）の置き場所。

- 地図の座標 (x, z) [m]：ガイアは北極（ワールドの +Y）が原点で、+X が右、+Z が前（奥）。
  月ルナは発着場が原点で、+Z が「月の北」（ワールドの +Y の方）。
- 景色の箱は惑星の地形（掘った跡は含まない）から毎回同じ形に作る（セーブには入らない）。
  ボクセルではないので軽い。プレイヤー・構築物は当たって上に乗れる（街 MOD の建物と同じ仕組み）。
- 構築物は最初に読み込んだときに1回だけ作る（addon.build_world）。そのあとはふつうの構築物としてセーブされる。
"""
import numpy as np

from main.scenery import (CONC, CYAN, DARK, GLASS, GREEN, LIME, METAL, MOON_G, NAVY, OFF, ORANGE, PINK,  # noqa: F401
                          RED, REDL, ROCK, ROCK2, ROCK3, WARM, WHITE, Map, Scenery, unit as _unit)


def terra_map(planet, craters=()):
    return Map(planet, (0, 1, 0), (0, 0, 1), craters)


# ---------------------------------------------------------------------------
# ガイア：ポッドの不時着地点 → 尾根の谷 → 観測基地 → ルミナス号の墜落地点
# ---------------------------------------------------------------------------
Z_RIDGE = (20.0, 38.0)        # 尾根（谷）の奥行き
NOTCH = 3.5                   # 谷の半分の幅
Z_LEDGE = 37.0                # 谷の出口の段差
OUTPOST = 70.0                # 観測基地の中心
SHIP = (2.0, 150.0)           # ルミナス号の墜落地点
LAUNCH = (26.0, 176.0)        # エピローグ：旅立ちのロケットの発射台
LPLAZA = (14.0, 168.0)        # 　　　　　 みんなが集まる所（発射台の手前）

# 構築物の置き場所（地図の x, z, 向き（度）, 地面からの高さ）
PLACES = {
    "pod": (1.4, 1.0, 20.0, 0.3),
    "beacon": (3.5, 10.0, 180.0, 0.05),
    "rock1": (-2.4, 28.0, 0.0, 0.05),
    "rock2": (-0.3, 27.4, 0.0, 0.05),
    "rock3": (1.6, 28.4, 0.0, 0.05),
    "power": (-11.0, 52.0, 0.0, 0.05),
    "model": (-16.0, 64.0, 0.0, 0.05),
    "task": (-9.0, 64.0, 0.0, 0.05),
    "console": (8.0, 66.0, 0.0, 0.05),
    "drone": (12.0, 72.0, 0.0, 0.05),
    "gate": (-16.6, 93.5, 0.0, 0.05),
    "bike": (-13.0, 85.0, 0.0, 0.05),
    "ship": (SHIP[0], SHIP[1], 0.0, 0.05),
    "sign_base": (4.0, 47.0, 180.0, 0.05),
    "crane": (20.0, 56.0, 0.0, 0.05),
    "factory": (-31.0, 69.0, 90.0, 0.05),
}
PAD_HALF = 6.0                # 月の発着場の半分の幅（場所の MOD「lunaport」と同じ）
CRANE = (20.0, 56.0)          # 搬入クレーン（第3章）
FACTORY = (-31.0, 69.0)       # 工場（第6章）
QUARRY_ORE = (-42.0, 60.0)    # 採掘場：灰色の鉱石
QUARRY_SULFUR = (-42.0, 79.0) # 採掘場：黄色い硫黄鉱石
QUARRY_R = 6.5
RING = (16.0, 76.0, 15.0)     # ドローンでくぐる輪（x, z, 高さ）
RING_R = 2.4


def terra_craters(m):
    """不時着の跡（球の形に地面を削る）：(惑星の中心から見た x, y, z, 半径) のリスト。"""
    out = []

    def cr(x, z, depth, r):
        p, up = m.ground(x, z)
        c = p + up * (r - depth) - m.pl.center
        out.append([float(c[0]), float(c[1]), float(c[2]), float(r)])
    cr(2.1, 1.7, 1.1, 4.0)                        # ポッド
    for k, z in enumerate((-6.0, -11.0, -15.0)):  # ポッドが滑った跡
        cr(0.3 - k * 0.2, z, 0.6 - k * 0.15, 2.2)
    cr(SHIP[0], SHIP[1], 1.6, 16.0)               # ルミナス号（広く浅いくぼ地）
    for k in range(6):                            # 船が滑った跡（溝）
        cr(SHIP[0] - 0.5 - k * 0.6, SHIP[1] - 9.0 - k * 4.5, 1.2 - k * 0.15, 3.6)
    return out


def _rng(seed):
    return np.random.default_rng(seed)


def build_terra(planet):
    m = terra_map(planet, terra_craters(terra_map(planet)))
    s = Scenery("terra", m)
    rng = _rng(11)
    # --- ポッドの不時着地点 ---
    s.box(-3.5, 4.5, 3.2, 0.12, 2.4, yaw=25, color=ORANGE, ground=True, sink=0.05)          # パラシュート
    s.box(-2.2, 6.0, 2.0, 0.1, 1.6, yaw=-10, color=WHITE, ground=True, sink=0.05)
    for k in range(5):                                                                   # 岩
        a = rng.uniform(0, 2 * np.pi)
        r = rng.uniform(7, 12)
        sz = rng.uniform(0.8, 1.6)
        s.box(np.cos(a) * r, np.sin(a) * r * 0.6 - 2, sz, sz * 0.8, sz * 1.1, yaw=rng.uniform(0, 90),
              color=ROCK3, pitch=rng.uniform(-8, 8))
    # --- 尾根（左右に長く続く岩の壁）と、真ん中の谷 ---
    z0, z1 = Z_RIDGE
    zc, dz = (z0 + z1) / 2, z1 - z0
    for side in (-1, 1):
        x = NOTCH + 1.6
        k = 0
        while x < 75:
            w = rng.uniform(6.0, 9.0) if k else 3.2
            h = rng.uniform(7.0, 11.0) if k else 8.5
            cx = side * (x + w / 2)
            col = (ROCK, ROCK2, ROCK3)[k % 3]
            s.box(cx, zc + rng.uniform(-1.5, 1.5) * (k > 0), w + 0.6, h, dz + rng.uniform(-2, 3) * (k > 0),
                  yaw=rng.uniform(-6, 6) * (k > 0), color=col)
            if k:          # 上の段（でこぼこに）
                s.box(cx + rng.uniform(-1, 1), zc + rng.uniform(-3, 3), w * 0.6, rng.uniform(2, 4), dz * 0.5,
                      y0=h - 0.3, yaw=rng.uniform(-20, 20), color=ROCK2)
            x += w
            k += 1
        # 谷の内側の壁（でこぼこ）
        for z in np.arange(z0 + 1, z1, 3.0):
            s.box(side * (NOTCH + 0.6), z, 1.2, rng.uniform(5.5, 7.5), 3.2, yaw=rng.uniform(-8, 8), color=ROCK3)
    # 谷の出口の段差（ジェットパックで越える）
    s.box(0.0, Z_LEDGE, NOTCH * 2 + 0.6, 3.2, 1.6, color=ROCK2)
    s.box(0.0, Z_LEDGE + 1.6, NOTCH * 2 + 0.6, 3.2, 1.6, color=ROCK)
    s.point("ledge", 0.0, Z_LEDGE + 0.8, 3.4)
    s.point("rocks", 0.0, 28.0, 1.0)
    # --- 観測基地 ---
    zb = OUTPOST
    for x, z, w, d in ((0.0, 48.5, 3.0, 9.0), (0.0, 60.0, 3.0, 14.0), (0.0, 79.0, 3.0, 24.0),
                       (-8.0, 62.0, 13.0, 2.4), (8.0, 62.0, 13.0, 2.4), (-7.0, 92.0, 14.0, 3.0),
                       (0.0, 118.0, 2.6, 30.0)):
        s.box(x, z, w, 0.18, d, color=CONC, sink=0.25)                 # 道
    # 本館はブロック（buildings.main_hall）。窓の明かり・扉の上の明かり・アンテナの灯（発電を始めると点く）
    for k in range(4):
        s.box(6.2 + k * 2.4, 48.42, 1.4, 1.1, 0.06, y0=2.4, color=OFF, glow=WARM, group="base", collide=False)
    s.box(10.0, 48.42, 1.4, 0.2, 0.06, y0=2.05, color=OFF, glow=(0.95, 0.75, 0.35), group="base", collide=False)
    s.box(12.25, 53.75, 0.5, 0.5, 0.5, y0=9.0, color=OFF, glow=REDL, group="base", blink=1.2, collide=False)
    s.box(10.0, 48.42, 6.0, 0.6, 0.1, y0=3.6, color=OFF, glow=CYAN, group="base")             # 看板の光
    # 電源設備の小屋・試験台の作業小屋はブロック（buildings）。作業小屋のはりの下の明かり
    s.box(-12.5, 62.25, 11.0, 0.12, 0.3, y0=4.85, color=OFF, glow=WARM, group="base", collide=False)
    # ドローン発着場
    s.box(12.0, 72.0, 4.0, 0.15, 4.0, color=DARK, sink=0.2)
    for x, z in ((10.1, 70.1), (13.9, 70.1), (10.1, 73.9), (13.9, 73.9)):
        s.box(x, z, 0.3, 0.12, 0.3, y0=0.15, color=OFF, glow=LIME, group="base", collide=False)
    # 電波塔はブロック（buildings.radio_tower）。上の灯と、ドローンでくぐる輪
    tx, tz = 20.0, 80.0
    s.box(tx + 0.25, tz + 0.25, 0.6, 0.6, 0.6, y0=24.0, color=OFF, glow=REDL, blink=1.0, collide=False)
    rx, rz, ry = RING
    for k in range(24):
        a = 2 * np.pi * k / 24
        s.box(rx + np.cos(a) * RING_R, rz + np.sin(a) * RING_R, 0.7, 0.3, 0.3, y0=ry, yaw=-np.degrees(a) + 90,
              color=OFF, glow=CYAN, collide=False, blink=2.0, ground=True)
    s.point("ring", rx, rz, ry + 0.1)
    # 格納庫（正面は +Z の側。入口に遮断機）：壁と屋根はブロック（buildings.hangar）。床と入口の明かり
    hx, hz = -13.0, 86.0
    s.box(hx, hz + 6.4, 8.0, 0.25, 0.1, y0=4.6, color=OFF, glow=(1.0, 0.75, 0.2), group="base", collide=False)
    s.box(hx, hz, 8.0, 0.1, 11.0, color=(0.40, 0.42, 0.45), sink=0.2)
    # 遮断機の横の柵はブロック（decorations）
    # 街灯の明かり（柱はブロック：decorations。電源が入ると点く）。ソーラーパネルもブロック
    for x, z in LAMPS:
        s.box(x, z, 0.45, 0.3, 0.45, y0=(LAMP_H + 1) * 0.5 + 0.02, color=OFF, glow=WARM, group="base", collide=False)
    s.point("ring_view", 12.0, 66.0, 1.6)
    # 本館の古い端末（第2章：ミカガの声がする）と、壁の「N」の印（N 財団の旧施設）
    s.box(13.6, 48.18, 1.2, 1.4, 0.5, color=(0.25, 0.3, 0.28), glow=(0.35, 1.0, 0.45), group="base", tag="terminal")
    s.point("terminal", 13.6, 47.0, 1.0)
    for k, (dx, dy) in enumerate(((0.0, 0.0), (0.0, 0.6), (0.0, 1.2), (0.0, 1.8), (0.35, 1.5), (0.7, 1.0), (1.05, 0.5),
                                  (1.4, 0.0), (1.4, 0.6), (1.4, 1.2), (1.4, 1.8))):
        s.box(14.6 - dx, 55.55, 0.3, 0.6, 0.06, y0=1.4 + dy, color=(0.3, 0.3, 0.32))
    # 搬入クレーンの置き場（第3章）：倉庫と荷台
    cx, cz = CRANE
    s.box(cx, cz, 9.0, 0.18, 9.0, color=CONC, sink=0.25)
    # 倉庫はブロック（buildings.crane_warehouse）。扉の上の明かり
    s.box(cx + 6.5, cz - 0.08, 2.6, 0.2, 0.1, y0=2.6, color=OFF, glow=WARM, group="base", collide=False)
    s.box(cx - 3.0, cz + 5.0, 3.0, 0.9, 2.0, color=(0.30, 0.42, 0.58))                       # 荷台
    s.box(cx - 3.0, cz + 5.0, 3.2, 0.12, 2.2, y0=0.9, color=OFF, glow=CYAN, group="base", collide=False)
    s.point("crane", cx, cz - 4.0, 1.0)
    # 工場（第6章）の屋根と、採掘場の露頭
    fx, fz = FACTORY
    # 工場の屋根と柱はブロック（buildings.factory_roof）
    s.box(fx + 3.7, fz, 0.15, 1.2, 1.6, y0=1.2, color=(0.3, 0.32, 0.3), glow=(0.95, 0.85, 0.6), tag="photo")
    s.point("photo", fx + 4.6, fz, 1.0)
    s.box(-10.0 - 9.0, 69.0, 18.0, 0.16, 2.6, color=CONC, sink=0.25)                         # 工場への道
    for (qx, qz), col, tag in ((QUARRY_ORE, (0.42, 0.40, 0.40), "quarry_ore"),
                               (QUARRY_SULFUR, (0.86, 0.78, 0.2), "quarry_sulfur")):
        for k in range(7):
            a = 2 * np.pi * k / 7 + rng.uniform(-0.3, 0.3)
            r = QUARRY_R + rng.uniform(0.0, 1.5)
            sz = rng.uniform(1.0, 2.2)
            s.box(qx + np.cos(a) * r, qz + np.sin(a) * r, sz, sz * 0.8, sz * 1.2, yaw=rng.uniform(0, 90), color=col,
                  pitch=rng.uniform(-10, 10))
        s.point(tag, qx, qz, 0.5)                    # 立て札はブロック（decorations）
    # --- ルミナス号の墜落地点：飛び散った破片と目印の旗はブロック（decorations） ---
    # --- エピローグ：発射台と、みんなが集まる広場 ---
    lx, lz = LAUNCH
    s.box(lx, lz, 9.0, 0.3, 9.0, color=CONC, sink=0.3)                               # 発射台
    for k in range(4):
        a = np.pi / 4 + k * np.pi / 2
        s.box(lx + np.cos(a) * 4.0, lz + np.sin(a) * 4.0, 0.6, 0.12, 0.6, y0=0.3, color=OFF, glow=REDL,
              collide=False, blink=1.2)
    gx = lx + 3.4                                                    # 整備塔はブロック（buildings.service_tower）
    s.box(gx - 1.15, lz, 0.5, 0.12, 0.6, y0=5.4, color=METAL)                              # 乗り口への橋
    s.box(gx, lz - 1.25, 1.0, 1.3, 0.12, y0=0.4, color=DARK, glow=LIME, tag="rocket")    # 乗り口の操作盤
    s.point("rocket", gx, lz - 1.6, 1.0)
    px, pz = LPLAZA
    s.box(px, pz, 12.0, 0.16, 9.0, color=(0.55, 0.52, 0.46), sink=0.25)                  # 広場
    # 旗・机と荷物はブロック（decorations）
    s.point("lplaza", px, pz, 1.0)
    return s.finish()


# ---------------------------------------------------------------------------
# アンカー：動かない構築物は、足もとの下にアンカーのブロックを足して地面に埋める（惑星に固定・物理計算なし）
# ---------------------------------------------------------------------------
from main.buildkit import ANCHOR, _N6, anchor_cells, anchored, anchored_inside, root_cells  # noqa: E402,F401


def without_base(V):
    """岩の台（倒れないための重し）を外して1段下げたもの。アンカーで固定するので台はいらない（軽くなる）。"""
    rest = [(c, m, d) for c, m, d in V if not (c[1] == 0 and m == "rock" and not d)]
    return [((c[0], c[1] - 1, c[2]), m, d) for c, m, d in rest]


# ---------------------------------------------------------------------------
# まわりの飾り（ブロック。アンカーで地面に固定）
# ---------------------------------------------------------------------------
LAMPS = ((2.2, 50.0), (-2.2, 58.0), (2.2, 66.0), (-2.2, 74.0), (2.2, 82.0), (-2.2, 90.0),
         (1.8, 104.0), (-1.8, 118.0), (1.8, 132.0))
LAMP_H = 7                       # 街灯の柱の高さ（ブロック）。明かりは景色の箱（電源が入ると点く）


def lamp_voxels():
    return [((0, y, 0), "steel", None) for y in range(LAMP_H)] + [((0, LAMP_H, 0), "hull_dark", None)]


def solar_stand_voxels():
    """ソーラーパネルの台：柱 2 本と、太陽光パネル 3 枚（本物の機器）。"""
    V = [((x, y, 0), "steel", None) for x in (-1, 1) for y in (0, 1)]
    V += [((x, 2, 0), "glass", {"def": "solar_panel", "dir": 2, "name": "太陽光パネル"}) for x in (-1, 0, 1)]
    V += [((x, 2, 1), "solar_navy", None) for x in (-1, 0, 1)]
    return V


def flag_voxels(cloth, h=6, w=2):
    """旗：柱と、上の布（w×2）。"""
    V = [((0, y, 0), "steel", None) for y in range(h)]
    V += [((x, y, 0), cloth, None) for x in range(1, w + 1) for y in (h - 2, h - 1)]
    return V


def signpost_voxels(board):
    """立て札：柱 3 段と板 3×2。"""
    V = [((0, y, 0), "steel", None) for y in range(3)]
    V += [((x, y, 0), board, None) for x in (-1, 0, 1) for y in (3, 4)]
    return V


def debris_voxels(seed):
    """ルミナス号の破片（地面に刺さったまま）。下の1つがアンカー。"""
    rng = _rng(seed)
    mat = ("hull_red", "hull_white", "hull_dark")[seed % 3]
    V = [((0, 0, 0), mat, None)]
    for k in range(int(rng.integers(1, 4))):
        c = V[-1][0]
        n = _N6[int(rng.choice([0, 1, 2, 4, 5]))]
        nb = (c[0] + n[0], max(0, c[1] + n[1]), c[2] + n[2])
        if all(nb != v[0] for v in V):
            V.append((nb, mat, None))
    return V + [((0, -1, 0), "steel", dict(ANCHOR))]


def fence_voxels(w=4, h=4):
    return [((x, y, 0), "bar_red" if (x + y) % 2 == 0 else "bar_white", None) for x in range(w) for y in range(h)]


def crates_voxels(n=3):
    """木箱の山（2×2×2 を n 個）。"""
    V = []
    spots = ((0, 0, 0), (2, 0, 0), (1, 2, 0), (0, 0, 2))[:n]
    for ox, oy, oz in spots:
        V += [((ox + x, oy + y, oz + z), "wood", None) for x in (0, 1) for y in (0, 1) for z in (0, 1)]
    return V


def barrels_voxels():
    """ドラム缶（赤・青、2 段）。"""
    return [((x, y, z), "hull_red" if (x + z) % 2 else "hull_blue", None) for x in (0, 1) for z in (0, 1) for y in (0, 1)]


def desk_voxels():
    """広場の机と荷物。"""
    V = [((x, 1, z), "wood", None) for x in range(4) for z in (0, 1)]
    V += [((x, 0, z), "wood", None) for x in (0, 3) for z in (0, 1)]
    V += [((1, 2, 0), "hull_orange", None), ((2, 2, 1), "hull_blue", None)]
    return V


def decorations():
    """[(役, 名前, ボクセル（アンカーつき）, 地図の場所 (x, z, 向き, 高さ), 地面に刺す)]。
    地面に刺す物（破片）は持ち上げずに置く。"""
    out = []
    for k, (x, z) in enumerate(LAMPS):
        out.append((f"deco_lamp{k}", "街灯", anchored(lamp_voxels()), (x, z, 0.0, 0.0), False))
    for k in range(4):
        out.append((f"deco_solar{k}", "ソーラーパネル", anchored(solar_stand_voxels(), 1), (-22.0, 48.0 + k * 4.0, 0.0, 0.0),
                    False))
    sx, sz = SHIP
    out.append(("deco_flag_ship", "目印の旗", anchored(flag_voxels("hull_orange", 4)), (sx + 7.5, sz - 3.0, 0.0, 0.0),
                False))
    rng = _rng(31)
    for k in range(9):
        a = rng.uniform(-1.2, 1.2) + (np.pi if k % 3 == 0 else 0)
        r = rng.uniform(5.5, 11)
        out.append((f"deco_debris{k}", "ルミナス号の破片", debris_voxels(k),
                    (sx + np.sin(a) * r, sz + np.cos(a) * r, float(rng.uniform(0, 180)), -0.2), True))
    for (qx, qz), board, tag in ((QUARRY_ORE, "rock", "ore"), (QUARRY_SULFUR, "hull_yellow", "sulfur")):
        out.append((f"deco_sign_{tag}", "採掘場の立て札", anchored(signpost_voxels(board)),
                    (qx, qz - QUARRY_R - 2.0, 0.0, 0.0), False))
    px, pz = LPLAZA
    for k, cloth in enumerate(("hull_orange", "hull_blue", "hull_pink", "hull_green", "hull_yellow")):
        out.append((f"deco_flag{k}", "広場の旗", anchored(flag_voxels(cloth)), (px - 5.0 + k * 2.5, pz + 5.2, 0.0, 0.0),
                    False))
    out.append(("deco_desk", "机と荷物", anchored(desk_voxels()), (px - 4.5, pz - 2.5, 0.0, 0.0), False))
    out.append(("deco_fence", "格納庫の柵", anchored(fence_voxels(), 1), (-10.0, 94.75, 0.0, 0.0), False))
    fx, fz = FACTORY
    out.append(("deco_crates_factory", "木箱", anchored(crates_voxels(3)), (fx - 5.5, fz + 6.0, 20.0, 0.0), False))
    out.append(("deco_barrels_factory", "ドラム缶", anchored(barrels_voxels(), 1), (fx + 5.0, fz - 6.0, 0.0, 0.0), False))
    cx, cz = CRANE
    out.append(("deco_crates_crane", "木箱", anchored(crates_voxels(4)), (cx + 6.5, cz - 4.0, -15.0, 0.0), False))
    return out


# ---------------------------------------------------------------------------
# 構築物（ボクセル）
# ---------------------------------------------------------------------------
# ルミナス号（5×5、前 = +Z、右 = +X、上 = +Y）。下の段の四隅と各辺の真ん中が推進機で、
# それぞれの真上にバルブ（燃料タンクとのあいだ。航法プログラムがバルブの開き具合で推力を変える）
SHIP_THRUSTERS = [(0, 0, 0), (4, 0, 0), (0, 0, 4), (4, 0, 4)]                 # 墜落でなくなった推進機
SHIP_THRUSTERS_OK = [(2, 0, 0), (0, 0, 2), (4, 0, 2), (2, 0, 4)]
SHIP_VALVES = [(x, 1, z) for x, _, z in SHIP_THRUSTERS + SHIP_THRUSTERS_OK]
SHIP_HOLES = [(0, 3, 2), (4, 3, 1), (1, 2, 0), (4, 2, 3), (3, 3, 0), (-1, 1, 2)]   # 墜落でなくなった外壁
SHIP_EMPTY_TANKS = [(1, 1, 1), (3, 1, 2), (2, 1, 3)]
SHIP_SEAT = (2, 2, 2)
SHIP_NAVI = (2, 2, 3)
# 給油：船の右の外の給油口（アクチュエーター。結合すると配管と同じく燃料が通る。発進のときに航法プログラムが切り離す）
FUEL_PIPE = (4, 2, 1)           # 燃料の輪（下の段の配管）から給油口までの配管
FUEL_PORT = (5, 2, 1)           # 給油口（+X 向き）
HOSE = [(6, 2, 1), (7, 2, 1)]   # 給油車から給油口までの配管（プレイヤーが置く。光る枠）
CART_PUMP = (9, 2, 1)
SHIP_TANK_CELLS = [(x, 1, z) for x in (1, 2, 3) for z in (1, 2, 3)]


def ship_voxels(broken=False):
    V = []
    for x in range(5):
        for z in range(5):
            if (x, 0, z) in SHIP_THRUSTERS or (x, 0, z) in SHIP_THRUSTERS_OK:
                V.append(((x, 0, z), "steel", {"def": "thruster", "dir": 2, "name": "推進機"}))
            else:
                V.append(((x, 0, z), "steel", None))          # 底板（配管ではない：推進機はバルブだけを通す）
            inner = 1 <= x <= 3 and 1 <= z <= 3
            if inner:
                st = {} if broken and (x, 1, z) in SHIP_EMPTY_TANKS else {"contents": {"fuel": 800.0}}
                V.append(((x, 1, z), "steel", {"def": "tank", "dir": 2, "name": "燃料タンク", "state": st}))
            elif (x, 1, z) in SHIP_VALVES:
                from .programs import SHIP_VALVE_NAMES
                V.append(((x, 1, z), "steel", {"def": "valve", "dir": 2, "name": SHIP_VALVE_NAMES[(x, z)],
                                               "state": {"open": 0.0, "set": 1.0}}))
            else:
                V.append(((x, 1, z), "steel", {"def": "pipe", "dir": 2}))
    deck = {(2, 2): {"def": "seat", "dir": 4, "name": "操縦席"},
            FUEL_PIPE[::2]: {"def": "pipe", "dir": 2, "name": "給油管"},
            (2, 3): {"def": "computer", "dir": 5, "name": "航法コンピューター",
                     "state": {"script": ""} if broken else
                     {"script": "moon.py", "blocks": "moon.blocks.json"}},
            (1, 1): {"def": "battery", "dir": 2, "name": "バッテリー"},
            (3, 1): {"def": "antenna", "dir": 2, "name": "アンテナ"}}
    for x in range(5):
        for z in range(5):
            d = deck.get((x, z))
            V.append(((x, 2, z), "steel" if d else ("hull_red" if z in (0, 4) else "hull_white"), d))
    V.append((FUEL_PORT, "steel", fuel_port_dev()))
    # 風よけ（前はガラス）と、後ろ・横の囲い
    V += [((x, 3, 4), "glass", None) for x in (1, 2, 3)]
    V += [((0, 3, 4), "hull_red", None), ((4, 3, 4), "hull_red", None)]
    V += [((x, 3, z), "hull_white", None) for x in (0, 4) for z in (1, 2, 3)]
    V += [((x, 3, 0), "hull_white", None) for x in (1, 2, 3)]
    V.append(((2, 4, 0), "hull_red", None))
    # 金の飾り（重さを変えないように、色だけ塗り替える：飛ぶプログラムがこの重さで作ってある）
    gold = {(0, 3, 4), (4, 3, 4), (2, 4, 0), (-1, 0, 2), (5, 0, 2)}
    V = [(c, "hull_gold" if c in gold and not d else m, d) for c, m, d in V]
    # ひれ
    V += [((-1, 0, 2), "hull_red", None), ((5, 0, 2), "hull_red", None), ((2, 0, -1), "hull_red", None),
          ((2, 0, 5), "hull_red", None), ((-1, 1, 2), "hull_white", None), ((5, 1, 2), "hull_white", None)]
    if broken:
        V = [v for v in V if v[0] not in SHIP_HOLES and v[0] not in SHIP_THRUSTERS]
    return V


def fuel_port_dev():
    return {"def": "actuator", "dir": 0, "name": "給油口",
            "state": {"preset": {"action": "attach", "amount": 0.0, "speed": 0.5}}}


def cart_voxels(legs):
    """給油車（船と同じ格子で作る：セルはルミナス号の座標）。配管の口 (8,2,1) → 燃料ポンプ (9,2,1)（-X 向きに送る）→
    燃料タンク4つ（満タン）。下は台と脚（legs：脚の (x, z) → いちばん下の段 y）。"""
    V = [((8, 2, 1), "steel", {"def": "pipe", "dir": 2, "name": "給油車の配管"}),
         (CART_PUMP, "steel", {"def": "fuel_pump", "dir": 1, "name": "燃料ポンプ", "state": {"enabled": False}})]
    for cell in ((10, 2, 0), (10, 2, 1), (10, 2, 2), (10, 3, 1)):
        V.append((cell, "steel", {"def": "tank", "dir": 2, "name": "給油車のタンク",
                                  "state": {"contents": {"fuel": 800.0}}}))
    for x in range(8, 12):
        for z in range(0, 3):
            V.append(((x, 1, z), "hull_orange", None))
    V += [((11, 2, z), "hull_orange", None) for z in (0, 2)] + [((8, 2, z), "hull_orange", None) for z in (0, 2)]
    for (x, z), y0 in legs.items():
        for y in range(min(y0, 0), 1):
            V.append(((x, y, z), "rubber" if y == min(y0, 0) else "steel", None))
    return V


CART_LEGS = [(8, 0), (11, 0), (8, 2), (11, 2)]

# 旅立ちのロケット（エピローグ）：3×3 の胴、上に操縦席のカプセル（窓はガラス）、左右にブースター（アクチュエーターで分離）
ROCKET_SEAT = (1, 10, 1)
ROCKET_PIVOT = (1.5, 0.0, 1.5)           # 胴の底の真ん中（打ち上げの軌道の基準）
BOOSTER_ACTS = [(-1, 3, 1), (3, 3, 1)]


def rocket_voxels():
    V = []
    for x in range(3):
        for z in range(3):
            edge = (x == 1) != (z == 1)
            center = x == 1 and z == 1
            V.append(((x, 0, z), "steel", {"def": "thruster", "dir": 2, "name": "主エンジン"}) if edge or center
                     else ((x, 0, z), "steel", None))
            for y in (1, 2, 3):
                V.append(((x, y, z), "steel", {"def": "tank", "dir": 2, "name": "燃料タンク",
                                               "state": {"contents": {"fuel": 800.0}}}))
            for y in range(4, 10):
                V.append(((x, y, z), "hull_red" if y == 6 else "hull_white", None))
            for y in (10, 11):
                if (x, y, z) == ROCKET_SEAT:
                    V.append((ROCKET_SEAT, "steel", {"def": "seat", "dir": 4, "name": "ロケットの操縦席"}))
                elif center:
                    continue                                   # 頭の上はあけておく
                elif edge:
                    V.append(((x, y, z), "glass", None))       # 窓
                else:
                    V.append(((x, y, z), "hull_white", None))
            V.append(((x, 12, z), "hull_white", None))
    V.append(((1, 13, 1), "hull_red", None))
    for (ax, ay, az), bx, d in ((BOOSTER_ACTS[0], -2, 1), (BOOSTER_ACTS[1], 4, 0)):
        V.append(((ax, ay, az), "steel", {"def": "actuator", "dir": d, "name": "ブースターの分離"}))
        V.append(((bx, 0, 1), "steel", {"def": "thruster", "dir": 2, "name": "ブースター"}))
        V += [((bx, y, 1), "hull_white", None) for y in range(1, 7)]
        V.append(((bx, 7, 1), "hull_red", None))
    return V


def pod_voxels():
    """脱出ポッド（3×3×3 の角を落とした形、オレンジと白、窓、記録の表示）。"""
    V = []
    for x in range(3):
        for y in range(3):
            for z in range(3):
                if sum(c in (0, 2) for c in (x, y, z)) == 3:
                    continue                                   # 角を落として丸く
                mat = "hull_orange" if y == 1 else "hull_white"
                if (x, y, z) == (1, 1, 2):
                    mat = "glass"
                if (x, y, z) == (1, 2, 0):
                    continue
                V.append(((x, y, z), mat, None))
    V.append(((1, 2, 0), "steel", {"def": "computer", "dir": 5, "name": "ポッドの記録", "state": {"display": {
        "text": "脱出ポッド 3号\n記録：ルミナス号から緊急分離\n着地：惑星ガイア", "face": 5, "width": 1.8,
        "color": [1.0, 0.8, 0.5], "background": [0.12, 0.05, 0.02, 0.92]}}}))
    return V


BEACON_SCRIPT = '''"""救難ビーコン：起動すると救難信号を送り、ランプを点滅させる。"""


def tick(api):
    on = int(api.time * 2) % 2 == 0
    api.display("救難ビーコン\\n" + ("● 送信中 SOS" if on else "○ 送信中 SOS"), width=1.4,
                color=(1.0, 0.35, 0.3) if on else (0.6, 0.25, 0.2), background=(0.08, 0.02, 0.02))
'''

POWER_SCRIPT = '''"""観測基地の電源の状態表示

発電用の推進機2つが背中合わせ（反対向き）なので、燃えても推力は打ち消し合い、電気だけが残る。
バルブの開き具合で燃料の量（＝発電の量）が変わる。閉じると火が消える。
"""


def tick(api):
    p = api.power
    valves = api.devices("valve")
    lit = sum(1 for t in api.devices("thruster") if t.lit)
    fuel = sum(t.contents.get("fuel", 0) for t in api.devices("tank"))
    o = valves[0].open if valves else 0
    v = f"開 {o:.0%}" if o > 0 else "閉"
    api.display(f"観測基地 電源\\nバルブ {v}　推進機 {lit}/2 燃焼中\\n発電 {p['production'] / 1000:.1f} kW\\n燃料 {fuel:.0f} kg",
                width=1.7, color=(1.0, 0.9, 0.5))
'''


def beacon_voxels():
    V = [((x, 0, z), "rock", None) for x in range(3) for z in range(3)]
    V += [((1, y, 1), "steel", None) for y in (1, 2)]
    V.append(((1, 3, 1), "steel", {"def": "battery", "dir": 2, "name": "バッテリー"}))
    V.append(((1, 4, 1), "steel", {"def": "computer", "dir": 4, "name": "救難ビーコン",
                                   "state": {"script": "beacon.py", "display": {
                                       "text": "救難ビーコン\n（E で起動）", "face": 4, "width": 1.4,
                                       "color": [1, 0.8, 0.6], "background": [0.08, 0.02, 0.02, 0.9]}}}))
    V.append(((1, 5, 1), "aluminum", {"def": "antenna", "dir": 2, "name": "アンテナ"}))
    V.append(((1, 6, 1), "hull_red", None))
    return V


def rock_voxels(seed, w=4, h=4, d=3):
    rng = _rng(seed)
    V = []
    for x in range(w):
        for y in range(h):
            for z in range(d):
                cx, cy, cz = (x - (w - 1) / 2) / w, y / h, (z - (d - 1) / 2) / d
                if cx * cx * 2 + cy * cy + cz * cz * 2 > 0.95 + rng.uniform(-0.15, 0.1):
                    continue
                V.append(((x, y, z), "rock", None))
    return V


def power_voxels():
    """観測基地の電源：タンク → バルブ（閉。E で 15% 開く）→ 配管 → 背中合わせの推進機2つ（発電用）
    ＋バッテリー（点火に使う分だけ）＋状態の表示。"""
    V = [((x, 0, z), "rock", None) for x in range(-1, 7) for z in range(-2, 3)]
    V += [((0, 1, 0), "steel", {"def": "tank", "dir": 2, "name": "燃料タンク", "state": {"contents": {"fuel": 800.0}}}),
          ((0, 2, 0), "steel", {"def": "tank", "dir": 2, "name": "燃料タンク", "state": {"contents": {"fuel": 800.0}}}),
          ((1, 1, 0), "steel", {"def": "valve", "dir": 0, "name": "バルブ", "state": {"open": 0.0, "set": 0.15}}),
          ((2, 1, 0), "steel", {"def": "pipe", "dir": 0}),
          ((3, 1, 0), "steel", {"def": "pipe", "dir": 0}),
          ((3, 1, 1), "steel", {"def": "thruster", "dir": 4, "name": "発電用の推進機"}),
          ((3, 1, -1), "steel", {"def": "thruster", "dir": 5, "name": "発電用の推進機"}),
          ((5, 1, 0), "steel", {"def": "battery", "dir": 2, "name": "基地のバッテリー", "state": {"charge": 4.0e5}}),
          ((5, 2, 0), "steel", {"def": "computer", "dir": 5, "name": "電源の表示",
                                "state": {"script": "power.py", "running": True}})]
    return V


def gate_voxels():
    """格納庫の遮断機：台・柱・バッテリー・モーター（出力は +Z の面）、その先の横棒（+X へ）。"""
    V = [((x, 0, z), "rock", None) for x in range(-1, 2) for z in range(-1, 2)]
    V += [((0, y, 0), "steel", None) for y in (1, 2)]
    V.append(((0, 3, 0), "steel", {"def": "battery", "dir": 2, "name": "バッテリー"}))
    V.append(((0, 3, 1), "steel", {"def": "actuator", "dir": 4, "name": "遮断機のモーター",
                                   "state": {"preset": {"action": "rotate", "amount": 90.0, "speed": 40.0}}}))
    V.append(((0, 3, 2), "bar_red", None))
    V += [((x, 3, 2), "bar_red" if (x // 2) % 2 == 0 else "bar_white", None) for x in range(1, 10)]
    return V


def bike_voxels():
    """ホバーバイク（前 = +Z）。四隅が推進機で、その真上にバルブ（制御のブロックプログラム bike.blocks.json が
    開き具合で推力を変える）。"""
    V = []
    corners = {(0, 0), (2, 0), (0, 4), (2, 4)}
    for x in range(3):
        for z in range(5):
            if (x, z) in corners:
                V.append(((x, 0, z), "steel", {"def": "thruster", "dir": 2, "name": "推進機"}))
            else:
                V.append(((x, 0, z), "hull_blue", None))
    names = {(0, 0): "後左", (2, 0): "後右", (0, 4): "前左", (2, 4): "前右"}     # 前 = +Z、右 = +X
    for x, z in corners:
        V.append(((x, 1, z), "steel", {"def": "valve", "dir": 2, "name": "バルブ " + names[(x, z)],
                                       "state": {"open": 0.0}}))
    V += [((1, 1, 0), "steel", {"def": "pipe", "dir": 2}),
          ((1, 1, 1), "steel", {"def": "tank", "dir": 2, "name": "燃料タンク", "state": {"contents": {"fuel": 800.0}}}),
          ((1, 1, 2), "steel", {"def": "seat", "dir": 4, "name": "バイクの座席"}),
          ((1, 1, 3), "steel", {"def": "tank", "dir": 2, "name": "燃料タンク", "state": {"contents": {"fuel": 800.0}}}),
          ((1, 1, 4), "steel", {"def": "pipe", "dir": 2}),
          ((1, 2, 0), "steel", {"def": "battery", "dir": 2, "name": "バッテリー"}),
          ((1, 2, 4), "steel", {"def": "computer", "dir": 5, "name": "バイクの制御",
                                "state": {"script": "bike.py", "blocks": "bike.blocks.json", "running": True}}),
          ((1, 2, 3), "glass", None)]
    return V


def sign_voxels(text, width=3.0, height=3, color=(1, 1, 1), bg=(0.05, 0.07, 0.12, 0.93)):
    v = [((x, 0, z), "rock", None) for x in range(-1, 2) for z in range(-1, 2)]
    v += [((0, y, 0), "steel", None) for y in range(1, height + 1)]
    v.append(((0, height + 1, 0), "steel", {
        "def": "computer", "dir": 5, "name": "看板",
        "state": {"display": {"text": text, "face": 5, "width": width, "color": list(color), "background": list(bg)}}}))
    return v


# ---------------------------------------------------------------------------
# 第3章：搬入クレーン（扉と旋回クレーンと、決まった手順のコンピューター）
# ---------------------------------------------------------------------------
CRANE_CRATE = [(4, 4, 1), (4, 4, 2), (5, 4, 1), (5, 4, 2)]      # クレーンの先の荷物（風で落ちる）


def crane_voxels():
    """台（7×7）・扉（モーターで回る板）・旋回クレーン（モーターの上に柱と腕、腕の先に荷物）・コンピューター。
    前 = +Z。扉のモーターは (7, 1, -2)、クレーンのモーターは (1, 1, 1)。"""
    V = [((x, 0, z), "steel" if (x + z) % 2 else "rock", None) for x in range(-1, 9) for z in range(-2, 5)]
    V += [((0, 1, 3), "steel", {"def": "battery", "dir": 2, "name": "バッテリー"}),
          ((0, 1, 4), "steel", {"def": "battery", "dir": 2, "name": "バッテリー"}),
          ((-1, 1, 4), "glass", {"def": "solar_panel", "dir": 2, "name": "太陽光パネル"}),
          ((0, 2, 4), "steel", {"def": "computer", "dir": 5, "name": "搬入の制御",
                                "state": {"script": "crane.py", "blocks": "crane.blocks.json"}})]
    # 旋回クレーン：モーター（出力は上）→ 柱 → 腕（+X へ）→ 荷物
    V.append(((1, 1, 1), "steel", {"def": "actuator", "dir": 2, "name": "クレーンの旋回",
                                   "state": {"rot_speed": 45.0}}))
    V += [((1, y, 1), "hull_orange", None) for y in (2, 3, 4)]
    V += [((x, 5, 1), "hull_orange" if x % 2 else "bar_white", None) for x in range(1, 6)]
    V += [((5, y, 1), "steel", None) for y in (4,)]
    V += [(c, "wood", None) for c in CRANE_CRATE if c != (5, 4, 1)]
    # 扉：モーター（出力は上）→ 板（+Z へ 3 ブロック・高さ 3）
    V.append(((7, 1, -2), "steel", {"def": "actuator", "dir": 2, "name": "扉のモーター",
                                    "state": {"rot_speed": 60.0}}))
    V += [((7, y, z), "hull_red" if y == 2 else "hull_white", None) for y in (2, 3, 4) for z in (-2, -1, 0)]
    return V


# ---------------------------------------------------------------------------
# 第6章：工場（精錬機・加工機・燃料のバルブ・配管・タンク）
# ---------------------------------------------------------------------------
FACTORY_CUT = (6, 1, 2)       # キンギのドローンが切る配管


def factory_voxels():
    """前 = +Z。精錬機 (2,1,2) → 配管 (2,1,1) → 加工機 (2,1,0)。精錬機 → 配管 (3,1,2) → バルブ (4,1,2)（閉）
    → 配管 (5〜7,1,2) → タンク (8,1,2)。バッテリーと太陽光パネルで動く。"""
    V = [((x, 0, z), "rock" if (x + z) % 3 else "steel", None) for x in range(-1, 10) for z in range(-1, 5)]
    V += [((2, 1, 2), "steel", {"def": "refiner", "dir": 4, "name": "精錬機"}),
          ((2, 1, 1), "steel", {"def": "pipe", "dir": 2, "name": "配管"}),
          ((2, 1, 0), "steel", {"def": "fabricator", "dir": 4, "name": "加工機"}),
          ((3, 1, 2), "steel", {"def": "pipe", "dir": 2, "name": "配管"}),
          ((4, 1, 2), "steel", {"def": "valve", "dir": 2, "name": "燃料のバルブ", "state": {"open": 0.0}}),
          ((5, 1, 2), "steel", {"def": "pipe", "dir": 2, "name": "配管"}),
          ((6, 1, 2), "steel", {"def": "pipe", "dir": 2, "name": "配管"}),
          ((7, 1, 2), "steel", {"def": "pipe", "dir": 2, "name": "配管"}),
          ((8, 1, 2), "steel", {"def": "tank", "dir": 2, "name": "燃料タンク", "state": {"contents": {}}}),
          ((0, 1, 0), "steel", {"def": "battery", "dir": 2, "name": "バッテリー"}),
          ((0, 1, 1), "steel", {"def": "battery", "dir": 2, "name": "バッテリー"}),
          ((0, 1, 4), "steel", {"def": "computer", "dir": 4, "name": "工場の表示", "state": {"script": "factory.py",
                                                                                         "running": True}})]
    V += [((x, 1, 4), "glass", {"def": "solar_panel", "dir": 2, "name": "太陽光パネル"}) for x in (2, 3, 4, 5, 6)]
    return V


FACTORY_SCRIPT = '''"""工場の状態の表示（精錬機・加工機の中身、燃料タンク）。"""


def tick(api):
    st = {}
    for d in api.devices("processor"):
        st[d.name] = d.state.get("store", {})
    tank = sum(t.contents.get("fuel", 0) for t in api.devices("tank"))
    r = st.get("精錬機", {})
    f = st.get("加工機", {})
    names = {"ore": "鉱石", "sulfur": "硫黄鉱石", "iron": "鉄", "silica": "ケイ砂", "fuel": "燃料", "parts": "機械部品"}
    def fmt(s):
        return "、".join(f"{names.get(k, k)} {v:.0f}" for k, v in s.items() if v >= 0.5) or "空"
    api.display(f"工場\\n精錬機：{fmt(r)}\\n加工機：{fmt(f)}\\n燃料タンク：{tank:.0f}", width=2.4,
                color=(1.0, 0.92, 0.6))
'''


# ---------------------------------------------------------------------------
# キンギの偵察ドローン（黒と赤。シナリオが動かす：物理では動かない）
# ---------------------------------------------------------------------------
def kdrone_voxels():
    V = [((x, 0, z), "steel", None) for x in range(-1, 2) for z in range(-1, 2)]
    V[4] = ((0, 0, 0), "steel", {"def": "battery", "dir": 2, "name": "バッテリー"})
    V += [((0, 1, 0), "hull_red", None), ((0, 0, 2), "glass", None)]
    for dx, dz in ((1, 1), (-1, 1), (1, -1), (-1, -1)):
        V.append(((dx * 2, 0, dz * 2), "steel", None))
    return V

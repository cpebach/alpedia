"""ルナ・ポートの建物（ブロック。アンカーで月に固定）：管制塔（受付と管制室）・ドーム 3 つ（居住・温室・研究）・
住居ブロック・通路・ソーラーパネル・電波の皿・止めてある探査車。

大きな殻（ドーム）は 1 m のブロック、中の家具は 0.5 m のブロック（別の構築物）。
明かり・受付（触れる物）・発着場は景色の箱のまま（port.build）。
"""
import numpy as np

from main.buildkit import Kit, anchored

TOWER = (-15.0, 2.0)
DOMES = ((14.0, 8.0, 7.0, "hull_teal", "hab"), (4.0, 22.0, 9.0, "hull_green", "green"),
         (-14.0, 20.0, 5.5, "hull_pink", "lab"))
HOUSES = ((20.0, -6.0), (26.0, 2.0), (22.0, 16.0))
TUBES = (((14.0, 1.0), (20.0, -4.0)), ((21.0, 8.0), (26.0, 4.0)), ((18.0, 12.0), (21.0, 14.0)),
         ((8.0, 14.0), (8.5, 10.0)), ((-9.0, 18.0), (-3.0, 18.0)), ((-12.0, 5.0), (-13.0, 15.0)))
ROVERS = ((6.0, -10.0, 30.0, "hull_orange"), (-6.0, -11.0, -20.0, "hull_white"))


def tower():
    """管制塔（0.5 m のブロック）：受付の階（6 m 角・高さ 4 m。入口は +X）・筒（3 m 角）・管制室（7 m 角・高さ 16〜19 m、
    まわりがガラス）・屋根・柱。受付には机・椅子・ソファ・植木・案内の画面、管制室には操作盤。"""
    k = Kit()
    k.shell(0, 0, 0, 11, 7, 11, "hull_white", floor="carpet_blue", roof="hull_white")
    k.paint("hull_navy", 0, 1, 0, 11, 1, 11, only="hull_white")
    k.door(11, 1, 5, 11, 4, 6)                                             # 入口（+X）
    k.windows(11, 3, 1, 11, 5, 3)
    k.windows(11, 3, 8, 11, 5, 10)
    k.windows(0, 4, 3, 0, 5, 8)
    # 受付の階の中
    k.fill(8, 1, 2, 8, 2, 9, "wood")                                       # 受付の台（入口の正面の奥）
    k.console(7, 1, 3, "+x", text="出発・到着\nルナ・ポート", width=0.9)
    k.sofa(2, 1, 9, "+z", w=4, mat="carpet_red")
    k.table(3, 1, 6, w=2, d=1)
    k.planter(1, 1, 1, w=1, d=1, h=3)
    k.planter(10, 1, 10, w=1, d=1, h=3)
    k.shelf(1, 1, 3, "+x", w=2, h=3)
    # 筒
    k.shell(3, 8, 3, 8, 31, 8, "hull_white", floor=False, roof=False)
    for y in range(10, 31, 4):
        k.windows(8, y, 5, 8, y + 1, 6)
        k.windows(3, y, 5, 3, y + 1, 6)
    # 管制室
    k.shell(-1, 32, -1, 12, 37, 12, "hull_dark", floor="hull_white", roof="hull_white")
    k.windows(-1, 33, 0, 12, 35, 11)
    k.fill(-2, 38, -2, 13, 38, 13, "hull_white")
    for x, z, f in ((2, 1, "-z"), (5, 1, "-z"), (8, 1, "-z"), (11 - 1, 5, "+x"), (1, 8, "-x")):
        k.console(x, 33, z, f, text="", mat="hull_navy")
    k.desk(4, 33, 6, "+z", w=4, mat="hull_navy")
    k.chair(5, 33, 8, "-z", mat="carpet_blue")
    k.fill(5, 39, 5, 5, 44, 5, "steel")                                    # 柱（上に灯：景色の箱）
    return anchored(k.voxels(), 3)


def dome(r, mat, kind):
    """ドームの殻（1 m のブロック）：半径 r、ガラスの帯と天窓、入口（-Z）。"""
    k = Kit()
    R = int(round(r))
    k.dome(0, 0, 0, r, mat, thick=1.5)
    for (x, y, z), (m, d) in list(k.v.items()):
        if y == max(1, R // 3) or y >= R - 1:                              # ガラスの帯と天窓
            k.v[(x, y, z)] = ("glass", None)
    k.door(-1, 0, -R - 1, 1, 2, -R + 2)                                    # 入口
    k.fill(-2, 3, -R, 2, 3, -R + 1, mat)                                   # 入口のひさし
    return anchored(k.voxels(), 4)


def dome_inside(r, kind):
    """ドームの中の家具（0.5 m のブロック、床の上）。kind：hab（居住）・green（温室）・lab（研究）。"""
    k = Kit()
    n = int(r * 2 * 0.62)                                                  # 使える範囲（半分の幅、セル）
    if kind == "hab":
        for j, x in enumerate((-n + 1, n - 3)):
            k.bed(x, 0, -2, "+z")
            k.bed(x, 0, 3, "+z", mat="carpet_blue" if j else "cloth_white")
        k.table(-1, 0, -1, w=3, d=2)
        for x in (-1, 1):
            k.chair(x, 0, 2, "-z", mat="hull_dark")
        k.sofa(-2, 0, n - 1, "-z", w=5, mat="carpet_red")
        k.shelf(-n + 1, 0, n - 1, "-z", w=3, h=4)
        k.rug(-3, 0, -4, 3, -3, "carpet_blue")
    elif kind == "green":
        for x in range(-n + 1, n - 1, 3):                                   # 畑の列
            k.planter(x, 0, -n + 3, w=2, d=2 * n - 6, h=2)
        k.fill(-1, 0, -n + 1, 0, 0, n - 2, "concrete")                     # 通路
    else:
        k.desk(-n + 1, 0, 0, "+x", w=4, mat="hull_navy")
        k.console(-n + 2, 0, -2, "+x", text="観測", width=0.6)
        k.console(n - 2, 0, -2, "-x", text="分析", width=0.6)
        k.shelf(-2, 0, n - 1, "-z", w=4, h=4, goods=("glass", "hull_yellow", "glass", "hull_teal"))
        k.chair(-n + 3, 0, 1, "-x")
    return anchored(k.voxels(), 3)


def house():
    """住居ブロック（5 m × 4 m、高さ 3 m）：入口（-Z）・窓・ベッド・机・棚。"""
    k = Kit()
    k.shell(0, 0, 0, 9, 5, 7, "hull_cream", floor="wood", roof=False)
    k.fill(-1, 6, -1, 10, 6, 8, "hull_dark")
    k.door(4, 1, 0, 5, 4, 0)
    k.windows(1, 3, 0, 2, 4, 0)
    k.windows(7, 3, 0, 8, 4, 0)
    k.windows(9, 3, 3, 9, 4, 5)
    k.bed(1, 1, 3, "+z")
    k.desk(6, 1, 5, "-z", w=3)
    k.chair(7, 1, 4, "-z")
    k.shelf(1, 1, 6, "-z", w=2, h=3)
    k.planter(8, 1, 1, w=1, d=1, h=2)
    return anchored(k.voxels(), 3)


def tube(length):
    """通路（幅 2 m・高さ 2 m の四角い筒。両端は開いている）。前 = +Z。"""
    k = Kit()
    n = max(2, int(round(length / 0.5)))
    for z in range(n):
        for x in range(-2, 2):
            for y in range(0, 4):
                if x in (-2, 1) or y in (0, 3):
                    k.put(x, y, z, "hull_white" if y < 3 else "hull_cream")
        if z % 4 == 2:
            k.windows(-2, 1, z, -2, 2, z)
            k.windows(1, 1, z, 1, 2, z)
    return anchored(k.voxels(), 4)


def solar_stand():
    k = Kit()
    for x in (-2, 2):
        k.fill(x, 0, 0, x, 1, 0, "steel")
    for x in range(-3, 4):
        k.put(x, 2, 0, "glass", {"def": "solar_panel", "dir": 2, "name": "太陽光パネル"})
        k.put(x, 2, 1, "hull_navy")
    return anchored(k.voxels(), 2)


def dish():
    """電波の皿：柱と、上の皿・灯（景色の箱）。"""
    k = Kit()
    k.fill(0, 0, 0, 0, 7, 0, "steel")
    for x in range(-3, 4):
        for z in range(-3, 4):
            if x * x + z * z <= 10:
                k.put(x, 8 + (1 if x * x + z * z > 5 else 0), z, "hull_white")
    k.fill(0, 9, 0, 0, 10, 0, "steel")
    return anchored(k.voxels(), 1)


def rover(color):
    """止めてある探査車：車体・ガラスの運転席・ゴムの車輪 4 つ。前 = +Z。"""
    k = Kit()
    k.fill(-2, 1, -3, 1, 2, 2, color)
    k.fill(-1, 3, 0, 0, 3, 2, "glass")
    k.fill(-2, 3, -3, 1, 3, -1, color)
    for x in (-3, 2):
        for z in (-2, 1):
            k.fill(x, 0, z, x, 1, z + 1, "rubber")
    k.put(0, 2, 3, "hull_yellow")
    return anchored(k.voxels(), 2)


def items():
    """[(役, 名前, ボクセル, (x, z, 向き, 高さ), ブロックの大きさ)]"""
    out = [("tower", "管制塔", tower(), (TOWER[0], TOWER[1], 0.0, 0.0), 0.5)]
    for x, z, r, mat, kind in DOMES:
        out.append((f"dome_{kind}", "ドーム", dome(r, mat, kind), (x, z, 0.0, 0.0), 1.0))
        out.append((f"dome_{kind}_inside", "ドームの中", dome_inside(r, kind), (x, z, 0.0, 0.0), 0.5))
    for k, (x, z) in enumerate(HOUSES):
        out.append((f"house{k}", "住居ブロック", house(), (x, z, 0.0, 0.0), 0.5))
    for k, ((x0, z0), (x1, z1)) in enumerate(TUBES):
        L_ = float(np.hypot(x1 - x0, z1 - z0))
        yaw = -np.degrees(np.arctan2(x1 - x0, z1 - z0))
        out.append((f"tube{k}", "通路", tube(L_), ((x0 + x1) / 2, (z0 + z1) / 2, yaw, 0.0), 0.5))
    for k in range(5):
        out.append((f"solar{k}", "ソーラーパネル", solar_stand(), (-26.0, -10.0 + k * 4.2, 0.0, 0.0), 0.5))
    out.append(("dish", "電波の皿", dish(), (10.0, -16.0, 0.0, 0.0), 0.5))
    for k, (x, z, yaw, col) in enumerate(ROVERS):
        out.append((f"rover{k}", "探査車", rover(col), (x, z, yaw, 0.0), 0.5))
    return out

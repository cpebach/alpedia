"""軌道都市アマツの甲板の上の建物（ブロック。アンカーで固定）：機関室・倉庫・詰め所。

甲板・円筒・港の塔は大きいので景色の箱のまま（city.build）。屋根の明かり・甲板の縁の明かり・非常灯（赤。まとまり
alert）はライトのブロック（バッテリーで光る）。
"""
from main.buildkit import Kit, anchored

HUTS = ((-22, -70, "engine"), (22, -40, "store"), (-18, 30, "guard"), (20, 75, "engine"), (-24, 80, "store"))
WARM = (1.0, 0.86, 0.55)
CYAN = (0.5, 0.95, 1.0)
RED = (1.0, 0.25, 0.2)
RAIL_X = 31.4                     # 甲板の縁（DECK_W / 2 - 0.6）
RAIL_LEN = 40.0                   # 縁の1本の長さ m（甲板 200 m を 5 本で）
RAIL_ZS = (-80.0, -40.0, 0.0, 40.0, 80.0)


def hut(kind):
    """6 m × 10 m・高さ 4 m（0.5 m のブロック）。入口は +Z と -Z、横に窓。中は kind ごとの物。"""
    k = Kit()
    k.shell(0, 0, 0, 11, 7, 19, "hull_white", floor=False, roof="hull_dark")
    k.paint("hull_navy", 0, 0, 0, 11, 1, 19, only="hull_white")
    for z0 in (0, 19):
        k.door(4, 0, z0, 7, 4, z0)
    for z in (4, 9, 14):
        k.windows(0, 4, z, 0, 5, z + 2)
        k.windows(11, 4, z, 11, 5, z + 2)
    if kind == "engine":                                   # 機関室：大きなタンクと配管・操作盤
        for z in (3, 9, 14):
            k.cylinder(3, 0, z, 1.6, 6, "hull_teal")
            k.fill(5, 3, z, 9, 3, z, "steel")
        k.console(9, 0, 6, "-x", text="主機\n出力 98%", width=0.8)
    elif kind == "store":                                   # 倉庫：棚と木箱
        for z in (3, 8, 13):
            k.shelf(1, 0, z, "+x", w=1, h=5)
            k.shelf(10, 0, z, "-x", w=1, h=5)
        k.crates(4, 0, 4, n=5)
        k.crates(6, 0, 12, n=4)
    else:                                                   # 詰め所：机・椅子・ベッド・画面
        k.desk(2, 0, 3, "+z", w=3)
        k.chair(3, 0, 2, "+z")
        k.console(8, 0, 3, "+z", text="甲板の監視", width=0.9)
        k.bed(1, 0, 13, "+z")
        k.bed(9, 0, 13, "+z")
        k.table(4, 0, 9, w=3, d=2)
    k.battery(10, 0, 1)
    for x in (4, 7):                                       # 屋根の明かり
        for z in (6, 13):
            k.light(x, 8, z, WARM, name="屋根の明かり")
    return anchored(k.voxels(), 3)


def rail():
    """甲板の縁石（0.5 m のブロック、長さ RAIL_LEN、前 = +Z）：10 m ごとに水色の点滅灯、12 m ごとに非常灯（赤。まとまり
    alert。ふだんは消えている）。"""
    k = Kit()
    n = int(round(RAIL_LEN / 0.5))
    k.fill(0, 0, 0, 0, 0, n - 1, "hull_dark")
    k.battery(0, 0, n // 2)
    for z in range(10, n, 20):
        k.light(0, 1, z, CYAN, blink=2.4, name="縁の明かり")
    for z in range(4, n, 24):
        k.light(0, 1, z, RED, group="alert", on=False, name="非常灯")
    return anchored(k.voxels(), 4)

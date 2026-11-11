"""ブロックの建物（ガイアの観測基地・発射台）と、宇宙の艦隊（連盟は箱っぽく無骨、帝国は潜水艦・飛行船のような流線型）。

どれも main.buildkit の Kit で組み立てて、アンカーで固定する（物理計算なし・吹き飛ばない）。
建物の明かり（窓・扉の光）・触れる物（端末など）は景色の箱のまま（sites.build_terra）。
"""
import numpy as np

from main.buildkit import Kit, anchor_center, anchored

WARM = (1.0, 0.82, 0.45)          # 明かりの色（ライト）
LAMP = (0.95, 0.75, 0.35)
RED = (1.0, 0.2, 0.15)
CYAN = (0.25, 0.95, 1.0)
LIME = (0.6, 1.0, 0.3)

# ---------------------------------------------------------------------------
# ガイア：観測基地
# ---------------------------------------------------------------------------
MAIN_HALL = (10.0, 52.0)          # 本館（前 = -Z の面に扉と窓）


def main_hall():
    """本館：18×14 ブロック・高さ 9（4.5 m）。前（-Z）の真ん中に扉、窓 4 つ。中は事務室：机と端末・棚・ソファ・植木。
    屋根は少し張り出した濃い灰色、屋根の上にアンテナ。"""
    k = Kit()
    W, D, H = 18, 14, 9
    k.shell(0, 0, 0, W - 1, H - 1, D - 1, "hull_white", floor="carpet_blue", roof=False)
    k.paint("hull_navy", 0, 1, 0, W - 1, 1, D - 1, only="hull_white")      # 下の帯
    k.fill(-1, H, -1, W, H, D, "hull_dark")                                 # 屋根（張り出し）
    k.door(8, 1, 0, 9, 4, 0)                                                # 扉（1 m × 2 m）
    for wx in (0, 5, 10, 14):                                               # 窓：明かりの箱（sites）と同じ所
        k.windows(wx, 5, 0, wx + 2, 6, 0)
    for wz in (3, 8):                                                       # 横の窓
        k.windows(0, 5, wz, 0, 6, wz + 2)
        k.windows(W - 1, 5, wz, W - 1, 6, wz + 2)
    # 中：机と端末（4 つ）・椅子・棚・ソファ・植木・受付の台
    for k_, x in enumerate((2, 6, 11, 14)):
        k.desk(x, 1, 6, "-z", w=2)
        k.console(x, 2, 7, "-z", text=("観測ログ", "気象", "通信", "電源")[k_], width=0.8)
        k.chair(x, 1, 5, "-z")
    k.shelf(1, 1, D - 2, "-z", w=5, h=4)
    k.shelf(12, 1, D - 2, "-z", w=5, h=4)
    k.sofa(7, 1, D - 3, "-z", w=4, mat="carpet_red")
    k.planter(1, 1, 1, w=1, d=1, h=3)
    k.planter(W - 2, 1, 1, w=1, d=1, h=3)
    k.fill(7, 1, 2, 10, 2, 2, "wood")                                       # 受付の台
    # 明かり（ライト。基地の電源が入ると点く）：天井・扉の上・屋根のアンテナの赤い灯（点滅）
    for x in (3, 8, 13):
        for z in (4, 9):
            k.light(x, H - 1, z, WARM, group="base")
    for x in (8, 9):
        k.light(x, 5, 0, LAMP, group="base")
    for y in range(H + 1, H + 9):
        k.put(13, y, 10, "steel")
    k.light(13, H + 9, 10, RED, group="base", blink=1.2)
    k.battery(W - 2, 1, D - 3)
    return anchored(k.voxels(), 3)


POWER_SHED = (-11.0, 53.0)


def power_shed():
    """電源設備の小屋：柱 4 本と屋根（8 m × 5 m、高さ 3.5 m）。"""
    k = Kit()
    for x in (0, 15):
        for z in (0, 9):
            k.fill(x, 0, z, x, 6, z, "steel")
    k.fill(-1, 7, -1, 16, 7, 10, "hull_dark")
    k.fill(0, 6, 0, 15, 6, 0, "steel")                                      # はり
    k.fill(0, 6, 9, 15, 6, 9, "steel")
    return anchored(k.voxels(), 9)


WORKSHOP = (-12.5, 65.5)


def workshop():
    """試験台の作業小屋：柱 4 本と屋根（12 m × 7 m、高さ 5.6 m）、奥に道具の棚と作業台。"""
    k = Kit()
    for x in (0, 23):
        for z in (0, 13):
            k.fill(x, 0, z, x, 10, z, "steel")
    k.fill(-1, 11, -1, 24, 11, 14, "hull_navy")
    for z in (0, 13):
        k.fill(0, 10, z, 23, 10, z, "steel")
    for x in range(3, 22, 4):                                               # はりの下の明かり
        k.light(x, 9, 0, WARM, group="base")
        k.put(x, 10, 0, "steel")
    k.battery(1, 0, 0)
    k.shelf(1, 0, 12, "-z", w=4, h=4, goods=("hull_red", "steel", "hull_yellow", "wood"))
    k.desk(19, 0, 12, "-z", w=3, mat="steel")
    k.crates(17, 0, 1, n=4)
    return anchored(k.voxels(), 4)


HANGAR = (-13.0, 86.0)


def hangar():
    """格納庫：左右と奥の壁・屋根（前 = +Z は開いている）。中に工具の棚・作業台・ドラム缶・部品の箱。"""
    k = Kit()
    for x in (-9, 8):
        k.fill(x, 0, -12, x, 9, 11, "hull_cream")
        k.windows(x, 6, -8, x, 7, -4)
        k.windows(x, 6, 2, x, 7, 6)
    k.fill(-9, 0, -12, 8, 9, -12, "hull_cream")
    k.fill(-10, 10, -13, 9, 10, 12, "hull_dark")
    k.paint("hull_yellow", -9, 0, -12, 8, 1, 11, only="hull_cream")        # 下の帯
    k.shelf(-8, 0, -11, "+z", w=5, h=5, goods=("hull_red", "steel", "hull_yellow", "rubber"))
    k.shelf(3, 0, -11, "+z", w=5, h=5, goods=("steel", "hull_blue", "wood", "rubber"))
    k.desk(-8, 0, -4, "+x", w=4, mat="steel")
    k.fill(6, 0, -6, 7, 1, -5, "hull_red")                                  # ドラム缶
    k.crates(5, 0, 4, n=5)
    for x in range(-8, 9, 4):                                               # 入口の上の明かり
        k.light(x, 9, 11, LAMP, group="base")
    for z in (-8, -2, 4):                                                   # 天井の明かり
        k.light(-1, 9, z, WARM, group="base")
    k.battery(7, 0, -11)
    return anchored(k.voxels(), 4)


def crane_warehouse():
    """搬入クレーンの倉庫（3 m × 4 m、高さ 3 m）：扉と棚・木箱。"""
    k = Kit()
    k.shell(0, 0, 0, 5, 5, 7, "hull_orange", floor="concrete", roof="hull_dark")
    k.door(2, 1, 0, 3, 3, 0)
    k.light(2, 4, 0, WARM, group="base")                                    # 扉の上の明かり
    k.light(3, 4, 0, WARM, group="base")
    k.battery(4, 1, 6)
    k.shelf(1, 1, 6, "-z", w=4, h=3, goods=("wood", "hull_blue", "wood"))
    k.crates(1, 1, 1, n=3)
    return anchored(k.voxels(), 2)


def factory_roof():
    """工場の屋根（7 m × 9 m、高さ 3.6 m）と柱 4 本。"""
    k = Kit()
    for x in (0, 13):
        for z in (0, 17):
            k.fill(x, 0, z, x, 6, z, "steel")
    k.fill(-1, 7, -1, 14, 7, 18, "hull_dark")
    k.fill(0, 6, 0, 0, 6, 17, "steel")
    k.fill(13, 6, 0, 13, 6, 17, "steel")
    return anchored(k.voxels(), 13)


RADIO_TOWER = (20.0, 80.0)


def radio_tower():
    """電波塔（高さ 20 m）：四隅の柱と 5 m ごとの床、上に柱。"""
    k = Kit()
    for x in (-2, 2):
        for z in (-2, 2):
            k.fill(x, 0, z, x, 39, z, "steel")
    for y in (9, 19, 29, 39):
        for x in range(-2, 3):
            for z in range(-2, 3):
                if x in (-2, 2) or z in (-2, 2):
                    k.put(x, y, z, "hull_red" if y == 39 else "steel")
        for x in (-1, 0, 1):                                                # 筋かい
            k.put(x, y - 4, -2, "steel")
            k.put(x, y - 4, 2, "steel")
    k.fill(0, 40, 0, 0, 47, 0, "steel")
    k.light(0, 48, 0, RED, blink=1.0)                                       # 上の赤い灯
    k.battery(0, 1, 0)
    return anchored(k.voxels(), 4)


SERVICE_TOWER_OFF = 3.4           # 発射台の整備塔：発射台の真ん中から +X へ


def service_tower():
    """発射台の整備塔（赤、高さ 8 m）：四隅の柱と 2 m ごとの床。"""
    k = Kit()
    for x in (0, 3):
        for z in (0, 3):
            k.fill(x, 0, z, x, 16, z, "hull_red")
    for y in (4, 8, 12, 16):
        for x in range(4):
            for z in range(4):
                if x in (0, 3) or z in (0, 3) or y == 16:
                    k.put(x, y, z, "hull_red")
    return anchored(k.voxels(), 3)


def terra_buildings(launch):
    """[(役, 名前, ボクセル, 地図の場所 (x, z, 向き, 高さ), 地面に刺す)]。"""
    lx, lz = launch
    return [
        ("bld_main", "観測基地の本館", main_hall(), (MAIN_HALL[0], MAIN_HALL[1], 0.0, 0.0), False),
        ("bld_power_shed", "電源設備の小屋", power_shed(), (POWER_SHED[0], POWER_SHED[1], 0.0, 0.0), False),
        ("bld_workshop", "試験台の作業小屋", workshop(), (WORKSHOP[0], WORKSHOP[1], 0.0, 0.0), False),
        ("bld_hangar", "格納庫", hangar(), (HANGAR[0], HANGAR[1], 0.0, 0.0), False),
        ("bld_warehouse", "倉庫", crane_warehouse(), (20.0 + 6.5, 56.0 + 2.0, 0.0, 0.0), False),
        ("bld_factory_roof", "工場の屋根", factory_roof(), (-31.0, 69.0, 0.0, 0.0), False),
        ("bld_radio_tower", "電波塔", radio_tower(), (RADIO_TOWER[0], RADIO_TOWER[1], 0.0, 0.0), False),
        ("bld_service_tower", "発射台の整備塔", service_tower(), (lx + SERVICE_TOWER_OFF, lz, 0.0, 0.3), False),
        ("bld_drone_pad", "ドローン発着場のふち", drone_pad_frame(), (12.0, 72.0, 0.0, 0.0), False),
    ] + [(f"bld_pad_light{k}", "発射台の灯", pad_light_post(RED, blink=1.2),
          (lx + np.cos(np.pi / 4 + k * np.pi / 2) * 4.0, lz + np.sin(np.pi / 4 + k * np.pi / 2) * 4.0, 0.0, 0.3), False)
         for k in range(4)]


# ---------------------------------------------------------------------------
# 宇宙：艦隊（2 m のブロック。遠くに浮かぶ張りぼての代わり）
# ---------------------------------------------------------------------------
FLEET_BLOCK = 2.0


def union_ship(length=30, width=7, flagship=False):
    """連盟の艦（箱っぽく無骨）：箱を重ねた胴・装甲板の段・角ばった艦橋の塔・砲塔・後ろの推進の箱。
    前 = +Z。セルは 2 m。"""
    k = Kit()
    L, W = length, width
    hw = W // 2
    k.fill(-hw, 0, 0, hw, 3, L - 1, "hull_white")                         # 胴
    k.fill(-hw - 1, 1, 3, hw + 1, 2, L - 5, "hull_navy")                  # 横の装甲の段
    k.fill(-hw + 1, 4, 2, hw - 1, 4, L - 6, "hull_white")                 # 上の段
    k.fill(-hw + 1, -1, 4, hw - 1, -1, L - 8, "steel")                    # 下の段
    k.fill(-1, 5, L // 3, 1, 8 if flagship else 7, L // 3 + 3, "hull_white")   # 艦橋の塔
    k.fill(-1, 7 if not flagship else 8, L // 3, 1, 7 if not flagship else 8, L // 3 + 3, "hull_navy")
    k.fill(-2, 6, L // 3 + 1, 2, 6, L // 3 + 2, "glass")                  # 艦橋の窓
    for z in (L - 6, L - 10, 6):                                           # 砲塔
        k.fill(-1, 5, z, 1, 5, z + 1, "steel")
        k.fill(0, 5, z + 2, 0, 5, z + 4, "hull_dark")
    k.fill(-hw, 0, -3, hw, 3, -1, "hull_dark")                            # 推進の箱と、推進の光
    for x in range(-hw + 1, hw, 2):
        k.light(x, 1, -4, CYAN)
        k.light(x, 2, -4, CYAN)
    k.battery(0, 1, 2)
    k.fill(-hw, 0, L - 1, hw, 2, L + 1, "hull_white")                     # 角ばった艦首
    k.paint("hull_navy", -hw, 3, L - 1, hw, 3, L + 1)
    if flagship:
        k.fill(-hw - 3, 1, L // 2, -hw - 1, 2, L // 2 + 8, "hull_white")  # 左右の格納庫
        k.fill(hw + 1, 1, L // 2, hw + 3, 2, L // 2 + 8, "hull_white")
        k.fill(-hw - 3, 3, L // 2, -hw - 3, 3, L // 2 + 8, "hull_gold")
        k.fill(hw + 3, 3, L // 2, hw + 3, 3, L // 2 + 8, "hull_gold")
    return anchor_center(k.voxels())


def empire_ship(length=35, radius=5.0, airship=False):
    """帝国の艦（流線型）：だ円体の胴（潜水艦・飛行船のよう）・司令塔・十字の尾びれ・赤い帯。
    airship なら胴の下にゴンドラ。前 = +Z。セルは 2 m。"""
    k = Kit()
    c = length / 2
    k.ellipsoid(0, 0, 0, radius, radius * 0.9, c, "hull_black", thick=1.2)
    for z in range(int(-c), int(c) + 1, 7):                                # 赤い帯
        for (x, y), _ in list(_ring_cells(k, z)):
            k.put(x, y, z, "hull_red")
    # 司令塔（潜水艦）とその窓
    k.fill(-1, int(radius * 0.9), int(c * 0.2), 1, int(radius * 0.9) + 3, int(c * 0.2) + 5, "hull_black")
    k.fill(-1, int(radius * 0.9) + 2, int(c * 0.2) + 5, 1, int(radius * 0.9) + 2, int(c * 0.2) + 5, "glass")
    # 十字の尾びれ（後ろ = -Z）
    z0 = int(-c) + 1
    top = int(radius * 0.5) + 4                                            # 尾の太さ ＋ ひれ
    for y in range(1, top + 1):
        n = max(1, (top - y) // 2 + 2)                                     # 根元ほど長い（後ろへ流れる形）
        k.fill(0, y, z0 + (y // 2), 0, y, z0 + (y // 2) + n, "hull_red")
        k.fill(0, -y, z0 + (y // 2), 0, -y, z0 + (y // 2) + n, "hull_red")
        k.fill(y, 0, z0 + (y // 2), y, 0, z0 + (y // 2) + n, "hull_black")
        k.fill(-y, 0, z0 + (y // 2), -y, 0, z0 + (y // 2) + n, "hull_black")
    # 横びれ（前の方）
    for x in range(int(radius), int(radius) + 3):
        k.fill(x, 0, int(c * 0.4), x, 0, int(c * 0.4) + 2, "hull_black")
        k.fill(-x, 0, int(c * 0.4), -x, 0, int(c * 0.4) + 2, "hull_black")
    if airship:                                                             # ゴンドラ
        gy = -int(radius * 0.9) - 2
        k.fill(-2, gy, -4, 2, gy + 1, 6, "hull_black")
        k.fill(-2, gy + 1, -3, 2, gy + 1, 5, "glass")
        k.fill(-1, gy + 2, -1, 1, gy + 2, 2, "hull_red")
    for x in (-1, 0, 1):                                                   # 推進口の光
        for y in (-1, 0, 1):
            k.light(x, y, int(-c) - 1, RED)
    k.battery(0, 0, 0)
    return anchor_center(k.voxels())


def _ring_cells(k, z):
    """Kit の z の断面のセル（帯の色を塗る所）。"""
    return [((x, y), m) for (x, y, zz), (m, d) in k.v.items() if zz == z and not d]


def union_interceptor_extra():
    """迎撃艦（space.interceptor_voxels）の見た目を豪華に：甲板の下の箱っぽい船体・推進の箱・装甲の段・艦橋の中の家具。
    セルは迎撃艦と同じ格子（0.5 m）。甲板は y = 0、前 = +Z、幅 x 0〜11、長さ z 0〜23。"""
    k = Kit()
    k.fill(1, -1, 1, 10, -1, 22, "hull_white")                            # 甲板の下の船体（3 段）
    k.fill(2, -2, 2, 9, -2, 21, "hull_navy")
    k.fill(3, -3, 4, 8, -3, 19, "steel")
    k.fill(-1, -1, 4, -1, 0, 18, "hull_navy")                              # 横の装甲の段
    k.fill(12, -1, 4, 12, 0, 18, "hull_navy")
    for x in (2, 3, 8, 9):                                                  # 推進の箱（後ろ）
        k.fill(x, -2, -2, x, -1, -1, "hull_dark")
    k.fill(4, 0, 24, 7, 0, 25, "hull_white")                               # 角ばった艦首
    k.fill(5, -1, 24, 6, -1, 26, "hull_navy")
    # 艦橋の中（x 4〜7・z 19〜21、床 y = 0 の上。ケンスは x 5〜6・z 19 に立つ）：両端に操作盤
    k.console(4, 1, 20, "-z", text="航法", width=0.6)
    k.console(7, 1, 20, "-z", text="通信", width=0.6)
    return k.voxels()


def with_extra(V, extra):
    """V に extra を足す（V に無いセルだけ）。"""
    have = {tuple(c) for c, _, _ in V}
    return list(V) + [(c, m, d) for c, m, d in extra if tuple(c) not in have]


def pad_light_post(color, blink=None, group=None):
    """発射台・発着場の灯：バッテリーの台の上にライト1つ。"""
    k = Kit()
    k.battery(0, 0, 0)
    k.light(0, 1, 0, color, group=group, blink=blink)
    return anchored(k.voxels(), 1)


def drone_pad_frame():
    """ドローン発着場のふち（4 m 角）と四隅の明かり（基地の電源が入ると点く）。"""
    k = Kit()
    for i in range(8):
        for j in (0, 7):
            k.put(i, 0, j, "hull_dark")
            k.put(j, 0, i, "hull_dark")
    for x, z in ((0, 0), (7, 0), (0, 7), (7, 7)):
        k.light(x, 0, z, LIME, group="base")
    k.battery(3, 0, 0)
    return anchored(k.voxels(), 2)

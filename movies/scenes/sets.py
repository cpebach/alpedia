"""場面で使う舞台（ブロックの部屋）と、共通の小道具。"""
import math

from cinema.shapes import Vox

VRM_DIR = None


def vrm(name="alpedia_player1.vrm"):
    from cinema.stage import DEV
    return DEV / "mods" / "skinmodel(dummy)" / name


def bridge():
    """旗艦アルテミスの艦橋（0.5 m のブロック）。前（+Z）に大きな窓、真ん中に丸い立体映像の台。
    床の上面が y = 0、台の中心が x = z = 0。"""
    v = Vox()
    X, Z0, Z1, H = 16, -14, 13, 11
    # 床（光る線の入った床）
    for x in range(-X, X + 1):
        for z in range(Z0, Z1 + 1):
            m = "m_floor"
            if z in (-8, 8) and abs(x) < X - 2:
                m = "g_dim"
            if abs(x) == 6 and -8 <= z <= 8:
                m = "m_dark"
            v.put(x, -1, z, m)
    # 天井と光の板
    for x in range(-X, X + 1):
        for z in range(Z0, Z1 + 1):
            m = "m_dark"
            if z % 6 == 0 and abs(x) % 8 in (2, 3, 4):
                m = "g_white"
            v.put(x, H, z, m)
    # 壁（左右・後ろ）
    for y in range(0, H):
        for z in range(Z0, Z1 + 1):
            for x in (-X - 1, X + 1):
                m = "m_navy"
                if y == 7 and z % 2 == 0:
                    m = "g_blue"
                if y in (0, 1):
                    m = "m_dark"
                v.put(x, y, z, m)
        for x in range(-X, X + 1):
            m = "m_navy" if y > 1 else "m_dark"
            if abs(x) <= 2 and y <= 5:          # 後ろの扉
                m = "m_panel" if abs(x) <= 1 else "m_gray"
            v.put(x, y, Z0 - 1, m)
    # 前の壁と大きな窓（窓の所は空ける。縦の桟と横の桟）
    for y in range(0, H):
        for x in range(-X, X + 1):
            window = 2 <= y <= 9 and abs(x) <= 13
            mull = window and (abs(x) == 9 or y == 9)
            if window and not mull:
                continue
            v.put(x, y, Z1 + 1, "m_dark" if not mull else "m_gray")
    for x in range(-13, 14):                    # 窓の下の光の線
        v.put(x, 1, Z1 + 1, "g_blue" if x % 4 == 0 else "m_navy")
    # 窓ぎわの操作卓（左右2列）
    for side in (-1, 1):
        for x in range(4, 13):
            xx = side * x
            v.put(xx, 0, 10, "m_panel")
            v.put(xx, 1, 10, "m_panel")
            v.put(xx, 2, 10, "m_panel")
            if x % 3:
                v.put(xx, 2, 9, "m_screen")
            v.put(xx, 0, 9, "m_dark")
    # 立体映像の台（丸）
    for x in range(-4, 5):
        for z in range(-4, 5):
            r = math.hypot(x, z)
            if r <= 4.2:
                v.put(x, 0, z, "g_dim" if r > 3.3 else "m_dark")
    # 艦長席（後ろ）
    v.box(-9, -7, 0, 1, -9, -8, "m_panel")
    v.box(-9, -7, 2, 4, -10, -10, "m_wine")
    v.box(-10, -6, 0, 2, -12, -12, "m_panel")
    v.box(-10, -6, 3, 3, -12, -12, "g_screen")
    # 横の画面（左右の壁ぎわ）
    for side in (-1, 1):
        v.box(side * 15, side * 15, 3, 6, -6, 4, "g_screen")
        v.box(side * 14, side * 14, 0, 1, -6, 4, "m_panel")
    return v

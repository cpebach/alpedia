"""ブロックで作る船・部屋・小道具（ボクセルの並び）。

Vox に箱や形を置いていき、最後に voxels() でゲームの構築物の形式 [(格子, 素材, None)] にする。
中身の詰まった形は、外から見える殻だけを残して軽くする（hollow）。
"""
import numpy as np


class Vox:
    def __init__(self):
        self.cells = {}

    def put(self, x, y, z, mat):
        if mat is None:
            self.cells.pop((x, y, z), None)
        else:
            self.cells[(int(x), int(y), int(z))] = mat

    def box(self, x0, x1, y0, y1, z0, z1, mat, hollow=False):
        """x0〜x1（両端を含む）などの箱。"""
        for x in range(int(x0), int(x1) + 1):
            for y in range(int(y0), int(y1) + 1):
                for z in range(int(z0), int(z1) + 1):
                    if hollow and x0 < x < x1 and y0 < y < y1 and z0 < z < z1:
                        continue
                    self.put(x, y, z, mat)

    def fill(self, fn, bounds, mat):
        """fn(x, y, z) が真の所を mat に（mat は関数でもよい：fn と同じ引数で素材を返す）。"""
        (x0, x1), (y0, y1), (z0, z1) = bounds
        for x in range(x0, x1 + 1):
            for y in range(y0, y1 + 1):
                for z in range(z0, z1 + 1):
                    if fn(x, y, z):
                        self.put(x, y, z, mat(x, y, z) if callable(mat) else mat)

    def paint(self, fn, mat):
        """置いてある所のうち fn が真の所の素材を変える。"""
        for c in list(self.cells):
            if fn(*c):
                self.cells[c] = mat(*c) if callable(mat) else mat

    def mirror_x(self, center2=0):
        """x = center2/2 を軸に左右対称にする（右半分を写す）。"""
        for (x, y, z), m in list(self.cells.items()):
            self.cells[(center2 - x, y, z)] = m

    def hollow(self):
        """外から見えない中のブロックを取り除く（6方向すべてがブロックなら中）。"""
        cs = self.cells
        inner = [c for c in cs if all((c[0] + d[0], c[1] + d[1], c[2] + d[2]) in cs for d in
                                      ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)))]
        for c in inner:
            del cs[c]
        return self

    def merge(self, other, off=(0, 0, 0)):
        for (x, y, z), m in other.cells.items():
            self.cells[(x + off[0], y + off[1], z + off[2])] = m

    def voxels(self, keep=None):
        out = []
        for c, m in self.cells.items():
            if keep is None or keep(*c):
                out.append((c, m, None))
        return out

    def bounds(self):
        a = np.array(list(self.cells))
        return a.min(0), a.max(0)


# ---------------------------------------------------------------------------
# 船（前 = +Z、右 = +X、上 = +Y。格子 1 つ = size m）
# ---------------------------------------------------------------------------
def luna_cruiser(length=46, half_w=5, height=6, seed=0):
    """ルナ艦隊の巡洋艦：白い船体に赤い帯、青く光る窓の列、後ろに水色の噴射口、上に艦橋。"""
    v = Vox()
    L = length

    def section(z):
        """前（z = L）ほど細くなる断面の半幅と高さ。"""
        k = z / L
        if k > 0.72:
            f = 1 - (k - 0.72) / 0.28
            return max(1, round(half_w * (0.35 + 0.65 * f))), max(2, round(height * (0.45 + 0.55 * f)))
        return half_w, height

    for z in range(0, L + 1):
        hw, h = section(z)
        for x in range(0, hw + 1):
            for y in range(0, h + 1):
                # 角を落とした断面
                if x + (h - y) * 0.6 > hw + 1.5 or x + y * 0.5 > hw + 2.2:
                    continue
                mat = "m_white"
                if y == 0 or (y == 1 and x >= hw - 1):
                    mat = "m_navy"
                if y == h // 2 + 1 and x >= hw - 1:
                    mat = "m_red"
                v.put(x, y, z, mat)
    v.mirror_x()
    v.hollow()
    # 窓（側面に光る点の列）
    for z in range(6, int(L * 0.75), 3):
        for side in (-1, 1):
            hw, h = section(z)
            v.put(side * hw, h // 2 - 1, z, "g_blue")
            v.put(side * hw, h // 2 - 1, z + 1, "g_blue")
    # 艦橋
    bz = int(L * 0.38)
    v.box(-2, 2, height + 1, height + 3, bz, bz + 6, "m_white")
    v.box(-2, 2, height + 3, height + 3, bz + 6, bz + 6, "g_screen")
    v.box(-1, 1, height + 4, height + 4, bz + 1, bz + 4, "m_gray")
    v.box(0, 0, height + 5, height + 7, bz + 2, bz + 2, "m_gray")      # アンテナ
    # 砲塔
    for tz in (int(L * 0.62), int(L * 0.18)):
        v.box(-1, 1, height + 1, height + 1, tz, tz + 2, "m_gray")
        v.box(0, 0, height + 1, height + 1, tz + 3, tz + 7, "m_dark")
    # 噴射口（後ろ）
    for x in (-3, 0, 3):
        for y in (2, 4):
            v.put(x, y, -1, "m_dark")
            v.put(x, y, -2, "g_cyan")
    v.box(-4, 4, 1, 5, 0, 0, "m_gray")
    # 翼（短い）
    v.box(half_w + 1, half_w + 5, 1, 1, 8, 16, "m_white")
    v.box(-half_w - 5, -half_w - 1, 1, 1, 8, 16, "m_white")
    v.box(half_w + 4, half_w + 5, 1, 1, 8, 9, "m_red")
    v.box(-half_w - 5, -half_w - 4, 1, 1, 8, 9, "m_red")
    return v


def artemis(length=72):
    """旗艦アルテミス：大きな巡洋艦の形に、前の長い主砲（光の砲口）と左右の大きな翼、高い艦橋。"""
    v = luna_cruiser(length=length, half_w=7, height=8)
    L = length
    # 主砲：船首から前へ伸びる砲身
    v.box(-1, 1, 3, 5, L - 2, L + 14, "m_gray")
    v.box(-2, 2, 2, 6, L - 6, L - 1, "m_white")
    v.box(-1, 1, 3, 5, L + 15, L + 15, "g_cyan")
    v.box(-2, 2, 2, 2, L + 2, L + 12, "m_red")
    # 大きな翼
    v.box(9, 22, 2, 3, 14, 30, "m_white")
    v.box(-22, -9, 2, 3, 14, 30, "m_white")
    v.box(18, 22, 2, 3, 14, 18, "m_red")
    v.box(-22, -18, 2, 3, 14, 18, "m_red")
    for x in range(10, 22, 3):
        v.put(x, 4, 22, "g_blue")
        v.put(-x, 4, 22, "g_blue")
    # 高い艦橋（前に大きな窓）
    bz = int(L * 0.4)
    v.box(-3, 3, 9, 14, bz, bz + 9, "m_white")
    v.box(-3, 3, 13, 13, bz + 9, bz + 9, "g_screen")
    v.box(-2, 2, 12, 12, bz + 9, bz + 9, "g_screen")
    v.box(-4, 4, 15, 15, bz + 1, bz + 8, "m_red")
    v.box(0, 0, 16, 21, bz + 4, bz + 4, "m_gray")
    v.box(-1, 1, 19, 19, bz + 4, bz + 4, "g_red")
    return v


def kurogane_cruiser(length=52, half_w=8, seed=0):
    """黒鉄艦隊の巡洋艦：黒い矢じりの形、赤く光るすき間、背中のとげ。"""
    v = Vox()
    L = length
    for z in range(0, L + 1):
        k = z / L
        hw = max(0, round(half_w * (1 - k) ** 0.9))
        h = max(1, round(5 * (1 - k * 0.7)))
        for x in range(0, hw + 1):
            top = round(h * (1 - (x / max(hw, 1)) ** 2 * 0.8))
            for y in range(-1, top + 1):
                mat = "m_black" if (x + z) % 7 else "m_dark"
                if y == -1:
                    mat = "m_wine"
                v.put(x, y, z, mat)
    v.mirror_x()
    v.hollow()
    # 赤いすき間（側面の線）
    for z in range(4, int(L * 0.8)):
        k = z / L
        hw = max(0, round(half_w * (1 - k) ** 0.9))
        if z % 5 != 0:
            v.put(hw, 0, z, "g_red")
            v.put(-hw, 0, z, "g_red")
    # 背中のとげと艦橋
    for z in range(6, int(L * 0.6), 6):
        v.box(0, 0, 5, 5 + (z % 12) // 3 + 2, z, z + 1, "m_dark")
    bz = int(L * 0.25)
    v.box(-2, 2, 5, 8, bz, bz + 5, "m_black")
    v.box(-2, 2, 8, 8, bz + 5, bz + 5, "g_red")
    # 噴射口
    v.box(-6, 6, 0, 4, -1, -1, "m_dark")
    for x in (-4, 0, 4):
        v.put(x, 2, -2, "g_red")
        v.put(x, 2, -3, "g_amber")
    # ミサイルの発射口（上面の穴の列）
    for x in (-3, 3):
        for z in range(int(L * 0.3), int(L * 0.55), 3):
            v.put(x, 4, z, "g_amber")
    return v


def kurogane_flagship(length=80):
    v = kurogane_cruiser(length=length, half_w=13)
    L = length
    # 大きな角
    for i in range(12):
        v.box(9 + i // 3, 10 + i // 3, 2 + i // 2, 2 + i // 2, int(L * 0.35) + i * 2, int(L * 0.35) + i * 2 + 2, "m_dark")
        v.box(-10 - i // 3, -9 - i // 3, 2 + i // 2, 2 + i // 2, int(L * 0.35) + i * 2, int(L * 0.35) + i * 2 + 2, "m_dark")
    v.box(-3, 3, 6, 12, int(L * 0.2), int(L * 0.2) + 8, "m_black")
    v.box(-3, 3, 11, 11, int(L * 0.2) + 8, int(L * 0.2) + 8, "g_red")
    return v


def missile():
    """ミサイル（0.5 m のブロック）：白い胴、赤い先、黒い尾翼。前 = +Z。"""
    v = Vox()
    v.box(0, 0, 0, 0, 0, 5, "m_white")
    v.put(0, 0, 6, "m_red")
    v.put(1, 0, 0, "m_dark")
    v.put(-1, 0, 0, "m_dark")
    v.put(0, 1, 0, "m_dark")
    v.put(0, -1, 0, "m_dark")
    return v


def kmissile():
    """黒鉄のミサイル：細長い灰色の胴に赤い帯、赤く光る先、後ろに小さな尾翼。前 = +Z。"""
    v = Vox()
    v.box(0, 0, 0, 0, 0, 8, "m_gray")
    v.put(0, 0, 2, "m_wine")
    v.put(0, 0, 6, "m_wine")
    v.put(0, 0, 9, "g_red")
    for d in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        v.put(d[0], d[1], 0, "m_dark")
    return v


def far_hull(length=40, w=8, h=5, mat="m_white", band="m_red", light="g_blue"):
    """遠くの船（張りぼて）：箱に帯と光る窓だけ。ブロックの数が少なく、作るのも描くのも軽い。"""
    v = Vox()
    v.box(-w // 2, w // 2, 0, h, 0, length, mat, hollow=True)
    v.box(-w // 2 + 1, w // 2 - 1, 1, h - 1, length + 1, length + 4, mat, hollow=True)
    v.paint(lambda x, y, z: y == h // 2 and abs(x) == w // 2, band)
    for z in range(4, length, 4):
        v.put(w // 2, h // 2 + 1, z, light)
        v.put(-w // 2, h // 2 + 1, z, light)
    v.box(-2, 2, 1, h - 1, -1, -1, light)
    return v


def cut(vox, fn):
    """形を2つに分ける（fn が真の側と偽の側）。"""
    a, b = Vox(), Vox()
    for c, m in vox.cells.items():
        (a if fn(*c) else b).cells[c] = m
    return a, b


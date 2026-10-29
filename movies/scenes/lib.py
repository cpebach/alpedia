"""本編のムービーで共通に使うもの：配役（VRM）・ポーズ・舞台（ブロックの部屋・船・都市・ロボット・ロケット）。"""
import math
import sys

import numpy as np

from cinema import anim, fx, shapes
from cinema.actor import Actor
from cinema.shapes import Vox
from cinema.stage import DEV

# ---------------------------------------------------------------------------
# 配役：キー → (VRM のファイル, 字幕の名前)
# ---------------------------------------------------------------------------
CAST = {
    "towa": ("towa.vrm", "トワ"), "mikaga": ("mikaga.vrm", "ミカガ"), "nemuri": ("nemuri.vrm", "ネムリ"),
    "sakura": ("sakura.vrm", "サクラ"), "siyama": ("siyama.vrm", "ヤマシナ"), "sirotsu": ("sirotsu.vrm", "シロツ"),
    "kensu": ("kensu.vrm", "ケンス"), "kingi": ("kingi.vrm", "キンギ"), "donachi": ("donachi.vrm", "ドナチ"),
    "nekosa": ("nekosa.vrm", "先生"), "dabai": ("dabai.vrm", "ダバイ"), "rutero": ("rutero.vrm", "ルテロ"),
    "omiku": ("omiku.vrm", "オミク"),
}
NAME = {k: v[1] for k, v in CAST.items()}


def vrm_path(key):
    return DEV / "mods" / "skinmodel(dummy)" / CAST[key][0]


def actor(st, key, pos=(0, 0, 0), yaw=0.0, scale=1.0, label=None):
    a = Actor(st, vrm_path(key), label or key, scale=scale)
    p = np.asarray(pos, float)
    a.place = lambda t: (p, yaw)
    return a


def still(pos, yaw):
    p = np.asarray(pos, float)
    return lambda t: (p, yaw)


def walk(keys):
    """[(時刻, (x, y, z), 向き)] を順になめらかに（足元の動きから歩きになる）。"""
    tp = anim.Track(*[(k[0], k[1], "inout") for k in keys])
    ty = anim.Track(*[(k[0], k[2], "inout") for k in keys])
    return lambda t: (tp(t), float(ty(t)))


def blink_every(t0, t1, gap=3.1, off=0.0):
    out, t = [], t0 + 0.6 + off
    while t < t1:
        out.append(t)
        t += gap + 0.7 * math.sin(t * 3.3)
    return out


def yaw_to(src, dst):
    d = np.asarray(dst, float) - np.asarray(src, float)
    return math.degrees(math.atan2(d[0], d[2]))


# ---------------------------------------------------------------------------
# ポーズ（かわいいモーション MOD と同じ書き方。腕・脚は (外, 上, 前)、胴・顔は (右, 上, 前)）
# ---------------------------------------------------------------------------
P = {
    "stand": {},
    "hands_back": {"arm_l": (0.3, -1.0, -0.35), "arm_r": (0.3, -1.0, -0.35), "fore_l": (-0.75, -0.45, -0.5),
                   "fore_r": (-0.75, -0.45, -0.5)},
    "crossed": {"arm_l": (0.3, -0.5, 0.8), "fore_l": (-0.95, 0.15, 0.25), "arm_r": (0.3, -0.5, 0.8),
                "fore_r": (-0.95, 0.25, 0.15), "chest": (0.0, 1.0, -0.05)},
    "hand_chest": {"arm_r": (0.25, -0.55, 0.6), "fore_r": (-0.75, 0.55, 0.35)},
    "point": {"arm_r": (0.35, 0.15, 0.95), "fore_r": (0.2, 0.15, 1.0)},
    "reach": {"arm_r": (0.3, -0.15, 0.95), "fore_r": (0.25, 0.05, 1.0)},
    "command": {"arm_r": (0.9, 0.05, 0.55), "fore_r": (0.85, 0.2, 0.7), "chest": (0.0, 1.0, 0.07)},
    "wave": {"arm_r": (0.85, 0.55, 0.2), "fore_r": (0.2, 1.0, 0.15)},
    "wave_big": {"arm_r": (0.6, 0.9, 0.1), "fore_r": (0.1, 1.0, 0.05), "arm_l": (0.6, 0.9, 0.1),
                 "fore_l": (0.1, 1.0, 0.05)},
    "camera": {"arm_r": (0.15, -0.35, 0.8), "fore_r": (-0.55, 0.75, 0.4), "arm_l": (0.15, -0.35, 0.8),
               "fore_l": (-0.55, 0.75, 0.4), "head": (0.0, -0.05, 1.0)},
    "head_hold": {"arm_r": (0.45, 0.2, 0.55), "fore_r": (-0.35, 0.85, 0.2), "chest": (0.0, 1.0, 0.15),
                  "head": (0.1, -0.35, 1.0), "tilt": 0.1},
    "smoke": {"arm_r": (0.25, -0.5, 0.55), "fore_r": (-0.6, 0.75, 0.3), "arm_l": (0.3, -0.5, 0.8),
              "fore_l": (-0.95, 0.15, 0.25)},
    "hands_pockets": {"arm_l": (0.35, -1.0, -0.05), "arm_r": (0.35, -1.0, -0.05), "fore_l": (0.1, -1.0, 0.35),
                      "fore_r": (0.1, -1.0, 0.35)},
    "watch": {"arm_l": (0.2, -0.7, 0.55), "fore_l": (-0.4, 0.15, 0.9), "head": (0.05, -0.3, 1.0)},
    "sit": {"seated": True},
    "sit_watch": {"seated": True, "arm_l": (0.2, -0.7, 0.55), "fore_l": (-0.4, 0.15, 0.9)},
    "grip": {"seated": True, "arm_l": (0.25, -0.75, 0.6), "fore_l": (0.05, -0.2, 1.0), "arm_r": (0.25, -0.75, 0.6),
             "fore_r": (0.05, -0.2, 1.0), "chest": (0.0, 1.0, 0.08)},
    "slump": {"chest": (0.0, 0.7, 0.7), "spine": (0.0, 0.85, 0.5), "head": (0.0, -0.6, 1.0),
              "arm_l": (0.3, -1.0, 0.3), "arm_r": (0.3, -1.0, 0.3), "bob": -0.25},
    "salute": {"arm_r": (0.75, 0.3, 0.45), "fore_r": (-0.75, 0.6, 0.15)},
    "bow": {"spine": (0.0, 0.85, 0.5), "chest": (0.0, 0.9, 0.3), "head": (0.0, -0.4, 1.0)},
    "fist": {"arm_r": (0.3, -0.85, 0.35), "fore_r": (0.0, -0.2, 1.0), "chest": (0.0, 1.0, 0.04)},
    "type": {"arm_l": (0.25, -0.8, 0.55), "fore_l": (0.1, -0.3, 1.0), "arm_r": (0.25, -0.8, 0.55),
             "fore_r": (0.1, -0.3, 1.0), "head": (0.0, -0.35, 1.0), "chest": (0.0, 1.0, 0.12)},
    "cheer": {"arm_r": (0.4, 1.0, 0.1), "fore_r": (0.0, 1.0, 0.1), "arm_l": (0.3, -1.0, 0.0)},
}


def pose(name, **over):
    d = dict(P[name])
    d.update(over)
    return d


# ---------------------------------------------------------------------------
# ゲームの構築物（ストーリー MOD のルミナス号など）
# ---------------------------------------------------------------------------
def story_sites():
    if "storycode" not in sys.modules:
        import importlib.util
        d = DEV / "mods" / "story" / "code"
        spec = importlib.util.spec_from_file_location("storycode", d / "__init__.py", submodule_search_locations=[str(d)])
        m = importlib.util.module_from_spec(spec)
        sys.modules["storycode"] = m
        spec.loader.exec_module(m)
    import storycode.sites as S
    return S


def luminous_voxels():
    """ルミナス号（修理後。ゲームと同じブロック）"""
    return story_sites().ship_voxels()


# ---------------------------------------------------------------------------
# 部屋（0.5 m のブロック。床の上面 y = 0、真ん中 x = z = 0）
# ---------------------------------------------------------------------------
def room(X, Z0, Z1, H, floor="m_floor", wall="m_navy", ceil="m_dark", light="g_white", strip="g_blue",
         window=None, window_mat=None):
    """箱の部屋。window=(壁, 下, 上, 半幅)：壁は 'front'（+Z）。"""
    v = Vox()
    for x in range(-X, X + 1):
        for z in range(Z0, Z1 + 1):
            v.put(x, -1, z, floor)
            v.put(x, H, z, light if (z % 6 == 0 and abs(x) % 8 in (2, 3)) else ceil)
    for y in range(0, H):
        for z in range(Z0, Z1 + 1):
            for x in (-X - 1, X + 1):
                v.put(x, y, z, strip if (y == 7 and z % 3 == 0 and strip) else wall)
        for x in range(-X, X + 1):
            v.put(x, y, Z0 - 1, wall)
            win = window is not None and window[1] <= y <= window[2] and abs(x) <= window[3]
            if win:
                if window_mat:
                    v.put(x, y, Z1 + 1, window_mat)
                continue
            v.put(x, y, Z1 + 1, wall)
    return v


def letter_N(v, x0, y0, z, mat, h=7):
    """壁に「N」の印。"""
    for y in range(h):
        v.put(x0, y0 + y, z, mat)
        v.put(x0 + h - 1, y0 + y, z, mat)
        v.put(x0 + y, y0 + h - 1 - y, z, mat)


def base_interior():
    """観測基地 ALP-7 の中（古い研究施設）。奥に古い端末、壁に N の印。"""
    v = room(12, -10, 10, 9, floor="m_panel", wall="m_gray", ceil="m_dark", light="g_white", strip=None)
    v.box(-4, 4, 0, 2, 7, 8, "m_dark")              # 端末の机
    v.box(-3, 3, 3, 7, 9, 9, "m_black")             # 画面の枠
    v.box(-2, 2, 4, 6, 8, 8, "g_green")             # 古い緑の画面
    for x in (-10, -9, 9, 10):                       # 棚
        v.box(x, x, 0, 6, -6, 4, "m_wine" if x < 0 else "m_panel")
    letter_N(v, -6, 3, -11 + 1, "m_dark")
    v.box(5, 9, 2, 5, -10, -10, "m_floor")          # 古い写真の額
    return v


def base_exterior():
    """観測基地 ALP-7 の外観（コンクリートの箱・アンテナ・皿）。1 m のブロック。"""
    v = Vox()
    v.box(-8, 8, 0, 5, -6, 6, "m_gray", hollow=True)
    v.box(-9, 9, 6, 6, -7, 7, "m_dark")
    v.box(-3, 3, 0, 3, 7, 7, "m_dark")              # 入口
    v.box(-1, 1, 0, 2, 7, 7, "g_amber")
    for x in range(-7, 8, 3):
        v.put(x, 3, 7, "g_screen")
    v.box(5, 5, 7, 15, 3, 3, "m_gray")              # アンテナ
    v.box(4, 6, 15, 15, 2, 4, "g_red")
    for a in range(16):                              # 皿
        th = a / 16 * 2 * math.pi
        v.put(-4 + round(math.cos(th) * 3), 9 + round(math.sin(th) * 1.5), round(math.sin(th) * 3), "m_white")
    v.box(-4, -4, 7, 9, 0, 0, "m_gray")
    return v


def imperial_room():
    """帝国の総統室（黒と赤。大きな窓の向こうに月面の街）。"""
    v = room(14, -10, 12, 12, floor="m_black", wall="m_dark", ceil="m_black", light="g_red", strip="g_red",
             window=("front", 2, 10, 10))
    for x in (-11, -5, 5, 11):                      # 赤い垂れ幕
        v.box(x, x + 1, 2, 10, -11 + 1, -11 + 1, "m_wine")
    v.box(-4, 4, 0, 0, -3, -1, "m_black")           # 机（高さ 1 m）
    v.box(-4, 4, 1, 1, -3, -1, "m_wine")
    v.box(-1, 1, 0, 4, -6, -5, "m_wine")            # 椅子の背
    for x in range(-10, 11):                         # 窓の下の赤い線
        v.put(x, 1, 13, "g_red" if x % 3 == 0 else "m_dark")
    return v


def lounge():
    """ルナ・ポートのラウンジ（木の床・ソファ・低い机・窓）。"""
    v = room(12, -8, 10, 9, floor="m_wood", wall="m_cream", ceil="m_cream", light="g_amber", strip=None,
             window=("front", 2, 7, 8))
    v.box(-5, 1, 0, 0, 4, 5, "m_wine")              # ソファ（座面の高さ 0.5 m）
    v.box(-5, 1, 1, 1, 6, 6, "m_wine")
    v.box(-3, 1, 0, 0, 0, 2, "m_dark")              # 低い机
    v.box(6, 7, 0, 5, -4, -3, "m_wood")             # 棚
    v.put(6, 6, -4, "g_amber")
    return v


def archive():
    """資料室（暗い。サーバーの棚の赤や緑の光、大きな画面）。"""
    v = room(12, -10, 10, 9, floor="m_dark", wall="m_black", ceil="m_black", light="g_dim", strip=None)
    for x in (-9, -5, 5, 9):
        v.box(x, x + 1, 0, 6, -8, 4, "m_black")
        for z in range(-8, 5, 2):
            for y in (1, 3, 5):
                v.put(x + (1 if x > 0 else 0) - (0 if x > 0 else 0), y, z, "g_green" if (z + y + x) % 3 else "g_red")
    v.box(-4, 4, 2, 7, 10, 10, "m_black")
    v.box(-3, 3, 3, 6, 9, 9, "m_black")             # 画面（中の絵は光の線で描く）
    v.box(-2, 2, 0, 1, 7, 8, "m_panel")
    return v


def city_core():
    """落ちていく軌道都市アマツの中心の広間（非常灯の赤、大きな窓）。"""
    v = room(16, -12, 14, 14, floor="m_panel", wall="m_gray", ceil="m_dark", light="g_red", strip="g_red",
             window=("front", 3, 12, 12))
    for x in (-13, 13):                              # 柱
        v.box(x, x + 1, 0, 13, -10, 12, "m_dark")
    v.box(-3, 3, 0, 0, -3, 3, "m_dark")              # 中央の台
    for x in range(-12, 13):
        v.put(x, 2, 15, "g_red" if x % 4 == 0 else "m_dark")
    return v


def calc_room():
    """計算室（白い机、立体映像の台、壁の画面）。"""
    v = room(12, -8, 10, 9, floor="m_floor", wall="m_white", ceil="m_panel", light="g_white", strip="g_blue")
    for x in range(-3, 4):
        for z in range(-3, 4):
            if x * x + z * z <= 10:
                v.put(x, 0, z, "g_dim" if x * x + z * z > 6 else "m_dark")
    v.box(-10, 10, 2, 7, 11, 11, "g_screen")
    v.box(-9, -6, 0, 1, 4, 6, "m_panel")
    v.box(6, 9, 0, 1, 4, 6, "m_panel")
    return v


def courtroom():
    """法廷（段になった席、中央の証言台、高い裁判官席）。"""
    v = room(16, -14, 12, 14, floor="m_wood", wall="m_cream", ceil="m_cream", light="g_amber", strip=None)
    v.box(-8, 8, 0, 5, 9, 11, "m_wood")             # 裁判官席
    v.box(-8, 8, 6, 6, 9, 11, "m_dark")
    for k, z in enumerate((-6, -9, -12)):            # 傍聴席
        v.box(-14, -3, 0, 1 + k, z, z + 1, "m_wood")
        v.box(3, 14, 0, 1 + k, z, z + 1, "m_wood")
    v.box(-1, 1, 0, 1, 1, 1, "m_wood")              # 証言台
    return v


def nocturne():
    """夜会：暗い部屋の長い机と燭台。壁に N の印。"""
    v = room(10, -8, 8, 8, floor="m_black", wall="m_black", ceil="m_black", light="m_black", strip=None)
    v.box(-2, 2, 0, 1, -6, 6, "m_wine")
    v.box(-2, 2, 2, 2, -6, 6, "m_black")
    letter_N(v, -3, 2, 9, "g_amber", 6)
    return v


# ---------------------------------------------------------------------------
# 外の物
# ---------------------------------------------------------------------------
def pod():
    """脱出ポッド（0.5 m のブロック。高さ約 3.5 m の円すい形。前 = +Z が窓）。"""
    v = Vox()
    for y in range(0, 7):
        r = 3.2 - y * 0.35
        for x in range(-4, 5):
            for z in range(-4, 5):
                if x * x + z * z <= r * r:
                    v.put(x, y, z, "m_white" if y not in (1, 2) else "m_dark")
    v.hollow()
    v.put(0, 4, 2, "g_screen")
    v.put(1, 4, 2, "g_screen")
    v.box(-1, 1, 7, 7, -1, 1, "m_red")
    v.box(-2, 2, -1, -1, -2, 2, "m_dark")
    return v


def pod_burnt():
    v = pod()
    v.paint(lambda x, y, z: y <= 2 or (x + z) % 3 == 0, "m_black")
    return v


def drone():
    """キンギの偵察ドローン（黒い体に赤い目、4つのプロペラの腕）。0.5 m。"""
    v = Vox()
    v.box(-1, 1, 0, 1, -1, 1, "m_black")
    v.put(0, 0, 2, "g_red")
    for dx, dz in ((1, 1), (-1, 1), (1, -1), (-1, -1)):
        v.put(dx * 2, 1, dz * 2, "m_dark")
        v.put(dx * 3, 1, dz * 3, "m_gray")
    return v


def amatsu(length=60, radius=16):
    """軌道都市アマツ（6 m のブロックで約 360 m の円筒）。外周に窓の光、両端に推進のブロック。前 = +Z。"""
    v = Vox()
    for z in range(0, length + 1):
        for k in range(int(2 * math.pi * radius)):
            th = k / (2 * math.pi * radius) * 2 * math.pi
            x, y = round(math.cos(th) * radius), round(math.sin(th) * radius)
            m = "m_white"
            if z % 6 == 3 and k % 3 != 0:
                m = "g_amber"
            elif z % 6 == 0:
                m = "m_gray"
            v.put(x, y, z, m)
    for end, sgn in ((0, -1), (length, 1)):          # 両端のふた
        for x in range(-radius, radius + 1):
            for y in range(-radius, radius + 1):
                if x * x + y * y <= radius * radius:
                    v.put(x, y, end + sgn, "m_gray" if (x * x + y * y) > (radius - 3) ** 2 else "m_panel")
    v.box(-2, 2, -2, 2, length + 2, length + 10, "m_gray")    # 港の塔
    for k in range(8):                                # 推進のブロック（外周）
        th = k / 8 * 2 * math.pi
        x, y = round(math.cos(th) * (radius + 2)), round(math.sin(th) * (radius + 2))
        v.box(x - 1, x + 1, y - 1, y + 1, -3, 2, "m_dark")
        v.put(x, y, -4, "g_cyan")
    return v


AMATSU_SIZE = 6.0


def rocket():
    """旅立ちのロケット（ソユーズに似た形。1 m のブロック。上 = +Y）。"""
    v = Vox()

    def cyl(cx, cz, y0, y1, r0, r1, mat):
        for y in range(y0, y1 + 1):
            k = (y - y0) / max(y1 - y0, 1)
            r = r0 + (r1 - r0) * k
            for x in range(-4, 5):
                for z in range(-4, 5):
                    if x * x + z * z <= r * r + 0.3:
                        v.put(cx + x, y, cz + z, mat(y) if callable(mat) else mat)
    cyl(0, 0, 0, 26, 1.9, 1.9, lambda y: "m_white" if y % 7 else "m_gray")
    cyl(0, 0, 27, 31, 1.9, 1.2, "m_navy")
    cyl(0, 0, 32, 36, 1.2, 0.4, "m_white")
    v.box(0, 0, 37, 40, 0, 0, "m_gray")
    for dx, dz in ((3, 0), (-3, 0), (0, 3), (0, -3)):
        cyl(dx, dz, 0, 10, 1.1, 1.1, "m_white")
        cyl(dx, dz, 11, 15, 1.1, 0.2, "m_white")
    v.paint(lambda x, y, z: y in (24, 25) and abs(x) + abs(z) >= 2, "m_red")
    v.hollow()
    return v


def launch_pad():
    v = Vox()
    v.box(-12, 12, -1, 0, -12, 12, "m_gray")
    v.box(-3, 3, -1, 0, -3, 3, "m_dark")
    for x in (-8, 8):                                 # 支柱の塔
        v.box(x, x + 1, 1, 34, -1, 0, "m_red" if x < 0 else "m_gray")
        for y in range(4, 34, 5):
            v.box(min(x, 0), max(x, 0), y, y, -1, 0, "m_gray")
    return v


def dome(r=6, mat="m_glass", ring="m_white"):
    v = Vox()
    for x in range(-r, r + 1):
        for y in range(0, r + 1):
            for z in range(-r, r + 1):
                d = math.sqrt(x * x + y * y + z * z)
                if r - 0.8 <= d <= r + 0.2:
                    v.put(x, y, z, ring if y <= 1 else mat)
    return v


def plaza():
    """ルナ・ポートの広場（床・柵・立体映像の大きな柱）。1 m のブロック。"""
    v = Vox()
    for x in range(-30, 31):
        for z in range(-30, 31):
            v.put(x, -1, z, "m_panel" if (x // 6 + z // 6) % 2 else "m_floor")
    for x in range(-30, 31, 2):
        v.put(x, 0, 30, "g_blue")
        v.put(x, 0, -30, "g_blue")
    v.box(-1, 1, 0, 1, 10, 12, "m_dark")              # 立体映像の台
    v.box(-2, 2, 0, 0, 9, 13, "g_cyan")
    return v


# ---------------------------------------------------------------------------
# ロボット（ブロックの人型。部分ごとの剛体を関節で動かす）
# ---------------------------------------------------------------------------
class Robot:
    """高さ約 8 m のブロックのロボット。pose(t) → {関節: 角度（度）} と、root(t) → (位置, 向きの行列) で動かす。
    関節：waist（腰のひねり）, sh_l / sh_r（肩：前後）, ab_l / ab_r（肩：横に開く）, el_l / el_r（ひじ）,
    hip_l / hip_r（脚：前後）, kn_l / kn_r（ひざ）, head（首：左右）"""

    def __init__(self, st, body="m_white", trim="m_navy", eye="g_cyan", size=0.5):
        s = size
        self.st, self.s = st, s
        st.add(self)                    # 部分より先に動かす（部分の位置を決めてから描く）

        def part(build, pivot):
            v = Vox()
            build(v)
            return st.ship(v.voxels(), size=s, pivot=np.asarray(pivot, float) * s)
        self.torso = part(lambda v: (v.box(-4, 4, 0, 6, -2, 2, body), v.box(-3, 3, 2, 4, 3, 3, trim),
                                     v.box(-1, 1, 5, 5, 3, 3, eye), v.box(-5, 5, 6, 6, -2, 2, trim)), (0, 0, 0))
        self.head = part(lambda v: (v.box(-2, 2, 0, 3, -2, 2, body), v.box(-2, 2, 2, 2, 3, 3, eye),
                                    v.box(0, 0, 4, 5, 0, 0, trim)), (0, 0, 0))
        self.hips = part(lambda v: v.box(-3, 3, -2, -1, -2, 2, trim), (0, 0, 0))
        self.parts = {}
        for side, sx in (("l", -1), ("r", 1)):
            self.parts["ua_" + side] = part(lambda v, sx=sx: (v.box(-1, 1, -5, 0, -1, 1, body),
                                                              v.box(-2, 2, -1, 1, -2, 2, trim)), (0, 0, 0))
            self.parts["fa_" + side] = part(lambda v: (v.box(-1, 1, -5, 0, -1, 1, body),
                                                       v.box(-1, 1, -6, -6, -1, 1, trim)), (0, 0, 0))
            self.parts["ul_" + side] = part(lambda v: v.box(-1, 1, -6, 0, -1, 1, trim), (0, 0, 0))
            self.parts["ll_" + side] = part(lambda v: (v.box(-1, 1, -6, 0, -1, 1, body),
                                                       v.box(-2, 2, -7, -7, -2, 3, trim)), (0, 0, 0))
        self.root = lambda t: (np.zeros(3), np.eye(3))
        self.pose = lambda t: {}

    def joints(self, t):
        """各部分の (位置, 回転)。"""
        s = self.s
        P0, R0 = self.root(t)
        P0 = np.asarray(P0, float)
        j = self.pose(t)
        A = anim.axis_angle
        g = lambda k: float(j.get(k, 0.0))  # noqa: E731
        Rw = R0 @ A((0, 1, 0), g("waist"))
        out = {"hips": (P0, R0), "torso": (P0, Rw)}
        out["head"] = (P0 + Rw @ (np.array([0, 7, 0]) * s), Rw @ A((0, 1, 0), g("head")))
        for side, sx in (("l", -1), ("r", 1)):
            sh = P0 + Rw @ (np.array([sx * 6, 5, 0]) * s)
            Ru = Rw @ A((1, 0, 0), -g("sh_" + side)) @ A((0, 0, 1), sx * g("ab_" + side))
            out["ua_" + side] = (sh, Ru)
            el = sh + Ru @ (np.array([0, -5, 0]) * s)
            out["fa_" + side] = (el, Ru @ A((1, 0, 0), -g("el_" + side)))
            hp = P0 + R0 @ (np.array([sx * 2, -2, 0]) * s)
            Rl = R0 @ A((1, 0, 0), -g("hip_" + side))
            out["ul_" + side] = (hp, Rl)
            kn = hp + Rl @ (np.array([0, -6, 0]) * s)
            out["ll_" + side] = (kn, Rl @ A((1, 0, 0), g("kn_" + side)))
        return out

    def update(self, t, origin):
        J = self.joints(t)
        for name, sh in (("torso", self.torso), ("head", self.head), ("hips", self.hips)):
            sh.place(*J[name])
        for k, sh in self.parts.items():
            sh.place(*J[k])

    def cockpit(self, t):
        """胸の中（操縦席）の位置。"""
        P0, R0 = self.root(t)
        return np.asarray(P0, float) + R0 @ (np.array([0, 4, 2.6]) * self.s)

    def height(self):
        return 16 * self.s


def walk_cycle(t, speed=1.0, amp=25.0):
    ph = t * speed * 2 * math.pi
    s = math.sin(ph)
    return {"hip_l": amp * s, "hip_r": -amp * s, "kn_l": max(0, -s) * amp * 1.2, "kn_r": max(0, s) * amp * 1.2,
            "sh_l": -amp * 0.6 * s, "sh_r": amp * 0.6 * s, "el_l": 20, "el_r": 20}


# ---------------------------------------------------------------------------
# 小さな仕掛け
# ---------------------------------------------------------------------------
def cigarette(glow, smoke, a, t0=-1e9, t1=1e9, lit=True):
    """くわえ煙草：口もとの火（光）と、上る煙。"""
    if lit:
        glow.add(fx.Glow(lambda t: a.mouth_pos(t) + a.face_dir(t) * 0.07 if t0 <= t <= t1 else None,
                         lambda t: 0.05 + 0.015 * math.sin(t * 7), (1.0, 0.45, 0.15, 0.9)))
        smoke.add(fx.Trail(lambda te: a.mouth_pos(te) + a.face_dir(te) * 0.08 if t0 <= te <= t1 else None,
                           max(t0, -5), min(t1, 400), rate=7, life=2.5, size=(0.04, 0.35),
                           color=(0.75, 0.75, 0.78, 0.35), drift=0.12, seed=31))


def thruster_flames(glow, beam, ship, locals_, dirs=(0, -1, 0), on=lambda t: 1.0, size=1.0, color=(1.0, 0.65, 0.3)):
    """船の噴射の炎（光と線）。locals_：噴射口のローカル位置（m）、dirs：噴く向き（ローカル）。"""
    d = np.asarray(dirs, float)
    for p in locals_:
        p = np.asarray(p, float)
        glow.add(fx.Glow(lambda t, p=p: ship.world(p) if on(t) > 0.02 else None,
                         lambda t: size * (1.2 + 0.25 * math.sin(t * 41)) * on(t), color + (0.9,)))
        beam.add(fx.Beam(lambda t, p=p: (ship.world(p), ship.world(p + d * size * 3.0 * on(t)), size * 0.7,
                                         color + (0.8 * on(t),)) if on(t) > 0.02 else None))


def space(st, terra=None, luna=None, stars=True):
    if stars:
        fx.Stars(st, dist=st.far * 0.85)
    out = {}
    if terra is not None:
        out["terra"] = fx.Planet(st, "terra", terra[0], scale=terra[1], spin=0.08,
                                 tilt=terra[2] if len(terra) > 2 else None)
    if luna is not None:
        out["luna"] = fx.Planet(st, "luna", luna[0], scale=luna[1], style="luna")
    return out


def fleet_ship(kind="union", **kw):
    """連盟（白と紺）／帝国（黒と赤）の艦。"""
    if kind == "union":
        v = shapes.luna_cruiser(**kw)
        v.paint(lambda x, y, z: v.cells[(x, y, z)] == "m_red", "m_navy")
        return v
    if kind == "union_flag":
        v = shapes.artemis()
        v.paint(lambda x, y, z: v.cells[(x, y, z)] == "m_red", "m_navy")
        return v
    if kind == "empire_flag":
        return shapes.kurogane_flagship()
    return shapes.kurogane_cruiser(**kw)


def bridge(kind="union"):
    """艦橋（パイロット版の部屋）。union：連盟（白と紺・水色の光）、empire：帝国（黒と赤）。"""
    from scenes import sets
    v = sets.bridge()
    if kind == "empire":
        swap = {"m_navy": "m_dark", "g_blue": "g_red", "g_dim": "m_wine", "m_floor": "m_black", "g_screen": "g_red",
                "m_screen": "m_wine", "g_cyan": "g_red", "m_panel": "m_dark", "g_white": "g_amber"}
        v.paint(lambda x, y, z: v.cells[(x, y, z)] in swap, lambda x, y, z: swap[v.cells[(x, y, z)]])
    return v

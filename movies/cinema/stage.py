"""撮影所：ゲームの描画（ブロックの構築物・惑星）を、画面なし（オフスクリーン）で 1 コマずつ描く。

描くのはゲームと同じ Rust の画面（alpedia_view）。窓は見えないまま開き、描く先を「窓の外の画」（set_target）にして、
描いた画素を読み出す（read_target）。ピントのぼかしも alpedia_view がかける（set_dof）。

位置はすべてワールド座標（m）で書く。描くときはゲームと同じ「浮動原点」で、カメラの位置を原点にしてから
GPU に渡す（遠くても精度が落ちない）。
"""
import math
import sys
from pathlib import Path

import numpy as np

MOVIES = Path(__file__).resolve().parents[1]
DEV = MOVIES.parent / "dev"
if str(DEV) not in sys.path:
    sys.path.insert(0, str(DEV))

from . import anim  # noqa: E402

# 映画用の素材（色だけ。光る素材は 1 より大きい値で「影でも明るい」）
MOVIE_MATERIALS = {
    "m_white": [0.92, 0.93, 0.95], "m_gray": [0.55, 0.58, 0.63], "m_dark": [0.22, 0.23, 0.27],
    "m_black": [0.12, 0.125, 0.145], "m_red": [0.82, 0.16, 0.14], "m_navy": [0.13, 0.2, 0.42],
    "m_blue": [0.22, 0.45, 0.88], "m_gold": [0.86, 0.66, 0.25], "m_orange": [0.95, 0.48, 0.12],
    "m_floor": [0.3, 0.33, 0.38], "m_panel": [0.42, 0.46, 0.52], "m_wine": [0.42, 0.06, 0.08],
    "m_wood": [0.5, 0.33, 0.2], "m_cream": [0.86, 0.82, 0.74], "m_glass": [0.35, 0.5, 0.62], "m_screen": [0.25, 0.62, 0.8],
    # 光る（ブルームで光ってにじむ）
    "g_cyan": [0.9, 2.4, 2.8], "g_blue": [0.7, 1.4, 3.2], "g_red": [3.0, 0.45, 0.35], "g_amber": [3.0, 1.8, 0.5],
    "g_white": [2.6, 2.6, 2.7], "g_green": [0.6, 2.6, 1.0], "g_screen": [0.3, 1.0, 1.35], "g_dim": [0.3, 0.75, 0.95],
}
GLOWING = {k for k in MOVIE_MATERIALS if k.startswith("g_")}


class Stage:
    def __init__(self, width=1920, height=816, fps=30, near=0.1, far=60000.0, fov=40.0,
                 sun=(0.5, 0.45, -0.75), ambient=0.3, bg=(0.0, 0.0, 0.0), mods=("story",)):
        # ゲームの定義（素材・機器）と描画の設定
        from main import log, native
        log.setup("WARNING")
        native.require()                     # Rust の本体（lib/ を読めるようにする）
        import alpedia_view
        from main.registry import Registry
        from main.client import gfx, render
        # Rust の画面（alpedia_view）を、見えない小さな窓で開き、描く先を「窓の外の画」（width×height）にする
        self.view = alpedia_view.View(320, 180, "Alpedia movies", False, False)
        gfx.set_view(self.view)
        self.view.set_target(int(width), int(height))
        self.view.set_clear(*[float(x) for x in bg])
        self.gfx = gfx
        self.w, self.h, self.fps = width, height, fps
        self.fov = fov
        self.near, self.far = near, far
        self.vertical_fov = True
        self.loader = None                   # 前の画面（Panda3D）の名残り。今の描画は使わない
        self.render = render
        self.reg = Registry().load(list(mods))
        base = dict(self.reg.material("steel"))
        for k, col in MOVIE_MATERIALS.items():
            m = dict(base, id=k, name=k, color=list(col))
            if k in GLOWING or k in ("m_glass",):
                m["texture"] = None
            self.reg.defs["materials"][k] = m
        render.SUN = anim.unit(sun)
        render.AMBIENT = ambient
        render.STYLE = {"edges": True, "ao": True}
        from main import paths
        tex = paths.ROOT / "sauce" / "textures"
        render.EDGE_TEXTURE = str(tex / "px_edge.png")
        render.DEVICE_TEXTURE = str(tex / "px_device.png")
        render.preload(self.loader, self.reg)
        self.root = gfx.RsNode(None, "movie_root")     # 浮動原点の根
        self.items = []                      # update(t, origin) を持つ物
        self.cam_pos = np.zeros(3)
        self.cam_basis = np.eye(3)
        self.vfov = fov
        self._sprite_tex = {}

    # ------------------------------------------------------------------
    def add(self, item):
        self.items.append(item)
        return item

    def camera(self, pos, target, up=(0, 1, 0), roll=0.0, fov=None, focus=None, aperture=0.0, max_blur=14.0):
        """カメラ：位置・見る点・上の目安・回転（度）・画角（縦、度）・ピントの距離（None で見る点まで）・ぼけの強さ。"""
        pos = np.asarray(pos, float)
        B = anim.look_basis(np.asarray(target, float) - pos, up, roll)
        self.cam_pos = pos
        self.cam_basis = B
        if fov is not None:
            self.fov = fov
        # fov は縦の画角（度）。vertical_fov が False の場面は横の画角として書いてあるので、縦に直す
        vfov = self.fov
        if not self.vertical_fov:
            vfov = math.degrees(2 * math.atan(math.tan(math.radians(self.fov) / 2) * self.h / self.w))
        self.vfov = vfov
        # 浮動原点：カメラはいつも原点。物のほうを、カメラの位置を引いてから置く
        self.view.camera([0.0, 0.0, 0.0], [float(x) for x in B[2]], [float(x) for x in B[1]], float(vfov), float(self.near))
        f = float(np.linalg.norm(np.asarray(target, float) - pos)) if focus is None else float(focus)
        self.view.set_dof(max(f, 0.05), float(aperture), float(max_blur) if aperture > 0 else 0.0)

    def draw(self, t):
        """時刻 t の場面を描いて、画（高さ×幅×3 の uint8）を返す。"""
        o = self.cam_pos
        for it in self.items:
            w = getattr(it, "when", None)
            if w is not None:
                shown = bool(w(t))
                for n in _nodes(it):
                    (n.show if shown else n.hide)()
                if not shown:
                    continue
            it.update(t, o)
        self.view.pump()                     # 窓の知らせを流しておく（止まった窓と見なされないように）
        self.view.render()
        return self.grab()

    def grab(self):
        """直前に描いた画（ピントのぼかし込み）。"""
        return np.frombuffer(self.view.read_target(), np.uint8).reshape(self.h, self.w, 3).copy()

    # --- 部品 --------------------------------------------------------------
    def ship(self, voxels, size=1.0, pivot=None, name="ship", sun=None, ambient=None):
        """ブロックの構築物を置く。sun / ambient：この物だけの光の向き・環境光（部屋の中など。陰影は作るときに焼き付ける）。"""
        r = self.render
        keep = (r.SUN, r.AMBIENT)
        if sun is not None:
            r.SUN = anim.unit(sun)
        if ambient is not None:
            r.AMBIENT = ambient
        try:
            return self.add(Ship(self, voxels, size, pivot, name))
        finally:
            r.SUN, r.AMBIENT = keep

    def sprite_texture(self, kind="glow"):
        """光の玉・煙・火花のテクスチャ（作ったものを使い回す）。"""
        if kind in self._sprite_tex:
            return self._sprite_tex[kind]
        n = 128
        yy, xx = np.mgrid[0:n, 0:n]
        r = np.sqrt((xx - n / 2 + 0.5) ** 2 + (yy - n / 2 + 0.5) ** 2) / (n / 2)
        if kind == "glow":
            a = np.clip(1 - r, 0, 1) ** 2.2 + np.clip(1 - r * 3.0, 0, 1) * 0.6
        elif kind == "smoke":
            rng = np.random.default_rng(3)
            nz = rng.standard_normal((n // 8, n // 8))
            from PIL import Image, ImageFilter
            nz = np.asarray(Image.fromarray(((nz + 3) / 6 * 255).clip(0, 255).astype(np.uint8))
                            .resize((n, n), Image.BICUBIC).filter(ImageFilter.GaussianBlur(4)), float) / 255
            a = np.clip(1 - r, 0, 1) ** 1.5 * (0.55 + 0.9 * (nz - 0.5))
        elif kind == "beam":                 # 線用：太さの方向だけ薄くなる（長さの方向は一様、端だけ少し）
            u = np.abs(xx - n / 2 + 0.5) / (n / 2)
            v = np.abs(yy - n / 2 + 0.5) / (n / 2)
            a = (np.clip(1 - u, 0, 1) ** 1.6 + np.clip(1 - u * 3, 0, 1) * 0.8) * np.clip((1 - v) * 8, 0, 1)
        elif kind == "ring":
            a = np.exp(-((r - 0.82) / 0.07) ** 2)
        else:  # spark（細長い光）
            a = np.clip(1 - r, 0, 1) ** 4
        a = np.clip(a, 0, 1)
        img = np.zeros((n, n, 4), np.uint8)
        img[..., :3] = 255
        img[..., 3] = (a * 255).astype(np.uint8)
        t = self.gfx.tex_rgba(n, n, img.tobytes(), f"sprite_{kind}", False)     # なめらかに（縮小した段つき）
        self._sprite_tex[kind] = t
        return t


def _nodes(it):
    """物の描画ノード（when で出し入れするため）。"""
    out = []
    for name in ("mesh", "clouds", "atmo", "e", "root"):
        x = getattr(it, name, None)
        if x is None:
            continue
        out.append(getattr(x, "np", x))
    v = getattr(it, "view", None)
    if v is not None:
        out.append(getattr(v, "root", v))
    return out


class Ship:
    """ブロックの構築物を、決めた位置・向きに置く（完全な剛体。形は1回だけ作る）。"""

    def __init__(self, stage, voxels, size=1.0, pivot=None, name="ship"):
        from main.construct import Construct
        c = Construct.from_voxels(stage.reg, voxels, np.zeros(3), np.eye(3), size=size, name=name)
        self.c = c
        com = c.center_of_mass()
        self.pivot = com if pivot is None else np.asarray(pivot, float)
        c.mode = "rigid"
        c.rb = {"R": np.eye(3), "x": com.copy()}
        c.rb_epoch = 0
        self.com_off = com - self.pivot           # 回転の中心から重心まで（ローカル）
        self.view = stage.render.ConstructView(stage.root, stage.loader, c)
        self.view.update(np.zeros(3), highlight_flames=False)     # 形を、置く前の向きで1回だけ作る
        self.pos = np.zeros(3)
        self.R = np.eye(3)
        self.visible = True
        self.motion = None                        # 時刻 → (位置, 回転行列) を返す関数

    def place(self, pos, R=None):
        self.pos = np.asarray(pos, float)
        if R is not None:
            self.R = np.asarray(R, float)

    def update(self, t, origin):
        if self.motion is not None:
            res = self.motion(t)
            if res is None:
                self.visible = False
            else:
                self.visible = True
                self.place(*res)
        self.view.set_visible(self.visible)
        if not self.visible:
            return
        c = self.c
        c.rb["R"] = self.R
        c.rb["x"] = self.pos + self.R @ self.com_off
        self.view.update(origin, highlight_flames=False)

    def world(self, local):
        """ローカル座標（ブロックの格子 × 大きさ、m）→ ワールド座標。"""
        return self.pos + self.R @ (np.asarray(local, float) - self.pivot)

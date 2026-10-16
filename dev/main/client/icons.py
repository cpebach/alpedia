"""アイテム（素材・機器）のアイコン画像を作る。

ブロックは球の形なので、アイコンも陰影付きの球にする。
素材はその色とテクスチャ、機器は機器の色に種類ごとのマーク（炎・タンクの帯・画面など）を重ねる。
画像は起動中だけメモリに置く（ファイルは作らない）。
"""
import math

import numpy as np
from PIL import Image, ImageDraw

LIGHT = np.array([-0.45, 0.6, 0.66])
LIGHT /= np.linalg.norm(LIGHT)
_img_cache = {}
_tex_cache = {}
_src_cache = {}


def _texture_array(path):
    if not path:
        return None
    if path in _src_cache:
        return _src_cache[path]
    try:
        a = np.asarray(Image.open(path).convert("RGB"), dtype=np.float64) / 255.0
    except Exception:  # noqa: BLE001
        a = None
    _src_cache[path] = a
    return a


def sphere(size, color, tex=None, alpha=1.0, shiny=0.25):
    """陰影付きの球（RGBA の PIL 画像）。"""
    s = size
    y, x = np.mgrid[0:s, 0:s].astype(np.float64)
    nx = (x + 0.5) / s * 2 - 1
    ny = 1 - (y + 0.5) / s * 2
    r2 = nx * nx + ny * ny
    R = 0.86
    nx, ny = nx / R, ny / R
    r2 = nx * nx + ny * ny
    inside = r2 <= 1.0
    nz = np.sqrt(np.clip(1 - r2, 0, 1))
    lam = np.clip(nx * LIGHT[0] + ny * LIGHT[1] + nz * LIGHT[2], 0, 1)
    # 鏡面反射（視線は +z）
    rz = 2 * lam * nz - LIGHT[2]
    spec = np.clip(rz, 0, 1) ** 24 * shiny
    base = np.ones((s, s, 3)) * np.asarray(color, dtype=np.float64)[None, None, :3]
    if tex is not None:
        th, tw = tex.shape[:2]
        u = (np.arctan2(nx, nz) / (2 * np.pi) + 0.5) * 2.0
        v = (ny * 0.5 + 0.5) * 1.0
        ui = (np.mod(u, 1.0) * (tw - 1)).astype(int)
        vi = ((1 - np.mod(v, 1.0)) * (th - 1)).astype(int)
        base = base * tex[vi, ui]
    k = 0.32 + 0.68 * lam
    rgb = np.clip(base * k[..., None] + spec[..., None], 0, 1)
    # 縁をなめらかに
    edge = np.clip((1.0 - np.sqrt(r2)) * s * R * 0.5, 0, 1)
    a = np.where(inside, edge * alpha, 0.0)
    # 影（下に薄く）
    out = np.zeros((s, s, 4))
    out[..., :3] = rgb
    out[..., 3] = a
    return Image.fromarray((out * 255).astype(np.uint8), "RGBA")


def _mark(img, kind, col=(255, 255, 255, 230)):
    """機器の種類ごとのマーク。"""
    s = img.size[0]
    d = ImageDraw.Draw(img)
    c = s / 2
    w = max(2, s // 16)
    dark = (25, 25, 30, 220)

    def P(x, y):
        return (c + x * s, c - y * s)
    if kind == "thruster":
        d.polygon([P(-0.16, 0.12), P(0.16, 0.12), P(0.24, -0.1), P(-0.24, -0.1)], fill=dark)
        d.polygon([P(-0.18, -0.12), P(0.18, -0.12), P(0.0, -0.4)], fill=(255, 150, 40, 240))
        d.polygon([P(-0.09, -0.12), P(0.09, -0.12), P(0.0, -0.3)], fill=(255, 235, 120, 250))
    elif kind == "tank":
        for yy in (0.16, -0.08):
            d.rectangle([P(-0.3, yy + 0.04), P(0.3, yy - 0.04)], fill=(60, 60, 70, 170))
        d.polygon([P(0.0, 0.34), P(-0.09, 0.2), P(0.09, 0.2)], fill=(40, 40, 50, 210))
        d.ellipse([P(-0.09, 0.26), P(0.09, 0.1)], fill=(40, 40, 50, 210))
    elif kind == "pipe":
        d.rectangle([P(-0.36, 0.07), P(0.36, -0.07)], fill=dark)
        d.rectangle([P(-0.07, 0.36), P(0.07, -0.36)], fill=dark)
    elif kind == "valve":
        d.polygon([P(-0.26, 0.2), P(0.0, 0.0), P(-0.26, -0.2)], fill=col)
        d.polygon([P(0.26, 0.2), P(0.0, 0.0), P(0.26, -0.2)], fill=col)
        d.line([P(0, 0), P(0, 0.3)], fill=col, width=w)
    elif kind == "battery":
        d.rectangle([P(-0.24, 0.14), P(0.24, -0.14)], outline=col, width=w)
        d.rectangle([P(0.24, 0.06), P(0.3, -0.06)], fill=col)
        d.line([P(-0.12, 0), P(0.0, 0)], fill=col, width=w)
        d.line([P(-0.06, 0.06), P(-0.06, -0.06)], fill=col, width=w)
    elif kind == "generator":
        d.polygon([P(0.06, 0.34), P(-0.18, -0.02), P(0.0, -0.02), P(-0.08, -0.34), P(0.2, 0.06), P(0.02, 0.06)],
                  fill=(255, 255, 255, 240))
    elif kind == "solar":
        d.rectangle([P(-0.3, 0.22), P(0.3, -0.22)], fill=(30, 50, 120, 235), outline=(210, 220, 240, 240),
                    width=max(1, w // 2))
        for xx in (-0.1, 0.1):
            d.line([P(xx, 0.22), P(xx, -0.22)], fill=(210, 220, 240, 200), width=max(1, w // 2))
        d.line([P(-0.3, 0.0), P(0.3, 0.0)], fill=(210, 220, 240, 200), width=max(1, w // 2))
        d.ellipse([P(0.18, 0.4), P(0.36, 0.24)], fill=(255, 210, 60, 245))
    elif kind == "crank":
        d.ellipse([P(-0.2, 0.2), P(0.2, -0.2)], outline=col, width=w)
        d.line([P(0, 0), P(0.28, 0.2)], fill=col, width=w)
        d.ellipse([P(0.24, 0.26), P(0.34, 0.16)], fill=col)
    elif kind == "computer":
        d.rectangle([P(-0.28, 0.2), P(0.28, -0.16)], fill=(15, 25, 20, 235), outline=(200, 220, 230, 240))
        for k, yy in enumerate((0.1, 0.02, -0.06)):
            d.line([P(-0.2, yy), P(0.05 + 0.07 * (k % 2), yy)], fill=(90, 255, 140, 240), width=max(1, w // 2))
    elif kind == "actuator":
        d.arc([P(-0.26, 0.26), P(0.26, -0.26)], start=200, end=500, fill=col, width=w)
        d.polygon([P(0.26, 0.08), P(0.36, -0.06), P(0.16, -0.06)], fill=col)
    elif kind == "processor":
        for k in range(8):
            a = k * math.pi / 4
            d.line([P(0.18 * math.cos(a), 0.18 * math.sin(a)), P(0.3 * math.cos(a), 0.3 * math.sin(a))],
                   fill=col, width=w + 1)
        d.ellipse([P(-0.2, 0.2), P(0.2, -0.2)], fill=col)
        d.ellipse([P(-0.08, 0.08), P(0.08, -0.08)], fill=dark)
    elif kind == "seat":
        d.rectangle([P(-0.2, 0.3), P(-0.1, -0.12)], fill=col)
        d.rectangle([P(-0.2, -0.02), P(0.22, -0.12)], fill=col)
        d.rectangle([P(0.14, -0.12), P(0.22, -0.3)], fill=col)
    elif kind == "air_vent":
        for yy in (0.14, 0.0, -0.14):
            d.rectangle([P(-0.26, yy + 0.035), P(0.26, yy - 0.035)], fill=dark)
    elif kind == "antenna":
        d.line([P(0.0, -0.3), P(0.0, 0.16)], fill=col, width=w)
        d.ellipse([P(-0.05, 0.21), P(0.05, 0.11)], fill=col)
        for r in (0.16, 0.27):
            d.arc([P(-r, 0.16 + r), P(r, 0.16 - r)], start=200, end=340, fill=col, width=max(1, w // 2 + 1))
            d.arc([P(-r, 0.16 + r), P(r, 0.16 - r)], start=20, end=160, fill=col, width=max(1, w // 2 + 1))
    else:
        d.polygon([P(0, 0.22), P(0.22, 0), P(0, -0.22), P(-0.22, 0)], fill=col)
    return img


def item_image(reg, item, size=96):
    key = (item, size)
    if key in _img_cache:
        return _img_cache[key]
    kind, id_ = item
    if kind == "material":
        m = reg.material(id_) or {}
        alpha = 0.72 if id_ in ("glass", "water") else 1.0
        shiny = 0.5 if id_ in ("steel", "aluminum", "glass", "titanium") else 0.18
        img = sphere(size, m.get("color", [0.7, 0.7, 0.7]), _texture_array(m.get("texture")), alpha, shiny)
    else:
        dd = reg.device(id_) or {}
        img = sphere(size, dd.get("color", [0.6, 0.6, 0.6]), _texture_array(dd.get("texture")), 1.0, 0.3)
        img = _mark(img, dd.get("mark") or dd.get("type"))
    _img_cache[key] = img
    return img


def item_texture(reg, item, size=96):
    """ursina のテクスチャ（キャッシュ）。"""
    key = (item, size)
    t = _tex_cache.get(key)
    if t is None:
        t = _to_texture(item_image(reg, item, size))
        _tex_cache[key] = t
    return t


def _to_texture(img):
    """PIL 画像 → ursina のテクスチャ。ursina の版によって PIL 画像を直接渡せないことがあるので、
    いったん PNG にして Panda3D のテクスチャを作ってから包む。"""
    import io
    from panda3d.core import PNMImage, StringStream, SamplerState, Texture as PandaTexture
    from ursina import Texture
    buf = io.BytesIO()
    img.save(buf, "PNG")
    pnm = PNMImage()
    if not pnm.read(StringStream(buf.getvalue()), "icon.png"):
        return Texture(img)
    pt = PandaTexture("icon")
    pt.load(pnm)
    pt.setMinfilter(SamplerState.FT_linear)
    pt.setMagfilter(SamplerState.FT_linear)
    return Texture(pt)


def item_name(reg, item):
    kind, id_ = item
    d = reg.material(id_) if kind == "material" else reg.device(id_)
    return (d or {}).get("name", id_)


# ---------------------------------------------------------------------------
# 機器ブロックの表面に貼る記号（デカール）
# ---------------------------------------------------------------------------
DECAL_TYPES = ["thruster", "tank", "pipe", "valve", "battery", "generator", "solar", "crank", "computer", "actuator",
               "processor", "seat", "air_vent", "antenna", "other"]
DECAL_VARIANTS = ["plain", "side", "front", "back"]
DECAL_CELL = 128
_atlas = None


def _decal_image(kind, variant, s=DECAL_CELL):
    """透明な背景に、半透明の丸い台座＋種類のマーク。
    side：上の縁に向きを示す三角（▲の方向が機器の向き）
    front：機器が向いている面（白い輪で囲む）
    back：向きの反対側の面（マークを薄く）"""
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    c = s / 2
    R = s * 0.40
    d.ellipse([c - R, c - R, c + R, c + R], fill=(10, 12, 18, 120))
    _mark(img, kind if kind != "other" else None)
    if kind != "computer":
        # 暗い色で描いた部分（配管・タンクの帯など）は暗い台座の上で見えないので明るくする
        a = np.asarray(img).copy()
        lum = a[..., :3].astype(int).sum(-1) / 3
        m = (a[..., 3] >= 160) & (lum < 100)
        a[m, :3] = (228, 230, 236)
        img = Image.fromarray(a, "RGBA")
    d = ImageDraw.Draw(img)
    if variant == "side":
        tri = [(c, s * 0.015), (c - s * 0.11, s * 0.13), (c + s * 0.11, s * 0.13)]
        d.polygon([(x, y + 1) for x, y in tri], fill=(0, 0, 0, 180))
        d.polygon(tri, fill=(255, 225, 90, 245))
    elif variant == "front":
        w = max(3, s // 20)
        d.ellipse([c - R - w, c - R - w, c + R + w, c + R + w], outline=(255, 225, 90, 245), width=w)
    elif variant == "back":
        a = np.asarray(img).copy()
        a[..., 3] = (a[..., 3] * 0.45).astype(np.uint8)
        img = Image.fromarray(a, "RGBA")
    return img


def decal_atlas():
    """(PIL 画像, {(種類, 変化): (u0, v0, u1, v1)})。種類ごと・変化ごとに1マス。"""
    global _atlas
    if _atlas is not None:
        return _atlas
    cols = len(DECAL_VARIANTS)
    rows = len(DECAL_TYPES)
    s = DECAL_CELL
    img = Image.new("RGBA", (cols * s, rows * s), (0, 0, 0, 0))
    uv = {}
    for r, kind in enumerate(DECAL_TYPES):
        for k, var in enumerate(DECAL_VARIANTS):
            img.paste(_decal_image(kind, var), (k * s, r * s))
            pad = 1.0 / (cols * s)
            u0, u1 = k / cols + pad, (k + 1) / cols - pad
            # 画像の上が v=1（Panda3D のテクスチャ座標は下が 0）
            v1 = 1 - r / rows - pad
            v0 = 1 - (r + 1) / rows + pad
            uv[(kind, var)] = (u0, v0, u1, v1)
    _atlas = (img, uv)
    return _atlas


def panda_texture(img, name="tex"):
    """PIL 画像 → Panda3D のテクスチャ。"""
    import io
    from panda3d.core import PNMImage, StringStream, SamplerState, Texture as PandaTexture
    buf = io.BytesIO()
    img.save(buf, "PNG")
    pnm = PNMImage()
    pnm.read(StringStream(buf.getvalue()), "img.png")
    t = PandaTexture(name)
    t.load(pnm)
    t.setMinfilter(SamplerState.FT_linear_mipmap_linear)
    t.setMagfilter(SamplerState.FT_linear)
    t.setWrapU(PandaTexture.WM_clamp)
    t.setWrapV(PandaTexture.WM_clamp)
    return t

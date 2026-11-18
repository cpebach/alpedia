"""滝の絵を作る：dev/sauce/textures/waterfall.png

    python tools/make_water_textures.py      （dev フォルダで）

左半分（256×512）：落ちる水の筋（縦につなげても継ぎ目が出ない。シェーダーが下へ流す）。
右半分（256×512）：しぶき（白いもや。滝つぼに置く）。乱数は固定（毎回同じ絵）。
"""
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

W, H = 256, 512
OUT = Path(__file__).resolve().parent.parent / "sauce" / "textures" / "waterfall.png"


def streaks(rng):
    """縦の筋：横は細かいノイズ、縦はゆっくり変わる（上下でつながるように sin で）"""
    x = np.arange(W)[None, :]
    y = np.arange(H)[:, None]
    a = np.zeros((H, W))
    for _ in range(90):
        cx = rng.uniform(0, W)
        wid = rng.uniform(1.5, 7)
        amp = rng.uniform(0.3, 1.0)
        ph = rng.uniform(0, 2 * np.pi)
        k = int(rng.integers(1, 4))                        # 縦の周期（H の整数分の 1 でつながる）
        dx = np.minimum(np.abs(x - cx), W - np.abs(x - cx))
        a += amp * np.exp(-(dx / wid) ** 2) * (0.55 + 0.45 * np.sin(y / H * 2 * np.pi * k + ph))
    a = a / a.max()
    a = np.clip(0.25 + a * 0.9, 0, 1)
    # 端（左右）は薄く
    edge = np.minimum(x, W - 1 - x) / (W * 0.12)
    a *= np.clip(edge, 0, 1)
    rgb = np.stack([0.86 + 0.14 * a, 0.92 + 0.08 * a, np.ones_like(a)], -1) * 255
    return np.concatenate([rgb, (a * 235)[..., None]], -1)


def mist(rng):
    img = Image.new("L", (W, H), 0)
    px = np.zeros((H, W))
    for _ in range(60):
        cx, cy = rng.normal(W / 2, W * 0.15), rng.normal(H * 0.6, H * 0.12)
        r = rng.uniform(20, 60)
        yy, xx = np.mgrid[0:H, 0:W]
        px += np.exp(-(((xx - cx) ** 2 + (yy - cy) ** 2) / (r * r)))
    px = np.clip(px / px.max(), 0, 1)
    img = Image.fromarray(np.uint8(px * 255)).filter(ImageFilter.GaussianBlur(6))
    a = np.asarray(img).astype(np.float32) / 255.0 * 0.85
    rgb = np.full((H, W, 3), 250.0)
    return np.concatenate([rgb, (a * 255)[..., None]], -1)


def main():
    rng = np.random.default_rng(5)
    out = np.concatenate([streaks(rng), mist(rng)], 1).astype(np.uint8)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(out).save(OUT)
    print(OUT, out.shape)


if __name__ == "__main__":
    main()

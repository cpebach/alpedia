"""草・木の絵（平面の板に貼る。A-02）を作る：dev/sauce/textures/plants.png

    python tools/make_plant_textures.py      （dev フォルダで）

4 列 × 2 段の絵（1 枚 128×256、背景は透明）。並びは render/vegetation.rs の SPRITES と同じ：
  0 草  1 花のある草  2 広葉樹  3 針葉樹  4 ヤシ  5 サボテン  6 枯れた低木  7 雪の針葉樹
色は明るめに描き、描くときに気候の色を掛ける。乱数は固定（毎回同じ絵）。
"""
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

W, H = 128, 256
OUT = Path(__file__).resolve().parent.parent / "sauce" / "textures" / "plants.png"


def cell():
    return Image.new("RGBA", (W, H), (0, 0, 0, 0))


def jitter(rng, c, k=18):
    return tuple(int(np.clip(v + rng.integers(-k, k + 1), 0, 255)) for v in c[:3]) + (255,)


def grass(rng, flowers=False):
    im = cell()
    d = ImageDraw.Draw(im)
    for _ in range(46):
        x0 = rng.uniform(14, W - 14)
        h = rng.uniform(70, 150)
        lean = rng.uniform(-28, 28)
        col = jitter(rng, (150, 200, 110), 30)
        d.line([(x0, H - 2), (x0 + lean * 0.5, H - h * 0.55), (x0 + lean, H - h)], fill=col, width=int(rng.integers(3, 6)))
    if flowers:
        for _ in range(9):
            x, y = rng.uniform(16, W - 16), rng.uniform(H - 150, H - 70)
            c = [(250, 240, 120), (250, 250, 250), (240, 140, 190), (170, 150, 250)][int(rng.integers(0, 4))]
            r = rng.uniform(5, 8)
            d.ellipse([x - r, y - r, x + r, y + r], fill=c + (255,))
            d.ellipse([x - 2, y - 2, x + 2, y + 2], fill=(250, 200, 60, 255))
    return im


def trunk(d, x, y0, y1, w0, w1, col=(110, 80, 55)):
    d.polygon([(x - w0, y0), (x + w0, y0), (x + w1, y1), (x - w1, y1)], fill=col + (255,))


def blobs(d, rng, cx, cy, rx, ry, n, base):
    for _ in range(n):
        a = rng.uniform(0, 2 * np.pi)
        rr = rng.uniform(0, 1) ** 0.6
        x, y = cx + np.cos(a) * rx * rr, cy + np.sin(a) * ry * rr
        r = rng.uniform(12, 22)
        k = 1.0 - 0.25 * (y - (cy - ry)) / (2 * ry)          # 上ほど明るい
        c = tuple(int(np.clip(v * k, 0, 255)) for v in base)
        d.ellipse([x - r, y - r, x + r, y + r], fill=jitter(rng, c, 14))


def broadleaf(rng):
    im = cell()
    d = ImageDraw.Draw(im)
    trunk(d, W / 2, H - 1, H - 110, 8, 5)
    blobs(d, rng, W / 2, H - 160, 52, 70, 70, (120, 180, 90))
    return im


def conifer(rng, snow=False):
    im = cell()
    d = ImageDraw.Draw(im)
    trunk(d, W / 2, H - 1, H - 40, 6, 4)
    tiers = 7
    for k in range(tiers):
        y = H - 30 - k * 30
        w = 58 * (1 - k / (tiers + 0.5))
        c = (70, 130, 80) if not snow else (80, 120, 90)
        d.polygon([(W / 2 - w, y), (W / 2 + w, y), (W / 2, y - 62)], fill=jitter(rng, c, 10))
        if snow:
            d.polygon([(W / 2 - w * 0.55, y - 22), (W / 2 + w * 0.55, y - 22), (W / 2, y - 60)], fill=(240, 245, 250, 255))
    return im


def palm(rng):
    im = cell()
    d = ImageDraw.Draw(im)
    pts = [(W / 2 + np.sin(t * 1.4) * 10, H - 1 - t * 165) for t in np.linspace(0, 1, 12)]
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        d.line([(x0, y0), (x1, y1)], fill=(130, 100, 70, 255), width=9)
    tx, ty = pts[-1]
    for k in range(9):
        a = -np.pi / 2 + (k - 4) * 0.42
        for s in np.linspace(0, 1, 14):
            x = tx + np.cos(a) * 60 * s
            y = ty + np.sin(a) * 60 * s + 34 * s * s
            r = 7 * (1 - s) + 2
            d.ellipse([x - r, y - r, x + r, y + r], fill=jitter(rng, (110, 175, 80), 8))
    return im


def cactus(rng):
    im = cell()
    d = ImageDraw.Draw(im)
    g = (110, 160, 90, 255)
    d.rounded_rectangle([W / 2 - 13, H - 200, W / 2 + 13, H - 1], 13, fill=g)
    d.rounded_rectangle([W / 2 - 46, H - 150, W / 2 - 30, H - 90], 8, fill=g)
    d.rounded_rectangle([W / 2 - 46, H - 100, W / 2 - 10, H - 86], 7, fill=g)
    d.rounded_rectangle([W / 2 + 30, H - 175, W / 2 + 46, H - 110], 8, fill=g)
    d.rounded_rectangle([W / 2 + 10, H - 124, W / 2 + 46, H - 110], 7, fill=g)
    for y in range(H - 196, H - 4, 14):
        d.point([(W / 2 - 4, y), (W / 2 + 4, y + 7)], fill=(210, 220, 180, 255))
    return im


def dry_bush(rng):
    im = cell()
    d = ImageDraw.Draw(im)

    def branch(x, y, a, ln, w, depth):
        x1, y1 = x + np.cos(a) * ln, y - np.sin(a) * ln
        d.line([(x, y), (x1, y1)], fill=(135, 110, 80, 255), width=max(1, int(w)))
        if depth > 0:
            for da in (-0.5, 0.45):
                branch(x1, y1, a + da + rng.uniform(-0.2, 0.2), ln * 0.7, w * 0.7, depth - 1)

    for a in (1.2, 1.57, 1.95):
        branch(W / 2, H - 1, a, 34, 5, 4)
    return im


def main():
    rng = np.random.default_rng(7)
    sprites = [grass(rng), grass(rng, True), broadleaf(rng), conifer(rng), palm(rng), cactus(rng), dry_bush(rng),
               conifer(rng, True)]
    atlas = Image.new("RGBA", (W * 4, H * 2), (0, 0, 0, 0))
    for k, s in enumerate(sprites):
        s = s.filter(ImageFilter.SMOOTH)
        atlas.paste(s, ((k % 4) * W, (k // 4) * H))
    # 縁のにじみ（ミップマップで透明の黒が混ざらないように、透明な所の色を近くの色で埋める）
    a = np.asarray(atlas).astype(np.float32)
    rgb, al = a[..., :3], a[..., 3:] / 255.0
    blur = np.asarray(Image.fromarray(np.uint8(rgb * al)).filter(ImageFilter.GaussianBlur(6))).astype(np.float32)
    wb = np.asarray(Image.fromarray(np.uint8(al[..., 0] * 255)).filter(ImageFilter.GaussianBlur(6))).astype(np.float32)[..., None] / 255.0
    fill = blur / np.maximum(wb, 1e-3)
    rgb = np.where(al > 0.01, rgb, np.clip(fill, 0, 255))
    out = np.concatenate([rgb, a[..., 3:]], -1).astype(np.uint8)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(out).save(OUT)
    print(OUT, out.shape)


if __name__ == "__main__":
    main()

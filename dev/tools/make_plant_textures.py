"""草・木の絵（平面の板に貼る。A-02）を作る：dev/sauce/textures/plants.png

    python tools/make_plant_textures.py      （dev フォルダで）

4 列 × 4 段の絵（1 枚 256×512、背景は透明）。並びは render/vegetation.rs の SPRITES と同じ：
   0 草          1 花のある草      2 広葉樹        3 針葉樹
   4 ヤシ        5 サボテン        6 枯れた低木    7 雪の針葉樹
   8 シラカバ    9 熱帯の大木     10 シダ・低木   11 アシ（水辺）
  12 岩の上の松 13 紅葉したカエデ 14 アカシア     15 砂丘の草
2 倍の大きさで描いて縮める（縁がなめらかになる）。葉は何層にも重ね、上（光の来る方）ほど明るく、内側ほど暗くする。
幹は丸みの陰と樹皮の筋。色は明るめ（描くときに気候の色を少し掛ける）。乱数は固定（毎回同じ絵）。
"""
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

W, H = 256, 512          # 1 枚の大きさ（出来上がり）
S = 2                    # 描くときの倍率
COLS, ROWS = 4, 4
OUT = Path(__file__).resolve().parent.parent / "sauce" / "textures" / "plants.png"


def canvas():
    im = Image.new("RGBA", (W * S, H * S), (0, 0, 0, 0))
    return im, ImageDraw.Draw(im)


def clamp(c):
    return tuple(int(np.clip(v, 0, 255)) for v in c)


def shade(base, k, jitter, rng):
    """色 base を明るさ k 倍にし、少しずらす"""
    return clamp([v * k + rng.uniform(-jitter, jitter) for v in base]) + (255,)


# ---------------------------------------------------------------------------
# 部品
# ---------------------------------------------------------------------------
def trunk(d, rng, x0, y0, x1, y1, w0, w1, col=(112, 84, 60), segs=24, lines=6, curve=0.0):
    """幹：下 (x0, y0) から上 (x1, y1) へ。丸みの陰（左が明るく右が暗い）と樹皮の筋"""
    pts = []
    for k in range(segs + 1):
        t = k / segs
        x = x0 + (x1 - x0) * t + np.sin(t * np.pi) * curve
        y = y0 + (y1 - y0) * t
        w = w0 + (w1 - w0) * t
        pts.append((x, y, w))
    for (xa, ya, wa), (xb, yb, wb) in zip(pts, pts[1:]):
        for s in np.linspace(-1, 1, 9):
            k = 1.05 - 0.45 * (s + 1) / 2                       # 左が明るい
            c = shade(col, k, 6, rng)
            d.line([(xa + s * wa, ya), (xb + s * wb, yb)], fill=c, width=max(2, int(wa / 4)))
    for _ in range(lines):                                     # 樹皮の筋
        s = rng.uniform(-0.8, 0.8)
        a = rng.uniform(0, 0.7)
        b = min(1, a + rng.uniform(0.1, 0.4))
        ka, kb = int(a * segs), int(b * segs)
        line = [(pts[k][0] + s * pts[k][2], pts[k][1]) for k in range(ka, kb + 1)]
        if len(line) > 1:
            d.line(line, fill=shade(col, 0.55, 8, rng), width=S)


def leaf_cloud(d, rng, cx, cy, rx, ry, n, base, leaf=(14, 26), light=(-0.5, -0.8), elong=1.6):
    """葉の塊：小さな葉（だ円）を n 枚。外ほど・光の来る方ほど明るく、内側・下は暗い"""
    lx, ly = light
    items = []
    for _ in range(n):
        a = rng.uniform(0, 2 * np.pi)
        r = rng.uniform(0, 1) ** 0.55
        x, y = cx + np.cos(a) * rx * r, cy + np.sin(a) * ry * r
        items.append((r, x, y, a))
    items.sort(key=lambda it: it[0])                           # 内側（暗い）から先に描く
    for r, x, y, a in items:
        nx, ny = (x - cx) / max(rx, 1), (y - cy) / max(ry, 1)
        k = 0.55 + 0.35 * r + 0.3 * max(0.0, -(nx * lx + ny * ly)) * r
        k *= rng.uniform(0.9, 1.08)
        c = shade(base, k, 10, rng)
        L = rng.uniform(*leaf) * S
        ang = a + rng.uniform(-0.8, 0.8)
        dx, dy = np.cos(ang) * L / 2, np.sin(ang) * L / 2
        w = L / elong / 2
        px, py = -np.sin(ang) * w, np.cos(ang) * w
        d.polygon([(x - dx, y - dy), (x + px, y + py), (x + dx, y + dy), (x - px, y - py)], fill=c)


def blade(d, rng, x0, y0, h, lean, w, base, tip):
    """草の葉 1 枚：根元が太く先が細い、曲がった帯。根元は暗く先は明るい"""
    n = 10
    pts_l, pts_r = [], []
    for k in range(n + 1):
        t = k / n
        x = x0 + lean * t * t
        y = y0 - h * t
        ww = w * (1 - t) ** 0.8
        pts_l.append((x - ww, y))
        pts_r.append((x + ww, y))
    for k in range(n):
        t = (k + 0.5) / n
        c = clamp([b + (tp - b) * t for b, tp in zip(base, tip)]) + (255,)
        d.polygon([pts_l[k], pts_l[k + 1], pts_r[k + 1], pts_r[k]], fill=c)


# ---------------------------------------------------------------------------
# 絵
# ---------------------------------------------------------------------------
def grass(rng, flowers=False, dry=False):
    im, d = canvas()
    base, tip = ((95, 120, 55), (205, 225, 140)) if not dry else ((150, 130, 70), (235, 215, 150))
    for _ in range(70):
        x = rng.uniform(30, W - 30) * S
        h = rng.uniform(0.25, 0.62) * H * S
        blade(d, rng, x, H * S - 2, h, rng.uniform(-60, 60) * S, rng.uniform(2.5, 5) * S,
              [v * rng.uniform(0.85, 1.1) for v in base], [v * rng.uniform(0.9, 1.05) for v in tip])
    if flowers:
        for _ in range(12):
            x, y = rng.uniform(40, W - 40) * S, rng.uniform(0.45, 0.75) * H * S
            c = [(250, 236, 120), (252, 252, 250), (240, 140, 190), (170, 150, 250), (250, 160, 80)][int(rng.integers(0, 5))]
            r = rng.uniform(7, 11) * S
            for k in range(5):
                a = k / 5 * 2 * np.pi
                d.ellipse([x + np.cos(a) * r * 0.6 - r * 0.5, y + np.sin(a) * r * 0.6 - r * 0.5,
                           x + np.cos(a) * r * 0.6 + r * 0.5, y + np.sin(a) * r * 0.6 + r * 0.5], fill=c + (255,))
            d.ellipse([x - r * 0.35, y - r * 0.35, x + r * 0.35, y + r * 0.35], fill=(240, 190, 50, 255))
    return im


def broadleaf(rng, base=(110, 165, 75), size=1.0):
    im, d = canvas()
    cx = W * S / 2
    trunk(d, rng, cx, H * S, cx + rng.uniform(-10, 10) * S, H * S * 0.45, 14 * S * size, 7 * S * size, curve=rng.uniform(-12, 12) * S)
    for _ in range(4):                                            # 枝
        y = H * S * rng.uniform(0.45, 0.6)
        sx = rng.choice([-1, 1])
        d.line([(cx, y), (cx + sx * rng.uniform(40, 80) * S, y - rng.uniform(50, 90) * S)], fill=(95, 70, 50, 255), width=5 * S)
    for k in range(6):                                            # 葉の塊を何層か
        cxx = cx + rng.uniform(-70, 70) * S * size
        cyy = H * S * rng.uniform(0.22, 0.42)
        leaf_cloud(d, rng, cxx, cyy, rng.uniform(55, 80) * S * size, rng.uniform(45, 70) * S * size, 520, base)
    return im


def conifer(rng, snow=False, base=(60, 112, 70)):
    im, d = canvas()
    cx = W * S / 2
    trunk(d, rng, cx, H * S, cx, H * S * 0.12, 10 * S, 3 * S, col=(100, 75, 55))
    tiers = 14
    for k in range(tiers):
        t = k / tiers
        y = H * S * (0.9 - 0.78 * t)
        w = 105 * S * (1 - t) ** 0.95 + 8 * S
        for _ in range(int(120 * (1 - t) + 30)):                  # 枝の葉（下へ垂れる針葉の束）
            s = rng.uniform(-1, 1)
            x = cx + s * w
            yy = y + abs(s) * 26 * S + rng.uniform(-6, 6) * S
            k_ = 0.55 + 0.45 * (1 - abs(s)) * 0.5 + (0.25 if s < 0 else 0.0)
            c = shade(base, k_, 8, rng)
            d.line([(cx + s * w * 0.3, y), (x, yy)], fill=c, width=int(rng.uniform(3, 6) * S))
        if snow:
            for _ in range(int(40 * (1 - t) + 8)):
                s = rng.uniform(-0.9, 0.9)
                x = cx + s * w * 0.8
                yy = y + abs(s) * 18 * S - 6 * S
                r = rng.uniform(4, 9) * S
                d.ellipse([x - r, yy - r * 0.6, x + r, yy + r * 0.6], fill=shade((236, 242, 250), 1.0, 6, rng))
    return im


def palm(rng):
    im, d = canvas()
    x0 = W * S / 2
    top = (x0 + rng.uniform(-30, 30) * S, H * S * 0.2)
    trunk(d, rng, x0, H * S, top[0], top[1], 12 * S, 8 * S, col=(140, 112, 80), curve=40 * S, lines=14)
    for k in range(11):                                            # 葉（羽状）
        a = -np.pi / 2 + (k - 5) * 0.36 + rng.uniform(-0.1, 0.1)
        L = rng.uniform(95, 125) * S
        prev = top
        for s in np.linspace(0, 1, 18)[1:]:
            x = top[0] + np.cos(a) * L * s
            y = top[1] + np.sin(a) * L * s + 70 * S * s * s
            d.line([prev, (x, y)], fill=(90, 120, 60, 255), width=3 * S)
            for side in (-1, 1):
                pa = a + side * 1.2 + 0.9 * s * side
                ll = 26 * S * (1 - s * 0.6)
                d.line([(x, y), (x + np.cos(pa) * ll, y + np.sin(pa) * ll + 10 * S)], fill=shade((110, 160, 70), 0.8 + 0.3 * (1 - s), 10, rng), width=3 * S)
            prev = (x, y)
    return im


def cactus(rng):
    im, d = canvas()
    g = (95, 145, 80)
    cx = W * S / 2

    def column(x0, y0, x1, y1, r):
        for s in np.linspace(-1, 1, 15):
            k = 1.1 - 0.5 * (s + 1) / 2
            d.line([(x0 + s * r, y0), (x1 + s * r, y1)], fill=shade(g, k, 5, rng), width=int(r / 5) + 2)
        d.ellipse([x1 - r, y1 - r, x1 + r, y1 + r], fill=shade(g, 1.0, 5, rng))
        for s in np.linspace(-0.8, 0.8, 5):                       # 稜
            d.line([(x0 + s * r, y0), (x1 + s * r, y1)], fill=shade(g, 0.7, 4, rng), width=S)
    column(cx, H * S, cx, H * S * 0.25, 26 * S)
    column(cx - 80 * S, H * S * 0.62, cx - 80 * S, H * S * 0.38, 16 * S)
    d.line([(cx - 80 * S, H * S * 0.62), (cx, H * S * 0.62)], fill=shade(g, 0.9, 5, rng), width=30 * S)
    column(cx + 75 * S, H * S * 0.5, cx + 75 * S, H * S * 0.32, 15 * S)
    d.line([(cx + 75 * S, H * S * 0.5), (cx, H * S * 0.5)], fill=shade(g, 0.9, 5, rng), width=28 * S)
    return im


def dry_bush(rng):
    im, d = canvas()

    def branch(x, y, a, ln, w, depth):
        x1, y1 = x + np.cos(a) * ln, y - np.sin(a) * ln
        d.line([(x, y), (x1, y1)], fill=shade((128, 104, 78), rng.uniform(0.8, 1.1), 6, rng), width=max(S, int(w)))
        if depth > 0:
            for da in (-0.55, 0.5):
                branch(x1, y1, a + da + rng.uniform(-0.25, 0.25), ln * 0.72, w * 0.7, depth - 1)
        elif rng.random() < 0.5:
            r = 3 * S
            d.ellipse([x1 - r, y1 - r, x1 + r, y1 + r], fill=(150, 140, 90, 255))
    for a in (1.15, 1.4, 1.57, 1.75, 2.0):
        branch(W * S / 2, H * S, a, 70 * S, 9 * S, 5)
    return im


def birch(rng):
    im, d = canvas()
    cx = W * S / 2
    trunk(d, rng, cx, H * S, cx + 8 * S, H * S * 0.1, 11 * S, 5 * S, col=(232, 230, 222), lines=0)
    for _ in range(40):                                           # 白い樹皮の黒い斑
        y = rng.uniform(0.15, 1.0) * H * S
        x = cx + rng.uniform(-9, 9) * S
        d.line([(x - 6 * S, y), (x + 6 * S, y)], fill=(40, 40, 40, 255), width=int(rng.uniform(2, 4) * S))
    for k in range(5):
        leaf_cloud(d, rng, cx + rng.uniform(-55, 55) * S, H * S * rng.uniform(0.15, 0.45), 50 * S, 70 * S, 380,
                   (150, 190, 85), leaf=(10, 18))
    return im


def jungle(rng):
    im, d = canvas()
    cx = W * S / 2
    trunk(d, rng, cx, H * S, cx, H * S * 0.3, 20 * S, 9 * S, col=(120, 100, 75))
    for s in (-1, 1):                                             # 板根
        d.polygon([(cx + s * 12 * S, H * S * 0.85), (cx + s * 55 * S, H * S), (cx + s * 10 * S, H * S)], fill=(105, 85, 62, 255))
    for k in range(9):
        leaf_cloud(d, rng, cx + rng.uniform(-95, 95) * S, H * S * rng.uniform(0.12, 0.33), rng.uniform(50, 75) * S,
                   rng.uniform(35, 55) * S, 480, (70, 135, 55), leaf=(16, 30))
    for _ in range(6):                                            # つる
        x = cx + rng.uniform(-80, 80) * S
        d.line([(x, H * S * 0.3), (x + rng.uniform(-10, 10) * S, H * S * rng.uniform(0.5, 0.75))], fill=(70, 110, 50, 255), width=2 * S)
    return im


def fern(rng):
    im, d = canvas()
    cx = W * S / 2
    for k in range(13):
        a = np.pi / 2 + (k - 6) * 0.2 + rng.uniform(-0.08, 0.08)
        L = rng.uniform(0.5, 0.75) * H * S
        prev = (cx, H * S)
        for s in np.linspace(0, 1, 26)[1:]:
            x = cx + np.cos(a) * L * s * 0.8
            y = H * S - np.sin(a) * L * s + 80 * S * s * s
            d.line([prev, (x, y)], fill=(70, 110, 50, 255), width=2 * S)
            for side in (-1, 1):
                ll = 30 * S * (1 - s) ** 0.7
                pa = a + side * 1.3
                d.line([(x, y), (x + np.cos(pa) * ll, y - np.sin(pa) * ll + 6 * S)], fill=shade((95, 150, 70), 0.75 + 0.35 * s, 10, rng), width=3 * S)
            prev = (x, y)
    return im


def reeds(rng):
    im, d = canvas()
    for _ in range(40):
        x = rng.uniform(50, W - 50) * S
        h = rng.uniform(0.55, 0.95) * H * S
        blade(d, rng, x, H * S - 2, h, rng.uniform(-30, 30) * S, rng.uniform(2, 3.5) * S, (110, 125, 70), (190, 200, 130))
    for _ in range(9):                                            # 穂
        x = rng.uniform(60, W - 60) * S
        y = rng.uniform(0.08, 0.35) * H * S
        d.ellipse([x - 5 * S, y - 22 * S, x + 5 * S, y + 22 * S], fill=shade((120, 85, 55), 1.0, 10, rng))
    return im


def crag_pine(rng):
    """岩の上の曲がった松（武陵源の石柱の上など）：幹が曲がり、平たい葉の段"""
    im, d = canvas()
    cx = W * S / 2
    trunk(d, rng, cx, H * S, cx + 50 * S, H * S * 0.35, 12 * S, 6 * S, col=(95, 72, 55), curve=-60 * S)
    for k in range(5):
        y = H * S * (0.25 + k * 0.08)
        x = cx + rng.uniform(-60, 70) * S
        leaf_cloud(d, rng, x, y, rng.uniform(55, 85) * S, 22 * S, 300, (55, 100, 60), leaf=(10, 18), elong=2.4)
    return im


def acacia(rng):
    im, d = canvas()
    cx = W * S / 2
    trunk(d, rng, cx, H * S, cx, H * S * 0.55, 11 * S, 7 * S, col=(110, 88, 64))
    for s in (-1, 1):
        d.line([(cx, H * S * 0.55), (cx + s * 80 * S, H * S * 0.33)], fill=(105, 82, 60, 255), width=8 * S)
    leaf_cloud(d, rng, cx, H * S * 0.3, 120 * S, 32 * S, 900, (120, 145, 70), leaf=(8, 14), elong=2.0)
    return im


def dune_grass(rng):
    return grass(rng, dry=True)


SPRITES = [
    lambda r: grass(r), lambda r: grass(r, True), broadleaf, conifer,
    palm, cactus, dry_bush, lambda r: conifer(r, True),
    birch, jungle, fern, reeds,
    crag_pine, lambda r: broadleaf(r, base=(205, 95, 45)), acacia, dune_grass,
]


def main():
    rng = np.random.default_rng(7)
    atlas = Image.new("RGBA", (W * COLS, H * ROWS), (0, 0, 0, 0))
    for k, make in enumerate(SPRITES):
        im = make(rng).resize((W, H), Image.LANCZOS)
        atlas.paste(im, ((k % COLS) * W, (k // COLS) * H))
    # 透明な所の色を近くの色で埋める（ミップマップで縁が黒くならないように）
    a = np.asarray(atlas).astype(np.float32)
    rgb, al = a[..., :3], a[..., 3:] / 255.0
    blur = np.asarray(Image.fromarray(np.uint8(rgb * al)).filter(ImageFilter.GaussianBlur(8))).astype(np.float32)
    wb = np.asarray(Image.fromarray(np.uint8(al[..., 0] * 255)).filter(ImageFilter.GaussianBlur(8))).astype(np.float32)[..., None] / 255.0
    fill = blur / np.maximum(wb, 1e-3)
    rgb = np.where(al > 0.01, rgb, np.clip(fill, 0, 255))
    out = np.concatenate([rgb, a[..., 3:]], -1).astype(np.uint8)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(out).save(OUT)
    print(OUT, out.shape)


if __name__ == "__main__":
    main()

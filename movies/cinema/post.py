"""描いた画の仕上げ：
  ・GPU（シェーダー）：被写界深度（ピントの合う距離から離れるほどぼかす。奥行きの画を使う）
  ・CPU（numpy / Pillow）：光のにじみ（ブルーム）・色の調整・周辺を暗く・フィルムの粒・閃光・フェード・黒帯・字幕
そして ffmpeg で mp4 に書き出す。
"""
import shutil
import subprocess
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

DOF_VERT = """#version 330
uniform mat4 p3d_ModelViewProjectionMatrix;
in vec4 p3d_Vertex;
in vec2 p3d_MultiTexCoord0;
out vec2 uv;
void main() {
    gl_Position = p3d_ModelViewProjectionMatrix * p3d_Vertex;
    uv = p3d_MultiTexCoord0;
}
"""

# ピントの距離 focus（m）、絞り aperture（大きいほどぼける。ぼけの半径 px = aperture × |1/z − 1/focus| × 焦点の目安）
# 円形のぼけを、黄金角のらせん状の 64 点で集める。手前のぼけた物が奥のくっきりした物ににじむように、
# 集める点の重みはその点のぼけの大きさで決める（散らばる形の近似）。
DOF_FRAG = """#version 330
uniform sampler2D tex;
uniform sampler2D dtex;
uniform float focus;
uniform float aperture;
uniform float max_px;
uniform float near;
uniform float far;
uniform vec2 px;
in vec2 uv;
out vec4 o;

float lin(float d) { return near * far / (far - d * (far - near)); }
float coc(float z) { return clamp(aperture * abs(1.0 / z - 1.0 / focus) * focus, 0.0, max_px); }

void main() {
    float z0 = lin(texture(dtex, uv).r);
    float c0 = coc(z0);
    vec4 base = texture(tex, uv);
    if (max_px < 0.5 || aperture <= 0.0) { o = base; return; }
    vec3 acc = base.rgb;
    float wsum = 1.0;
    float rad = max(c0, 0.0);
    const int N = 64;
    for (int i = 1; i < N; i++) {
        float r = sqrt(float(i) / float(N));
        float a = float(i) * 2.39996323;
        vec2 off = vec2(cos(a), sin(a)) * r * max_px;
        vec2 suv = uv + off * px;
        float zs = lin(texture(dtex, suv).r);
        float cs = coc(zs);
        // その点のぼけがこの画素まで届いているか（手前の物は自分の大きさでにじむ）
        float reach = (zs < z0) ? cs : min(cs, c0);
        float w = smoothstep(r * max_px - 1.5, r * max_px + 0.5, reach);
        acc += texture(tex, suv).rgb * w;
        wsum += w;
    }
    o = vec4(acc / wsum, 1.0);
}
"""


class DOF:
    """ピントのシェーダー。場面を色と奥行き（32 ビット）の画に描いてから、画面に仕上げる。
    （Ursina の camera.shader は奥行きが 16 ビットで、遠くの惑星や星が消えてしまうので使わない）"""

    def __init__(self, camera, width, height, near, far):
        from direct.filter.FilterManager import FilterManager
        from panda3d.core import FrameBufferProperties, Shader, Texture
        from ursina import application
        base = application.base
        self.fm = FilterManager(base.win, base.cam)
        self.ctex, self.dtex = Texture("color"), Texture("depth")
        fbp = FrameBufferProperties()
        fbp.setRgbColor(True)
        fbp.setRgbaBits(8, 8, 8, 8)
        fbp.setDepthBits(32)
        fbp.setFloatDepth(True)
        self.quad = self.fm.renderSceneInto(colortex=self.ctex, depthtex=self.dtex, fbprops=fbp)
        self.quad.setShader(Shader.make(Shader.SL_GLSL, DOF_VERT, DOF_FRAG))
        q = self.quad
        q.setShaderInput("tex", self.ctex)
        q.setShaderInput("dtex", self.dtex)
        q.setShaderInput("near", float(near))
        q.setShaderInput("far", float(far))
        q.setShaderInput("px", (1.0 / width, 1.0 / height))
        self.set(10.0, 0.0, 0.0)

    def set(self, focus, aperture, max_px=16.0):
        q = self.quad
        q.setShaderInput("focus", float(max(focus, 0.05)))
        q.setShaderInput("aperture", float(aperture))
        q.setShaderInput("max_px", float(max_px) if aperture > 0 else 0.0)


# ---------------------------------------------------------------------------
# 仕上げ（1コマごと）
# ---------------------------------------------------------------------------
FONT_CANDIDATES = [
    "C:/Windows/Fonts/BIZ-UDGothicB.ttc", "C:/Windows/Fonts/meiryob.ttc", "C:/Windows/Fonts/YuGothB.ttc",
    "C:/Windows/Fonts/meiryo.ttc", "C:/Windows/Fonts/msgothic.ttc",
    "/System/Library/Fonts/ヒラギノ角ゴシック W6.ttc", "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc", "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
]


def find_font():
    for f in FONT_CANDIDATES:
        if Path(f).exists():
            return f
    return None


def _font(path, size):
    if path is None:
        return ImageFont.load_default()
    for idx in (0, 1, 2):
        try:
            return ImageFont.truetype(path, size, index=idx) if path.endswith(".ttc") else ImageFont.truetype(path, size)
        except OSError:
            continue
    return ImageFont.load_default()


class Grade:
    """1コマの仕上げのつまみ（場面が時刻ごとに決める）。"""

    def __init__(self, **kw):
        self.exposure = 1.0           # 明るさ
        self.contrast = 1.06
        self.saturation = 1.05
        self.tint = (1.0, 1.0, 1.0)   # 色かぶり（掛け算）
        self.lift = (0.0, 0.0, 0.0)   # 暗部の色（足し算）
        self.bloom = 0.35             # 光のにじみの強さ
        self.bloom_threshold = 0.72
        self.vignette = 0.35
        self.grain = 0.01
        self.flash = 0.0              # 白い閃光 0〜1
        self.flash_color = (1.0, 0.95, 0.85)
        self.fade = 1.0               # 1 = 見える、0 = 真っ黒
        self.aberration = 0.0         # 色ずれ（衝撃の瞬間など）px
        self.__dict__.update(kw)


class Composer:
    """描いた画（numpy uint8 の RGB）に仕上げをかけ、黒帯と字幕を付ける。"""

    def __init__(self, width, height, out_w, out_h, font=None):
        self.w, self.h = width, height                 # 描いた画の大きさ（黒帯の内側）
        self.ow, self.oh = out_w, out_h                # 出す動画の大きさ
        self.font_path = font or find_font()
        self.sub_font = _font(self.font_path, int(out_h * 0.034))
        self.name_font = _font(self.font_path, int(out_h * 0.024))
        self.title_font = _font(self.font_path, int(out_h * 0.075))
        self.small_font = _font(self.font_path, int(out_h * 0.026))
        yy, xx = np.mgrid[0:height, 0:width]
        r = np.sqrt(((xx - width / 2) / (width / 2)) ** 2 + ((yy - height / 2) / (height / 2)) ** 2 * 0.8)
        self.vig = np.clip(1.0 - (r - 0.55) * 0.9, 0.0, 1.0)[..., None].astype(np.float32)
        self.rng = np.random.default_rng(7)
        self.grain_tex = [(self.rng.standard_normal((height // 2, width // 2)).astype(np.float32)) for _ in range(6)]

    def finish(self, rgb, g, frame, subtitle=None, title=None, caption=None):
        img = rgb.astype(np.float32) / 255.0
        if g.aberration > 0.3:
            s = int(round(g.aberration))
            img[..., 0] = np.roll(img[..., 0], s, axis=1)
            img[..., 2] = np.roll(img[..., 2], -s, axis=1)
        img *= g.exposure
        # 光のにじみ：明るい所だけを取り出し、小さくしてぼかして足す
        if g.bloom > 0:
            hi = np.clip((img - g.bloom_threshold) / max(1e-3, 1 - g.bloom_threshold), 0, None)
            small = Image.fromarray(np.clip(hi * 255, 0, 255).astype(np.uint8)).resize((self.w // 4, self.h // 4),
                                                                                       Image.BILINEAR)
            b1 = np.asarray(small.filter(ImageFilter.GaussianBlur(self.w / 400)), np.float32)
            b2 = np.asarray(small.filter(ImageFilter.GaussianBlur(self.w / 120)), np.float32)
            bl = Image.fromarray(np.clip(b1 * 0.6 + b2 * 0.9, 0, 255).astype(np.uint8)).resize((self.w, self.h),
                                                                                              Image.BILINEAR)
            img += np.asarray(bl, np.float32) / 255.0 * g.bloom
        # 色
        img = img * np.asarray(g.tint, np.float32) + np.asarray(g.lift, np.float32)
        lum = img @ np.array([0.299, 0.587, 0.114], np.float32)
        img = lum[..., None] + (img - lum[..., None]) * g.saturation
        img = (img - 0.5) * g.contrast + 0.5
        # トーンカーブ（明るすぎる所をなめらかに抑える）
        img = np.where(img > 0.8, 0.8 + (1 - np.exp(-(img - 0.8) * 5)) * 0.2, img)
        if g.flash > 0:
            img = img + (np.asarray(g.flash_color, np.float32) - img) * min(g.flash, 1.0) * 0.85
        if g.vignette > 0:
            img *= 1 - (1 - self.vig) * g.vignette * 1.6
        if g.grain > 0:
            gr = self.grain_tex[frame % len(self.grain_tex)]
            gr = np.repeat(np.repeat(gr, 2, axis=0), 2, axis=1)[:self.h, :self.w]
            img += gr[..., None] * g.grain
        img *= g.fade
        out8 = np.clip(img * 255, 0, 255).astype(np.uint8)
        # 黒帯の付いた画に置く
        canvas = Image.new("RGB", (self.ow, self.oh), (0, 0, 0))
        canvas.paste(Image.fromarray(out8).resize((self.ow, int(self.h * self.ow / self.w)), Image.LANCZOS)
                     if self.w != self.ow else Image.fromarray(out8), (0, (self.oh - int(self.h * self.ow / self.w)) // 2))
        d = ImageDraw.Draw(canvas)
        if subtitle:
            self._subtitle(d, *subtitle)
        if title:
            self._title(d, *title)
        if caption:
            self._caption(d, *caption)
        return np.asarray(canvas)

    def _subtitle(self, d, who, text, alpha=1.0, color=(255, 255, 255)):
        """下の黒帯に字幕（話す人の名前は小さく色付き）。"""
        bar = (self.oh - int(self.h * self.ow / self.w)) // 2
        y = self.oh - bar / 2
        a = clamp01(alpha)
        if a <= 0:
            return
        col = tuple(int(c * a) for c in color)
        if who:
            nb = d.textbbox((0, 0), who, font=self.name_font)
            tb = d.textbbox((0, 0), text, font=self.sub_font)
            total = (nb[2] - nb[0]) + 24 + (tb[2] - tb[0])
            x = (self.ow - total) / 2
            ncol = (int(140 * a), int(205 * a), int(255 * a))
            d.text((x, y), who, font=self.name_font, fill=ncol, anchor="lm")
            d.text((x + (nb[2] - nb[0]) + 24, y), text, font=self.sub_font, fill=col, anchor="lm")
        else:
            d.text((self.ow / 2, y), text, font=self.sub_font, fill=col, anchor="mm")

    def _title(self, d, text, sub="", alpha=1.0):
        a = clamp01(alpha)
        if a <= 0:
            return
        d.text((self.ow / 2, self.oh / 2 - self.oh * 0.02), text, font=self.title_font,
               fill=(int(235 * a), int(240 * a), int(255 * a)), anchor="mm")
        if sub:
            d.text((self.ow / 2, self.oh / 2 + self.oh * 0.06), sub, font=self.small_font,
                   fill=(int(150 * a), int(190 * a), int(230 * a)), anchor="mm")

    def _caption(self, d, text, alpha=1.0):
        """左上の黒帯に、場所などの説明。"""
        a = clamp01(alpha)
        if a <= 0:
            return
        bar = (self.oh - int(self.h * self.ow / self.w)) // 2
        d.text((self.ow * 0.04, bar / 2), text, font=self.small_font, fill=(int(200 * a), int(210 * a), int(225 * a)),
               anchor="lm")


def clamp01(x):
    return 0.0 if x < 0 else 1.0 if x > 1 else x


# ---------------------------------------------------------------------------
# 書き出し
# ---------------------------------------------------------------------------
def ffmpeg_exe():
    try:
        import imageio_ffmpeg
        return imageio_ffmpeg.get_ffmpeg_exe()
    except Exception:  # noqa: BLE001
        exe = shutil.which("ffmpeg")
        if exe:
            return exe
    raise RuntimeError("ffmpeg が見つかりません。`pip install imageio-ffmpeg` を実行してください")


class Encoder:
    def __init__(self, path, width, height, fps, crf=20):
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        cmd = [ffmpeg_exe(), "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24",
               "-s", f"{width}x{height}", "-r", str(fps), "-i", "-"]
        cmd += ["-c:v", "libx264", "-preset", "slow", "-crf", str(crf), "-pix_fmt", "yuv420p",
                "-movflags", "+faststart", str(self.path)]
        self.p = subprocess.Popen(cmd, stdin=subprocess.PIPE)

    def write(self, rgb):
        self.p.stdin.write(np.ascontiguousarray(rgb, np.uint8).tobytes())

    def close(self):
        self.p.stdin.close()
        if self.p.wait() != 0:
            raise RuntimeError("ffmpeg が失敗しました")


def ease_alpha(t, t0, t1, fade=0.25):
    """t0〜t1 の間だけ 1（前後 fade 秒でなめらかに出入り）。字幕などに使う。"""
    if t < t0 or t > t1:
        return 0.0
    return min(1.0, (t - t0) / fade, (t1 - t) / fade)


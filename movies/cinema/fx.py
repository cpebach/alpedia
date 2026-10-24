"""光・煙・火花・ビーム・破片・惑星の遠景・星空。

どれも「時刻 t → その瞬間の見え方」を計算するだけ（前のコマの状態を持たない）。だから、どのコマから描いても同じになる。
光の玉・煙は、カメラに向いた四角（ビルボード）をまとめて1つのメッシュにして描く。
"""
import math

import numpy as np

from . import anim


def _rng(seed):
    return np.random.default_rng(int(seed) & 0x7FFFFFFF)


class Sprites:
    """光の玉・煙を描く入れ物（種類ごと）。毎コマ、各「出すもの」に自分の玉を足してもらって、まとめて描く。
    玉: (位置, 大きさ(m), (r,g,b,a), 回転(rad))、伸びた玉: ("line", 始点, 終点, 太さ, (r,g,b,a))"""

    def __init__(self, stage, kind="glow", blend="add", order=10):
        from panda3d.core import ColorBlendAttrib, TransparencyAttrib
        self.stage = stage
        self.mesh = stage.render.DynamicMesh(stage.root, f"sprites_{kind}")
        np_ = self.mesh.np
        np_.setTexture(stage.sprite_texture(kind), 1)
        np_.setDepthWrite(False)
        np_.setBin("fixed", order)
        if blend == "add":
            np_.setAttrib(ColorBlendAttrib.make(ColorBlendAttrib.M_add, ColorBlendAttrib.O_incoming_alpha,
                                                ColorBlendAttrib.O_one))
        else:
            np_.setTransparency(TransparencyAttrib.M_alpha)
        self.blend = blend
        self.emitters = []
        stage.add(self)

    def add(self, emitter):
        self.emitters.append(emitter)
        return emitter

    def update(self, t, origin):
        pts, lines = [], []
        for e in self.emitters:
            e.sprites(t, pts, lines)
        B = self.stage.cam_basis
        right, up = B[0], B[1]
        nq = len(pts) + len(lines)
        if nq == 0:
            self.mesh.set(np.zeros((0, 12), np.float32))
            return
        v = np.zeros((nq * 4, 12), np.float32)
        k = 0
        if pts:
            if self.blend != "add":          # 奥から順に（半透明）
                pts.sort(key=lambda p: -float(np.sum((np.asarray(p[0]) - origin) ** 2)))
            P = np.array([p[0] for p in pts], float) - origin
            S = np.array([p[1] for p in pts], float)
            C = np.array([p[2] for p in pts], float)
            A = np.array([p[3] if len(p) > 3 else 0.0 for p in pts], float)
            ca, sa = np.cos(A), np.sin(A)
            r = (ca[:, None] * right + sa[:, None] * up) * S[:, None] * 0.5
            u = (-sa[:, None] * right + ca[:, None] * up) * S[:, None] * 0.5
            n = len(pts)
            corners = [(-1, -1, 0, 0), (1, -1, 1, 0), (1, 1, 1, 1), (-1, 1, 0, 1)]
            for j, (a, b, uu, vv) in enumerate(corners):
                v[j:n * 4:4, 0:3] = P + r * a + u * b
                v[j:n * 4:4, 6:10] = C
                v[j:n * 4:4, 10] = uu
                v[j:n * 4:4, 11] = vv
            k = n
        for (_, a, b, w, c) in lines:
            a = np.asarray(a, float) - origin
            b = np.asarray(b, float) - origin
            d = b - a
            mid = (a + b) / 2
            side = np.cross(d, mid)
            ns = np.linalg.norm(side)
            side = side / ns * w * 0.5 if ns > 1e-9 else right * w * 0.5
            # 少し前後に伸ばして端を丸く
            ext = anim.unit(d) * w * 0.5
            q = [a - ext - side, a - ext + side, b + ext + side, b + ext - side]
            for j in range(4):
                v[k * 4 + j, 0:3] = q[j]
                v[k * 4 + j, 6:10] = c
            v[k * 4:k * 4 + 4, 10] = [0, 1, 1, 0]
            v[k * 4:k * 4 + 4, 11] = [0, 0, 1, 1]
            k += 1
        v[:, 4] = 1.0
        self.mesh.set(v, self.stage.render.quad_tris(nq))


# ---------------------------------------------------------------------------
# 出すもの（emitter）
# ---------------------------------------------------------------------------
class Glow:
    """ひとつの光（位置・大きさ・色を時刻の関数で）。"""

    def __init__(self, pos, size, color, t0=-1e9, t1=1e9):
        self.pos, self.size, self.color, self.t0, self.t1 = pos, size, color, t0, t1

    def sprites(self, t, pts, lines):
        if not (self.t0 <= t <= self.t1):
            return
        p = self.pos(t) if callable(self.pos) else self.pos
        s = self.size(t) if callable(self.size) else self.size
        c = self.color(t) if callable(self.color) else self.color
        if p is None or s <= 0 or c[3] <= 0.002:
            return
        pts.append((p, s, c, 0.0))


class Beam:
    """光の線（ビーム・噴射の炎）：始点・終点・太さ・色を時刻の関数で。"""

    def __init__(self, fn, t0=-1e9, t1=1e9):
        self.fn, self.t0, self.t1 = fn, t0, t1

    def sprites(self, t, pts, lines):
        if not (self.t0 <= t <= self.t1):
            return
        r = self.fn(t)
        if r is None:
            return
        for a, b, w, c in (r if isinstance(r, list) else [r]):
            if c[3] > 0.002:
                lines.append(("line", a, b, w, c))


class Func:
    """自由に書く出すもの：fn(t, pts, lines)。"""

    def __init__(self, fn):
        self.fn = fn

    def sprites(self, t, pts, lines):
        self.fn(t, pts, lines)


class Trail:
    """動く物の後ろに残る煙（path(t) → 位置。t0〜t1 の間に一定の間隔で出し、life 秒で薄くなって広がる）。"""

    def __init__(self, path, t0, t1, rate=40.0, life=2.5, size=(0.6, 3.5), color=(0.75, 0.75, 0.78, 0.5),
                 drift=0.6, seed=1):
        self.path, self.t0, self.t1, self.rate, self.life = path, t0, t1, rate, life
        self.size, self.color, self.drift = size, color, drift
        rng = _rng(seed)
        self.jit = rng.standard_normal((int((t1 - t0) * rate) + 2, 3))
        self.rot = rng.uniform(0, 2 * math.pi, len(self.jit))

    def sprites(self, t, pts, lines):
        if t < self.t0:
            return
        k1 = int((min(t, self.t1) - self.t0) * self.rate)
        k0 = max(0, int((t - self.life - self.t0) * self.rate))
        for k in range(k0, k1 + 1):
            te = self.t0 + k / self.rate
            age = t - te
            if age < 0 or age > self.life or te > self.t1:
                continue
            p = self.path(te)
            if p is None:
                continue
            a = age / self.life
            pos = np.asarray(p, float) + self.jit[k % len(self.jit)] * self.drift * age
            s = self.size[0] + (self.size[1] - self.size[0]) * anim.ease_out(a)
            c = self.color
            alpha = c[3] * max(0.0, 1 - a) ** 1.5 * min(1.0, age * 12)
            pts.append((pos, s, (c[0], c[1], c[2], alpha), self.rot[k % len(self.rot)] + age * 0.3))


class Explosion:
    """爆発：閃光・火の玉（黄→橙→暗い赤）・衝撃の輪・火花。煙は smoke() で別の入れ物へ。"""

    def __init__(self, t0, pos, scale=1.0, seed=0, n=28, vel=(0, 0, 0)):
        self.t0, self.pos, self.k, self.vel = t0, np.asarray(pos, float), scale, np.asarray(vel, float)
        rng = _rng(seed + 11)
        d = rng.standard_normal((n, 3))
        self.dirs = d / np.linalg.norm(d, axis=1, keepdims=True)
        self.speed = rng.uniform(0.3, 1.0, n)
        self.sz = rng.uniform(0.6, 1.3, n)
        self.delay = rng.uniform(0, 0.12, n)
        ds = rng.standard_normal((40, 3))
        self.sdirs = ds / np.linalg.norm(ds, axis=1, keepdims=True)
        self.sspeed = rng.uniform(0.6, 1.6, 40)

    def center(self, t):
        return self.pos + self.vel * (t - self.t0)

    def sprites(self, t, pts, lines):
        a = t - self.t0
        if a < 0 or a > 3.5:
            return
        k = self.k
        c = self.center(t)
        # 閃光
        if a < 0.5:
            f = (1 - a / 0.5) ** 2
            pts.append((c, k * (6 + 24 * anim.ease_out(min(a / 0.15, 1))), (1.0, 0.85, 0.6, f * 0.8), 0.0))
        # 火の玉
        for i in range(len(self.dirs)):
            ai = a - self.delay[i]
            if ai < 0 or ai > 2.2:
                continue
            x = ai / 2.2
            p = c + self.dirs[i] * k * 7 * self.speed[i] * anim.expo_out(min(ai / 1.2, 1))
            size = k * self.sz[i] * (3 + 7 * anim.ease_out(x))
            if x < 0.15:
                col = (1.0, 0.95, 0.75)
            elif x < 0.5:
                y = (x - 0.15) / 0.35
                col = (1.0, 0.95 - 0.5 * y, 0.75 - 0.6 * y)
            else:
                y = (x - 0.5) / 0.5
                col = (1.0 - 0.6 * y, 0.45 - 0.35 * y, 0.15 - 0.1 * y)
            pts.append((p, size, col + (max(0.0, 1 - x) ** 1.3 * 0.6,), i * 0.7 + ai))
        # 衝撃の輪は ring の入れ物（shock()）
        # 火花（線）
        for i in range(len(self.sdirs)):
            if a > 1.2:
                break
            x = a / 1.2
            p0 = c + self.sdirs[i] * k * 16 * self.sspeed[i] * anim.ease_out(x)
            p1 = c + self.sdirs[i] * k * 16 * self.sspeed[i] * anim.ease_out(max(x - 0.08, 0))
            lines.append(("line", p1, p0, k * 0.25, (1.0, 0.8, 0.45, (1 - x) ** 2)))

    def smoke(self, seed=0):
        exp = self

        def fn(t, pts, lines):
            a = t - exp.t0
            if a < 0.15 or a > 7:
                return
            c = exp.center(t)
            for i in range(len(exp.dirs)):
                x = (a - 0.15) / 6.8
                p = c + exp.dirs[i] * exp.k * 8 * exp.speed[i] * anim.expo_out(min(a / 2.5, 1))
                s = exp.k * exp.sz[i] * (6 + 10 * anim.ease_out(x))
                g = 0.18 + 0.1 * exp.speed[i]
                pts.append((p, s, (g, g * 0.95, g * 0.92, 0.55 * max(0.0, 1 - x) ** 1.4 * min(1.0, a * 3)), i * 1.3 + a * 0.2))
        return Func(fn)

    def shock(self):
        exp = self

        def fn(t, pts, lines):
            a = t - exp.t0
            if a < 0 or a > 0.8:
                return
            x = a / 0.8
            pts.append((exp.center(t), exp.k * (4 + 46 * anim.ease_out(x)), (1.0, 0.85, 0.6, 0.7 * (1 - x) ** 2), 0.0))
        return Func(fn)


# ---------------------------------------------------------------------------
# 破片（小さな立方体。回りながら飛ぶ）
# ---------------------------------------------------------------------------
_CUBE = None


def _cube():
    global _CUBE
    if _CUBE is None:
        P, N = [], []
        for ax in range(3):
            for s in (-1, 1):
                n = np.zeros(3)
                n[ax] = s
                u = np.zeros(3)
                u[(ax + 1) % 3] = 1
                w = np.cross(n, u)
                for a, b in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
                    P.append(n * 0.5 + u * a * 0.5 + w * b * 0.5)
                    N.append(n)
        _CUBE = (np.array(P), np.array(N))
    return _CUBE


class Debris:
    """破片の群れ：t0 に center から飛び散る（cols：色の候補）。"""

    def __init__(self, stage, t0, center, n=60, speed=14.0, size=(0.4, 1.2), cols=((0.6, 0.6, 0.65),), seed=0,
                 vel=(0, 0, 0), life=20.0):
        self.stage, self.t0, self.c, self.life = stage, t0, np.asarray(center, float), life
        rng = _rng(seed + 5)
        d = rng.standard_normal((n, 3))
        self.d = d / np.linalg.norm(d, axis=1, keepdims=True) * rng.uniform(0.3, 1.0, (n, 1)) * speed
        self.vel = np.asarray(vel, float)
        self.s = rng.uniform(size[0], size[1], n)
        self.ax = rng.standard_normal((n, 3))
        self.ax /= np.linalg.norm(self.ax, axis=1, keepdims=True)
        self.w = rng.uniform(1.5, 7.0, n)
        self.col = np.array([cols[i % len(cols)] for i in rng.integers(0, len(cols), n)], float)
        self.hot = rng.uniform(0, 1, n) < 0.35
        self.mesh = stage.render.DynamicMesh(stage.root, "debris")
        stage.add(self)

    def update(self, t, origin):
        a = t - self.t0
        if a < 0 or a > self.life:
            self.mesh.set(np.zeros((0, 12), np.float32))
            return
        P0, N0 = _cube()
        n = len(self.s)
        pos = self.c + self.vel * a + self.d * (1 - math.exp(-a * 0.15)) / 0.15
        V = np.zeros((n * 24, 12), np.float32)
        sun = self.stage.render.SUN
        for i in range(n):
            R = anim.axis_angle(self.ax[i], math.degrees(self.w[i] * a))
            p = (P0 * self.s[i]) @ R.T + pos[i] - origin
            nn = N0 @ R.T
            lam = np.clip(nn @ sun, 0, 1)
            col = self.col[i][None] * (0.3 + 0.7 * lam)[:, None]
            if self.hot[i]:                     # 熱い破片は最初は赤く光る
                h = max(0.0, 1 - a / 2.5)
                col = col * (1 - h) + np.array([2.2, 0.8, 0.3]) * h
            V[i * 24:(i + 1) * 24, 0:3] = p
            V[i * 24:(i + 1) * 24, 3:6] = nn
            V[i * 24:(i + 1) * 24, 6:9] = col
        V[:, 9] = 1.0
        self.mesh.set(V, self.stage.render.quad_tris(n * 6))


# ---------------------------------------------------------------------------
# 遠景：惑星（大気の縁の光つき）・星空・太陽
# ---------------------------------------------------------------------------
ATMO_VERT = """#version 330
uniform mat4 p3d_ModelViewProjectionMatrix;
uniform mat4 p3d_ModelMatrix;
in vec4 p3d_Vertex;
in vec3 p3d_Normal;
out vec3 vn;
out vec3 vp;
void main() {
    gl_Position = p3d_ModelViewProjectionMatrix * p3d_Vertex;
    vn = mat3(p3d_ModelMatrix) * p3d_Normal;     // 描画の座標（カメラが原点）
    vp = (p3d_ModelMatrix * p3d_Vertex).xyz;
}
"""
ATMO_FRAG = """#version 330
uniform vec3 sun_v;
uniform vec3 tint;
uniform float strength;
in vec3 vn;
in vec3 vp;
out vec4 o;
void main() {
    vec3 n = normalize(vn);
    vec3 v = normalize(-vp);
    float rim = 1.0 - abs(dot(n, v));
    float lit = smoothstep(-0.35, 0.6, dot(n, sun_v));
    float a = pow(rim, 3.5) * (0.08 + 0.92 * lit) * strength * 0.9;
    o = vec4(tint * a, a);
}
"""


def _cube_sphere(n):
    g = np.linspace(-1, 1, n + 1)
    gx, gy = np.meshgrid(g, g, indexing="ij")
    verts, tris = [], []
    off = 0
    for face in range(6):
        major, neg = face // 2, face % 2
        d = np.zeros(gx.shape + (3,))
        d[..., major] = -1.0 if neg else 1.0
        d[..., (major + 1) % 3] = gx
        d[..., (major + 2) % 3] = gy
        d = d.reshape(-1, 3)
        d /= np.linalg.norm(d, axis=1, keepdims=True)
        verts.append(d)
        i = (np.arange(n)[:, None] * (n + 1) + np.arange(n)[None, :]).ravel()
        tris.append(np.stack([i, i + n + 1, i + n + 2, i, i + n + 2, i + 1], axis=1).reshape(-1, 3) + off)
        off += len(d)
    V, T = np.concatenate(verts), np.concatenate(tris)
    # 三角形の向きをそろえる（外から見て表）
    nrm = np.cross(V[T[:, 1]] - V[T[:, 0]], V[T[:, 2]] - V[T[:, 0]])
    flip = np.einsum("ij,ij->i", nrm, V[T[:, 0]]) < 0
    T[flip] = T[flip][:, ::-1]
    return V, T


def wave_noise(dirs, seed=0, octaves=5, base=1.5, warp=0.0):
    """なめらかな雑音（いろいろな向き・細かさの波を重ねる）。-1〜1 くらい。warp で渦のようにゆがめる。"""
    rng = _rng(seed + 101)
    p = np.asarray(dirs, float)
    if warp:
        w = np.stack([wave_noise(p, seed + 7 + i, 3, base * 0.8) for i in range(3)], 1)
        p = p + w * warp
    total = np.zeros(len(p))
    amp, freq, norm = 1.0, base, 0.0
    for o in range(octaves):
        for _ in range(6):
            k = rng.standard_normal(3)
            k = k / np.linalg.norm(k) * freq * rng.uniform(0.8, 1.25)
            total += amp * np.sin(p @ k + rng.uniform(0, 2 * math.pi)) / 6 ** 0.5
        norm += amp
        amp *= 0.5
        freq *= 2.0
    return total / norm * 1.6


STYLES = {
    # 海の割合・海の色（深い／浅い）・陸の色（低い／高い）・氷・雲の量・大気の色
    "terra": dict(sea=0.52, deep=(0.03, 0.12, 0.32), shallow=(0.08, 0.35, 0.55), low=(0.2, 0.42, 0.16),
                  high=(0.5, 0.45, 0.3), ice=True, clouds=0.55, atmo=(0.4, 0.65, 1.0)),
    "luna": dict(sea=0.0, low=(0.42, 0.42, 0.44), high=(0.68, 0.68, 0.7), ice=False, clouds=0.0, atmo=None),
    "red": dict(sea=0.0, low=(0.45, 0.16, 0.1), high=(0.8, 0.45, 0.28), ice=True, clouds=0.15, atmo=(1.0, 0.5, 0.35)),
}


class Planet:
    """遠景の惑星（映画用の張りぼて）。ゲームの惑星の形（高さ）を使い、海・陸・氷・雲・大気の縁の光を付ける。
    実際の大きさの scale 倍にして center に置く。光は場面の太陽の向き（作るときに焼き付け）。"""

    def __init__(self, stage, defn_id, center, scale=1.0, style="terra", spin=0.0, n=128, seed=3, tilt=None,
                 atmo=1.0):
        from main.world import make_planet
        from panda3d.core import ColorBlendAttrib, TransparencyAttrib
        st = STYLES[style]
        base = stage.reg.get("planets", defn_id)
        pl = make_planet(base, [0, 0, 0], None)
        self.stage, self.center, self.scale, self.spin = stage, np.asarray(center, float), scale, spin
        self.tilt = np.eye(3) if tilt is None else tilt
        dirs, tris = _cube_sphere(n)
        r = pl.surface_radius_exact(dirs)
        h = r - pl.radius
        # 大陸の形：ゲームの高さに大きなノイズを足す（遠くから見て海と陸がはっきり分かれるように）
        cont = wave_noise(dirs, seed, 5, 1.3, warp=0.35)
        hh = (h - h.mean()) / (h.std() + 1e-9) * 0.35 + cont * 1.0
        sea = np.quantile(hh, st["sea"]) if st["sea"] > 0 else -1e9
        col = np.zeros((len(dirs), 3))
        land = hh > sea
        if st["sea"] > 0:
            k = np.clip((sea - hh) / 0.6, 0, 1)[:, None]
            col[~land] = (np.asarray(st["shallow"]) * (1 - k) + np.asarray(st["deep"]) * k)[~land]
        lk = np.clip((hh - sea) / 1.4, 0, 1)[:, None]
        tex = (wave_noise(dirs, seed + 7, 3, 14.0) * 0.5 + 0.5)[:, None]
        lc = np.asarray(st["low"]) * (1 - lk) + np.asarray(st["high"]) * lk
        col[land] = (lc * (0.8 + 0.4 * tex))[land]
        if style == "luna":
            cr = wave_noise(dirs, seed + 3, 4, 6.0) * 0.5 + 0.5
            col *= (0.75 + 0.5 * cr)[:, None]
        if st["ice"]:
            ice = np.clip((np.abs(dirs[:, 1]) - 0.9 + 0.05 * cont) / 0.05, 0, 1)[:, None]
            col = col * (1 - ice) + np.array([0.92, 0.95, 1.0]) * ice
        rr = pl.radius + np.where(land, np.maximum(h, 0) * 0.6, 0.0)
        pos = dirs * rr[:, None]
        self.radius = pl.radius
        self.mesh = self._mesh(pos, dirs, col, tris, "globe")
        self.clouds = None
        if st["clouds"] > 0:
            cn = wave_noise(dirs, seed + 21, 6, 2.5, warp=0.5) * 0.5 + 0.5
            band = 0.15 * np.cos(dirs[:, 1] * 9.0)              # 緯度の帯（赤道付近と中緯度で多い）
            a = np.clip((cn + band - (1 - st["clouds"])) / 0.22, 0, 1) ** 1.3 * 0.9
            ccol = np.ones((len(dirs), 3)) * 0.97
            self.clouds = self._mesh(dirs * pl.radius * 1.012, dirs, ccol, tris, "clouds", alpha=a)
            self.clouds.np.setTransparency(TransparencyAttrib.M_alpha)
            self.clouds.np.setDepthWrite(False)
            self.clouds.np.setBin("fixed", 1)
        self.atmo = None
        if st["atmo"] is not None and atmo > 0:
            from panda3d.core import Shader
            am = self._mesh(dirs, dirs, np.ones((len(dirs), 3)), tris, "atmo")
            e = am.np
            e.setShader(Shader.make(Shader.SL_GLSL, ATMO_VERT, ATMO_FRAG))
            e.setShaderInput("tint", tuple(st["atmo"]))
            e.setShaderInput("strength", float(atmo))
            e.setShaderInput("sun_v", tuple(float(x) for x in stage.render.SUN))
            e.setAttrib(ColorBlendAttrib.make(ColorBlendAttrib.M_add, ColorBlendAttrib.O_one, ColorBlendAttrib.O_one))
            e.setDepthWrite(False)
            e.setBin("fixed", 2)
            self.atmo = e
        stage.add(self)

    def _mesh(self, pos, nrm, col, tris, name, alpha=None):
        sun = self.tilt.T @ self.stage.render.SUN          # 惑星の向きで見た太陽（光は焼き付けるので）
        lam = nrm @ sun
        k = np.clip(lam * 1.1 + 0.08, 0, 1) ** 0.8 + 0.015      # 夜の側はほとんど真っ暗
        v = np.zeros((len(pos), 12), np.float32)
        v[:, 0:3] = pos
        v[:, 3:6] = nrm
        v[:, 6:9] = col * k[:, None]
        v[:, 9] = 1.0 if alpha is None else alpha
        m = self.stage.render.DynamicMesh(self.stage.root, name)
        m.np.setTwoSided(False)
        m.set(v, tris)
        return m

    def update(self, t, origin):
        from panda3d.core import LMatrix4f
        R = self.tilt @ anim.axis_angle((0, 1, 0), self.spin * t)
        c = self.center - origin
        for m, extra in ((self.mesh, 1.0), (self.clouds, 1.0)):
            if m is None:
                continue
            M = R.T * self.scale * extra
            m.np.setMat(LMatrix4f(*M[0], 0, *M[1], 0, *M[2], 0, *c, 1))
        if self.atmo is not None:
            s = self.radius * self.scale * 1.045
            self.atmo.setMat(LMatrix4f(s, 0, 0, 0, 0, s, 0, 0, 0, 0, s, 0, *c, 1))


class Stars:
    """星空（カメラといっしょに動く遠い球面の点）。"""

    def __init__(self, stage, n=2600, dist=None, seed=5, milky=True):
        from ursina import Entity, Mesh
        rng = _rng(seed)
        d = rng.normal(size=(n, 3))
        d /= np.linalg.norm(d, axis=1)[:, None]
        if milky:                       # 天の川のような帯
            m = rng.normal(size=(n, 3)) * np.array([1.0, 0.12, 1.0])
            m /= np.linalg.norm(m, axis=1)[:, None]
            band = anim.axis_angle((1, 0, 0.3), 28)
            d = np.concatenate([d, (m @ band.T)])
        far = (dist or stage.far * 0.8)
        b = rng.uniform(0.25, 1.0, len(d)) ** 2.2
        tint = rng.uniform(0.8, 1.0, (len(d), 3))
        col = stage.U["color"]
        cols = [col.rgba(float(bb * tt[0]), float(bb * tt[1]), float(bb), 1.0) for bb, tt in zip(b, tint)]
        self.e = Entity(parent=stage.root, model=Mesh(vertices=(d * far).tolist(), colors=cols, mode="point",
                                                      thickness=2.0), unlit=True)
        self.e.setBin("background", 5)
        self.e.setDepthWrite(False)
        self.e.setLightOff()
        stage.add(self)

    def update(self, t, origin):
        self.e.setPos(0, 0, 0)

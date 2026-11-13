"""惑星：距離場（SDF）で表した地形。掘削（クレーター）で形が変わる。"""
import numpy as np

from . import native


def fcross(a, b):
    """np.cross より速い外積（(N,3) と (N,3) / (3,) の組み合わせ）。"""
    a = np.asarray(a)
    b = np.asarray(b)
    a0, a1, a2 = a[..., 0], a[..., 1], a[..., 2]
    b0, b1, b2 = b[..., 0], b[..., 1], b[..., 2]
    return np.stack([a1 * b2 - a2 * b1, a2 * b0 - a0 * b2, a0 * b1 - a1 * b0], axis=-1)


def _pts(p):
    p = np.asarray(p, dtype=np.float64)
    return np.ascontiguousarray(p.reshape(-1, 3)), p.shape[:-1]


def value_noise(p, seed=0):
    """p: (..., 3) → (...) 0..1 の値ノイズ（Rust で計算）。"""
    q, sh = _pts(p)
    return native.require().noise(q, int(seed), 0).reshape(sh)


def fbm(p, seed=0, octaves=3):
    """値ノイズを octaves 段重ねたもの（0..1、Rust で計算）。"""
    q, sh = _pts(p)
    return native.require().noise(q, int(seed), int(octaves)).reshape(sh)


def smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0.0, 1.0)
    return t * t * (3 - 2 * t)


def make_planet(pdef: dict, position, overrides=None):
    """定義の "terrain" に応じた惑星を作る（sphere = 従来の球体、earth = 山・洞窟・水のある地球型）。"""
    d = dict(pdef)
    d.update(overrides or {})
    if d.get("terrain") == "earth":
        from .terrain_earth import EarthPlanet
        return EarthPlanet(pdef, position, overrides)
    return Planet(pdef, position, overrides)


class Planet:
    has_water = False
    has_caves = False
    lipschitz = 0.8    # レイマーチの歩幅（急斜面があるほど小さく）

    def __init__(self, pdef: dict, position, overrides=None):
        d = dict(pdef)
        d.update(overrides or {})
        self.defn = d
        self.id = d["id"]
        self.name = d.get("name", self.id)
        self.center = np.asarray(position, dtype=np.float64)
        self.radius = float(d["radius"])
        self.gm = float(d["surface_gravity"]) * self.radius ** 2
        self.amp = float(d["noise_amp"])
        self.freq = float(d["noise_freq"])
        self.seed = int(d["seed"])
        self.craters = []  # [(x,y,z,r)] 掘削跡（中心は惑星ローカル座標）
        self.version = 0
        self.bound = self.radius + self.amp * 1.5 + 2
        if "lipschitz" in d:
            self.lipschitz = float(d["lipschitz"])

    # --- 地形の計算は Rust（alpedia_core の PlanetShape） -------------------
    def _native_params(self):
        """Rust 側に渡す地形の値：(radius, amp, freq, seed, earth の dict か None)"""
        return self.radius, self.amp, self.freq, self.seed, None

    def _nat(self):
        """Rust の地形。掘削跡が増えていたら渡し直す。"""
        core = native.require()
        s = getattr(self, "_nat_shape", None)
        if s is None or getattr(self, "_nat_core", None) is not core:
            r, amp, freq, seed, earth = self._native_params()
            s = self._nat_shape = core.PlanetShape(r, amp, freq, int(seed), [float(x) for x in self.center], earth)
            self._nat_core = core
            self._nat_ncr = -1
        if self._nat_ncr != len(self.craters):
            s.set_craters(np.asarray(self.craters, dtype=np.float64).reshape(-1, 4))
            self._nat_ncr = len(self.craters)
        return s

    # --- 形状 -----------------------------------------------------------
    TILE = 32          # タイル1辺のサンプル数
    RES = 0.25         # 地表サンプル間隔 (m)

    def surface_radius_exact(self, dirs):
        """単位方向 (N,3) における地表半径（ノイズを直接計算）。"""
        return self._nat().surface_radius_exact(np.ascontiguousarray(dirs, dtype=np.float64).reshape(-1, 3))

    def _sample(self, dirs):
        """キューブマップ状のタイルにキャッシュした値（地表半径・水面半径）を双線形補間（Rust）。"""
        return self._nat().sample(np.ascontiguousarray(dirs, dtype=np.float64).reshape(-1, 3))

    def surface_radius(self, dirs):
        return self._sample(dirs)[:, 0]

    def water_radius(self, dirs):
        """水面の半径（水が無い所は 0）。"""
        return self._sample(dirs)[:, 1]

    def _tile_values(self, dirs):
        """タイルに保存する値 (N,2)：地表半径・水面半径（補間しない値。Rust）。"""
        return self._nat().tile_values(np.ascontiguousarray(dirs, dtype=np.float64).reshape(-1, 3))

    def far_samples(self, dirs):
        """遠景用：半径と色。"""
        r = self.surface_radius_exact(dirs)
        col = np.tile(np.asarray(self.defn["color"], dtype=np.float64), (len(dirs), 1))
        return r, self.deposit_tint(dirs, r - self.radius, col)

    def surface_color(self, dirs, r, normals):
        """近景の地表の色（頂点ごと）。"""
        col = np.tile(np.asarray(self.defn["color"], dtype=np.float64), (len(dirs), 1))
        return self.deposit_tint(dirs, np.asarray(r) - self.radius, col)

    # --- 鉱床（掘ると出る資源） -------------------------------------------
    # 定義の "deposits": [{"resource", "color", "freq", "threshold", "min_height", "seed"}]
    # ノイズで決まるまだら模様の場所（高さ min_height 以上）を掘るとその資源が出る。地表もその色になる。
    def _deposit_weights(self, dirs, h):
        out = []
        dirs = np.asarray(dirs, dtype=np.float64)
        h = np.asarray(h, dtype=np.float64)
        for k, dep in enumerate(self.defn.get("deposits", [])):
            th = float(dep.get("threshold", 0.6))
            n = fbm(dirs * float(dep.get("freq", 40.0)) + 17.0 + 13.0 * k, self.seed + int(dep.get("seed", 50 + k)), 3)
            w = smoothstep(th, th + 0.035, n)
            mh = dep.get("min_height")
            if mh is not None:
                w = w * smoothstep(float(mh) - 2.0, float(mh) + 2.0, h)
            out.append((dep, w))
        return out

    def deposit_tint(self, dirs, h, col):
        for dep, w in self._deposit_weights(dirs, h):
            if "color" in dep:
                c = np.asarray(dep["color"], dtype=np.float64)
                w = w[:, None] * float(dep.get("strength", 0.85))
                col = col * (1 - w) + c * w
        return col

    def underground_tint(self, local_pts, col):
        """地中（掘った穴・洞窟の壁）の色を鉱床に合わせて変える。既定では何もしない。"""
        return col

    def resource_at(self, world_point):
        """その場所を掘ったときに出る資源の ID。"""
        q = np.asarray(world_point, dtype=np.float64) - self.center
        r = float(np.linalg.norm(q))
        dirs = (q / max(r, 1e-9))[None]
        h = self.surface_radius(dirs) - self.radius
        for dep, w in self._deposit_weights(dirs, h):
            if float(w[0]) > 0.5:
                return dep["resource"]
        return self.defn.get("dig_resource", "ore")

    def cave_dist(self, q, grad=False):
        """洞窟（くり抜き）までの距離（惑星ローカル座標）。洞窟の無い惑星は None。"""
        return None

    def water_depth(self, p):
        """世界座標 (N,3) の点の水深（水面からの深さ。水の外は負）。"""
        q = np.asarray(p, dtype=np.float64) - self.center
        r = np.linalg.norm(q, axis=1)
        dirs = q / np.maximum(r, 1e-9)[:, None]
        s = self._sample(dirs)
        depth = s[:, 1] - r
        return np.where(s[:, 1] > s[:, 0], depth, -1e9)

    def sdf(self, p):
        """世界座標 (N,3) → 符号付き距離（負=地中。Rust）。"""
        return self._nat().sdf(np.ascontiguousarray(p, dtype=np.float64).reshape(-1, 3))

    def sdf_parts(self, p):
        """(掘削跡込みの地形の距離, 洞窟の距離 or None)。描画用。"""
        q = np.asarray(p, dtype=np.float64) - self.center
        r = np.linalg.norm(q, axis=1)
        dirs = q / np.maximum(r, 1e-9)[:, None]
        h = r - self.surface_radius(dirs)
        d = h
        if self.craters:
            c = np.asarray(self.craters)
            dist = np.linalg.norm(q[:, None, :] - c[None, :, :3], axis=2) - c[None, :, 3]
            d = np.maximum(h, -dist.min(axis=1))
        cv = self.cave_dist(q) if self.has_caves else None
        return h, d, cv

    def sdf_grad(self, p, eps=0.05):
        """距離と勾配（法線）を同時に求める（Rust）。"""
        return self._nat().sdf_grad(np.ascontiguousarray(p, dtype=np.float64).reshape(-1, 3), eps)

    def normal(self, p):
        return self.sdf_grad(p)[1]

    def up(self, p):
        v = np.asarray(p, dtype=np.float64) - self.center
        return v / max(np.linalg.norm(v), 1e-9)

    def gravity(self, p):
        q = self.center - np.asarray(p, dtype=np.float64)
        r2 = np.maximum(np.einsum("ij,ij->i", q, q), (self.radius * 0.5) ** 2)
        return q * (self.gm / (r2 * np.sqrt(r2)))[:, None]

    def dig(self, world_point, radius):
        local = np.asarray(world_point, dtype=np.float64) - self.center
        self.craters.append([float(local[0]), float(local[1]), float(local[2]), float(radius)])
        self.version += 1

    def raycast(self, origin, direction, max_dist):
        """球面行進で地表との交点を求める。"""
        o = np.asarray(origin, dtype=np.float64)
        dvec = np.asarray(direction, dtype=np.float64)
        t = 0.0
        for _ in range(160):
            p = o + dvec * t
            d = float(self.sdf(p[None])[0])
            if d < 0.01:
                return t, p
            t += max(d * self.lipschitz, 0.02)
            if t > max_dist:
                break
        return None

    def water_raycast(self, origin, direction, max_dist):
        """水面との交点（目が水中にあるときは None）。"""
        if not self.has_water:
            return None
        o = np.asarray(origin, dtype=np.float64)
        dvec = np.asarray(direction, dtype=np.float64)
        ts = np.arange(0.0, max_dist + 1e-9, 0.12)
        depth = self.water_depth(o[None] + ts[:, None] * dvec)
        if depth[0] > 0:
            return None
        wet = np.nonzero(depth > 0)[0]
        if not len(wet):
            return None
        k = wet[0]
        a, b = ts[k - 1], ts[k]
        for _ in range(8):
            m = (a + b) * 0.5
            if self.water_depth((o + dvec * m)[None])[0] > 0:
                b = m
            else:
                a = m
        return b, o + dvec * b

    # --- 保存 -----------------------------------------------------------
    def to_dict(self):
        return {"def": self.defn["id"], "position": self.center.tolist(), "craters": self.craters}

"""惑星：距離場（SDF）で表した地形。掘削（クレーター）で形が変わる。"""
import threading

import numpy as np

_TILE_LOCK = threading.Lock()   # 物理演算を複数スレッドで進めるので、タイルの作成は1つずつ


def fcross(a, b):
    """np.cross より速い外積（(N,3) と (N,3) / (3,) の組み合わせ）。"""
    a = np.asarray(a)
    b = np.asarray(b)
    a0, a1, a2 = a[..., 0], a[..., 1], a[..., 2]
    b0, b1, b2 = b[..., 0], b[..., 1], b[..., 2]
    return np.stack([a1 * b2 - a2 * b1, a2 * b0 - a0 * b2, a0 * b1 - a1 * b0], axis=-1)


_M1, _M2, _M3, _M4 = (np.uint32(374761393), np.uint32(668265263), np.uint32(2246822519), np.uint32(1274126177))


def value_noise(p, seed=0):
    """p: (N,3) → (N,) 0..1 の値ノイズ。"""
    p = np.asarray(p, dtype=np.float64)
    fl = np.floor(p)
    f = p - fl
    u = f * f * (3 - 2 * f)
    i = fl.astype(np.int64).astype(np.uint32)
    s = np.uint32((seed * 1442695041) & 0xFFFFFFFF)
    out = np.zeros(len(p))
    with np.errstate(over="ignore"):
        hx = (i[:, 0] * _M1, (i[:, 0] + np.uint32(1)) * _M1)
        hy = (i[:, 1] * _M2, (i[:, 1] + np.uint32(1)) * _M2)
        hz = (i[:, 2] * _M3 + s, (i[:, 2] + np.uint32(1)) * _M3 + s)
        wx = (1 - u[:, 0], u[:, 0])
        wy = (1 - u[:, 1], u[:, 1])
        wz = (1 - u[:, 2], u[:, 2])
        for dx in (0, 1):
            for dy in (0, 1):
                hxy = hx[dx] + hy[dy]
                wxy = wx[dx] * wy[dy]
                for dz in (0, 1):
                    h = hxy + hz[dz]
                    h = (h ^ (h >> np.uint32(13))) * _M4
                    h = h ^ (h >> np.uint32(16))
                    out += wxy * wz[dz] * ((h & np.uint32(0xFFFF)) * (1.0 / 65535.0))
    return out


def fbm(p, seed=0, octaves=3):
    total, amp, freq, norm = 0.0, 1.0, 1.0, 0.0
    for o in range(octaves):
        total = total + amp * value_noise(p * freq, seed + o * 31)
        norm += amp
        amp *= 0.5
        freq *= 2.0
    return total / norm


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

    # --- 形状 -----------------------------------------------------------
    TILE = 32          # タイル1辺のサンプル数
    RES = 0.25         # 地表サンプル間隔 (m)

    def surface_radius_exact(self, dirs):
        """単位方向 (N,3) における地表半径（ノイズを直接計算）。"""
        n = fbm(dirs * self.freq + 100.0, self.seed)
        return self.radius + (n - 0.5) * 2 * self.amp

    def _sample(self, dirs):
        """キューブマップ状のタイルにキャッシュした値（地表半径・水面半径）を双線形補間。"""
        dirs = np.asarray(dirs, dtype=np.float64)
        if not hasattr(self, "_N"):
            with _TILE_LOCK:
                if not hasattr(self, "_tiles"):
                    self._tiles = {}
                self._N = int(np.ceil(np.pi / 2 * self.radius / self.RES / self.TILE)) * self.TILE
        N, T = self._N, self.TILE
        ab = np.abs(dirs)
        major = ab.argmax(axis=1)
        sign = np.sign(dirs[np.arange(len(dirs)), major])
        face = major * 2 + (sign < 0)
        a1, a2 = (major + 1) % 3, (major + 2) % 3
        m = ab[np.arange(len(dirs)), major]
        u = dirs[np.arange(len(dirs)), a1] / m
        v = dirs[np.arange(len(dirs)), a2] / m
        gx = np.clip((u + 1) * 0.5 * N, 0, N - 1e-6)
        gy = np.clip((v + 1) * 0.5 * N, 0, N - 1e-6)
        ix, iy = np.floor(gx).astype(np.int64), np.floor(gy).astype(np.int64)
        fx, fy = gx - ix, gy - iy
        tx, ty = ix // T, iy // T
        lx, ly = ix - tx * T, iy - ty * T
        key = (face * 100000 + tx) * 100000 + ty
        out = np.empty((len(dirs), 2))
        if len(key) and key.min() == key.max():   # よくある場合：全部同じタイル
            keys = [key[0]]
        else:
            keys = np.unique(key)
        for k in keys:
            sel = key == k if len(keys) > 1 else slice(None)
            f0 = int(face[sel][0]); t0 = int(tx[sel][0]); t1 = int(ty[sel][0])
            tile = self._tiles.get((f0, t0, t1))
            if tile is None:
                with _TILE_LOCK:
                    tile = self._tiles.get((f0, t0, t1))
                    if tile is None:
                        tile = self._make_tile(f0, t0, t1)
            a, b = lx[sel], ly[sel]
            px, py = fx[sel][:, None], fy[sel][:, None]
            out[sel] = (tile[a, b] * (1 - px) * (1 - py) + tile[a + 1, b] * px * (1 - py)
                        + tile[a, b + 1] * (1 - px) * py + tile[a + 1, b + 1] * px * py)
        return out

    def surface_radius(self, dirs):
        return self._sample(dirs)[:, 0]

    def water_radius(self, dirs):
        """水面の半径（水が無い所は 0）。"""
        return self._sample(dirs)[:, 1]

    def _tile_values(self, dirs):
        """タイルに保存する値 (N,2)：地表半径・水面半径。"""
        h = self.surface_radius_exact(dirs)
        return np.stack([h, np.zeros_like(h)], axis=1)

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

    def _make_tile(self, face, tx, ty):
        N, T = self._N, self.TILE
        major, neg = face // 2, face % 2
        g = np.arange(T + 1)
        gx, gy = np.meshgrid((tx * T + g) / N * 2 - 1, (ty * T + g) / N * 2 - 1, indexing="ij")
        d = np.zeros(gx.shape + (3,))
        d[..., major] = -1.0 if neg else 1.0
        d[..., (major + 1) % 3] = gx
        d[..., (major + 2) % 3] = gy
        d /= np.linalg.norm(d, axis=-1, keepdims=True)
        tile = self._tile_values(d.reshape(-1, 3)).reshape(T + 1, T + 1, 2)
        self._tiles[(face, tx, ty)] = tile
        if len(self._tiles) > 4000:
            self._tiles.pop(next(iter(self._tiles)))
        return tile

    def sdf(self, p):
        """世界座標 (N,3) → 符号付き距離（負=地中）。"""
        q = np.asarray(p, dtype=np.float64) - self.center
        r = np.linalg.norm(q, axis=1)
        dirs = q / np.maximum(r, 1e-9)[:, None]
        d = r - self.surface_radius(dirs)
        if self.craters:
            c = np.asarray(self.craters)
            dist = np.linalg.norm(q[:, None, :] - c[None, :, :3], axis=2) - c[None, :, 3]
            d = np.maximum(d, -dist.min(axis=1))
        if self.has_caves:
            c = self.cave_dist(q)
            d = np.maximum(d, -c)
        return d

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
        """距離と勾配（法線）を同時に求める（ノイズ評価を1回にまとめる）。"""
        q = np.asarray(p, dtype=np.float64) - self.center
        r = np.linalg.norm(q, axis=1)
        dirs = q / np.maximum(r, 1e-9)[:, None]
        ref = np.where(np.abs(dirs[:, 1:2]) < 0.9, np.array([[0, 1, 0]]), np.array([[1, 0, 0]]))
        t1 = fcross(dirs, ref)
        t1 /= np.linalg.norm(t1, axis=1, keepdims=True)
        t2 = fcross(dirs, t1)
        ang = eps / max(self.radius, 1.0)
        d1 = dirs + t1 * ang
        d2 = dirs + t2 * ang
        d1 /= np.linalg.norm(d1, axis=1, keepdims=True)
        d2 /= np.linalg.norm(d2, axis=1, keepdims=True)
        n = len(q)
        rs = self.surface_radius(np.concatenate([dirs, d1, d2]))
        r0, r1, r2 = rs[:n], rs[n:2 * n], rs[2 * n:]
        d = r - r0
        g = dirs - t1 * ((r1 - r0) / eps)[:, None] - t2 * ((r2 - r0) / eps)[:, None]
        if self.craters:
            c = np.asarray(self.craters)
            diff = q[:, None, :] - c[None, :, :3]
            dist = np.linalg.norm(diff, axis=2) - c[None, :, 3]
            k = dist.argmin(axis=1)
            cd = -dist[np.arange(n), k]
            use = cd > d
            if use.any():
                v = diff[np.arange(n), k]
                cg = -v / np.maximum(np.linalg.norm(v, axis=1, keepdims=True), 1e-9)
                d = np.where(use, cd, d)
                g = np.where(use[:, None], cg, g)
        if self.has_caves:
            c, cg = self.cave_dist(q, grad=True)
            use = -c > d
            if use.any():
                d = np.where(use, -c, d)
                g = np.where(use[:, None], -cg, g)
        g /= np.maximum(np.linalg.norm(g, axis=1, keepdims=True), 1e-9)
        return d, g

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

    # --- 衝突 -----------------------------------------------------------
    def collide(self, pos, prev, friction):
        """pos を地表の外へ押し出し、摩擦を適用。接触した頂点のマスクを返す。"""
        rel = pos - self.center
        near = np.einsum("ij,ij->i", rel, rel) < self.bound ** 2
        if not near.any():
            return near
        idx = np.nonzero(near)[0]
        d, g = self.sdf_grad(pos[idx])
        hit = d < 0
        if not hit.any():
            near[:] = False
            return near
        hi = idx[hit]
        n = g[hit]
        pos[hi] -= n * d[hit][:, None]
        disp = pos[hi] - prev[hi]
        tang = disp - n * np.einsum("ij,ij->i", disp, n)[:, None]
        fr = np.clip(np.broadcast_to(np.asarray(friction, dtype=np.float64), (len(pos),))[hi], 0, 1)
        pos[hi] -= tang * fr[:, None]
        mask = np.zeros(len(pos), bool)
        mask[hi] = True
        return mask

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

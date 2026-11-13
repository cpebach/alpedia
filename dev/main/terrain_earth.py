"""地球型の惑星：山・丘・海・池（水たまり）・洞窟のある地形。

- 地表は高さ場（方向 → 半径）。大陸のノイズ＋丘＋尾根状の山脈。
- 海面より低い所は海、ところどころに平らな水面の池がある（水は浮力・抵抗がはたらく）。
- 洞窟は「芋虫」のように地中を進むカプセルの列で、地形の距離場からくり抜く。
- 出現地点（+Y 方向）の周りは平らな陸地にし、近くに池・洞窟の入口・山を必ず置く。
すべて seed から決まるので、保存するのは掘削跡だけでよい。
"""
import numpy as np

from . import native
from .planet import Planet, fbm, smoothstep

SPAWN_DIR = np.array([0.0, 1.0, 0.0])


def _tangent_frame(d):
    ref = np.where(np.abs(d[..., 1:2]) < 0.9, np.array([0.0, 1.0, 0.0]), np.array([1.0, 0.0, 0.0]))
    t1 = np.cross(d, ref)
    t1 /= np.linalg.norm(t1, axis=-1, keepdims=True)
    t2 = np.cross(d, t1)
    return t1, t2


def _fib_sphere(n, seed):
    k = np.arange(n) + 0.5
    phi = np.arccos(1 - 2 * k / n)
    th = np.pi * (1 + 5 ** 0.5) * k + seed * 0.37
    return np.stack([np.cos(th) * np.sin(phi), np.cos(phi), np.sin(th) * np.sin(phi)], axis=1)


def _seg_dist(p, a, b, ra, rb):
    """点とカプセル（端の半径 ra→rb）の距離と最近点。"""
    ab = b - a
    t = np.clip(np.einsum("ij,ij->i", p - a, ab) / np.maximum(np.einsum("ij,ij->i", ab, ab), 1e-12), 0, 1)
    c = a + ab * t[:, None]
    v = p - c
    L = np.linalg.norm(v, axis=1)
    return L - (ra + (rb - ra) * t), c, v, L


class EarthPlanet(Planet):
    has_water = True
    has_caves = True
    lipschitz = 0.45
    CELL = 8.0       # 洞窟検索の格子 (m)
    MARGIN = 3.0

    def __init__(self, pdef, position, overrides=None):
        super().__init__(pdef, position, overrides)
        d = self.defn
        self.sea = float(d.get("sea_level", 0.0))
        self.mountain_h = float(d.get("mountain_height", 45.0))
        self.hill_h = float(d.get("hill_height", 4.0))
        self.flat_r = float(d.get("spawn_flat_radius", 38.0))
        self.spawn_h = float(d.get("spawn_height", 3.0))
        self.n_caves = int(d.get("caves", 220))
        self.n_ponds = int(d.get("ponds", 260))
        self.bound = self.radius + self.mountain_h * 1.4 + self.hill_h * 2 + 20
        self.amp = self.mountain_h
        self._make_ponds()
        self._make_caves()

    # ------------------------------------------------------------------
    # 高さ場
    # ------------------------------------------------------------------
    def _ang(self, dirs, ref):
        return np.arccos(np.clip(dirs @ ref, -1, 1)) * self.radius

    def height0(self, dirs):
        """池を除いた地表の高さ（半径からの m。Rust）。池・洞窟を作る前は、それらの無い地形で計算する。"""
        d = np.ascontiguousarray(dirs, dtype=np.float64).reshape(-1, 3)
        if hasattr(self, "_ccap"):
            return self._nat().height0(d)
        core = native.require()
        bare = getattr(self, "_bare", None)
        if bare is None:
            e = self._earth_params(empty=True)
            bare = self._bare = core.PlanetShape(self.radius, self.amp, self.freq, int(self.seed),
                                                 [float(x) for x in self.center], e)
        return bare.height0(d)

    def _dir_at(self, ox, oz):
        """出現地点から地表に沿って (x, z) m 離れた方向。"""
        p = np.array([ox, self.radius, oz], dtype=np.float64)
        return p / np.linalg.norm(p)

    # ------------------------------------------------------------------
    # 池（平らな水面のくぼみ）
    # ------------------------------------------------------------------
    def _make_ponds(self):
        rs = np.random.default_rng(self.seed * 7919 + 13)
        cand = _fib_sphere(9000, self.seed)
        h = self.height0(cand)
        a_sp = self._ang(cand, SPAWN_DIR)
        ok = (h > self.sea + 1.5) & (h < self.mountain_h * 0.6) & (a_sp > self.flat_r * 2.2)
        cand = cand[ok]
        rs.shuffle(cand)
        cand = cand[: self.n_ponds * 3]
        P = [(self._dir_at(-20.0, 16.0), 6.5, 1.6)]             # 出現地点のそばの池
        P += [(c, float(rs.uniform(4, 12)), float(rs.uniform(1.0, 3.0))) for c in cand]
        ponds = []
        for c, rad, depth in P:
            t1, t2 = _tangent_frame(c)
            ring = []
            for f in (1.0, 1.4, 1.9):
                ang = np.linspace(0, 2 * np.pi, 12, endpoint=False)
                off = (np.cos(ang)[:, None] * t1 + np.sin(ang)[:, None] * t2) * (rad * f / self.radius)
                dd = c + off
                ring.append(dd / np.linalg.norm(dd, axis=1, keepdims=True))
            hr = self.height0(np.concatenate(ring))
            level = float(hr.min()) - 0.35
            if len(ponds) and (hr.max() - hr.min() > 2.2):   # 斜面すぎる所には作らない（出現地点のそばは必ず作る）
                continue
            ponds.append((c, rad, depth, level))
            if len(ponds) > self.n_ponds:
                break
        self.ponds_dir = np.array([p[0] for p in ponds])
        self.ponds_rad = np.array([p[1] for p in ponds])
        self.ponds_depth = np.array([p[2] for p in ponds])
        self.ponds_level = np.array([p[3] for p in ponds])

    # ------------------------------------------------------------------
    # 洞窟（芋虫状のカプセル列）
    # ------------------------------------------------------------------
    def _make_caves(self):
        rs = np.random.default_rng(self.seed * 104729 + 7)
        cand = _fib_sphere(4000, self.seed + 1)
        h = self.height0(cand)
        a_sp = self._ang(cand, SPAWN_DIR)
        ok = (h > self.sea + 2.0) & (a_sp > self.flat_r * 1.6)
        cand = cand[ok]
        rs.shuffle(cand)
        starts = [self._dir_at(30.0, -48.0)] + list(cand[: self.n_caves])   # 出現地点のそばの洞窟
        K = len(starts)
        S = np.array(starts)
        t1, t2 = _tangent_frame(S)
        yaw = rs.uniform(0, 2 * np.pi, K)
        yaw[0] = np.arctan2(-1.0, 0.3)        # 出現地点から離れる向き
        steps = rs.integers(18, 42, K)
        steps[0] = 34
        depth_goal = rs.uniform(7, 16, K)
        depth_goal[0] = 9.0
        rad = rs.uniform(1.8, 2.6, K)
        rad[0] = 2.3
        pos = S * (self.radius + self.height0(S) + 0.6)[:, None]
        A, B, RA, RB, ID = [], [], [], [], []
        r_prev = rad.copy()
        alive = np.ones(K, bool)
        for s in range(int(steps.max())):
            alive &= s < steps
            if not alive.any():
                break
            up = pos / np.linalg.norm(pos, axis=1, keepdims=True)
            t1, t2 = _tangent_frame(up)
            yaw += rs.normal(0, 0.35, K)
            hs = self.radius + self.height0(up)
            depth = hs - np.linalg.norm(pos, axis=1)
            pitch = np.where(s < 4, -0.55, np.clip((depth - depth_goal) * 0.06, -0.5, 0.3))
            pitch = np.where((s >= 4) & (depth < r_prev + 2.5), -0.45, pitch)
            fwd = np.cos(yaw)[:, None] * t1 + np.sin(yaw)[:, None] * t2
            dvec = fwd * np.cos(pitch)[:, None] + up * np.sin(pitch)[:, None]
            r_new = np.clip(r_prev * np.exp(rs.normal(0, 0.12, K)), 1.5, 3.4)
            new = pos + dvec * (2.2 + r_new * 0.3)[:, None]
            for k in np.nonzero(alive)[0]:
                A.append(pos[k].copy()); B.append(new[k].copy()); RA.append(r_prev[k]); RB.append(r_new[k])
                ID.append(k)
            pos = np.where(alive[:, None], new, pos)
            r_prev = np.where(alive, r_new, r_prev)
        # 終点は広間に
        for k in range(K):
            if rs.random() < 0.5 or k == 0:
                rr = float(rs.uniform(3.2, 4.8))
                A.append(pos[k].copy()); B.append(pos[k] + 0.01); RA.append(rr); RB.append(rr)
                ID.append(k)
        self.cave_a = np.array(A)
        self.cave_b = np.array(B)
        self.cave_ra = np.array(RA)
        self.cave_rb = np.array(RB)
        self.cave_id = np.array(ID)
        self._index_caves()

    def _index_caves(self):
        lo = np.minimum(self.cave_a, self.cave_b) - (np.maximum(self.cave_ra, self.cave_rb) + self.MARGIN)[:, None]
        hi = np.maximum(self.cave_a, self.cave_b) + (np.maximum(self.cave_ra, self.cave_rb) + self.MARGIN)[:, None]
        i0 = np.floor(lo / self.CELL).astype(np.int64)
        i1 = np.floor(hi / self.CELL).astype(np.int64)
        keys, caps = [], []
        for k in range(len(i0)):
            g = np.stack(np.meshgrid(*[np.arange(i0[k, j], i1[k, j] + 1) for j in range(3)], indexing="ij"),
                         -1).reshape(-1, 3)
            keys.append(self._key(g))
            caps.append(np.full(len(g), k))
        keys, caps = np.concatenate(keys), np.concatenate(caps)
        o = np.argsort(keys, kind="stable")
        keys, caps = keys[o], caps[o]
        self._ck, start = np.unique(keys, return_index=True)
        self._cstart = start
        self._cend = np.append(start[1:], len(keys))
        self._ccap = caps
        self._bare = None

    @staticmethod
    def _key(g):
        g = g + 4096
        return (g[..., 0] * 8192 + g[..., 1]) * 8192 + g[..., 2]

    def _earth_params(self, empty=False):
        """Rust の地形（PlanetShape）に渡す地球型の値。empty：池・洞窟の無い地形（池・洞窟を作るときの高さ用）。"""
        earth = {"sea": self.sea, "mountain_h": self.mountain_h, "hill_h": self.hill_h, "flat_r": self.flat_r,
                 "spawn_h": self.spawn_h, "cell": self.CELL}
        for k in ("ponds_dir", "ponds_rad", "ponds_depth", "ponds_level", "cave_a", "cave_b", "cave_ra", "cave_rb"):
            earth[k] = (np.zeros((0, 3) if k in ("ponds_dir", "cave_a", "cave_b") else 0) if empty
                        else np.ascontiguousarray(getattr(self, k), dtype=np.float64))
        for k in ("ck", "cstart", "cend", "ccap"):
            earth[k] = np.zeros(0, np.int64) if empty else np.ascontiguousarray(getattr(self, "_" + k), dtype=np.int64)
        return earth

    def _native_params(self):
        return self.radius, self.amp, self.freq, self.seed, self._earth_params()

    def cave_dist(self, q, grad=False):
        d, g = self._nat().cave_dist(np.ascontiguousarray(q, dtype=np.float64).reshape(-1, 3))
        return (d, g) if grad else d


    def caves_near(self, center_local, radius):
        """center_local の周り radius 以内にあるカプセルの番号。"""
        c = np.asarray(center_local)
        d, *_ = _seg_dist(np.broadcast_to(c, self.cave_a.shape), self.cave_a, self.cave_b, self.cave_ra, self.cave_rb)
        return np.nonzero(d < radius)[0]

    # ------------------------------------------------------------------
    # Planet の上書き
    # ------------------------------------------------------------------


    def water_depth(self, p):
        q = np.asarray(p, dtype=np.float64) - self.center
        r = np.linalg.norm(q, axis=1)
        dirs = q / np.maximum(r, 1e-9)[:, None]
        s = self._sample(dirs)
        wet = s[:, 1] > s[:, 0]
        return np.where(wet, s[:, 1] - r, -1e9)

    def palette(self, h, slope, water_h=None):
        """高さ（m）と傾き（0=平ら〜1=垂直）から地表の色。"""
        grass = np.array([0.36, 0.55, 0.26])
        dry = np.array([0.52, 0.55, 0.3])
        sand = np.array([0.78, 0.72, 0.5])
        rock = np.array([0.47, 0.44, 0.41])
        snow = np.array([0.93, 0.94, 0.96])
        mud = np.array([0.42, 0.36, 0.27])
        h = h[:, None]
        slope = slope[:, None]
        col = grass * (1 - smoothstep(8, 30, h)) + dry * smoothstep(8, 30, h)
        col = col * (1 - smoothstep(1.6, 0.6, h - self.sea)) + sand * smoothstep(1.6, 0.6, h - self.sea)
        col = col * (1 - smoothstep(0.45, 0.65, slope)) + rock * smoothstep(0.45, 0.65, slope)
        col = col * (1 - smoothstep(0.45 * self.mountain_h, 0.6 * self.mountain_h, h)) + rock * smoothstep(0.45 * self.mountain_h, 0.6 * self.mountain_h, h)
        sn = smoothstep(self.mountain_h * 0.82, self.mountain_h * 0.95, h) * (1 - smoothstep(0.55, 0.75, slope))
        col = col * (1 - sn) + snow * sn
        if water_h is not None:
            wet = smoothstep(-0.2, 0.4, water_h[:, None] - h)
            col = col * (1 - wet) + mud * wet
        return col

    def surface_color(self, dirs, r, normals):
        h = r - self.radius
        slope = 1 - np.clip(np.einsum("ij,ij->i", normals, dirs), 0, 1)
        slope = np.clip(slope * 3.3, 0, 1)   # 1-cos → 0..1
        w = self._sample(dirs)[:, 1] - self.radius
        return self.deposit_tint(dirs, h, self.palette(h, slope, w))

    # --- 洞窟のまわりの鉱石 ----------------------------------------------
    def _cave_res(self):
        return self.defn.get("cave_resource")

    def resource_at(self, world_point):
        res = self._cave_res()
        if res:
            q = np.asarray(world_point, dtype=np.float64)[None] - self.center
            if float(self.cave_dist(q)[0]) < float(self.defn.get("cave_resource_dist", 3.0)):
                return res
        return super().resource_at(world_point)

    def underground_tint(self, local_pts, col):
        """洞窟の壁のまわりに、鉱石の粒をまだらに見せる（掘るとその鉱石が出る範囲の目印）。"""
        res = self._cave_res()
        c = self.defn.get("cave_resource_color")
        if not res or c is None or not len(local_pts):
            return col
        d = self.cave_dist(local_pts)
        near = 1 - smoothstep(float(self.defn.get("cave_resource_dist", 3.0)) - 0.5,
                              float(self.defn.get("cave_resource_dist", 3.0)), d)
        spots = smoothstep(0.55, 0.6, fbm(local_pts * 1.1 + 5.0, self.seed + 77, 2))
        w = (near * (0.25 + 0.65 * spots))[:, None]
        return col * (1 - w) + np.asarray(c, dtype=np.float64) * w

    def far_samples(self, dirs):
        h = self.height0(dirs)      # 遠景では池は省略（海だけ）
        water = np.full(len(dirs), self.sea)
        wet = water > h
        # 傾きは近くの高さとの差から
        t1, _ = _tangent_frame(dirs)
        e = 3.0 / self.radius
        d2 = dirs + t1 * e
        d2 /= np.linalg.norm(d2, axis=1, keepdims=True)
        slope = np.clip(np.abs(self.height0(d2) - self.height0(dirs)) / 3.0, 0, 1)
        col = self.deposit_tint(dirs, h, self.palette(h, slope))
        sea_col = np.array([0.16, 0.33, 0.55])
        col[wet] = sea_col
        return self.radius + np.where(wet, water, h), col

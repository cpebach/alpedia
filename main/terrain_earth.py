"""地球型の惑星：山・丘・海・池（水たまり）・洞窟のある地形。

- 地表は高さ場（方向 → 半径）。大陸のノイズ＋丘＋尾根状の山脈。
- 海面より低い所は海、ところどころに平らな水面の池がある（水は浮力・抵抗がはたらく）。
- 洞窟は「芋虫」のように地中を進むカプセルの列で、地形の距離場からくり抜く。
- 出現地点（+Y 方向）の周りは平らな陸地にし、近くに池・洞窟の入口・山を必ず置く。
すべて seed から決まるので、保存するのは掘削跡だけでよい。
"""
import numpy as np

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


def ridged(p, seed, octaves=3):
    """尾根状のノイズ（0..1、尾根で 1）。"""
    n = (fbm(p, seed, octaves) - 0.5) / 0.12
    return np.clip(1 - np.abs(n) * 0.6, 0, 1) ** 2


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
        """池を除いた地表の高さ（半径からの m）。"""
        dirs = np.asarray(dirs, dtype=np.float64)
        sd = self.seed
        cont = (fbm(dirs * 1.3 + 11.0, sd, 4) - 0.545) / 0.1
        a_sp = self._ang(dirs, SPAWN_DIR)
        e_sp = np.exp(-(a_sp / 300.0) ** 2)
        cont = cont * (1 - e_sp) + (np.maximum(cont, 1.2) + 0.3) * e_sp   # 出現地点のまわりは陸
        base = np.where(cont > 0, cont * 6.0, cont * 12.0)
        hills = (fbm(dirs * 9.0 + 5.0, sd + 1, 3) - 0.5) * 2.5 * self.hill_h
        # 山脈：尾根状のノイズを、別の低周波ノイズで山域に限定
        ridge = ridged(dirs * 3.2 + 3.0, sd + 2, 4)
        rough = ridged(dirs * 11.0 + 7.0, sd + 4, 3) * 0.6 + ridged(dirs * 27.0 + 1.0, sd + 5, 2) * 0.4
        rng = smoothstep(0.52, 0.66, fbm(dirs * 1.6 + 40.0, sd + 3, 3)) * smoothstep(0.2, 1.2, cont)
        # 出現地点から 170 m ほど先（-Z 側）に必ず山を置く
        mdir = np.array([0.0, np.cos(170 / self.radius), -np.sin(170 / self.radius)])
        am = self._ang(dirs, mdir)
        rng = np.maximum(rng, np.exp(-(am / 70.0) ** 2))
        mtn = rng * (ridge * 0.55 + 0.45) * (0.65 + 0.35 * rough)
        h = base + hills + mtn * self.mountain_h * 1.1
        # 出現地点のまわりは平らに
        w = 1 - smoothstep(self.flat_r, self.flat_r * 1.9, a_sp)
        h = h * (1 - w) + (self.spawn_h + hills * 0.15) * w
        return h

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

    def _apply_ponds(self, dirs, h):
        """dirs の高さ h を池で削り、水面の高さ（無い所は -inf）を返す。"""
        water = np.full(len(dirs), -np.inf)
        if not len(self.ponds_dir):
            return h, water
        mean = dirs.mean(0)
        mean /= max(np.linalg.norm(mean), 1e-9)
        spread = self._ang(dirs, mean).max()
        near = np.nonzero(self._ang(self.ponds_dir, mean) < spread + self.ponds_rad * 2.0)[0]
        for k in near:
            t = self._ang(dirs, self.ponds_dir[k]) / self.ponds_rad[k]
            m = t < 2.0
            if not m.any():
                continue
            L = self.ponds_level[k]
            tt = t[m]
            bowl = np.where(tt < 1, L - self.ponds_depth[k] * (1 - tt * tt), L + (tt - 1) * self.ponds_rad[k] * 0.8)
            h[m] = np.minimum(h[m], bowl)
            w = tt < 1.15
            idx = np.nonzero(m)[0][w]
            water[idx] = np.maximum(water[idx], L)
        return h, water

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

    @staticmethod
    def _key(g):
        g = g + 4096
        return (g[..., 0] * 8192 + g[..., 1]) * 8192 + g[..., 2]

    def cave_dist(self, q, grad=False):
        q = np.asarray(q, dtype=np.float64)
        n = len(q)
        out = np.full(n, 1e9)
        gout = np.zeros((n, 3)) if grad else None
        key = self._key(np.floor(q / self.CELL).astype(np.int64))
        pos = np.searchsorted(self._ck, key)
        pos = np.minimum(pos, len(self._ck) - 1)
        found = self._ck[pos] == key
        if found.any():
            pi = np.nonzero(found)[0]
            st, en = self._cstart[pos[pi]], self._cend[pos[pi]]
            cnt = en - st
            rep = np.repeat(pi, cnt)
            off = np.arange(cnt.sum()) - np.repeat(np.cumsum(cnt) - cnt, cnt)
            cap = self._ccap[np.repeat(st, cnt) + off]
            d, c, v, L = _seg_dist(q[rep], self.cave_a[cap], self.cave_b[cap], self.cave_ra[cap], self.cave_rb[cap])
            # 点ごとの最小
            o = np.lexsort((d, rep))
            first = np.ones(len(o), bool)
            first[1:] = rep[o][1:] != rep[o][:-1]
            sel = o[first]
            out[rep[sel]] = d[sel]
            if grad:
                gout[rep[sel]] = v[sel] / np.maximum(L[sel], 1e-9)[:, None]
        return (out, gout) if grad else out

    def caves_near(self, center_local, radius):
        """center_local の周り radius 以内にあるカプセルの番号。"""
        c = np.asarray(center_local)
        d, *_ = _seg_dist(np.broadcast_to(c, self.cave_a.shape), self.cave_a, self.cave_b, self.cave_ra, self.cave_rb)
        return np.nonzero(d < radius)[0]

    # ------------------------------------------------------------------
    # Planet の上書き
    # ------------------------------------------------------------------
    def surface_radius_exact(self, dirs):
        dirs = np.asarray(dirs, dtype=np.float64)
        h, _ = self._apply_ponds(dirs, self.height0(dirs))
        return self.radius + h

    def _tile_values(self, dirs):
        h, water = self._apply_ponds(dirs, self.height0(dirs))
        water = np.maximum(water, self.sea)
        wr = np.where(water > h, self.radius + water, 0.0)
        # 水の無い所も水面の値を入れておく（補間で水際がなめらかになる）
        wr = np.where(wr == 0.0, self.radius + np.minimum(water, h - 0.05), wr)
        return np.stack([self.radius + h, wr], axis=1)

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

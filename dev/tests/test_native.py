"""Rust の物理コア（alpedia_core）のテスト：計算の性質（動かない物は動かさない・式どおり）と、
Python（numpy）で書ける所は同じ結果を出すか。"""
import sys
import unittest
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from main import native  # noqa: E402

core = native.core
TOL = 1e-9


def rnd(*shape, seed=0):
    return np.random.default_rng(seed).normal(size=shape)


def rot(rng):
    q = np.linalg.qr(rng.normal(size=(3, 3)))[0]
    return q if np.linalg.det(q) > 0 else -q


@unittest.skipIf(core is None, "Rust の物理コアがありません")
class TestKernels(unittest.TestCase):
    def test_shape_match_keeps_rigid_shape(self):
        """形が回って動いただけ（伸び縮みなし）なら、形合わせは位置を変えず、その回転を返す。"""
        n = 200
        rng = np.random.default_rng(1)
        rest = (np.array([[x, y, z] for x in (0, 1) for y in (0, 1) for z in (0, 1)], float) - 0.5)[None].repeat(n, 0)
        rest = np.ascontiguousarray(rest)
        Rs = np.stack([rot(rng) for _ in range(n)])
        pos = (np.einsum("nij,nkj->nki", Rs, rest) + rng.normal(0, 5, (n, 1, 3))).reshape(-1, 3)
        sc, st = np.ones((n, 3)), np.ones(n)
        vox = np.arange(n, dtype=np.int64)
        for par in (False, True):
            p = pos.copy()
            r = np.zeros((n, 3, 3))
            q = np.zeros((n, 4)); q[:, 0] = 1
            core.shape_match(p, rest, sc, st, vox, r, q, par)
            self.assertLess(np.abs(p - pos).max(), 1e-7)
            self.assertLess(np.abs(r - Rs).max(), 1e-6)

    def test_joints_keep_satisfied_lengths(self):
        rng = np.random.default_rng(2)
        nv = 400
        pos = rng.normal(size=(nv, 3))
        ja, jb = rng.integers(0, nv, 300), rng.integers(0, nv, 300)
        jr = np.linalg.norm(pos[ja] - pos[jb], axis=1)
        p = pos.copy()
        core.solve_joints(p, ja, jb, jr, np.ones(300), np.ones(nv), np.ones(nv))
        self.assertLess(np.abs(p - pos).max(), 1e-9)
        jr2 = jr * 0.5                                   # 縮めたい長さ：近づく
        p = pos.copy()
        core.solve_joints(p, ja, jb, jr2, np.ones(300), np.ones(nv), np.ones(nv))
        self.assertLess(np.linalg.norm(p[ja] - p[jb], axis=1).mean(), jr.mean())

    def test_skin_normals_write(self):
        rng = np.random.default_rng(3)
        pos = rng.normal(size=(500, 3))
        vi = rng.integers(0, 500, (200, 8))
        w = rng.random((200, 8))
        s = core.skin(pos, vi, w)
        self.assertLess(np.abs(s - np.einsum("vk,vkj->vj", w, pos[vi])).max(), TOL)
        tri = np.array([[0.0, 0, 0], [1, 0, 0], [0, 1, 0]])
        n = core.vertex_normals(tri, np.array([[0, 1, 2]]))
        self.assertLess(np.abs(np.abs(n[:, 2]) - 1).max(), 1e-9)
        X = rng.normal(size=(4, 3))
        R = np.stack([rot(rng) for _ in range(4)])
        gv, vg, vl = np.arange(30), rng.integers(0, 4, 30), rng.normal(size=(30, 3))
        p = np.zeros((30, 3))
        core.art_write(p, X, R, gv, vg, vl)
        self.assertLess(np.abs(p - (X[vg] + np.einsum("nij,nj->ni", R[vg], vl))).max(), 1e-9)

    def test_box_sdf(self):
        from main import geom
        rng = np.random.default_rng(3)
        C = rng.normal(0, 3, (20, 3))
        A = np.stack([np.linalg.qr(rng.normal(size=(3, 3)))[0] for _ in range(20)])
        E = rng.random((20, 3)) + 0.2
        P = rng.normal(0, 4, (500, 3))
        d2, n2, k2 = geom.box_sdf(P, C, A, E)
        # numpy で同じ計算をした値と比べる
        q = np.einsum("bij,nbj->nbi", A, P[:, None, :] - C[None])
        aq = np.abs(q) - E[None]
        d_all = np.linalg.norm(np.maximum(aq, 0.0), axis=-1) + np.minimum(aq.max(-1), 0.0)
        k1 = np.argmin(d_all, axis=1)
        d1 = d_all[np.arange(len(P)), k1]
        self.assertLess(np.abs(d1 - d2).max(), 1e-9)
        self.assertTrue(np.array_equal(k1, k2))
        self.assertLess(np.abs(np.linalg.norm(n2, axis=1) - 1).max(), 1e-9)
        out = d1 > 0                                     # 外の点：向きは最近点からの向き
        e = 1e-4
        d3, _, _ = geom.box_sdf(P[out] + n2[out] * e, C, A, E)
        self.assertLess(np.abs(d3 - d1[out] - e).max(), 1e-6)

    def test_ray_boxes(self):
        from main import geom
        rng = np.random.default_rng(4)
        n_hit = 0
        for _ in range(300):
            B = int(rng.integers(1, 8))
            C = rng.normal(0, 3, (B, 3))
            A = np.stack([np.linalg.qr(rng.normal(size=(3, 3)))[0] for _ in range(B)])
            E = rng.random((B, 3)) + 0.2
            o = rng.normal(0, 6, 3)
            d = C[0] - o
            d /= np.linalg.norm(d)
            r = geom.ray_boxes(o, d, C, A, E, 30.0)
            if r is None:
                continue
            n_hit += 1
            t, k, nrm = r
            p = o + d * t
            dd, _, kk = geom.box_sdf(p[None], C, A, E)
            if t > 0:
                self.assertLess(abs(dd[0]), 1e-6)            # 当たった点は箱の表面
                self.assertLess(float(nrm @ d), 0.0)         # 法線はレイと向き合う
            # それより手前の点はどの箱にも入っていない
            for s in np.linspace(0, max(t - 1e-3, 0), 20)[:-1]:
                self.assertGreater(geom.box_sdf((o + d * s)[None], C, A, E)[0][0], -1e-9)
        self.assertGreater(n_hit, 100)

    def test_art_runs(self):
        """関節つき剛体の解き方（回転だけ・ヒンジあり）が、値を壊さずに動く。"""
        rng = np.random.default_rng(4)
        G, NR, NC = 4, 3, 20
        X = rng.normal(size=(G, 3))
        R = np.stack([rot(rng) for _ in range(G)])
        invM = rng.random(G) + 0.5
        IB = np.stack([np.diag(rng.random(3) + 0.5) for _ in range(G)])
        ra, rb = np.array([0, 1, 2]), np.array([1, 2, 3])
        LA, LB, LC = rng.normal(size=(NR, 4, 3)), rng.normal(size=(NR, 4, 3)), rng.normal(size=(NR, 3))
        ang, cap = rng.normal(size=NR), np.ones(NR) * 0.3
        cg = rng.integers(0, G, NC)
        cl, cn = rng.normal(size=(NC, 3)), rng.normal(size=(NC, 3))
        cn /= np.linalg.norm(cn, axis=1, keepdims=True)
        cd, cp, cprev, cvs, cmu = rng.random(NC) * 0.1, rng.normal(size=(NC, 3)), rng.normal(size=(NC, 3)), rng.normal(size=(NC, 3)), rng.random(NC)
        for HK in (None, np.array([[0, 1], [-1, -1], [2, 3]], np.int64)):     # 回転だけ・ヒンジあり
            x, r, lam = X.copy(), R.copy(), np.zeros(NC)
            core.art_solve(x, r, invM, IB, ra, rb, LA, LB, LC, ang, cap, cg, cl, cn, cd, cp, cprev, cvs, cmu, 0.005, 6, lam, HK)
            self.assertTrue(np.isfinite(x).all() and np.isfinite(r).all() and (lam >= 0).all())
            self.assertLess(np.abs(np.einsum("nij,nkj->nik", r, r) - np.eye(3)).max(), 1e-6)    # 回転のまま
        V, W = rng.normal(size=(G, 3)), rng.normal(size=(G, 3))
        core.art_contact_vel(V, W, R, invM, IB, cg, cl, cn, np.abs(rng.normal(size=NC)), 3)
        self.assertTrue(np.isfinite(V).all() and np.isfinite(W).all())

    def test_bad_arrays_are_rejected(self):
        with self.assertRaises(ValueError):
            core.solve_joints(np.zeros((4, 3)), np.array([0, 9]), np.array([1, 2]), np.zeros(2), np.zeros(2), np.ones(4), np.ones(4))


@unittest.skipIf(core is None, "Rust の物理コアがありません")
class TestPlanet(unittest.TestCase):
    def _compare(self, pdef_id, n=3000):
        from main.registry import Registry
        from main.planet import make_planet
        reg = Registry().load([])
        pl = make_planet(reg.get("planets", pdef_id), [10.0, -20.0, 5.0])
        rng = np.random.default_rng(5)
        d = rng.normal(size=(n, 3)); d /= np.linalg.norm(d, axis=1, keepdims=True)
        d[:50] = [0, 1, 0]                                     # 出現地点のまわり
        d[50:100] = [1e-9, 1, 0]
        p = pl.center + d * (pl.radius + rng.uniform(-30, 30, (n, 1)))
        pl.dig(p[0], 2.0); pl.dig(p[1], 3.0)
        # タイルの補間値は、補間しない値に近い
        s = pl._sample(d)
        tv = pl._tile_values(d)
        self.assertLess(np.abs(s[:, 0] - tv[:, 0]).max(), 0.25)
        self.assertLess(np.abs(pl.surface_radius_exact(d) - tv[:, 0]).max(), 1e-9)
        # 距離と勾配：sdf_grad の距離は sdf と同じ、勾配は単位ベクトルで、差分の向きとだいたい同じ
        dd = pl.sdf(p)
        dg, g = pl.sdf_grad(p)
        self.assertLess(np.abs(dd - dg).max(), 1e-9)
        self.assertLess(np.abs(np.linalg.norm(g[2:], axis=1) - 1).max(), 1e-9)   # 0・1 は掘削跡の中心（勾配なし）
        e = 1e-3
        fd = np.stack([(pl.sdf(p + np.eye(3)[k] * e) - pl.sdf(p - np.eye(3)[k] * e)) / (2 * e) for k in range(3)], 1)
        fn = fd / np.maximum(np.linalg.norm(fd, axis=1, keepdims=True), 1e-9)
        self.assertGreater(np.median(np.einsum("ij,ij->i", fn, g)), 0.99)
        # 掘削跡：中心は地中ではない（くり抜かれている）
        for cr in pl.craters:
            q = pl.center + np.asarray(cr[:3])
            self.assertGreaterEqual(float(pl.sdf(q[None])[0]), cr[3] - 1e-6)
        # 地表の上下で符号が変わる
        up = pl.center + d[:200] * (pl.surface_radius_exact(d[:200]) + 1.0)[:, None]
        self.assertTrue((pl.sdf(up) > 0).mean() > 0.95)
        if pl.has_caves:
            cd, cg = pl.cave_dist(p - pl.center, grad=True)
            self.assertLess(np.abs(cd - pl.cave_dist(p - pl.center)).max(), 1e-12)
            m = cd < 50
            self.assertLess(np.abs(np.linalg.norm(cg[m], axis=1) - 1).max(), 1e-6)

    def test_sphere_planet(self):
        self._compare("terra")

    def test_earth_planet(self):
        self._compare("gaia")

    def test_gravity(self):
        c = np.array([[0, 0, 0], [0, 0, 4000.0]])
        gm, r = np.array([1e6, 3e5]), np.array([600.0, 200.0])
        p = np.random.default_rng(6).normal(0, 800, (100, 3))
        ref = sum((c[k] - p) * (gm[k] / np.maximum(((c[k] - p) ** 2).sum(1), (r[k] * 0.5) ** 2) ** 1.5)[:, None]
                  for k in range(2))
        self.assertLess(np.abs(core.gravity(p, c, gm, r) - ref).max(), 1e-12)


@unittest.skipIf(core is None, "Rust の物理コアがありません")
class TestPairs(unittest.TestCase):
    def test_pairs_match_brute_force(self):
        rng = np.random.default_rng(7)
        a, b = rng.random((800, 3)) * 10, rng.random((600, 3)) * 10 + 2
        D = np.linalg.norm(a[:, None] - a[None], axis=2)
        ref = {(i, j) for i, j in zip(*np.nonzero(np.triu(D <= 0.6, 1)))}
        x, y = core.pairs_within(a, 0.6)
        self.assertEqual(ref, set(zip(x.tolist(), y.tolist())))
        D = np.linalg.norm(a[:, None] - b[None], axis=2)
        ref = {(i, j) for i, j in zip(*np.nonzero(D <= 0.7))}
        x, y = core.pairs_between(a, b, 0.7)
        self.assertEqual(ref, set(zip(x.tolist(), y.tolist())))

    def test_nearest_label_hull_mc(self):
        """前は scipy・scikit-image でしていた計算（いちばん近い点・空洞の塗り分け・凸包・等値面）。"""
        rng = np.random.default_rng(9)
        A, B = rng.random((300, 3)), rng.random((200, 3)) * 1.2 - 0.1
        ref = np.argmin(np.linalg.norm(B[:, None] - A[None], axis=2), axis=1)
        self.assertTrue((core.nearest(A, B) == ref).all())
        solid = np.ones((7, 7, 7), bool)
        solid[1:3, 1:3, 1:3] = False                     # 中の空洞
        solid[4:6, 4:6, 4:6] = False                     # もう1つ
        solid[0, 0, 0] = False                           # 角（外）
        lab, n = core.label_empty(solid)
        lab = lab.reshape(solid.shape)
        self.assertEqual(n, 3)
        self.assertEqual(len({int(lab[1, 1, 1]), int(lab[5, 5, 5]), int(lab[0, 0, 0])}), 3)
        cube = np.array([[x, y, z] for x in (-1, 0, 1) for y in (-1, 0, 1) for z in (-1, 0, 1)], float)
        t = core.convex_hull(cube)
        vol = abs(np.einsum("ij,ij->", cube[t[:, 0]], np.cross(cube[t[:, 1]], cube[t[:, 2]]))) / 6
        self.assertAlmostEqual(vol, 8.0)
        g = np.linspace(-1, 1, 20)
        X, Y, Z = np.meshgrid(g, g, g, indexing="ij")
        v, f = core.marching_cubes((np.sqrt(X ** 2 + Y ** 2 + Z ** 2) - 0.7).astype(np.float32), 0.0, (1.0, 1.0, 1.0))
        r = np.linalg.norm(v - 9.5, axis=1) / 9.5        # 格子の番号 → 半径（中心は 9.5）
        self.assertLess(np.abs(r - 0.7).max(), 0.02)
        svol = np.einsum("ij,ij->", v[f[:, 0]], np.cross(v[f[:, 1]], v[f[:, 2]])) / 6
        self.assertGreater(svol, 0)                       # 外向き
        from collections import Counter
        E = Counter(tuple(sorted(e)) for t_ in f.tolist() for e in ((t_[0], t_[1]), (t_[1], t_[2]), (t_[2], t_[0])))
        self.assertTrue(all(n_ == 2 for n_ in E.values()))   # 閉じている

    def test_collide_planet(self):
        """地面との接触（Rust）：めり込んだ頂点だけを集め、押し戻したあとは地面の上（ほぼ）にある。"""
        import json, shutil, tempfile
        from main import config, log, paths
        from main.construct import Construct
        from main.world import World, DEFAULT_SETTINGS
        log.setup("ERROR")
        tmp = Path(tempfile.mkdtemp()); saves = paths.SAVES; paths.SAVES = tmp
        try:
            d = paths.world_dir("t"); (d / "scripts").mkdir(parents=True)
            (d / "world.json").write_text(json.dumps(dict(DEFAULT_SETTINGS, name="t", format=2, planets=[{"def": "gaia", "position": [0, 0, 0], "craters": []}])), "utf-8")
            w = World("t", json.loads(json.dumps(config.DEFAULTS)), addons=[]).load()
            pl = w.planets[0]
            up = np.array([0, 1.0, 0])
            g = pl.center + up * float(pl.surface_radius(up[None])[0])
            c = Construct.from_voxels(w.registry, [((x, 0, z), "steel", None) for x in range(4) for z in range(4)],
                                      g - up * 0.3, np.eye(3), size=0.5)
            c._prev = c.pos.copy()
            c.vel[:] = -up * 5
            c.mode = "rigid"
            r = c.size * 0.5
            K = c.contact_cores()
            ok = ~np.isnan(K[:, 0])
            sd = np.full(len(K), np.inf)
            sd[ok] = pl.sdf(K[ok]) - r
            hi, dep, n = c._collide_planet(pl, collect=True)
            self.assertTrue(len(hi))
            self.assertTrue(np.array_equal(np.sort(hi), np.nonzero(sd < 0)[0]))     # めり込んだ頂点だけ
            self.assertLess(np.abs(dep - (-sd[hi])).max(), 1e-6)
            self.assertLess(np.abs(np.linalg.norm(n, axis=1) - 1).max(), 1e-9)
            self.assertGreater(float((n @ up).min()), 0.5)                            # 上向きに押す
            c._collide_planet(pl)
            K2 = c.contact_cores()
            self.assertGreater(float((pl.sdf(K2[hi]) - r).min()), -0.3 * float(dep.max()))
            self.assertTrue(c._want_soft)                                             # 剛体のまま速くぶつかる
        finally:
            paths.SAVES = saves
            shutil.rmtree(tmp, ignore_errors=True)


@unittest.skipIf(core is None or not hasattr(core, "Body"), "Rust の物理コア（段階B）がありません")
class TestStep(unittest.TestCase):
    """1サブステップ（Rust）を何回か進めて、物理として正しく動くか（値が壊れない・地面に乗る・関節が動く など）。"""
    H = 1 / 180

    @classmethod
    def setUpClass(cls):
        import json, tempfile
        from main import config, log, paths
        from main.world import World, DEFAULT_SETTINGS
        log.setup("ERROR")
        cls.tmp = Path(tempfile.mkdtemp())
        cls._saves = paths.SAVES
        paths.SAVES = cls.tmp
        d = paths.world_dir("t")
        (d / "scripts").mkdir(parents=True)
        planets = [{"def": "gaia", "position": [0, 0, 0], "craters": []},
                   {"def": "luna", "position": [2500, 0, 0], "craters": []}]
        (d / "world.json").write_text(json.dumps(dict(DEFAULT_SETTINGS, name="t", format=2, planets=planets,
                                                      gravity_scale=1.3)), "utf-8")
        cls.w = World("t", json.loads(json.dumps(config.DEFAULTS)), addons=[]).load()
        cls.w.planets[0].dig(cls.w.planets[0].center + np.array([0, 600.0, 0]), 2.0)   # 掘削跡も渡す

    @classmethod
    def tearDownClass(cls):
        import shutil
        from main import paths
        paths.SAVES = cls._saves
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def _ground(self, water=False):
        pl = self.w.planets[0]
        rng = np.random.default_rng(5)
        for _ in range(20000):
            u = rng.normal(size=3)
            u /= np.linalg.norm(u)
            s = pl._sample(u[None])[0]
            if (s[1] > s[0] + 3) == water:
                return pl.center + u * (s[1] if water else s[0]), u
        raise RuntimeError

    def _make(self, voxels, extra, water=False, mode="soft", seed=0, size=0.5, setup=None):
        from main import devices
        from main.construct import Construct
        g, up = self._ground(water)
        q = np.linalg.qr(np.c_[up, np.random.default_rng(1).normal(size=(3, 2))])[0]
        R = np.c_[q[:, 1], q[:, 0], q[:, 2]]
        c = Construct.from_voxels(self.w.registry, voxels, g, R, size=size)
        if not water:      # いちばん低い所が地面から extra の高さになるように
            k = c.contact_cores()
            k = k[~np.isnan(k[:, 0])]
            c.pos += up * (extra - (float(self.w.planets[0].sdf(k).min()) - c.size * 0.5))
        else:
            c.pos += up * extra
        devices.rebuild_networks(c)
        rng = np.random.default_rng(seed)
        c.pos += rng.normal(0, 0.01, c.pos.shape)
        c.vel = rng.normal(0, 0.3, c.vel.shape) - up * 2.0
        c.force = rng.normal(0, 50, c.force.shape)
        if setup:
            setup(c)
        if mode != "soft":
            c.enter_rigid()
            c._full_rigid = mode == "full"
        return c

    def _run(self, make, n=12):
        c = make()
        x0 = c.pos.copy()
        br = []
        for _ in range(n):
            c.substep_begin(self.H, self.w.iters, self.w.gravity, self.w.planets, float(self.w.settings["gravity_scale"]))
            br.append(c.substep_end(self.H))
        r = c.size * 0.5
        k = c.contact_cores()
        self.touch = float(np.nanmin(self.w.planets[0].sdf(k[~np.isnan(k[:, 0])]))) - r
        self.broke = br
        self.assertTrue(hasattr(c, "_nb_cache"))                    # Rust の Body で計算した
        self.assertTrue(np.isfinite(c.pos).all() and np.isfinite(c.vel).all())
        self.assertGreater(np.abs(c.pos - x0).max(), 1e-6)          # 動いた
        self.assertLess(np.abs(c.vel).max(), 200.0)                 # 吹き飛んでいない
        self.assertGreater(self.touch, -0.2)                        # 地面に深くめり込んでいない
        return c, c

    SLAB = [((x, 0, z), "steel", None) for x in range(4) for z in range(3)]

    def test_soft_on_ground(self):
        self._run(lambda: self._make(self.SLAB, 0.02))
        self.assertLess(self.touch, 0.01)

    def test_soft_plastic_clay(self):
        a, _ = self._run(lambda: self._make([((x, 0, 0), "clay", None) for x in range(3)], 0.1, seed=3))

    def test_soft_rotor(self):
        vox = [((0, 0, 0), "steel", {"def": "battery", "dir": 2}),
               ((1, 0, 0), "steel", {"def": "actuator", "dir": 2, "name": "arm"}),
               ((1, 1, 0), "steel", None), ((2, 1, 0), "steel", None)]

        def setup(c):
            next(d for d in c.devs if d and d["def"] == "actuator")["state"]["angle"] = 35.0
        a, b = self._run(lambda: self._make(vox, 0.3, setup=setup))
        self.assertTrue(a.rotors)

    def test_soft_hinge(self):
        """ヒンジの関節（ブロックの中心を軸に振る）も Rust で"""
        vox = [((0, 0, 0), "steel", {"def": "battery", "dir": 2}),
               ((1, 0, 0), "steel", {"def": "hinge", "dir": 2, "name": "arm", "state": {"hinge": 0}}),
               ((1, 1, 0), "steel", None), ((1, 2, 0), "steel", None)]

        def setup(c):
            next(d for d in c.devs if d and d["def"] == "hinge")["state"]["angle"] = 35.0
        a, b = self._run(lambda: self._make(vox, 0.3, setup=setup))
        self.assertTrue(a.rotors)

    def test_soft_breaks(self):
        def setup(c):
            c.pos[8:16] += np.array([2.0, 0, 0])      # 2つめのボクセルを引きはがす
        a, b = self._run(lambda: self._make([((0, 0, 0), "wood", None), ((1, 0, 0), "wood", None)], 2.0, setup=setup), n=2)
        self.assertEqual(len(a.links), 0)
        self.assertTrue(any(self.broke))

    def test_soft_water_with_cavity(self):
        box = [((x, y, z), "steel", None) for x in range(3) for y in range(3) for z in range(3) if (x, y, z) != (1, 1, 1)]
        a, _ = self._run(lambda: self._make(box, 0.0, water=True))
        self.assertTrue(a._net.cavities)

    def test_rigid(self):
        self._run(lambda: self._make(self.SLAB, 0.03, mode="rigid"))
        self.assertLess(self.touch, 0.01)

    def test_rigid_water(self):
        self._run(lambda: self._make(self.SLAB, 0.0, water=True, mode="rigid"))

    def test_full_rigid(self):
        self._run(lambda: self._make(self.SLAB, 0.03, mode="full"))
        self.assertLess(self.touch, 0.01)

    def _art_setup(self, angle=30.0, ext=0.0):
        def setup(c):
            for d in c.devs:
                if d and d["def"] == "actuator":
                    d["state"]["angle"] = angle
                    d["state"]["ext"] = ext
        return setup

    def test_art(self):
        """完全剛体モードの関節つき剛体（回転・押し出し・地面）"""
        vox = [((x, 0, z), "steel", None) for x in range(3) for z in range(2)]
        vox += [((1, 1, 0), "steel", {"def": "battery", "dir": 2}), ((1, 2, 0), "steel", {"def": "actuator", "dir": 2}),
                ((1, 3, 0), "aluminum", None), ((2, 3, 0), "aluminum", None), ((0, 1, 1), "steel", {"def": "actuator", "dir": 4}),
                ((0, 1, 2), "wood", None)]

        def make():
            c = self._make(vox, 0.03, setup=self._art_setup(25.0, 0.3))
            c._full_rigid = True
            c.enter_art()
            for d in c.devs:                       # 押し出しは art を作ったあとで動かす
                if d and d["def"] == "actuator":
                    d["state"]["ext"] = 0.6
            return c
        a, b = self._run(make, n=10)
        self.assertEqual(a.mode, "art")
        self.assertEqual(len(a.art["ra"]), 2)
        self.assertLess(np.abs(a.art["X"] - b.art["X"]).max(), 1e-6)
        self.assertLess(np.abs(a.art["applied"] - b.art["applied"]).max(), 1e-9)
        self.assertLess(np.abs(a.art["vl"] - b.art["vl"]).max(), 1e-9)
        self.assertGreater(np.abs(a.art["vl"] - a.art["vl0"]).max(), 0.01)      # 押し出しで形が変わった
        self.assertLess(self.touch, 0.01)

    def test_art_hinge(self):
        """完全剛体モードのヒンジ（とローター）。Rust の ArtTopo で解いている"""
        vox = [((x, 0, z), "steel", None) for x in range(3) for z in range(2)]
        vox += [((1, 1, 0), "steel", {"def": "battery", "dir": 2}),
                ((1, 2, 0), "steel", {"def": "hinge", "dir": 2, "state": {"hinge": 5}}),
                ((1, 3, 0), "aluminum", None), ((1, 4, 0), "aluminum", None),
                ((0, 1, 1), "steel", {"def": "actuator", "dir": 4}), ((0, 1, 2), "wood", None)]

        def make():
            c = self._make(vox, 0.03)
            c._full_rigid = True
            c.enter_art()
            for d in c.devs:
                if d and d["def"] in ("hinge", "actuator"):
                    d["state"]["angle"] = 40.0         # art を作ったあとで動かす（applied が追いかける）
            return c
        a, b = self._run(make, n=10)
        self.assertEqual(a.mode, "art")
        self.assertIsNotNone(b.art["nat"], "ヒンジがあっても Rust の ArtTopo")
        self.assertTrue((a.art["HK"] >= 0).any())
        self.assertGreater(np.abs(a.art["applied"]).max(), 1.0, "動いた")
        self.assertLess(np.abs(a.art["X"] - b.art["X"]).max(), 1e-6)
        self.assertLess(np.abs(a.art["R"] - b.art["R"]).max(), 1e-6)
        self.assertLess(np.abs(a.art["applied"] - b.art["applied"]).max(), 1e-9)

    def _with_box(self, mode, vox):
        """construct の下に MOD の箱（動いている板）を置く"""
        class Owner:
            def is_moving(self):
                return True

            def point_velocity(self, p):
                return np.tile([0.0, 0.0, 0.3], (len(np.atleast_2d(p)), 1))

        def make():
            c = self._make(vox, 0.6, setup=self._art_setup(20.0, 0.0))
            g, up = self._ground(False)
            k = c.contact_cores()
            k = k[~np.isnan(k[:, 0])]
            low = k[np.argmin(k @ up)]
            side = np.cross(up, [0, 0, 1.0]); side /= np.linalg.norm(side)
            A = np.array([np.stack([side, up, np.cross(side, up)])])
            C = np.array([low - up * (c.size * 0.5 + 0.5 - 0.03)])
            c.ext_colliders = [(Owner(), C, A, np.array([[3.0, 0.5, 3.0]]), np.array([7]))]
            c.ext_events = []
            if mode == "full":
                c.enter_rigid()
                c._full_rigid = True
            else:
                c._full_rigid = True
                c.enter_art()
            return c
        a, b = self._run(make, n=10)
        self.assertTrue(a.ext_events)
        J = [e for e in a.ext_events if e[2] is not None]
        self.assertTrue(J)                                     # 動いている箱を押した反動
        up = self._ground(False)[1]
        self.assertTrue(all(float((e[2] @ up).sum()) < 0 for e in J))   # 箱は下向きに押される

    def test_full_rigid_on_mod_box(self):
        """完全剛体：地面と MOD の箱（動いている）の接触を一緒に解く"""
        self._with_box("full", self.SLAB)

    def test_art_on_mod_box(self):
        """関節つき剛体：MOD の箱の上で"""
        vox = [((x, 0, z), "steel", None) for x in range(3) for z in range(2)]
        vox += [((1, 1, 0), "steel", {"def": "battery", "dir": 2}), ((1, 2, 0), "steel", {"def": "actuator", "dir": 2}),
                ((1, 3, 0), "aluminum", None)]
        self._with_box("art", vox)

    def test_contacts(self):
        """構築物どうし・結合していないボクセルどうしの押し出し（Rust）：重なった物が押し出され、眠っていた物が起きる"""
        from main.construct import Construct
        c1 = self._make(self.SLAB, 0.0, seed=1)
        c2 = self._make([((x, 0, z), "wood", None) for x in range(2) for z in range(2)], 0.0, seed=2)
        c2.pos += (c1.voxel_centers().mean(0) - c2.voxel_centers().mean(0)) + c1.size * 0.2
        g, up = self._ground(False)
        c3 = Construct.from_voxels(self.w.registry, [((0, 0, 0), "steel", None), ((1, 0, 0), "steel", None),
                                                     ((1, 0, 1), "steel", None)], c1.voxel_centers()[0] + up * 0.3,
                                   np.eye(3), size=0.5, links=set())
        c3.pos += c1.voxel_centers()[0] + up * 0.3 - c3.voxel_centers()[0]
        c3.asleep = True
        p1, p2 = c1.pos.copy(), c2.pos.copy()
        d0 = np.linalg.norm(c1.voxel_centers().mean(0) - c2.voxel_centers().mean(0))
        woke = self.w._voxel_collisions([c1, c2], [c3])
        self.assertEqual([c.name for c in woke], [c3.name])                      # 眠っていた構築物が起きた
        self.assertGreater(np.abs(c1.pos - p1).max(), 1e-4)                      # 本当に押し出した
        self.assertGreater(np.abs(c2.pos - p2).max(), 1e-4)
        self.assertGreater(np.linalg.norm(c1.voxel_centers().mean(0) - c2.voxel_centers().mean(0)), d0)   # 離れる向き
        self.assertTrue(np.isfinite(c1.pos).all() and np.isfinite(c2.pos).all() and np.isfinite(c3.pos).all())

    def test_nnls(self):
        """負にならない最小二乗：最適の条件（x≥0、x>0 の所は傾き 0、x=0 の所は傾き ≥0）を満たす。"""
        rng = np.random.default_rng(4)
        for n in (1, 3, 8, 32):
            J = rng.normal(size=(n, 6))
            A = J @ J.T
            b = rng.normal(size=n) + 0.3
            x = core.nnls(np.ascontiguousarray(A), b)
            g = A.T @ (A @ x - b)
            tol = 1e-6 * max(1.0, np.abs(A).max() ** 2 * max(1.0, np.abs(b).max()))
            self.assertTrue((x >= 0).all())
            self.assertLess(np.abs(g[x > 1e-9]).max(initial=0.0), tol)
            self.assertGreater(g[x <= 1e-9].min(initial=0.0), -tol)


@unittest.skipIf(core is None or not hasattr(core, "mesh_pack"), "Rust の物理コア（描画用）がありません")
class TestMesh(unittest.TestCase):
    def test_mesh_pack_matches_numpy(self):
        """描画用の頂点（位置・法線・陰影・噴射の色・UV）が numpy 版と同じ。"""
        rng = np.random.default_rng(7)
        nvox, m, nf = 6, 120, 200
        pos = rng.normal(size=(8 * nvox, 3)) + 500.0
        vidx = rng.integers(0, 8 * nvox, (m, 8))
        w = rng.random((m, 8)); w /= w.sum(1, keepdims=True)
        faces = rng.integers(0, m, (nf, 3))
        origin = np.array([499.0, 501.0, 500.5])
        firing = np.zeros(nvox); firing[2] = 0.7
        sun = np.array([0.35, 0.85, 0.4]); sun /= np.linalg.norm(sun); amb = 0.38
        groups = []
        for vs in (np.arange(0, m, 2), np.arange(1, m, 3)):
            groups.append((vs.astype(np.int64), rng.random((len(vs), 3)), rng.random((len(vs), 2)).astype(np.float32),
                           rng.integers(0, nvox, len(vs))))
        for fr in (None, firing):
            outs = core.mesh_pack(pos, vidx, w, faces, list(origin), groups, list(sun), amb, fr)
            V = np.einsum("mk,mkj->mj", w, pos[vidx]) - origin
            N = core.vertex_normals(V, faces)
            for (vs, cols, uv, owner), o in zip(groups, outs):
                n = N[vs]
                k = amb + (1 - amb) * np.clip(n @ sun, 0, 1)
                col = cols * k[:, None]
                if fr is not None:
                    fl = fr[owner][:, None]
                    col = col * (1 - fl * 0.6) + np.array([1.0, 0.75, 0.25]) * fl
                ref = np.concatenate([V[vs], n, col, np.ones((len(vs), 1)), uv], axis=1).astype(np.float32)
                self.assertEqual(o.dtype, np.float32)
                self.assertLess(np.abs(o - ref).max(), 1e-5)


if __name__ == "__main__":
    unittest.main()

"""Rust の物理コア（alpedia_core）と Python（numba / numpy）版が同じ結果を出すかのテスト。
Rust の物理コアが無いときは飛ばす。"""
import sys
import unittest
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from main import fastphys, native  # noqa: E402

core = native.core
TOL = 1e-9


def rnd(*shape, seed=0):
    return np.random.default_rng(seed).normal(size=shape)


@unittest.skipIf(core is None, "Rust の物理コアがありません")
class TestKernels(unittest.TestCase):
    def test_shape_match(self):
        n = 300
        rng = np.random.default_rng(1)
        rest = (np.array([[x, y, z] for x in (0, 1) for y in (0, 1) for z in (0, 1)], float) - 0.5)[None].repeat(n, 0)
        rest = np.ascontiguousarray(rest + rng.normal(0, 0.02, rest.shape))
        pos = (rest * 1.1 + rng.normal(0, 0.05, rest.shape)).reshape(-1, 3) + np.repeat(rng.normal(0, 5, (n, 3)), 8, 0)
        sc = 1 + rng.random((n, 3)) * 0.2
        st = rng.random(n)
        vox = np.arange(0, n, 2, dtype=np.int64)
        for par in (False, True):
            p1, p2 = pos.copy(), pos.copy()
            r1, r2 = np.zeros((n, 3, 3)), np.zeros((n, 3, 3))
            q1 = np.zeros((n, 4)); q1[:, 0] = 1; q2 = q1.copy()
            (fastphys._nb_shape_match_par if par else fastphys._nb_shape_match)(p1, rest, sc, st, vox, r1, q1)
            core.shape_match(p2, rest, sc, st, vox, r2, q2, par)
            self.assertLess(np.abs(p1 - p2).max(), TOL)
            self.assertLess(np.abs(r1 - r2).max(), TOL)
            self.assertLess(np.abs(q1 - q2).max(), TOL)

    def test_joints_cores_skin_normals(self):
        rng = np.random.default_rng(2)
        nv = 800
        pos = rng.normal(size=(nv, 3))
        ja, jb = rng.integers(0, nv, 500), rng.integers(0, nv, 500)
        jr, jk, im = rng.random(500), rng.random(500), rng.random(nv) + 0.5
        jc = rng.integers(1, 5, nv).astype(float)
        p1, p2 = pos.copy(), pos.copy()
        fastphys._nb_solve_joints(p1, ja, jb, jr, jk, im, jc)
        core.solve_joints(p2, ja, jb, jr, jk, im, jc)
        self.assertLess(np.abs(p1 - p2).max(), TOL)
        ct = rng.integers(-1, 3, nv)
        ci = rng.integers(0, nv // 8, (nv, 3))
        k1, k2 = fastphys._nb_contact_cores(pos, ct, ci), core.contact_cores(pos, ct, ci)
        self.assertTrue(np.array_equal(np.isnan(k1), np.isnan(k2)))
        self.assertLess(np.nanmax(np.abs(k1 - k2)), 1e-9)
        vi = rng.integers(0, nv, (200, 8))
        w = rng.random((200, 8))
        s1, s2 = fastphys._nb_skin(pos, vi, w), core.skin(pos, vi, w)
        self.assertLess(np.abs(s1 - s2).max(), TOL)
        faces = rng.integers(0, 200, (300, 3))
        self.assertLess(np.abs(fastphys._nb_vertex_normals(s1, faces) - core.vertex_normals(s1, faces)).max(), 1e-9)

    def test_box_sdf(self):
        from main import geom
        rng = np.random.default_rng(3)
        C = rng.normal(0, 3, (20, 3))
        A = np.stack([np.linalg.qr(rng.normal(size=(3, 3)))[0] for _ in range(20)])
        E = rng.random((20, 3)) + 0.2
        P = rng.normal(0, 4, (500, 3))
        d1, n1, k1 = geom.box_sdf(P, C, A, E)
        d2, n2, k2 = core.box_sdf(P, C, A, E)
        self.assertLess(np.abs(d1 - d2).max(), 1e-9)
        self.assertLess(np.abs(n1 - n2).max(), 1e-9)
        self.assertTrue(np.array_equal(k1, k2))

    def test_art(self):
        rng = np.random.default_rng(4)
        G, NR, NC = 4, 3, 20
        def state():
            X = rng.normal(size=(G, 3))
            R = np.stack([np.linalg.qr(rng.normal(size=(3, 3)))[0] for _ in range(G)])
            R[np.linalg.det(R) < 0] *= -1
            return X, R
        X, R = state()
        invM = rng.random(G) + 0.5
        IB = np.stack([np.diag(rng.random(3) + 0.5) for _ in range(G)])
        ra, rb = np.array([0, 1, 2]), np.array([1, 2, 3])
        LA, LB, LC = rng.normal(size=(NR, 4, 3)), rng.normal(size=(NR, 4, 3)), rng.normal(size=(NR, 3))
        ang, cap = rng.normal(size=NR), np.ones(NR) * 0.3
        cg = rng.integers(0, G, NC)
        cl, cn = rng.normal(size=(NC, 3)), rng.normal(size=(NC, 3))
        cn /= np.linalg.norm(cn, axis=1, keepdims=True)
        cd, cp, cprev, cvs, cmu = rng.random(NC) * 0.1, rng.normal(size=(NC, 3)), rng.normal(size=(NC, 3)), rng.normal(size=(NC, 3)), rng.random(NC)
        a = [X.copy(), R.copy(), np.zeros(NC)]
        b = [X.copy(), R.copy(), np.zeros(NC)]
        fastphys._nb_art_solve(a[0], a[1], invM, IB, ra, rb, LA, LB, LC, ang, cap, cg, cl, cn, cd, cp, cprev, cvs, cmu, 0.005, 6, a[2])
        core.art_solve(b[0], b[1], invM, IB, ra, rb, LA, LB, LC, ang, cap, cg, cl, cn, cd, cp, cprev, cvs, cmu, 0.005, 6, b[2])
        for u, v in zip(a, b):
            self.assertLess(np.abs(u - v).max(), 1e-7)
        V, W = rng.normal(size=(G, 3)), rng.normal(size=(G, 3))
        lam = np.abs(rng.normal(size=NC))
        v1, w1, v2, w2 = V.copy(), W.copy(), V.copy(), W.copy()
        fastphys._nb_art_contact_vel(v1, w1, X, R, invM, IB, cg, cl, cn, lam, 3)
        core.art_contact_vel(v2, w2, R, invM, IB, cg, cl, cn, lam, 3)
        self.assertLess(max(np.abs(v1 - v2).max(), np.abs(w1 - w2).max()), 1e-9)
        gv = np.arange(30)
        vg = rng.integers(0, G, 30)
        vl = rng.normal(size=(30, 3))
        p1, p2 = np.zeros((30, 3)), np.zeros((30, 3))
        fastphys._nb_art_write(p1, X, R, gv, vg, vl)
        core.art_write(p2, X, R, gv, vg, vl)
        self.assertLess(np.abs(p1 - p2).max(), 1e-12)

    def test_bad_arrays_are_rejected(self):
        with self.assertRaises(ValueError):
            core.solve_joints(np.zeros((4, 3)), np.array([0, 9]), np.array([1, 2]), np.zeros(2), np.zeros(2), np.ones(4), np.ones(4))


if __name__ == "__main__":
    unittest.main()


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
        out = {}
        for mode in ("python", "rust"):
            saved = native.core
            if mode == "python":
                native.core = None
            try:
                pl2 = make_planet(reg.get("planets", pdef_id), [10.0, -20.0, 5.0])
                pl2.craters = [list(c) for c in pl.craters]
                s = pl2._sample(d)
                dd = pl2.sdf(p)
                dg, g = pl2.sdf_grad(p)
                cv = pl2.cave_dist(p - pl2.center, grad=True) if pl2.has_caves else None
                out[mode] = (s, dd, dg, g, cv)
            finally:
                native.core = saved
        a, b = out["python"], out["rust"]
        for k in range(4):
            self.assertLess(np.abs(a[k] - b[k]).max(), 1e-7, f"{pdef_id} 項目 {k}")
        if a[4] is not None:
            self.assertLess(np.abs(a[4][0] - b[4][0]).max(), 1e-9)
            self.assertLess(np.abs(a[4][1] - b[4][1]).max(), 1e-9)

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
    def test_pairs_match_kdtree(self):
        from scipy.spatial import cKDTree
        rng = np.random.default_rng(7)
        a, b = rng.random((800, 3)) * 10, rng.random((600, 3)) * 10 + 2
        pr = cKDTree(a).query_pairs(0.6, output_type="ndarray")
        x, y = core.pairs_within(a, 0.6)
        self.assertEqual(set(map(tuple, pr.tolist())), set(zip(x.tolist(), y.tolist())))
        res = cKDTree(a).query_ball_tree(cKDTree(b), 0.7)
        ref = {(i, j) for i, lst in enumerate(res) for j in lst}
        x, y = core.pairs_between(a, b, 0.7)
        self.assertEqual(ref, set(zip(x.tolist(), y.tolist())))

    def test_collide_matches_python(self):
        """地面との接触：Rust と Python で、同じ頂点が同じだけ押し戻される。"""
        import json, shutil, tempfile
        from main import config, log, paths
        from main.construct import Construct
        from main.world import World, DEFAULT_SETTINGS
        log.setup("ERROR")
        tmp = Path(tempfile.mkdtemp()); saves = paths.SAVES; paths.SAVES = tmp
        try:
            d = paths.world_dir("t"); (d / "scripts").mkdir(parents=True)
            (d / "world.json").write_text(json.dumps(dict(DEFAULT_SETTINGS, name="t", format=2, planets=[{"def": "gaia", "position": [0, 0, 0], "craters": []}])), "utf-8")
            res = {}
            for mode in ("python", "rust"):
                saved = native.core
                if mode == "python":
                    native.core = None
                try:
                    w = World("t", json.loads(json.dumps(config.DEFAULTS)), addons=[]).load()
                    pl = w.planets[0]
                    up = np.array([0, 1.0, 0])
                    g = pl.center + up * float(pl.surface_radius(up[None])[0])
                    c = Construct.from_voxels(w.registry, [((x, 0, z), "steel", None) for x in range(4) for z in range(4)],
                                              g - up * 0.3, np.eye(3), size=0.5)
                    c._prev = c.pos.copy() + 0.01
                    c.vel[:] = -up * 5
                    c.mode = "rigid"
                    out = c._collide_planet(pl, collect=True)
                    c._collide_planet(pl)
                    res[mode] = (out, c.pos.copy(), getattr(c, "_want_soft", False))
                finally:
                    native.core = saved
            (h1, d1, n1), p1, s1 = res["python"]
            (h2, d2, n2), p2, s2 = res["rust"]
            self.assertTrue(np.array_equal(np.sort(h1), np.sort(h2)))
            o1, o2 = np.argsort(h1), np.argsort(h2)
            self.assertLess(np.abs(d1[o1] - d2[o2]).max(), 1e-7)
            self.assertLess(np.abs(n1[o1] - n2[o2]).max(), 1e-6)
            self.assertLess(np.abs(p1 - p2).max(), 1e-7)
            self.assertEqual(s1, s2)
        finally:
            paths.SAVES = saves
            shutil.rmtree(tmp, ignore_errors=True)


@unittest.skipIf(core is None or not hasattr(core, "Body"), "Rust の物理コア（段階B）がありません")
class TestStep(unittest.TestCase):
    """1サブステップをまとめて Rust で計算したものと、Python で順に計算したものが同じになる。"""
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
        out = {}
        for native_ in (False, True):
            c = make()
            br = []
            for _ in range(n):
                c.substep_begin(self.H, self.w.iters, self.w.gravity, self.w.planets,
                                float(self.w.settings["gravity_scale"]) if native_ else None)
                br.append(c.substep_end(self.H))
            out[native_] = (c, br)
        (a, ba), (b, bb) = out[False], out[True]
        r = a.size * 0.5
        self.touch = min(float(np.nanmin(pl.sdf(a.contact_cores()[~np.isnan(a.contact_cores()[:, 0])]))) - r
                         for pl in self.w.planets[:1])
        self.assertEqual(ba, bb)
        self.assertTrue(hasattr(b, "_nb_cache") and not hasattr(a, "_nb_cache"))    # 本当に Rust でまとめて計算した
        self.assertEqual(a.links, b.links)
        self.assertEqual(a.n_vox, b.n_vox)
        self.assertLess(np.abs(a.pos - b.pos).max(), 1e-6)
        self.assertLess(np.abs(a.vel - b.vel).max(), 1e-4)
        self.assertLess(np.abs(a.rest - b.rest).max(), 1e-6)
        self.assertEqual(getattr(a, "_want_soft", False), getattr(b, "_want_soft", False))
        return a, b

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

    def test_soft_breaks(self):
        def setup(c):
            c.pos[8:16] += np.array([2.0, 0, 0])      # 2つめのボクセルを引きはがす
        a, b = self._run(lambda: self._make([((0, 0, 0), "wood", None), ((1, 0, 0), "wood", None)], 2.0, setup=setup), n=2)
        self.assertEqual(len(a.links), 0)

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

    def test_nnls(self):
        from scipy.optimize import nnls
        rng = np.random.default_rng(4)
        for n in (1, 3, 8, 32):
            J = rng.normal(size=(n, 6))
            A = J @ J.T
            b = rng.normal(size=n) + 0.3
            x1, _ = nnls(A, b, maxiter=200)
            x2 = core.nnls(np.ascontiguousarray(A), b)
            self.assertTrue((x2 >= 0).all())
            self.assertLess(np.abs(A @ x1 - A @ x2).max(), 1e-7)    # 解が1つでなくても、押し戻しの結果は同じ


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
            N = fastphys._nb_vertex_normals(V, faces) if hasattr(fastphys, "_nb_vertex_normals") else core.vertex_normals(V, faces)
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

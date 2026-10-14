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

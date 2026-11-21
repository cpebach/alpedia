"""レールと車輪（本体）・大都市圏 MOD（megalopolis）のテスト。

    python -m unittest discover -s tests -p test_rail.py -v      （dev フォルダで）
"""
import json
import unittest

import numpy as np

from test_core import CoreCase, DT
from main import paths
from main.construct import Construct

import sys  # noqa: E402
sys.path.insert(0, str(paths.MODS))


def rail_line(n0, n1, y=0, anchor=True):
    V = [((i, y, 0), "steel", {"def": "rail", "dir": 0}) for i in range(n0, n1)]
    if anchor:
        V.append(((0, y - 1, 0), "steel", {"def": "anchor", "dir": 2}))
    return V


def cart():
    """車輪 2 つ・操縦席（+x 向き）・バッテリーの小さな台車"""
    return [((0, 0, 0), "steel", {"def": "wheel", "dir": 4}), ((1, 0, 0), "steel", None), ((2, 0, 0), "steel", None),
            ((3, 0, 0), "steel", {"def": "wheel", "dir": 4}), ((1, 1, 0), "steel", {"def": "seat", "dir": 0}),
            ((2, 1, 0), "steel", {"def": "battery", "dir": 2})]


class TestRail(CoreCase):
    def setup_track(self, n1=60):
        w = self.world()
        p, up = self.ground(w, 0.6)
        tr = Construct.from_voxels(w.registry, rail_line(-4, n1), p, np.eye(3), size=0.5, name="線路")
        w.add_construct(tr)
        c = Construct.from_voxels(w.registry, cart(), p + up * 0.5, np.eye(3), size=0.5, name="台車")
        w.add_construct(c)
        return w, tr, c

    def test_track_from_rail_blocks(self):
        w, tr, c = self.setup_track()
        self.run_for(w, 0.6)
        ra = w.addon("rail")
        mine = [t for k, t in ra.vox.items() if k.startswith(f"v:{tr.id}:")]
        self.assertEqual(len(mine), 1)
        self.assertAlmostEqual(mine[0].length, 64 * 0.5, delta=0.6)
        self.assertIn(c.id, ra.riding, "車輪の下に線路があれば乗る")

    def test_drive_with_keys_and_stop_at_the_end(self):
        w, tr, c = self.setup_track(40)
        self.run_for(w, 0.6)
        ra = w.addon("rail")
        self.assertIn(c.id, ra.riding)
        x0 = c.center_of_mass().copy()
        seat = next(i for i, d in enumerate(c.devs) if d and d["def"] == "seat")
        w.player.seated = (c.id, tuple(c.cells[seat]))
        w.input_keys = {"w"}
        bat = next(d for d in c.devs if d and d["def"] == "battery")
        ch0 = float(bat["state"]["charge"])
        self.run_for(w, 2.0)
        x1 = c.center_of_mass().copy()
        self.assertGreater(float(x1[0] - x0[0]), 2.0, "W で前（操縦席の向き）へ走る")
        self.assertLess(float(bat["state"]["charge"]), ch0, "電気を使う")
        self.run_for(w, 8.0)
        st = ra.riding[c.id]
        trk = ra.track(st["key"])
        self.assertAlmostEqual(st["s"], trk.length, delta=0.01, msg="端で止まる")
        self.assertEqual(st["v"], 0.0)
        # 離すと止まっている（ブレーキ）
        w.input_keys = {"s"}
        self.run_for(w, 1.0)
        w.input_keys = set()
        self.run_for(w, 3.0)
        x2 = c.center_of_mass().copy()
        self.run_for(w, 1.0)
        self.assertLess(float(np.linalg.norm(c.center_of_mass() - x2)), 0.05)

    def test_wheel_e_detaches(self):
        w, tr, c = self.setup_track()
        self.run_for(w, 0.6)
        i = next(k for k, d in enumerate(c.devs) if d and d["def"] == "wheel")
        w.interact({"kind": "voxel", "construct": c, "index": i, "face": 2})
        self.assertNotIn(c.id, w.addon("rail").riding)
        self.run_for(w, 1.0)
        self.assertNotIn(c.id, w.addon("rail").riding, "降ろした車輪はひとりでに乗らない")
        w.interact({"kind": "voxel", "construct": c, "index": i, "face": 2})
        self.assertIn(c.id, w.addon("rail").riding)


class TestMegalopolis(CoreCase):
    """大都市圏 MOD：大きな街 2 つ・工業地帯・複線の線路・電車"""

    def mega(self):
        import sys
        sys.path.insert(0, str(paths.MODS))
        from megalopolis.code.addon import MegaAddon
        from main.world import World, default_bodies
        from test_core import CFG
        self.world(planets=default_bodies("terra", False, False))
        w = World("t", CFG, addons=[MegaAddon()]).load()
        return w, w.addon("megalopolis")

    def test_plan(self):
        from megalopolis.code import plan
        w, m = self.mega()
        kinds = [s["kind"] for s in m.plan["sites"]]
        for k in ("tokyo", "newyork", "industrial"):
            self.assertIn(k, kinds)
        self.assertGreater(len(m.plan["buildings"]), 1000)
        self.assertGreaterEqual(len(m.plan["lines"]), 3)
        # 場所はみなつながっている
        conn = {m.plan["sites"][0]["id"]}
        for _ in range(5):
            for ln in m.plan["lines"]:
                if ln["a"] in conn or ln["b"] in conn:
                    conn |= {ln["a"], ln["b"]}
        self.assertEqual(conn, {s["id"] for s in m.plan["sites"]})
        # 複線がレールの機能に登録されている
        ra = w.addon("rail")
        self.assertEqual(len([k for k in ra.ext if k.startswith("mega:")]), 2 * len(m.plan["lines"]))
        # 線路のそばに建物は無い・高い塔がある
        pl = w.planets[0]
        LP = plan.unit(np.concatenate([ln["P"] for ln in m.plan["lines"]]))
        bd = np.array([b["dir"] for b in m.plan["buildings"]])
        near = np.arccos(np.clip(bd @ LP.T, -1, 1)).min(1) * pl.radius
        self.assertGreater(float(near.min()), plan.CLEAR)
        self.assertGreater(max(b["h"] for b in m.plan["buildings"]), 200)
        # 同じ惑星なら毎回同じ
        w2, m2 = self.mega()
        self.assertEqual([b["id"] for b in m2.plan["buildings"]][:50], [b["id"] for b in m.plan["buildings"]][:50])

    def test_paving(self):
        """街の道はアスファルト・区画はコンクリート・公園は草地のまま。場所どうしの道路も舗装。地表の色に出る"""
        from megalopolis.code import pave, plan
        w, m = self.mega()
        pl = w.planets[0]
        nat = pl._nat()
        T = next(s for s in m.plan["sites"] if s["kind"] == "tokyo")
        # 東京型：区画は 70 m ごと（真ん中 56 m が区画、まわり 14 m が道）
        lot = next(b for b in T["blocks"] if b[4] == "lot")
        x, z = lot[0], lot[1]
        # 街の外：縁から 600 m・どの線からも離れた所
        LP = plan.unit(np.concatenate([ln["P"] for ln in m.plan["lines"]]))
        outs = [(np.cos(a) * (T["R"] + 600.0), np.sin(a) * (T["R"] + 600.0)) for a in np.linspace(0, 2 * np.pi, 16, endpoint=False)]
        ox, oz = max(outs, key=lambda o: float(np.arccos(np.clip(LP @ plan.along(T["d"], T["F"], o[0], o[1], pl.radius), -1, 1)).min()))
        dirs = plan.along(T["d"], T["F"], np.array([x, x + 35.0, ox]), np.array([z, z, oz]), pl.radius)
        pv = nat.pave_at(dirs)
        self.assertGreater(pv[0, 0], 0.4, "区画はコンクリート（明るい灰色）")
        self.assertLess(pv[1, 0], 0.25, "道はアスファルト（暗い灰色）")
        self.assertEqual(pv[2, 3], 0.0, "街の外は塗らない")
        self.assertTrue((pv[:2, 3] > 0.99).all())
        parks = [b for s in m.plan["sites"] for b in s["blocks"] if b[4] == "park"]
        if parks:
            s = next(s for s in m.plan["sites"] if parks[0] in s["blocks"])
            self.assertEqual(nat.pave_at(plan.along(s["d"], s["F"], np.array([parks[0][0]]), np.array([parks[0][1]]), pl.radius))[0, 3], 0.0,
                             "公園は草地のまま")
        # 線路の横の道路
        ln = m.plan["lines"][0]
        i = len(ln["P"]) // 2
        road = plan.unit(plan.offset_track(ln["P"], ln["U"], pave.ROAD_SIDE))[i:i + 1]
        off = plan.unit(plan.offset_track(ln["P"], ln["U"], pave.ROAD_SIDE + 30.0))[i:i + 1]
        self.assertGreater(nat.pave_at(road)[0, 3], 0.99)
        self.assertEqual(nat.pave_at(off)[0, 3], 0.0)
        # 地表の色（近く・遠景）にも出る
        q = np.concatenate([dirs[1:2], road])
        col = pl.surface_color(q, pl.surface_radius(q), q)
        self.assertTrue((np.abs(np.asarray(col) - pave.ASPHALT[0]).max(1) < 0.06).all(), col)
        _, col = pl.far_samples(dirs[1:2])
        self.assertLess(float(np.abs(np.asarray(col)[0] - pave.ASPHALT[0]).max()), 0.06)
        # 遠景のメッシュ（点の間隔が道より広い）では、細い道は薄く広がる（ぎざぎざにしない）
        a, b = plan.offset_track(ln["P"], ln["U"], pave.ROAD_SIDE)[i], ln["P"][i + 1] - ln["P"][i]
        side = plan.unit(np.cross(ln["U"][i], b))
        row = plan.unit(np.array([a + side * k for k in np.arange(-120, 121, 40.0)]))
        _, col = pl.far_samples(row)
        mid = np.asarray(col)[3]
        self.assertGreater(float(np.abs(mid - pave.ASPHALT[0]).max()), 0.05, "40 m 間隔では道の色だけにはならない")

    def test_schedule(self):
        from megalopolis.code import addon as A
        Ln = 4000.0
        ts = np.arange(0, 1200, 0.5)
        sv = np.array([A.schedule(Ln, t, 0.0) for t in ts])
        s, v = sv[:, 0], sv[:, 1]
        self.assertGreaterEqual(s.min(), A.STOP - 1e-6)
        self.assertLessEqual(s.max(), Ln - A.STOP + 1e-6)
        self.assertLessEqual(np.abs(v).max(), A.VMAX + 1e-6)
        self.assertLess(np.abs(np.diff(s)).max(), A.VMAX * 0.5 + 0.1, "なめらかに動く")
        self.assertTrue((s == A.STOP).sum() >= int(A.DWELL / 0.5) - 1, "駅で止まる")
        self.assertTrue(np.isclose(s, Ln - A.STOP).any(), "向こうの駅まで行く")

    def test_streaming_and_trains(self):
        w, m = self.mega()
        pl = w.planets[0]
        site = next(s for s in m.plan["sites"] if s["kind"] == "tokyo")
        w.player.flying = True

        def hold(p, sec):
            for _ in range(int(sec / DT)):
                w.player.pos[:] = p
                w.player.vel[:] = 0
                w.step(DT)

        hold(pl.center + site["d"] * (pl.radius + 80), 6.0)
        mine = [cid for cid in w.constructs if str(cid).startswith(f"mb-{site['id']}-")]
        self.assertGreater(len(mine), 10, "近くの建物をブロックで置く")
        self.assertTrue(all(w.constructs[c].anchored for c in mine))
        self.assertTrue(any(str(c).startswith("ms-") for c in w.constructs), "線路の区間")
        # 電車：線路の端の駅のそばで待つと、車両が来て動く
        tr = m.trains[0][2]
        R, x, _ = tr.frame(200.0)
        hold(x + R[:, 1] * 30, 1.0)
        cars = [c for c in w.constructs if str(c).startswith("mt-")]
        self.assertGreater(len(cars), 0, "近くの電車は車両を作る")
        moving = False
        for _ in range(16):
            here = {c: w.constructs[c].center_of_mass().copy() for c in w.constructs if str(c).startswith("mt-")}
            hold(x + R[:, 1] * 30, 4.0)
            if any(c in w.constructs and np.linalg.norm(w.constructs[c].center_of_mass() - p0) > 20 for c, p0 in here.items()):
                moving = True
                break
        self.assertTrue(moving, "電車が走る")
        # 離れると片付く
        hold(pl.center + site["d"] * (pl.radius + 80) + site["F"][0] * 4000.0, 3.0)
        self.assertLess(len([c for c in w.constructs if str(c).startswith(f"mb-{site['id']}-")]), len(mine))


class TestMegaLife(CoreCase):
    """A-10：大都市圏の街の暮らし（街 MOD と一緒に）。NPC・百貨店と値下げ・高い部屋・収益物件・工場の仕事"""

    def setUp(self):
        super().setUp()
        from city.code.addon import CityAddon
        from megalopolis.code.addon import MegaAddon
        from main.world import World, default_bodies
        from test_core import CFG
        self.world(planets=default_bodies("terra", False, False), mode="survival")
        self.w = World("t", CFG, addons=[CityAddon(), MegaAddon()]).load()
        self.w.step(DT)
        self.ca = self.w.addon("city")
        self.mc = [c for c in self.ca.cities.cities if "-metro-" in c.id]

    def city(self, stype):
        return next(c for c in self.mc if c.stype == stype)

    def test_districts(self):
        """店・不動産屋・部屋は大都市圏の建物の中に点在する。高層階は街でいちばん高いビルの上の階"""
        from city.code import citylife
        m = self.w.addon("megalopolis")
        self.assertEqual(sorted(c.stype for c in self.mc).count("metro"), 2)
        self.assertGreaterEqual(sorted(c.stype for c in self.mc).count("works"), 1)
        M = self.city("metro")
        self.assertTrue({"dept", "estate", "trader", "citizen"} <= {n["role"] for n in M.npcs})
        self.assertTrue(any(n.get("gossip") for n in M.npcs), "情報通がいる")
        self.assertTrue({"foreman", "worker"} <= {n["role"] for n in self.city("works").npcs})
        by = {b["id"]: b for b in m.plan["buildings"]}
        site = next(s for s in m.plan["sites"] if M.id.endswith(s["id"]))
        R = self.w.planets[0].radius
        depts = [n for n in M.npcs if n["role"] == "dept"]
        self.assertEqual(len(depts), 2)
        for n in [x for x in M.npcs if x["role"] in ("dept", "estate", "trader")]:
            b = by[n["inside"]]
            self.assertIn(b["style"], ("office", "tower", "apartment"), "既にある建物の中")
            # NPC は建物の足もとの範囲の中
            q = M.x + n["pos"] - self.w.planets[0].center
            q /= np.linalg.norm(q)
            loc = np.array([q @ site["F"][0], q @ site["F"][2]]) * R / max(q @ site["d"], 1e-9)
            self.assertLess(abs(loc[0] - b["x"]), b["w"] / 2 + 1.0)
            self.assertLess(abs(loc[1] - b["z"]), b["d"] / 2 + 1.0)
        a, b2 = (by[n["inside"]] for n in depts)
        self.assertGreater(np.hypot(a["x"] - b2["x"], a["z"] - b2["z"]), 200.0, "都心と郊外に点在")
        # 高層階：いちばん高いビル（入口が道に面している物）の上の方
        views = [h for h in M.homes if h.get("grade") == "view"]
        self.assertGreaterEqual(len(views), 2)
        tower = by[views[0]["inside"]]
        tall = sorted((b for b in m.plan["buildings"] if b["site"] == site["id"]), key=lambda b: -b["h"])[:6]
        self.assertIn(tower["id"], [b["id"] for b in tall])
        up = (M.x - self.w.planets[0].center) / np.linalg.norm(M.x - self.w.planets[0].center)
        hgt = float((np.asarray(views[0]["lift_up"]) - np.asarray(views[0]["lift_down"])) @ up)
        self.assertGreater(hgt, tower["h"] * 0.7, "上の方の階")
        # 部屋：ふつう < 駅近 < 高層階
        price = {g: np.mean([citylife.home_price(M, h) for h in M.homes if h.get("grade") == g]) for g in ("normal", "station", "view")}
        self.assertLess(price["normal"], price["station"])
        self.assertLess(price["station"], price["view"])
        self.assertTrue(all(h.get("inside") in by for h in M.homes))

    def test_station_building(self):
        """都心の駅は駅ビル（壁・屋根・上の階）。工業地帯の駅はそのまま"""
        from megalopolis.code import build
        m = self.w.addon("megalopolis")
        sts = [o for o in m.objs if o["kind"] == "st"]
        self.assertTrue(any(o["big"] for o in sts) and not all(o["big"] for o in sts))
        plain, big = build.station(False), build.station(True)
        self.assertGreater(len(big), len(plain) + 3000)
        ys = [c[1] for c, _, _ in big]
        self.assertGreaterEqual(max(ys), 10 + build.STATION_TOWER[1])
        # 両端は電車が通れる（線路の上の真ん中が空いている）
        cells = {c for c, _, _ in big}
        for z in (0, build.STATION_LEN - 1):
            self.assertFalse(any((x, y, z) in cells for x in range(-3, 4) for y in range(1, 5)))

    def test_dept_sales_and_tips(self):
        from city.code import citylife, metro
        M = self.city("metro")
        p = metro.dept_prices(M, 0.0)
        quiet = next(t for t in np.arange(0, 7200, 20.0) if not metro.sale_now(M, t))
        pq = metro.dept_prices(M, quiet)
        self.assertGreater(pq["fuel"][0], citylife.prices(M)["fuel"][0], "ふだんは商人より高い")
        big = [x for s in range(12) for x in metro.sales(M, s) if x[1] >= metro.BIG_SALE]
        self.assertTrue(big, "ときどき大きな値下げ")
        res, off, a, b = big[0]
        mid = (a + b) / 2
        self.assertAlmostEqual(metro.dept_prices(M, mid)[res][0], round(metro.dept_prices(M, quiet)[res][2] * (1 - off), 2), places=1)
        # うわさ：始まる前に、情報通が教えてくれる
        gossip = next(n for n in M.npcs if n.get("gossip"))
        line = citylife.talk_line(M, gossip, self.ca, a - 60.0)
        self.assertIn("割引", line)
        self.assertEqual(metro.sales(M, 3), metro.sales(M, 3), "毎回同じ")
        # 買う
        w = self.w
        w.player.inventory["credits"] = 1000.0
        msg = self.ca.economy.dept_buy(M, "electronics", 2)
        self.assertIn("買いました", msg)
        self.assertEqual(w.player.inventory["electronics"], 2.0)
        del p

    def test_properties_pay(self):
        from city.code import metro
        eco, w = self.ca.economy, self.w
        props = metro.properties(self.ca)
        self.assertTrue({"factory", "mine", "farm"} <= {p["kind"] for p in props})
        fac = next(p for p in props if p["kind"] == "factory")
        w.player.inventory["credits"] = fac["price"] - 1
        self.assertIn("足りません", eco.buy_property(fac["id"]))
        w.player.inventory["credits"] = fac["price"] + 10
        self.assertIn("買いました", eco.buy_property(fac["id"]))
        w.time += metro.PROP_PERIOD * 2 + 5
        eco.update(5.0)
        self.assertEqual(w.player.inventory.get("electronics"), 6.0)
        self.assertAlmostEqual(w.player.inventory["credits"], 10 + 240.0, places=1)
        w2 = self.reload(w, addons=[type(self.ca)(), type(w.addon("megalopolis"))()])
        self.assertIn(fac["id"], w2.settings["economy"]["props"], "保存される")

    def test_factory_job(self):
        from city.code import metro
        eco, w = self.ca.economy, self.w
        Wk = self.city("works")
        fore = next(n for n in Wk.npcs if n["role"] == "foreman")
        self.assertTrue(any(p.get("tag", (None,))[0] == "pad" for p in Wk.parts))
        self.assertEqual(sum(1 for p in Wk.parts if p.get("tag", (None,))[0] == "machine"), len(metro.MACHINES))
        w.player.inventory["credits"] = 0.0
        eco.job_talk(Wk, fore)
        job = eco.state["job"]
        wrong = ("pad", (job["target"] + 1) % metro.N_PADS) if job["type"] == "pad" else ("machine", (job["target"] + 1) % 3)
        eco.job_act(Wk, *wrong)
        self.assertEqual(w.player.inventory["credits"], 0.0, "違う所では払われない")
        self.assertIsNotNone(metro.job_target(self.ca))
        eco.job_act(Wk, job["type"], job["target"])
        self.assertGreaterEqual(w.player.inventory["credits"], metro.JOB_PAY)
        self.assertIn("お疲れ", eco.job_talk(Wk, fore))
        self.assertIsNone(eco.state["job"])

    def test_view_room(self):
        M = self.city("metro")
        h = next(x for x in M.homes if x.get("grade") == "view")
        up = (M.x - self.w.planets[0].center) / np.linalg.norm(M.x - self.w.planets[0].center)
        self.assertGreater(float((np.asarray(h["lift_up"]) - np.asarray(h["lift_down"])) @ up), 60.0, "最上階は高い")
        self.assertEqual(sum(1 for p in M.parts if p.get("tag", (None,))[0] == "lift" and p["tag"][1] == h["id"]), 2)
        self.w.player.inventory["credits"] = 1e6
        self.assertFalse(self.ca.can_use_home(h["id"]))
        self.ca.economy.rent_home(M, h)
        self.assertTrue(self.ca.can_use_home(h["id"]))


if __name__ == "__main__":
    unittest.main()

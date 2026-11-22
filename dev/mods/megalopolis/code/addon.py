"""大都市圏（ワールド側）：計画（plan）を作り、近くの建物・線路・駅をブロックで置き、電車を時刻表どおりに走らせる。

- 建物（NEAR_B）・線路の区間と駅（NEAR_T）：プレイヤーが近づいたら少しずつ置き（固定。動かない）、離れたら片付ける。
  手を加えた物は残し、壊されて無くなった物は作り直さない（settings["megalopolis"] に保存）。
- 線路：各線は複線。2 本ともレールの機能（main.rail）に登録するので、車輪のある乗り物も走れる。
- 電車：各線の 2 本の線路に 1 編成ずつ（3 両）。端の駅の間を行ったり来たりする（駅で DWELL 秒止まる）。
  位置はワールドの時刻だけで決まる（保存しない）。プレイヤーの近く（NEAR_C）では車両をブロックで作って動かす（乗れる・座れる）。
"""
import numpy as np

from main import log
from main.construct import Construct
from main.hooks import Addon

from . import build, district, pave, plan

L = log.get("megalopolis")

NEAR_B, FAR_B = 320.0, 450.0       # 建物
NEAR_T, FAR_T = 520.0, 680.0       # 線路の区間・駅
NEAR_C, FAR_C = 800.0, 1000.0      # 電車の車両
CHECK, CHECK_BUSY = 1.0, 0.15
PER_CHECK = 4
VOX_BUDGET = 4500                  # 1 回に置くブロックの数の目安（一度にたくさん置くと固まる）
VMAX, ACC, DWELL = 33.0, 0.7, 25.0
CARS = 3
STOP = 70.0                        # 線路の端から、止まる所（編成の真ん中）まで


def schedule(length, t, phase):
    """線路の長さ length の端と端（STOP）の間を行き来する編成の (真ん中の s, 速さ)"""
    sa, sb = STOP, max(length - STOP, STOP + 1.0)
    Ln = sb - sa
    d_acc = VMAX * VMAX / (2 * ACC)
    if Ln < 2 * d_acc:
        t_acc = np.sqrt(Ln / ACC)
        t_run = 2 * t_acc
        vpk = ACC * t_acc
    else:
        t_acc = VMAX / ACC
        t_run = 2 * t_acc + (Ln - 2 * d_acc) / VMAX
        vpk = VMAX

    def run(tt):
        if tt < t_acc:
            return 0.5 * ACC * tt * tt, ACC * tt
        if tt > t_run - t_acc:
            r = t_run - tt
            return Ln - 0.5 * ACC * r * r, ACC * r
        return 0.5 * ACC * t_acc * t_acc + vpk * (tt - t_acc), vpk

    period = 2 * (t_run + DWELL)
    tt = (t + phase * period) % period
    if tt < DWELL:
        return sa, 0.0
    tt -= DWELL
    if tt < t_run:
        d, v = run(tt)
        return sa + d, v
    tt -= t_run
    if tt < DWELL:
        return sb, 0.0
    tt -= DWELL
    d, v = run(min(tt, t_run))
    return sb - d, -v


class MegaAddon(Addon):
    name = "megalopolis"
    settings_keys = ("megalopolis",)

    def __init__(self):
        self.world = None
        self.planet = None
        self.plan = None
        self.objs = []            # 置く物：{"id", "kind", "pos"（世界）, "near", "far", ...}
        self.pos = np.zeros((0, 3))
        self.made = {}            # ID → 作ったときの版（None：次の確認で覚える）
        self.kept, self.gone = set(), set()
        self.cars = {}            # 車両の ID → (線の番号, 線路の番号, 何両目)
        self.trains = []          # [(線, 線路の番号, Track, 色)]
        self.version = 0          # 置いた建物の組が変わったら増える（画面側の遠景を作り直す）
        self._last = None
        self._busy = False
        self._ct = 0.0
        self._inside = {}
        self._mt = 0.0
        self.districts = {}       # 街の暮らしに使う区画（district.py）
        self._dist_done = False

    # ------------------------------------------------------------------ 読み込み
    def on_load(self, world):
        self.world = world
        st = world.settings.get("megalopolis") or {}
        self.kept, self.gone = set(st.get("kept", [])), set(st.get("gone", []))
        pl = world.planets[0] if world.planets else None
        if pl is None or pl.radius < 8000 or float(pl.defn.get("atmosphere_pressure", 101.3) or 0.0) <= 0:
            L.info("大都市圏：主な惑星に空気が無い・小さいので作りません")
            return
        self.planet = pl
        avoid = []
        ca = world.addon("city")
        for c in getattr(getattr(ca, "cities", None), "cities", []) or []:
            if getattr(c, "host", None) is pl and getattr(c, "kind", "") == "surface":
                d = np.asarray(c.x, float) - np.asarray(pl.center, float)
                avoid.append((d / np.linalg.norm(d), float(getattr(c, "radius", 200.0))))
        seed = (pl.id, pl.seed, round(pl.radius))
        self.plan = plan.generate(pl, seed, avoid)
        self._seed = seed
        pave.apply(pl, self.plan, seed)              # 街の道・区画・場所どうしの道路を舗装の色に
        self.sites = {s["id"]: s for s in self.plan["sites"]}
        self._objects()
        # 線路をレールの機能に登録・電車
        ra = world.addon("rail")
        for li, ln in enumerate(self.plan["lines"]):
            for ti, P in enumerate(ln["tracks"]):
                key = f"mega:{ln['id']}:{ti}"
                tr = ra.add_track(key, pl, P, ln["U"], ln["name"]) if ra is not None else None
                if tr is None:
                    from main.rail import Track
                    tr = Track(key, pl, P, ln["U"], ln["name"])
                self.trains.append((li, ti, tr, build.LINE_COLORS[li % len(build.LINE_COLORS)]))
        # 保存されていた車両は作り直す（位置は時刻で決まる）
        for cid in [k for k in world.constructs if str(k).startswith("mt-")]:
            del world.constructs[cid]
        # 保存されていた建物・線路（手を加えていない物）：これからも出し入れする
        for cid, c in list(world.constructs.items()):
            if str(cid).startswith(("mb-", "ms-", "mst-")):
                c.flat_mesh = True                       # 面をまとめた平らなメッシュ（A-15。前のセーブの物にも）
            if str(cid).startswith(("mb-", "ms-", "mst-")) and cid not in self.kept:
                if cid not in self.by_id or str(cid).startswith("mst-"):
                    del world.constructs[cid]        # 今は建てない物・駅（形が変わったかもしれないので、近づいたら作り直す）
                    continue
                self.made[cid] = c.version
        self._ensure_districts(world)
        L.info("大都市圏：%d か所・建物 %d・線 %d", len(self.plan["sites"]), len(self.plan["buildings"]), len(self.plan["lines"]))

    def _objects(self):
        pl = self.planet
        pc = np.asarray(pl.center, float)
        objs = []
        for b in self.plan["buildings"]:
            p = pc + b["dir"] * (b["g"] + b["h"] / 2)
            objs.append({"id": b["id"], "kind": "b", "b": b, "near": NEAR_B + b["h"] * 0.3, "far": FAR_B + b["h"] * 0.3, "p": p})
        for ln in self.plan["lines"]:
            P, U, g = ln["P"], ln["U"], ln["ground"]
            for i in range(len(P) - 1):
                pier = float(np.linalg.norm(P[i]) - g[i]) - build.RAIL_TOP
                objs.append({"id": f"ms-{ln['id']}-{i}", "kind": "s", "i": i, "line": ln, "pier": pier,
                             "near": NEAR_T, "far": FAR_T, "p": pc + P[i]})
            # 駅：両端（線路の端から STOP の所が真ん中）
            S = np.concatenate([[0.0], np.cumsum(np.linalg.norm(np.diff(P, axis=0), axis=1))])
            for end, s in (("a", STOP), ("b", S[-1] - STOP)):
                i = int(np.clip(np.searchsorted(S, s) - 1, 0, len(P) - 2))
                f = (s - S[i]) / max(S[i + 1] - S[i], 1e-9)
                p = P[i] * (1 - f) + P[i + 1] * f
                T = plan.unit(P[i + 1] - P[i])
                big = self.sites[ln[end]]["kind"] in ("tokyo", "newyork")     # 都心の駅は駅ビル
                objs.append({"id": f"mst-{ln['id']}-{end}", "kind": "st", "P": p, "U": plan.unit(p), "T": T, "big": big,
                             "near": NEAR_T, "far": FAR_T, "p": pc + p})
        self.objs = objs
        self.pos = np.array([o["p"] for o in objs]) if objs else np.zeros((0, 3))
        self.by_id = {o["id"]: o for o in objs}
        self._idx = {o["id"]: i for i, o in enumerate(objs)}
        self._nears = np.array([o["near"] for o in objs])
        self._fars = np.array([o["far"] for o in objs])

    def on_save(self, world):
        world.settings["megalopolis"] = {"kept": sorted(self.kept), "gone": sorted(self.gone)}

    # ------------------------------------------------------------------ 毎ティック
    def players(self):
        w = self.world
        ps = [w.player] + list(getattr(w, "remote", {}).values())
        return [np.asarray(p.pos, float) for p in ps if p is not None]

    def _ensure_districts(self, world):
        """街 MOD があれば、場所ごとの街（NPC・店・部屋・仕事場）を加える（街 MOD の読み込みの後で。A-10）"""
        if self._dist_done or self.plan is None:
            return
        ca = world.addon("city")
        if ca is None:
            self._dist_done = True
            return
        if getattr(ca, "economy", None) is None:
            return                                   # 街 MOD の読み込みがまだ（次のティックで）
        self._dist_done = True
        if not ca.cities.enabled:
            return
        pi = list(world.planets).index(self.planet)
        # 前の版の街（駅前の広場に建てていた）の建物を片付ける
        for cid in [k for k in world.constructs if str(k).startswith("blk-") and f"-mega-m" in str(k)]:
            del world.constructs[cid]
        n = 0
        for s in self.plan["sites"]:
            try:
                c = district.build(ca, self.planet, pi, s, self.plan, self._seed)
                if c is None:
                    continue
                ca.cities.add_city(c)
                n += 1
            except Exception:  # noqa: BLE001
                L.exception("大都市圏：%s の街を作れませんでした", s["name"])
        L.info("大都市圏：街の暮らし %d か所", n)

    def before_physics(self, world, dt):
        if self.plan is None:
            return set()
        self._ensure_districts(world)
        self._dt = float(dt)
        self._where(world, float(dt))
        return self._trains_move(world)

    def after_physics(self, world, cons):
        """置く・片付けるのは物理計算の後（ティックの途中で構築物を消さない）"""
        if self.plan is None:
            return
        if not (getattr(world, "net", None) is not None and getattr(world.net, "is_client", False)):
            self._stream(world)
            self._trains_make(world, getattr(self, "_dt", 1 / 60))

    def _where(self, world, dt):
        """大きな街に入ったら名前を知らせる"""
        self._mt += dt
        if self._mt < 1.0:
            return
        self._mt = 0.0
        pc = np.asarray(self.planet.center, float)
        for p in [world.player] + list(getattr(world, "remote", {}).values()):
            if p is None:
                continue
            q = np.asarray(p.pos, float) - pc
            q /= max(np.linalg.norm(q), 1e-9)
            here = None
            for s in self.plan["sites"]:
                if plan.angdist(q, s["d"], self.planet.radius) < s["R"]:
                    here = s["name"]
                    break
            if here != self._inside.get(id(p)):
                self._inside[id(p)] = here
                if here and p is world.player:
                    world.message(f"{here}に入りました")

    def _stream(self, world):
        w = world
        if self._last is not None and w.time - self._last < (CHECK_BUSY if self._busy else CHECK):
            return
        self._last = w.time
        pp = self.players()
        if not pp or not len(self.pos):
            return
        cons = w.constructs
        changed = False
        for cid, ver in list(self.made.items()):
            c = cons.get(cid)
            if c is None:
                self.gone.add(cid)
                del self.made[cid]
                changed = True
            elif ver is None:
                self.made[cid] = c.version
            elif c.version != ver:
                self.kept.add(cid)
                del self.made[cid]
        dist = np.min(np.stack([np.linalg.norm(self.pos - p, axis=1) for p in pp]), axis=0)
        # 遠くなった物を片付ける（手を加えていない物だけ）
        for cid in list(self.made):
            i = self._idx.get(cid)
            if i is None:
                continue
            if dist[i] > self._fars[i] and cid in cons and self.made[cid] in (None, cons[cid].version):
                del cons[cid]
                del self.made[cid]
                changed = True
        # 近い物から少しずつ置く
        near = np.nonzero(dist < self._nears)[0]
        left = PER_CHECK
        budget = VOX_BUDGET
        busy = False
        for i in near[np.argsort(dist[near])]:
            o = self.objs[i]
            cid = o["id"]
            if cid in cons or cid in self.gone or cid in self.kept or o.get("skip"):
                continue
            if left <= 0 or budget <= 0:
                busy = True
                break
            c = self._make(w, o)
            if c is None:
                o["skip"] = True
                continue
            c.flat_mesh = True                           # 面をまとめた平らなメッシュ（A-15）
            w.add_construct(c)
            self.made[cid] = None
            left -= 1
            budget -= c.n_vox
            changed = True
        self._busy = busy
        if changed:
            self.version += 1

    def _make(self, w, o):
        pl = self.planet
        pc = np.asarray(pl.center, float)
        if o["kind"] == "b":
            b = o["b"]
            r = build.realize(b, self.sites[b["site"]], pl)
            if r is None:
                return None
            V, org, rot, s, name = r
            return Construct.from_voxels(w.registry, V, org, rot, size=s, name=f"{self.sites[b['site']]['name']}の{name}",
                                         id_=o["id"])
        if o["kind"] == "s":
            ln, i = o["line"], o["i"]
            P0, P1 = pc + ln["P"][i], pc + ln["P"][i + 1]
            org, rot, n = build.segment_place(P0, P1, ln["U"][i])
            V = build.segment(n, o["pier"])
            from main import buildkit
            V = buildkit.anchored(V, 4)
            return Construct.from_voxels(w.registry, V, org, rot, size=build.SEG_S, name=ln["name"] + "の線路", id_=o["id"])
        if o["kind"] == "st":
            org, rot = build.station_place(pc + o["P"], o["U"], o["T"])
            from main import buildkit
            V = buildkit.anchored(build.station(o.get("big", False)), 3)
            ln_name = o["id"].split("-")[1]
            name = next((ln["name"] for ln in self.plan["lines"] if ln["id"] == ln_name), "駅")
            return Construct.from_voxels(w.registry, V, org, rot, size=1.0, name=name + ("の駅ビル" if o.get("big") else "の駅"),
                                         id_=o["id"])
        return None

    # ------------------------------------------------------------------ 電車
    def car_s(self, k, t):
        """編成 k の車両ごとの (s, 速さ)"""
        li, ti, tr, _ = self.trains[k]
        phase = 0.5 * ti + 0.13 * li
        sc, v = schedule(tr.length, t, phase)
        step = build.CAR_LEN + build.CAR_GAP
        return [(sc + (j - (CARS - 1) / 2) * step, v) for j in range(CARS)]

    def _trains_make(self, world, dt):
        self._ct += dt
        if self._ct < 0.5:
            return
        self._ct = 0.0
        pp = self.players()
        cons = world.constructs
        for k, (li, ti, tr, col) in enumerate(self.trains):
            for j, (s, v) in enumerate(self.car_s(k, world.time)):
                cid = f"mt-{li}-{ti}-{j}"
                R, x, t = tr.frame(s)
                d = min(float(np.linalg.norm(x - p)) for p in pp) if pp else 1e9
                if cid in cons:
                    if d > FAR_C and not world.seated_in(cons[cid]):
                        del cons[cid]
                        self.cars.pop(cid, None)
                    continue
                if d > NEAR_C:
                    continue
                V = build.car(col, lead=(j == CARS - 1), tail=(j == 0))
                org = x + R @ build.car_origin_local()
                c = Construct.from_voxels(world.registry, V, org, R, size=build.CAR_S,
                                          name=f"{self.plan['lines'][li]['name']}の電車 {j + 1} 両目", id_=cid)
                c.flat_mesh = True                       # 面をまとめた平らなメッシュ（A-15）
                world.add_construct(c)
                c.hold_in_frame(R, x, t * v, np.zeros(3))
                self.cars[cid] = (k, j)

    def _trains_move(self, world):
        cons = world.constructs
        out = set()
        for cid, (k, j) in list(self.cars.items()):
            c = cons.get(cid)
            if c is None:
                del self.cars[cid]
                continue
            tr = self.trains[k][2]
            s, v = self.car_s(k, world.time)[j]
            R, x, t = tr.frame(s)
            c.move_with_frame(R, x, t * v, np.zeros(3))
            out.add(cid)
        return out

    def train_positions(self):
        """画面・テスト用：[(線の名前, 世界の点, 速さ)]（編成の真ん中）"""
        out = []
        t = self.world.time if self.world is not None else 0.0
        for k, (li, ti, tr, _) in enumerate(self.trains):
            s, v = self.car_s(k, t)[CARS // 2]
            out.append((self.plan["lines"][li]["name"], tr.frame(s)[1], v))
        return out

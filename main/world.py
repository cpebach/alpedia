"""ワールド：読み込み・保存・シミュレーション・編集操作。

saves/<ワールド名>/
    world.json       ワールド設定（惑星・自動削除・クリエイティブ等）
    constructs.json  構築物のデータ（ボクセル・機器・結合・資源）
    spawn.json       構築物の初期位置・初期速度・向き
    mods.json        このワールドに適用するMOD
    player.json      プレイヤーの状態
    scripts/         コンピューター用 Python スクリプト
    exports/         OBJ 出力先
    imports/         OBJ 読み込み元
"""
import json
import time
from collections import deque

import numpy as np
from scipy.spatial import cKDTree

from . import paths, log, devices, objio, savefmt
from .construct import Construct, DIRS, REST_UNIT, link_key
from .planet import make_planet
from .player import Player
from .registry import Registry
from .scripting import ScriptRunner

L = log.get("world")

DEFAULT_SETTINGS = {
    "creative": True,
    "voxel_size": 0.5,
    "gravity_scale": 1.0,
    "cleanup": {"enabled": True, "distance": 1500.0, "seconds": 60.0, "max_vertices": 400,
                "check_interval": 5.0},
    "player": "default",
    "planets": [{"def": "terra", "position": [0, 0, 0]}],
}


# 新しいワールドの天体：選んだ惑星＋月（ルナ）＋小惑星3つ。月・小惑星・洞窟のまわりでは星鉱石が採れる
MOON = {"def": "luna", "position": [0, 0, 4000], "craters": []}
ASTEROIDS = [
    {"def": "asteroid", "position": [5200, 1500, -3000], "craters": [], "override": {"name": "小惑星A", "seed": 21}},
    {"def": "asteroid", "position": [-5800, -800, 2600], "craters": [],
     "override": {"name": "小惑星B", "seed": 22, "radius": 20}},
    {"def": "asteroid", "position": [2500, 3500, 6500], "craters": [],
     "override": {"name": "小惑星C", "seed": 23, "radius": 36}},
]


def default_bodies(main="terra", moon=True, asteroids=True):
    out = [{"def": main, "position": [0, 0, 0], "craters": []}]
    if moon:
        out.append(json.loads(json.dumps(MOON)))
    if asteroids:
        out += json.loads(json.dumps(ASTEROIDS))
    return out


def _read_json(path, default):
    if not path.exists():
        return default
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception as e:  # noqa: BLE001
        L.error("%s の読み込みに失敗: %s", path.name, e)
        return default


def _write_json(path, data):
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(data, ensure_ascii=False, indent=1), encoding="utf-8")
    tmp.replace(path)


class World:
    # 拡張（hooks.Addon）を作る関数のリスト。addons を渡さずに作ったワールドはこれを使う
    # （ゲームでは main/addons/__init__.py の install() が街パックなどを登録する）
    default_addons = []

    def __init__(self, name, cfg, addons=None):
        self.name = name
        self.cfg = cfg
        self.dir = paths.world_dir(name)
        self.time = 0.0
        self.constructs = {}
        self.planets = []
        self.msgs = deque(maxlen=8)
        self.input_keys = set()
        self.runner = ScriptRunner(self)
        from .radio import RadioBus
        self.radio = RadioBus(self)
        self.addons = list(addons) if addons is not None else [make() for make in self.default_addons]
        self._timers = {"rebond": 0.0, "cleanup": 0.0}
        phys = cfg.get("physics", {})
        self.substeps = int(phys.get("substeps", 3))
        self.iters = int(phys.get("iterations", 2))

    # ------------------------------------------------------------------
    # 読み込み・保存
    # ------------------------------------------------------------------
    def load(self):
        if not self.dir.exists():
            raise FileNotFoundError(f"ワールド '{self.name}' がありません: {self.dir}")
        for sub in ("scripts", "exports", "imports"):
            (self.dir / sub).mkdir(exist_ok=True)
        s = _read_json(self.dir / "world.json", {})
        extra = [k for a in self.addons for k in a.settings_keys]
        for msg in savefmt.check_world(s, extra):
            L.warning("保存形式: %s", msg)
        self.settings = {**DEFAULT_SETTINGS, **s}
        self.settings["cleanup"] = {**DEFAULT_SETTINGS["cleanup"], **s.get("cleanup", {})}
        self.time = float(self.settings.get("time", 0.0))
        mods = _read_json(self.dir / "mods.json", {"enabled": []}).get("enabled", [])
        self.registry = Registry().load(mods)
        for err in self.registry.errors:
            self.message("⚠ " + err)
        self.planets = []
        for pd in self.settings["planets"]:
            base = self.registry.get("planets", pd["def"])
            if base is None:
                L.error("惑星定義 '%s' がありません（スキップ）", pd["def"])
                continue
            p = make_planet(base, pd.get("position", [0, 0, 0]), pd.get("override"))
            p.craters = [list(c) for c in pd.get("craters", [])]
            p.defn_override = pd.get("override")
            self.planets.append(p)
        cons = _read_json(self.dir / "constructs.json", [])
        spawn = _read_json(self.dir / "spawn.json", {})
        for msg in [m for cd in cons for m in savefmt.check_construct(cd)] + savefmt.check_spawn(spawn):
            L.warning("保存形式: %s", msg)
        for cd in cons:
            try:
                c = Construct.from_dict(self.registry, cd, spawn.get(cd["id"], {}))
                self.constructs[c.id] = c
            except Exception as e:  # noqa: BLE001
                L.exception("構築物 %s の読み込みに失敗: %s", cd.get("id"), e)
        pdef = self.registry.get("players", self.settings.get("player", "default")) or \
            self.registry.get("players", "default")
        self.player = Player(pdef, self._default_spawn())
        pj = _read_json(self.dir / "player.json", None)
        if pj:
            for msg in savefmt.check_player(pj):
                L.warning("保存形式: %s", msg)
            self.player.load_dict(pj)
        self._give_starter_kit()
        for a in self.addons:
            a.on_load(self)
        L.info("ワールド '%s' を読み込み: 構築物 %d, 惑星 %d, MOD %s", self.name, len(self.constructs),
               len(self.planets), mods)
        self.message(f"ワールド「{self.name}」を読み込みました")
        return self

    def _default_spawn(self):
        if not self.planets:
            return np.zeros(3)
        p = self.planets[0]
        up = np.array([0.0, 1.0, 0.0])
        r = float(p.surface_radius(up[None])[0])
        return p.center + up * (r + 2.0)

    def save(self):
        self.settings["time"] = self.time
        self.settings["format"] = savefmt.FORMAT
        for a in self.addons:
            a.on_save(self)
        self.settings["planets"] = [
            {**pl.to_dict(), **({"override": pl.defn_override} if getattr(pl, "defn_override", None) else {})}
            for pl in self.planets]
        _write_json(self.dir / "world.json", self.settings)
        _write_json(self.dir / "constructs.json", [c.to_dict() for c in self.constructs.values()])
        _write_json(self.dir / "spawn.json", {c.id: c.spawn_dict() for c in self.constructs.values()})
        _write_json(self.dir / "player.json", self.player.to_dict())
        if not (self.dir / "mods.json").exists():
            _write_json(self.dir / "mods.json", {"enabled": []})
        L.info("保存しました: %s", self.dir)
        self.message("保存しました")

    # ------------------------------------------------------------------
    # 共通
    # ------------------------------------------------------------------
    def message(self, text):
        self.msgs.append((time.time(), text))

    def gravity(self, p):
        g = np.zeros_like(np.asarray(p, dtype=np.float64))
        for pl in self.planets:
            g += pl.gravity(p)
        return g * float(self.settings.get("gravity_scale", 1.0))

    def nearest_planet(self, p):
        best, bd = None, 1e300
        p = np.asarray(p, dtype=np.float64)
        for pl in self.planets:
            d = (np.linalg.norm(p - pl.center) - pl.radius) / max(pl.gm, 1e-9) ** 0.5
            if d < bd:
                best, bd = pl, d
        return best

    def orbit_info(self, pos, vel):
        """最寄り惑星に対する軌道要素（二体問題の近似）。"""
        pl = self.nearest_planet(pos)
        if pl is None:
            return None
        mu = pl.gm * float(self.settings.get("gravity_scale", 1.0))
        r = np.asarray(pos, dtype=np.float64) - pl.center
        v = np.asarray(vel, dtype=np.float64)
        rn = float(np.linalg.norm(r))
        up = r / rn
        vr = float(np.dot(v, up))
        vh = float(np.linalg.norm(v - up * vr))
        out = {"planet": pl.name, "radius": pl.radius, "altitude": rn - pl.radius, "vertical_speed": vr,
               "horizontal_speed": vh, "orbital_speed": float(np.sqrt(mu / rn)) if mu > 0 else 0.0,
               "apoapsis": float("inf"), "periapsis": rn - pl.radius, "eccentricity": 1.0,
               "period": float("inf"), "time_to_apoapsis": float("inf")}
        if mu <= 0:
            return out
        energy = float(np.dot(v, v)) / 2 - mu / rn
        h = float(np.linalg.norm(np.cross(r, v)))
        e = float(np.sqrt(max(0.0, 1 + 2 * energy * h * h / (mu * mu))))
        out["eccentricity"] = e
        if energy < 0:
            a = -mu / (2 * energy)
            out["apoapsis"] = a * (1 + e) - pl.radius
            out["periapsis"] = a * (1 - e) - pl.radius
            out["period"] = float(2 * np.pi * np.sqrt(a ** 3 / mu))
            if e > 1e-6:
                # 真近点角から遠点までの時間
                cosE = np.clip((1 - rn / a) / e, -1, 1)
                E = float(np.arccos(cosE))
                if vr < 0:
                    E = 2 * np.pi - E
                M = E - e * np.sin(E)
                n = np.sqrt(mu / a ** 3)
                out["time_to_apoapsis"] = float(((np.pi - M) % (2 * np.pi)) / n)
        else:
            out["periapsis"] = (h * h / mu) / (1 + e) - pl.radius
        return out

    # --- アクチュエーター用 ------------------------------------------------
    def actuator_detach(self, c, i):
        from . import actuator
        if not actuator.cmd_detach(c, i):
            return False
        for part in c.split():
            self.add_construct(part)
            part.wake()
        c.wake()
        return True

    def merge_constructs(self, c, other, cell_map, xform=None):
        """other を c に取り込む（座っているプレイヤーの参照も付け替える）。"""
        c.absorb(other, cell_map, xform)
        if self.player.seated and self.player.seated[0] == other.id:
            self.player.seated = (c.id, cell_map[self.player.seated[1]])
        self.constructs.pop(other.id, None)
        L.info("構築物を結合: %s ← %s", c.name, other.name)

    def seated_in(self, c):
        return self.player.seated is not None and self.player.seated[0] == c.id

    def add_construct(self, c):
        self.constructs[c.id] = c
        return c

    # ------------------------------------------------------------------
    # シミュレーション
    # ------------------------------------------------------------------
    def step(self, dt, move=np.zeros(3), jump=False, thrust_dir=np.zeros(3)):
        self.time += dt
        ambient = 0.0
        for c in list(self.constructs.values()):
            devices.update(c, dt, frozenset(self.input_keys), self.seated_in(c), ambient)
            self.runner.tick(c, dt)
        self.player.update(self, dt, move, jump, thrust_dir)
        h = dt / self.substeps
        # 拡張（軌道上の街など）が位置を決める構築物は、物理計算しない
        anchored_ids = set()
        for a in self.addons:
            anchored_ids |= a.before_physics(self, dt) or set()
        for c in self.constructs.values():
            if c.id in anchored_ids:
                continue
            if c.asleep and np.any(c.force):
                c.wake()
            c._touch_moving = False
            # 軽量化：剛体化・レール（遠方）の切り替え
            c.update_mode(dt, self)
        # レールに乗っている構築物は重心の軌道だけを進める
        for c in self.constructs.values():
            if c.mode == "rails" and not c.asleep and c.id not in anchored_ids:
                c.rails_step(dt, self.gravity)
        cons = [c for c in self.constructs.values() if c.n_vox and not c.asleep and c.mode != "rails"]
        sleeping = [c for c in self.constructs.values() if c.n_vox and (c.asleep or c.id in anchored_ids)]
        for c in cons:
            self._prep_ext(c, dt)
        broken = set()
        for _ in range(self.substeps):
            self._each(cons, lambda c: c.substep_begin(h, self.iters, self.gravity, self.planets))
            woke = self._voxel_collisions(cons, sleeping)
            woke = [c for c in woke if c.id not in anchored_ids]
            ends = self._each(cons, lambda c: c.substep_end(h))
            for c, b in zip(cons, ends):
                if b:
                    broken.add(c.id)
            if woke:
                for c in woke:
                    c.wake()
                    c._prev = c.pos.copy()
                    self._prep_ext(c, dt)
                cons = cons + woke
                sleeping = [c for c in sleeping if c.asleep or c.id in anchored_ids]
        for c in cons:
            c.update_sleep(dt)
        for a in self.addons:
            a.after_physics(self, cons)
        for c in cons:
            c.ext_events = []
        for cid in broken:
            c = self.constructs.get(cid)
            if c is None:
                continue
            parts = c.split()
            for part in parts:
                self.add_construct(part)
            if parts:
                L.info("構築物 %s が %d 個に分離", c.name, len(parts) + 1)
        for cid in [k for k, c in self.constructs.items() if not c.n_vox]:
            del self.constructs[cid]
        self._timers["rebond"] += dt
        if self._timers["rebond"] > 0.25:
            self._timers["rebond"] = 0.0
            for c in list(self.constructs.values()):
                c.try_rebond()
        self._timers["cleanup"] += dt
        cl = self.settings["cleanup"]
        if self._timers["cleanup"] >= float(cl.get("check_interval", 5.0)):
            self._cleanup(self._timers["cleanup"])
            self._timers["cleanup"] = 0.0

    # ------------------------------------------------------------------
    # 拡張（hooks.Addon）
    # ------------------------------------------------------------------
    @property
    def creative(self):
        """クリエイティブ（資源を使わない）か。False ならサバイバル。"""
        return bool(self.settings.get("creative", True))

    def addon(self, name):
        """名前で拡張を探す（無ければ None）。"""
        return next((a for a in self.addons if a.name == name), None)

    def _prep_ext(self, c, dt):
        """このティックで当たりそうな、拡張の当たり判定の箱を集めておく。"""
        c.ext_events = []
        c.ext_colliders = []
        if not self.addons:
            return
        sp = float(np.abs(c.vel).max()) * dt + c.size * 2
        lo, hi = c.pos.min(0), c.pos.max(0)
        for a in self.addons:
            c.ext_colliders += a.colliders(self, lo, hi, sp)

    def push_sphere(self, center, r):
        """拡張の物体（街の建物など）から球を押し出す：[(押し出し, 法線, owner)]"""
        out = []
        for a in self.addons:
            out += a.push_sphere(self, center, r)
        return out

    def _each(self, cons, fn):
        """構築物ごとの計算を、複数の CPU コアで同時に進める（numba の計算は GIL を離すので並列に動く）。
        大きな構築物はそれ自体をボクセルごとに並列化するので、先にメインスレッドで1つずつ進める。"""
        from . import fastphys
        workers = self._workers()
        if workers <= 1 or len(cons) < 2:
            return [fn(c) for c in cons]
        out = [None] * len(cons)
        small = []
        for k, c in enumerate(cons):
            if c.n_vox >= fastphys.PARALLEL_MIN_VOXELS:
                out[k] = fn(c)
            else:
                small.append(k)
        if len(small) == 1:
            out[small[0]] = fn(cons[small[0]])
            return out
        pool = self._pool()

        def run(k):
            c = cons[k]
            c._in_pool = True
            try:
                return fn(c)
            finally:
                c._in_pool = False
        for k, r in zip(small, pool.map(run, small)):
            out[k] = r
        return out

    def _workers(self):
        import os
        n = int(self.cfg.get("physics", {}).get("threads", 0) or 0)
        return n if n > 0 else max(1, (os.cpu_count() or 2) - 1)

    def _pool(self):
        if getattr(self, "_pool_obj", None) is None:
            from concurrent.futures import ThreadPoolExecutor
            self._pool_obj = ThreadPoolExecutor(max_workers=self._workers(), thread_name_prefix="phys")
        return self._pool_obj

    def _voxel_collisions(self, active, sleeping=()):
        """構築物どうし（と、同じ構築物の結合していないボクセルどうし）の押し出し。
        形は球（ボクセル中心）＋結合部（線の中点・三角形の重心の球）で近似する。
        押し出された眠っている構築物のリストを返す。"""
        if not active:
            return []
        cons = list(active) + list(sleeping)
        n_active = len(active)
        pts, owner, row, is_ctr, inv, sizes = [], [], [], [], [], []
        Ws = []
        for k, c in enumerate(cons):
            n = c.n_vox
            W = c.proxW
            cache = getattr(c, "_prox_cache", None)
            if k >= n_active and cache is not None and cache[0] == (c.sleep_epoch, c.version):
                X = cache[1]          # 静止中の構築物は当たり判定の点を作り直さない
            else:
                X = W @ c.voxel_centers()
                if k >= n_active:
                    c._prox_cache = ((c.sleep_epoch, c.version), X)
            P = X.shape[0]
            Ws.append(W.tocsr())
            pts.append(X)
            owner.append(np.full(P, k))
            row.append(np.arange(P))
            flag = np.zeros(P, bool)
            flag[:n] = True
            is_ctr.append(flag)
            vinv = c.inv_mass[::8]
            inv.append(W @ vinv)
            sizes.append(np.full(P, c.size))
        offs = np.cumsum([0] + [len(x) for x in pts])
        pts_list = pts
        pts = np.concatenate(pts)
        if len(pts) < 2:
            return []
        owner = np.concatenate(owner)
        row = np.concatenate(row)
        is_ctr = np.concatenate(is_ctr)
        inv = np.concatenate(inv)
        sizes = np.concatenate(sizes)
        # 近い点の組を探す。全点をまとめて探すと同じ構築物の中の組が大量に出て遅いので、
        # 同じ構築物の中はボクセル中心どうしだけ、別の構築物どうしは外枠（AABB）が重なる組だけ調べる。
        pa_list, pb_list = [], []
        trees = {}

        def tree_of(k):
            if k not in trees:
                c = cons[k]
                tc = getattr(c, "_prox_tree", None)
                key = (c.sleep_epoch, c.version)
                if k >= n_active and tc is not None and tc[0] == key:
                    trees[k] = tc[1]          # 静止中の構築物は木も使い回す
                else:
                    trees[k] = cKDTree(pts_list[k])
                    if k >= n_active:
                        c._prox_tree = (key, trees[k])
            return trees[k]
        for k in range(n_active):
            n = cons[k].n_vox
            if n > 1:
                t = cKDTree(pts_list[k][:n])
                pr = t.query_pairs(0.92 * cons[k].size, output_type="ndarray")
                if len(pr):
                    pa_list.append(pr[:, 0] + offs[k])
                    pb_list.append(pr[:, 1] + offs[k])
        lo = np.array([x.min(0) for x in pts_list])
        hi = np.array([x.max(0) for x in pts_list])
        csz = np.array([c.size for c in cons])
        rr = 0.46 * (csz[:, None] + csz[None, :])
        ov = np.all((lo[:, None, :] <= hi[None, :, :] + rr[..., None]) &
                    (lo[None, :, :] <= hi[:, None, :] + rr[..., None]), axis=2)
        ov = np.triu(ov, 1)
        ov[n_active:, n_active:] = False          # 静止中どうしは調べない
        for k1, k2 in zip(*np.nonzero(ov)):
            k1, k2 = int(k1), int(k2)
            r = rr[k1, k2]
            res = tree_of(k1).query_ball_tree(tree_of(k2), r)
            ia = [i for i, lst in enumerate(res) for _ in lst]
            if not ia:
                continue
            ib = [j for lst in res for j in lst]
            pa_list.append(np.asarray(ia, np.int64) + offs[k1])
            pb_list.append(np.asarray(ib, np.int64) + offs[k2])
        if not pa_list:
            return []
        a = np.concatenate(pa_list)
        b = np.concatenate(pb_list)
        d = pts[b] - pts[a]
        dist = np.linalg.norm(d, axis=1)
        mind = 0.46 * (sizes[a] + sizes[b])
        over = dist < mind
        a, b, d, dist, mind = a[over], b[over], d[over], dist[over], mind[over]
        keep = np.ones(len(a), bool)
        for k in range(len(a)):
            if owner[a[k]] == owner[b[k]]:
                c = cons[owner[a[k]]]
                key = link_key(c.cells[row[a[k]]], c.cells[row[b[k]]])
                if key in c.links or key in c.extras:
                    keep[k] = False
        if not keep.any():
            return []
        a, b, d, dist, mind = a[keep], b[keep], d[keep], dist[keep], mind[keep]
        n = d / np.maximum(dist, 1e-9)[:, None]
        pen = (mind - dist)
        wa, wb = inv[a], inv[b]
        ws = wa + wb
        woke = set()
        # 別の構築物どうしの接触：動いている相手と触れている間は静止扱いにしない。
        # 剛体として動いている構築物が速くぶつかったら、ブロックごとの計算に戻す（壊れ方を残す）
        diff = owner[a] != owner[b]
        if diff.any():
            vels = {}
            for oa, ob in set(zip(owner[a][diff].tolist(), owner[b][diff].tolist())):
                for o in (oa, ob):
                    if o not in vels:
                        vels[o] = cons[o].velocity() if o < n_active else np.zeros(3)
                rel = float(np.linalg.norm(vels[oa] - vels[ob]))
                if rel > 0.05:
                    cons[oa]._touch_moving = True
                    cons[ob]._touch_moving = True
                if rel > 2.0:
                    for o in (oa, ob):
                        if getattr(cons[o], "mode", "soft") == "rigid":
                            cons[o]._want_soft = True

        def move(k_con, prow, vec):
            c = cons[k_con]
            W = Ws[k_con]
            s0, s1 = W.indptr[prow], W.indptr[prow + 1]
            for j in W.indices[s0:s1]:
                c.pos[8 * j:8 * j + 8] += vec

        for k in range(len(a)):
            oa, ob = owner[a[k]], owner[b[k]]
            corr = n[k] * pen[k]
            if oa >= n_active and pen[k] < 0.01 * cons[oa].size:
                move(ob, row[b[k]], corr)  # 眠っている側は動かさない
                continue
            if ob >= n_active and pen[k] < 0.01 * cons[ob].size:
                move(oa, row[a[k]], -corr)
                continue
            move(oa, row[a[k]], -corr * (wa[k] / ws[k]))
            move(ob, row[b[k]], corr * (wb[k] / ws[k]))
            if oa >= n_active:
                woke.add(oa)
            if ob >= n_active:
                woke.add(ob)
        return [cons[k] for k in woke]

    def _cleanup(self, elapsed):
        cl = self.settings["cleanup"]
        if not cl.get("enabled", True):
            return
        pp = self.player.pos
        for cid, c in list(self.constructs.items()):
            if self.seated_in(c):
                c.far_time = 0
                continue
            far = np.linalg.norm(c.center_of_mass() - pp) > float(cl["distance"])
            if far and not c.powered and c.n_vert <= int(cl["max_vertices"]):
                c.far_time += elapsed
                if c.far_time >= float(cl["seconds"]):
                    del self.constructs[cid]
                    L.info("自動削除: %s（頂点 %d, 距離 %.0f m）", c.name, c.n_vert,
                           np.linalg.norm(c.center_of_mass() - pp))
            else:
                c.far_time = 0.0

    # ------------------------------------------------------------------
    # 問い合わせ
    # ------------------------------------------------------------------
    def voxel_contacts(self, center, radius):
        """球（プレイヤーなど）と構築物の形（球・カプセル・斜面）の接触: [(construct, idx, push, normal)]"""
        from . import shape
        out = []
        center = np.asarray(center, dtype=np.float64).copy()
        for c in self.constructs.values():
            if not c.n_vox:
                continue
            for _ in range(2):
                res = shape.closest_on_shape(c, center, radius)
                if res is None:
                    break
                dist, q, vi = res
                if dist >= radius:
                    break
                v = center - q
                L = np.linalg.norm(v)
                n = v / L if L > 1e-9 else (center - c.voxel_centers()[vi]) / max(
                    np.linalg.norm(center - c.voxel_centers()[vi]), 1e-9)
                pen = radius - dist
                out.append((c, vi, n * pen, n))
                center = center + n * pen
        return out

    def raycast(self, origin, direction, max_dist=8.0):
        """視線の先のボクセル／地形。戻り値 dict または None。"""
        o = np.asarray(origin, dtype=np.float64)
        dvec = np.asarray(direction, dtype=np.float64)
        dvec = dvec / np.linalg.norm(dvec)
        best = None
        for c in self.constructs.values():
            if not c.n_vox:
                continue
            ctrs = c.voxel_centers()
            rel = ctrs - o
            tproj = rel @ dvec
            perp = np.linalg.norm(rel - np.outer(tproj, dvec), axis=1)
            cand = np.nonzero((perp < c.size) & (tproj > -c.size) & (tproj < max_dist + c.size))[0]
            if not len(cand):
                continue
            F = c.voxel_frames()[cand]
            lo_ = np.einsum("nij,nj->ni", F, o - ctrs[cand])
            ld = np.einsum("nij,j->ni", F, dvec)
            half = c.size * 0.5
            with np.errstate(divide="ignore", invalid="ignore"):
                inv = 1.0 / np.where(np.abs(ld) < 1e-12, 1e-12, ld)
                t1 = (-half - lo_) * inv
                t2 = (half - lo_) * inv
            tmin = np.minimum(t1, t2)
            tmax = np.maximum(t1, t2)
            tn = tmin.max(axis=1)
            tf = tmax.min(axis=1)
            ok = (tn <= tf) & (tf > 0) & (tn < max_dist)
            for k in np.nonzero(ok)[0]:
                t = max(tn[k], 0.0)
                if best is None or t < best["t"]:
                    ax = int(np.argmax(tmin[k]))
                    sign = -np.sign(ld[k, ax])
                    face = ax * 2 + (0 if sign > 0 else 1)
                    best = {"kind": "voxel", "t": t, "construct": c, "index": int(cand[k]), "face": face,
                            "point": o + dvec * t}
        for pl in self.planets:
            if np.linalg.norm(o - pl.center) > pl.bound + max_dist:
                continue
            r = pl.raycast(o, dvec, max_dist)
            if r and (best is None or r[0] < best["t"]):
                best = {"kind": "terrain", "t": r[0], "planet": pl, "point": r[1],
                        "normal": pl.normal(r[1][None])[0]}
            # 水面（海・池）：地面や構築物より手前にあれば水を指す
            if pl.has_water:
                lim = best["t"] if best is not None else max_dist
                w = pl.water_raycast(o, dvec, lim)
                if w and (best is None or w[0] < best["t"]):
                    best = {"kind": "water", "t": w[0], "planet": pl, "point": w[1],
                            "normal": pl.up(w[1])}
        for a in self.addons:
            r = a.raycast(self, o, dvec, best["t"] if best is not None else max_dist)
            if r and (best is None or r["t"] < best["t"]):
                best = r
        return best

    # ------------------------------------------------------------------
    # 編集
    # ------------------------------------------------------------------
    def _pay(self, cost):
        if self.creative:
            return True
        inv = self.player.inventory
        if any(inv.get(k, 0.0) < v for k, v in cost.items()):
            short = {k: v - inv.get(k, 0.0) for k, v in cost.items() if inv.get(k, 0.0) < v}
            self.message("資源が足りません（不足: " + self.registry.res_text(short, ", ") + "）")
            return False
        for k, v in cost.items():
            inv[k] -= v
        return True

    def _refund(self, mat, dev):
        if self.creative:
            return
        cost = dict(self.registry.material(mat).get("cost", {}))
        if dev:
            dd = self.registry.device(dev["def"])
            if dd:
                cost = dict(dd.get("cost", {}))
        for k, v in cost.items():
            self.player.inventory[k] = self.player.inventory.get(k, 0.0) + v

    def item_cost(self, item):
        kind, id_ = item
        if kind == "material":
            return dict(self.registry.material(id_).get("cost", {}))
        dd = self.registry.device(id_)
        return dict(dd.get("cost", {})) if dd else {}

    def place(self, hit, item, dir_index, look_dir):
        kind, id_ = item
        if kind == "material":
            mat, dev = id_, None
        else:
            dd = self.registry.device(id_)
            mat = dd["material"]
            dev = {"def": id_, "dir": dir_index}
        if hit and hit["kind"] == "voxel":
            c = hit["construct"]
            i = hit["index"]
            cell = np.array(c.cells[i]) + DIRS[hit["face"]]
            if tuple(cell) in c.index:
                self.message("そこには既にボクセルがあります")
                return False
            F = c.voxel_frames()[i]
            axis, sign = hit["face"] // 2, (1 if hit["face"] % 2 == 0 else -1)
            ctr = c.pos[8 * i:8 * i + 8].mean(0) + F[axis] * sign * c.size
            verts = ctr + (REST_UNIT * c.size) @ F
            if self._blocked(ctr, c.size):
                return False
            if not self._pay(self.item_cost(item)):
                return False
            c.add_voxel(tuple(cell), mat, dev, verts)
            self._survival_init_device(c, c.index.get(tuple(cell)))
            return True
        # 何もない所・地面 → 新しい構築物
        size = float(self.settings.get("voxel_size", 0.5))
        if hit and (hit["kind"] in ("terrain", "water") or hit.get("placeable")):
            up = hit["normal"]
            ctr = hit["point"] + up * size * 0.55
        else:
            pl = self.nearest_planet(self.player.pos)
            up = pl.up(self.player.pos) if pl else np.array([0, 1.0, 0])
            ctr = self.player.eye() + np.asarray(look_dir) * 3.0
        if self._blocked(ctr, size):
            return False
        fwd = np.asarray(look_dir, dtype=np.float64)
        fwd = fwd - up * np.dot(fwd, up)
        if np.linalg.norm(fwd) < 1e-6:
            fwd = np.cross(up, [1, 0, 0])
        fwd /= np.linalg.norm(fwd)
        right = np.cross(up, fwd)
        R = np.stack([right, up, fwd], axis=1)  # ローカルx,y,z → ワールド
        if not self._pay(self.item_cost(item)):
            return False
        origin = ctr - R @ (np.array([0.5, 0.5, 0.5]) * size)
        c = Construct.from_voxels(self.registry, [((0, 0, 0), mat, dev)], origin, R, size=size,
                                  name="構築物")
        self.add_construct(c)
        self._survival_init_device(c, 0)
        if hit and hit.get("placeable"):
            for a in self.addons:
                a.on_placed(self, hit, c)
        return True

    def _survival_init_device(self, c, i):
        """サバイバルでは置いたばかりのバッテリーは空（置き直しで電気が増えないように）。"""
        if self.creative or i is None or not c.devs[i]:
            return
        dd = self.registry.device(c.devs[i]["def"])
        if dd and dd["type"] == "battery":
            c.devs[i]["state"]["charge"] = 0.0

    def _blocked(self, ctr, size):
        p = self.player
        for off in (0.0, p.height - 2 * p.radius):
            if np.linalg.norm(p.pos + p.up * off - ctr) < size * 0.5 + p.radius * 0.9:
                self.message("自分の位置には置けません")
                return True
        return False

    def remove(self, hit):
        if not hit or hit["kind"] != "voxel":
            return False
        c = hit["construct"]
        i = hit["index"]
        mat, dev = c.mats[i], c.devs[i]
        if self.player.seated and self.player.seated[0] == c.id and self.player.seated[1] == c.cells[i]:
            self.player.seated = None
        c.remove_voxels([i])
        self._refund(mat, dev)
        if not c.n_vox:
            del self.constructs[c.id]
        else:
            for part in c.split():
                self.add_construct(part)
        return True

    DIG_AMOUNT = 20.0
    WATER_AMOUNT = 20.0

    def dig(self, hit, radius=0.8):
        if hit and hit["kind"] == "water":
            return self.collect_water(hit)
        if not hit or hit["kind"] != "terrain":
            return False
        pl = hit["planet"]
        res = pl.resource_at(hit["point"])      # 掘る前の場所で判定（山の硫黄・洞窟のまわりの星鉱石など）
        pl.dig(hit["point"], radius)
        if not self.creative:
            self._gain(res, self.DIG_AMOUNT)
        return True

    def collect_water(self, hit):
        """水面をクリックして水をくむ。海や池の水はいくらくんでも減らない。"""
        if self.creative:
            return False
        self._gain("water", self.WATER_AMOUNT)
        return True

    def _gain(self, res, amount):
        inv = self.player.inventory
        inv[res] = inv.get(res, 0.0) + amount
        self.message(f"{self.registry.res_name(res)} +{amount:.0f}（所持 {inv[res]:.0f}）")

    # サバイバルの最初の所持品（詰まないように：精錬機・手回し発電機・太陽光パネル・タンクが作れて、少し飛べる）
    STARTER_KIT = {"iron": 250, "silica": 40, "parts": 12, "fuel": 60}

    def _give_starter_kit(self):
        p = self.player
        if self.creative or getattr(p, "starter_given", False):
            return
        for k, v in self.STARTER_KIT.items():
            p.inventory[k] = max(p.inventory.get(k, 0.0), float(v))
        p.starter_given = True
        self.message("サバイバル開始: 最初の資材を受け取りました（I でインベントリ）")

    def interact(self, hit):
        p = self.player
        if p.seated:
            p.seated = None
            self.message("操縦席から降りました")
            return
        if not hit or hit["kind"] != "voxel":
            return
        c = hit["construct"]
        i = hit["index"]
        d = c.devs[i]
        if d:
            dd = self.registry.device(d["def"])
            if dd and dd["type"] == "seat":
                p.seated = (c.id, c.cells[i])
                self.message(f"{c.name} の操縦席に座りました（E で降りる）")
                return
            if dd and dd["type"] == "tank" and not self.creative:
                self.message(self._fill_tank(d, dd))
                return
            if dd and dd["type"] == "tank" and self.creative:
                # クリエイティブ: E でタンクの中身を 燃料満タン → 空気満タン → 空 と切り替え
                cap = float(dd["params"].get("capacity", 0))
                cont = d["state"].setdefault("contents", {})
                if cont.get("fuel", 0) >= cap * 0.99:
                    cont.clear()
                    cont["air"] = cap
                    self.message(f"{d['name']}: 空気を満タンにしました（E で空に）")
                elif cont.get("air", 0) >= cap * 0.99:
                    cont.clear()
                    self.message(f"{d['name']}: 空にしました（E で燃料満タン）")
                else:
                    cont.clear()
                    cont["fuel"] = cap
                    self.message(f"{d['name']}: 燃料を満タンにしました（E で空気に）")
                return
            if dd and dd["type"] == "processor" and not self.creative:
                # 所持品の原料を入れ、製品を受け取る
                moved = []
                for r in dd["params"].get("recipes", []):
                    for k in r.get("in", {}):
                        amt = p.inventory.pop(k, 0.0)
                        if amt:
                            c.storage[k] = c.storage.get(k, 0.0) + amt
                            moved.append(f"{self.registry.res_name(k)} {amt:.0f} 投入")
                    for k in r.get("out", {}):
                        amt = c.storage.pop(k, 0.0)
                        if amt:
                            p.inventory[k] = p.inventory.get(k, 0.0) + amt
                            moved.append(f"{self.registry.res_name(k)} {amt:.0f} 回収")
                if moved:
                    st = c.status or {}
                    hint = "" if (st.get("stored", 0) > 1 or st.get("production", 0) > 0) else \
                        "（電気がありません：手回し発電機・太陽光パネル・発電機をつなげて）"
                    self.message(f"{d['name']}: " + ", ".join(moved) + hint)
                    return
            if dd and dd["type"] == "actuator":
                from . import actuator
                self.message(actuator.execute_preset(self, c, i))
                return
            msg = devices.interact(c, i, p)
            if msg:
                self.message(msg)
            return
        for a in self.addons:
            msg = a.interact(self, c, i)
            if msg:
                self.message(msg)
                return
        m = self.registry.material(c.mats[i])
        self.message(f"{c.name}: {m['name']} / ボクセル {c.n_vox} / 質量 {c.total_mass()/1000:.1f} t")

    def _fill_tank(self, d, dd):
        """サバイバル: 手持ちの燃料をタンクに入れる（入りきらない分は手元に残る）。"""
        p = self.player
        cap = float(dd["params"].get("capacity", 0))
        cont = d["state"].setdefault("contents", {})
        have = p.inventory.get("fuel", 0.0)
        room = max(0.0, cap - sum(cont.values()))
        if have <= 0:
            now = ", ".join(f"{self.registry.res_name(k)} {v:.0f}" for k, v in cont.items() if v) or "空"
            return f"{d['name']}: {now} / {cap:.0f}（燃料を持っていません。硫黄鉱石や水を精錬機で燃料に）"
        if room <= 0:
            return f"{d['name']}: 満タンです"
        mv = min(have, room)
        cont["fuel"] = cont.get("fuel", 0.0) + mv
        p.inventory["fuel"] = have - mv
        return f"{d['name']}: 燃料 {mv:.0f} を入れました（タンク {sum(cont.values()):.0f}/{cap:.0f}、手持ち {have - mv:.0f}）"

    def key_pressed(self, key):
        """操縦席で押されたキー：キー割り当てのあるアクチュエータを切り替える。"""
        if not self.player.seated:
            return
        c = self.constructs.get(self.player.seated[0])
        if not c:
            return
        for i, d in enumerate(c.devs):
            if not d:
                continue
            dd = self.registry.device(d["def"])
            if dd and dd["type"] == "actuator" and dd["params"].get("key") == key:
                from . import actuator
                self.message(actuator.execute_preset(self, c, i))

    # ------------------------------------------------------------------
    # OBJ
    # ------------------------------------------------------------------
    def export_obj(self, c):
        name = "".join(ch if ch.isalnum() or ch in "-_" else "_" for ch in c.name) + "_" + c.id[:4]
        path = objio.export_construct(c, self.dir / "exports", name, scripts_dir=self.dir / "scripts")
        self.message(f"OBJ 出力: exports/{path.name}")
        return path

    def voxels_cost(self, voxels):
        """ボクセル一覧を置くのに必要な資源（機器は機器の費用、それ以外は素材の費用）。"""
        total = {}
        for _, mat, dev in voxels:
            cost = self.item_cost(("device", dev["def"]) if dev else ("material", mat))
            for k, v in cost.items():
                total[k] = total.get(k, 0.0) + float(v)
        return total

    def import_obj(self, path, position=None, look_dir=(0, 0, 1), **opts):
        data = objio.import_construct(self.registry, path, **opts)
        return self.place_import(data, position, look_dir)

    def place_import(self, data, position=None, look_dir=(0, 0, 1)):
        """読み込んだ構築物（objio.import_construct の結果）をプレイヤーの前に置く。サバイバルでは資源を払う。"""
        if not data["voxels"]:
            self.message("ボクセルが0個でした")
            return None
        if not self._pay(self.voxels_cost(data["voxels"])):
            return None
        size = data["size"]
        cells = np.array([v[0] for v in data["voxels"]], dtype=np.float64)
        pl = self.nearest_planet(self.player.pos)
        up = pl.up(self.player.pos) if pl else np.array([0, 1.0, 0])
        fwd = np.asarray(look_dir, dtype=np.float64)
        fwd = fwd - up * np.dot(fwd, up)
        fwd = fwd / np.linalg.norm(fwd) if np.linalg.norm(fwd) > 1e-6 else np.cross(up, [1, 0, 0])
        right = np.cross(up, fwd)
        R = np.stack([right, up, fwd], axis=1)
        extent = (cells.max(0) - cells.min(0) + 1) * size
        if position is None:
            base = self.player.pos + fwd * (extent.max() * 0.7 + 3.0)
            if pl is not None:
                # 地表の上に置く
                dd = float(pl.sdf(base[None])[0])
                base = base - up * dd + up * 0.5
            position = base
        local_min = cells.min(0) * size
        center_off = np.array([(extent[0]) / 2, 0, (extent[2]) / 2]) + local_min
        origin = position - R @ center_off
        c = Construct.from_voxels(self.registry, data["voxels"], origin, R, size=size, links=data["links"],
                                  name=data["name"])
        c.storage = dict(data.get("storage", {}))
        self.add_construct(c)
        # 一緒に入っていたプログラムを scripts へ（同じ名前で中身が違うものがあれば上書きしない）
        sdir = self.dir / "scripts"
        sdir.mkdir(exist_ok=True)
        kept = []
        for f, text in (data.get("scripts") or {}).items():
            if "/" in f or "\\" in f or ".." in f:
                continue
            fp = sdir / f
            if not fp.exists():
                fp.write_text(text, encoding="utf-8")
            elif fp.read_text(encoding="utf-8") != text:
                kept.append(f)
        if kept:
            self.message("同じ名前のプログラムが既にあるので、そちらを使います: " + ", ".join(kept))
        self.message(f"読み込み: {data['name']}（{c.n_vox} ボクセル）")
        return c

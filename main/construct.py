"""構築物（プレイヤーの建築物・乗り物・デブリ）。

各ボクセルは自分専用の8頂点を持つ「ポリゴンのボクセル」。
- ボクセル内部: 形状マッチングで元の形に戻ろうとする（素材の stiffness）
- 隣接ボクセル間: 同じ位置の頂点同士を結合（素材の joint_stiffness）。
  頂点間の距離が break_distance を超えると結合が離れ、構築物がちぎれる。
- 塑性: 変形が yield_strain を超えると plasticity に応じて元の形自体が変わる。
- 再結合: rebond な素材（液体・粘土）は近づいた頂点同士が新たに結合する。
"""
import uuid
from itertools import combinations

import numpy as np

CORNER_BITS = np.array([[c & 1, (c >> 1) & 1, (c >> 2) & 1] for c in range(8)], dtype=np.float64)
REST_UNIT = CORNER_BITS - 0.5
DIRS = np.array([[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]], dtype=np.int64)
DIR_TUPLES = [tuple(int(x) for x in d) for d in DIRS]
DIR_NAMES = ["+X", "-X", "+Y", "-Y", "+Z", "-Z"]


def _face_quad(d):
    axis, positive = d // 2, d % 2 == 0
    u, v = (axis + 1) % 3, (axis + 2) % 3
    quad = []
    for bu, bv in ((0, 0), (1, 0), (1, 1), (0, 1)):
        bits = [0, 0, 0]
        bits[axis] = 1 if positive else 0
        bits[u], bits[v] = bu, bv
        quad.append(bits[0] + 2 * bits[1] + 4 * bits[2])
    return quad if positive else quad[::-1]


FACE_QUADS = np.array([_face_quad(d) for d in range(6)], dtype=np.int64)
# リンク a→b（b = a + DIRS[d]）で結合する頂点ペア
JOINT_PAIRS = []
for _d in range(6):
    _axis = _d // 2
    JOINT_PAIRS.append([(c, c ^ (1 << _axis)) for c in FACE_QUADS[_d]])
JOINT_PAIRS = np.array(JOINT_PAIRS, dtype=np.int64)  # (6,4,2)
EDGE_PAIRS = np.array(list(combinations(range(8), 2)))


def dir_index(delta):
    try:
        return DIR_TUPLES.index(tuple(int(x) for x in delta))
    except ValueError:
        return -1


def _clean_dev(d):
    if not d:
        return None
    return {"def": d["def"], "dir": d["dir"], "name": d["name"],
            "state": {k: v for k, v in d["state"].items() if not k.startswith("_")}}


def link_key(a, b):
    a, b = tuple(a), tuple(b)
    return (a, b) if a <= b else (b, a)


CONTACT_SOFT = 1.0

from . import fastphys  # noqa: E402
from .planet import fcross  # noqa: E402
FAST = fastphys.AVAILABLE    # numba があれば重い部分を機械語で実行する


class Construct:
    def __init__(self, registry, size=1.0, id_=None, name="構築物"):
        self.reg = registry
        self.id = id_ or uuid.uuid4().hex[:10]
        self.name = name
        self.size = float(size)
        self.cells = []
        self.mats = []
        self.devs = []
        self.links = set()
        self.extras = {}       # key(link_key) -> [(ca, cb, rest), ...]
        self.pos = np.zeros((0, 3))
        self.vel = np.zeros((0, 3))
        self.rest = np.zeros((0, 8, 3))
        self.act_scale = np.zeros((0, 3))
        self.storage = {}      # 構築物共有の固形資源 {資源ID: kg}
        self.cavity_air = {}   # 空洞代表セル -> 空気 kg
        self.far_time = 0.0
        self.version = 0
        self.powered = False
        self.status = {}
        self.index = {}
        # 軽量化：mode = "soft"（ブロックごと）/ "rigid"（形を保ったまま1つの剛体）/ "rails"（遠方：重心だけ軌道計算）
        self.mode = "soft"
        self._calm_t = 0.0
        self._want_soft = False
        self.rb = None
        self.rb_epoch = 0
        self.sleep_epoch = 0
        self._in_pool = False        # 並列実行中（スレッドから呼ばれている）か
        self.chunk_size = 0          # >0 のときチャンク剛体化（大型構築物）
        self.chunk_of = np.zeros(0, np.int64)

    # ------------------------------------------------------------------
    # 構築
    # ------------------------------------------------------------------
    @classmethod
    def from_voxels(cls, registry, voxels, origin, rotation=None, velocity=(0, 0, 0),
                    angular_velocity=(0, 0, 0), size=1.0, links=None, **kw):
        """voxels: [(cell(tuple), material_id, device_dict|None), ...]"""
        c = cls(registry, size=size, **kw)
        R = np.eye(3) if rotation is None else np.asarray(rotation, dtype=np.float64)
        origin = np.asarray(origin, dtype=np.float64)
        for cell, mat, dev in voxels:
            c.cells.append(tuple(int(x) for x in cell))
            c.mats.append(mat)
            c.devs.append(c._init_device(dev) if dev else None)
        n = len(c.cells)
        local = (np.asarray(c.cells, dtype=np.float64)[:, None, :] + CORNER_BITS[None]) * size
        c.pos = (local.reshape(-1, 3) @ R.T) + origin
        c.vel = np.tile(np.asarray(velocity, dtype=np.float64), (8 * n, 1))
        c.rest = np.tile(REST_UNIT * size, (n, 1, 1))
        c.act_scale = np.ones((n, 3))
        if links is None:
            cs = set(c.cells)
            for cell in c.cells:
                for d in (0, 2, 4):
                    nb = (cell[0] + DIR_TUPLES[d][0], cell[1] + DIR_TUPLES[d][1], cell[2] + DIR_TUPLES[d][2])
                    if nb in cs:
                        c.links.add(link_key(cell, nb))
        else:
            c.links = set(links)
        c.rebuild()
        w = np.asarray(angular_velocity, dtype=np.float64)
        if np.any(w):
            com = c.center_of_mass()
            c.vel += np.cross(w, c.pos - com)
        return c

    def _init_device(self, dev):
        dd = self.reg.device(dev["def"])
        out = {"def": dev["def"], "dir": int(dev.get("dir", 2)), "name": dev.get("name") or (dd["name"] if dd else dev["def"]),
               "state": dict(dev.get("state", {}))}
        if dd:
            p = dd["params"]
            st = out["state"]
            t = dd["type"]
            if t == "tank":
                st.setdefault("contents", dict(p.get("contents", {})))
            elif t == "battery":
                st.setdefault("charge", float(p.get("charge", 0)))
            elif t == "valve":
                st.setdefault("open", bool(p.get("open", True)))
            elif t in ("thruster",):
                st.setdefault("throttle", 0.0)
            elif t == "actuator":
                from .actuator import init_state
                init_state(st, p)
            elif t == "computer":
                st.setdefault("script", p.get("script", ""))
                st.setdefault("running", False)
                st.setdefault("memory", {})
            elif t == "processor":
                st.setdefault("progress", 0.0)
                st.setdefault("enabled", True)
            elif t == "air_vent":
                st.setdefault("mode", p.get("mode", "fill"))
                st.setdefault("enabled", True)
            elif t in ("generator", "antenna"):
                st.setdefault("enabled", True)
        return out

    def rebuild(self):
        """トポロジ（ボクセル・結合）が変わったら呼ぶ。"""
        n = len(self.cells)
        self.index = {c: i for i, c in enumerate(self.cells)}
        mats = [self.reg.material(m) for m in self.mats]
        g = lambda k: np.array([m[k] for m in mats], dtype=np.float64)  # noqa: E731
        if n:
            self.v_stiff = np.clip(g("stiffness"), 0, 1)
            self.v_jstiff = np.clip(g("joint_stiffness"), 0, 1)
            self.v_break = g("break_distance") * self.size
            self.v_yield = g("yield_strain")
            self.v_plast = np.clip(g("plasticity"), 0, 1)
            self.v_density = g("density")
            self.v_rebond = np.array([bool(m["rebond"]) for m in mats])
            self.v_rebond_dist = g("rebond_distance") * self.size
            vm = self.v_density * self.size ** 3 / 8.0
            self.inv_mass = np.repeat(1.0 / vm, 8)
            self.mass = np.repeat(vm, 8)
            self.damp = np.repeat(g("damping"), 8)
            self.fric = np.repeat(g("friction"), 8)
        else:
            for k in ("v_stiff", "v_jstiff", "v_break", "v_yield", "v_plast", "v_density",
                      "v_rebond_dist", "inv_mass", "mass", "damp", "fric"):
                setattr(self, k, np.zeros(0))
            self.v_rebond = np.zeros(0, bool)
        # アクチュエーターの出力面の結合は「回転ジョイント」として別に扱う
        rotor_of = {}
        for i, dv in enumerate(self.devs):
            if dv and (self.reg.device(dv["def"]) or {}).get("type") == "actuator":
                d = int(dv.get("dir", 2))
                cell = self.cells[i]
                out = (cell[0] + DIR_TUPLES[d][0], cell[1] + DIR_TUPLES[d][1], cell[2] + DIR_TUPLES[d][2])
                rotor_of[link_key(cell, out)] = (i, d, dv)
        self.rotors = []
        for (i_act, _, _) in rotor_of.values():
            self.v_plast[i_act] = 0.0  # アクチュエーター自身は塑性変形させない
        # 結合（ジョイント）
        ja, jb, jr, jk, jbrk, jkey = [], [], [], [], [], []
        good_links = set()
        for key in self.links:
            a, b = key
            ia, ib = self.index.get(a), self.index.get(b)
            if ia is None or ib is None:
                continue
            d = dir_index(np.subtract(b, a))
            if d < 0:
                continue
            good_links.add(key)
            if key in rotor_of:
                i_act, d_act, dv = rotor_of[key]
                i_out = ib if ia == i_act else ia
                self.v_plast[i_out] = 0.0
                self.rotors.append({"a": i_act, "b": i_out, "d": d_act, "dev": dv, "key": key,
                                    "br": max(min(self.v_break[ia], self.v_break[ib]) * 3, 0.3 * self.size)})
                continue
            k = min(self.v_jstiff[ia], self.v_jstiff[ib])
            br = min(self.v_break[ia], self.v_break[ib])
            for ca, cb in JOINT_PAIRS[d]:
                ja.append(8 * ia + ca); jb.append(8 * ib + cb); jr.append(0.0)
                jk.append(k); jbrk.append(br); jkey.append(key)
        self.links = good_links
        for key, lst in list(self.extras.items()):
            ia, ib = self.index.get(key[0]), self.index.get(key[1])
            if ia is None or ib is None:
                del self.extras[key]
                continue
            k = min(self.v_jstiff[ia], self.v_jstiff[ib])
            br = min(self.v_break[ia], self.v_break[ib])
            for ca, cb, rest in lst:
                ja.append(8 * ia + ca); jb.append(8 * ib + cb); jr.append(rest)
                jk.append(k); jbrk.append(br + rest); jkey.append(("x",) + key)
        self.ja = np.array(ja, dtype=np.int64)
        self.jb = np.array(jb, dtype=np.int64)
        self.jr = np.array(jr, dtype=np.float64)
        self.jk = np.array(jk, dtype=np.float64)
        self.jbrk = np.array(jbrk, dtype=np.float64)
        self.jkey = jkey
        nv = 8 * n
        cnt = np.bincount(self.ja, minlength=nv) + np.bincount(self.jb, minlength=nv)
        self.jcount = np.maximum(cnt, 1).astype(np.float64)
        # 表面（描画・OBJ出力用）
        fv, fd = [], []
        for i, c in enumerate(self.cells):
            for d in range(6):
                nb = (c[0] + DIR_TUPLES[d][0], c[1] + DIR_TUPLES[d][1], c[2] + DIR_TUPLES[d][2])
                if link_key(c, nb) in self.links:
                    continue
                fv.append(i)
                fd.append(d)
        self.face_vox = np.array(fv, dtype=np.int64)
        self.face_dir = np.array(fd, dtype=np.int64)
        self.face_idx = (8 * self.face_vox[:, None] + FACE_QUADS[self.face_dir]) if fv else np.zeros((0, 4), np.int64)
        if getattr(self, "force", None) is None or self.force.shape[0] != nv:
            self.force = np.zeros((nv, 3))
        # 形（球＋結合部を埋める図形）と当たり判定用の球
        from . import shape
        self.topo = shape.topology(self) if n else {"points": np.zeros(0, np.int64),
                                                     "segs": np.zeros((0, 2), np.int64),
                                                     "tris": np.zeros((0, 3), np.int64)}
        self.proxW = shape.collision_proxies(self) if n else None
        if n:
            self._build_contacts()
        self.version += 1
        self.dirty = True  # 配管・電力・気密の再計算が必要
        self._setup_chunks(fresh=True)
        self.wake(soft=True)

    def wake(self, soft=False):
        """soft=True：形が変わる操作（アクチュエーターの動作・ブロックの追加削除）なので剛体扱いも解く。"""
        self.asleep = False
        self.sleep_t = 0.0
        if soft:
            self._want_soft = True
            self._calm_t = 0.0
            if getattr(self, "mode", "soft") not in ("soft", "art"):   # 関節つき剛体はそのまま（形は作り直す）
                self.mode = "soft"
                self.rb = None

    def update_sleep(self, dt, threshold=0.05, delay=1.0):
        """ほぼ静止している構築物は計算を省く（スリープ）。"""
        if self.asleep:
            return
        if not self.n_vox or np.any(self.force):
            self.sleep_t = 0.0
            return
        vmax = float(np.abs(self.vel).max())
        if vmax < threshold:
            self.sleep_t += dt
            if self.sleep_t > delay and not getattr(self, "_touch_moving", False):
                self.asleep = True
                self.vel[:] = 0.0
                self.sleep_epoch += 1
        else:
            self.sleep_t = 0.0

    # ------------------------------------------------------------------
    # 物理
    # ------------------------------------------------------------------
    @property
    def n_vox(self):
        return len(self.cells)

    @property
    def n_vert(self):
        return 8 * len(self.cells)

    def center_of_mass(self):
        if not len(self.mass):
            return np.zeros(3)
        return (self.pos * self.mass[:, None]).sum(0) / self.mass.sum()

    def velocity(self):
        if not len(self.mass):
            return np.zeros(3)
        return (self.vel * self.mass[:, None]).sum(0) / self.mass.sum()

    def total_mass(self):
        return float(self.mass.sum())

    def voxel_centers(self):
        return self.pos.reshape(-1, 8, 3).mean(axis=1)

    def voxel_frames(self):
        """各ボクセルのローカル軸 (n,3,3)。行が x,y,z 軸（単位ベクトル）。"""
        P = self.pos.reshape(-1, 8, 3)
        ex = (P[:, [1, 3, 5, 7]] - P[:, [0, 2, 4, 6]]).mean(1)
        ey = (P[:, [2, 3, 6, 7]] - P[:, [0, 1, 4, 5]]).mean(1)
        ez = (P[:, [4, 5, 6, 7]] - P[:, [0, 1, 2, 3]]).mean(1)
        F = np.stack([ex, ey, ez], axis=1)
        return F / np.maximum(np.linalg.norm(F, axis=2, keepdims=True), 1e-9)

    def substep(self, h, iters, gravity_fn, planets):
        """単独で1サブステップ進める（テスト用）。"""
        self.substep_begin(h, iters, gravity_fn, planets)
        return self.substep_end(h)

    def substep_begin(self, h, iters, gravity_fn, planets):
        if not self.n_vox:
            return
        if self.mode == "rigid":
            return self._rigid_begin(h, gravity_fn, planets)
        if self.mode == "art":
            return self._art_begin(h, gravity_fn, planets)
        acc = gravity_fn(self.pos) + self.force * self.inv_mass[:, None]
        self.vel += acc * h
        for pl in planets:
            if pl.has_water and self._near_planet(pl):
                self._buoyancy(pl, h, gravity_fn)
        # 減衰は構築物全体の並進運動にはかけない（空気抵抗のように軌道が落ちないように）
        vm = (self.vel * self.mass[:, None]).sum(0) / self.mass.sum()
        self.vel = vm + (self.vel - vm) * np.maximum(0.0, 1.0 - self.damp * h)[:, None]
        self._prev = self.pos.copy()
        self.pos += self.vel * h
        if self.rotors:
            iters = iters * 2  # 可動部のある構築物は反復を増やして硬くする
        # 地面との接触を反復の中で解くと、接触による局所的なずれを結合が直せる
        near = [pl for pl in planets if self._near_planet(pl)]
        for _ in range(iters):
            if self.chunk_size:
                self._shape_match_chunks()
            else:
                self._shape_match()
            self._solve_joints()
            if self.rotors:
                self._solve_rotors()
        for pl in near:
            self._collide_planet(pl)
        for bs in getattr(self, "_city_boxes", ()):
            self._collide_boxes(bs, h)

    WATER_DENSITY = 1000.0

    def _cavity_neighbors(self):
        """気密の空洞セルと、その6方向で接するボクセルの番号（浮力を伝える先）。"""
        net = getattr(self, "_net", None)
        if net is None or not net.cavities:
            return None
        if getattr(self, "_cav_cache", (None,))[0] is net:
            return self._cav_cache[1]
        cells, nb = [], []
        for cav in net.cavities:
            for cell in cav["cells"]:
                row = [self.index.get((cell[0] + d[0], cell[1] + d[1], cell[2] + d[2]), -1) for d in DIR_TUPLES]
                if max(row) >= 0:
                    cells.append(cell)
                    nb.append(row)
        data = (np.asarray(nb, dtype=np.int64),) if nb else None
        self._cav_cache = (net, data)
        return data

    def _buoyancy(self, pl, h, gravity_fn):
        """水の浮力と抵抗。ボクセルの水に沈んだ割合で、押しのけた水の重さだけ上向きの力。
        気密の空洞（中が空気）も水を押しのけるので、空洞の大きい船は重い素材でも浮きやすい。"""
        cen = self.voxel_centers()
        wd = pl.water_depth(cen)
        frac = np.clip(wd / self.size + 0.5, 0.0, 1.0)
        if not (frac > 0).any() and wd.max() < -2 * self.size:
            return        # 水に触れていない（空洞も水の外とみなせる）
        cav = self._cavity_neighbors()
        g = gravity_fn(cen)
        vol = self.size ** 3
        force = -g * (self.WATER_DENSITY * vol * frac)[:, None]           # 各ボクセルが受ける力
        if cav is not None:
            nb = cav[0]
            ok = nb >= 0
            cnt = ok.sum(1)
            cc = (cen[np.maximum(nb, 0)] * ok[..., None]).sum(1) / cnt[:, None]
            f2 = np.clip(pl.water_depth(cc) / self.size + 0.5, 0.0, 1.0)
            if (f2 > 0).any():
                gc = gravity_fn(cc)
                fc = -gc * (self.WATER_DENSITY * vol * f2)[:, None] / cnt[:, None]
                for k in range(nb.shape[1]):
                    m = ok[:, k]
                    np.add.at(force, nb[m, k], fc[m])
        if not (np.abs(force).sum() > 0):
            return
        self.vel += np.repeat(force / (self.v_density * vol)[:, None], 8, axis=0) * h
        # 水の抵抗（沈んでいる頂点ほど減速）
        fv = np.repeat(frac, 8)
        self.vel *= (1.0 - np.minimum(1.0, 1.5 * h * fv))[:, None]

    def _near_planet(self, pl):
        rel = self.pos[::8] - pl.center
        return np.einsum("ij,ij->i", rel, rel).min() < (pl.bound + self.size * 2) ** 2

    def _build_contacts(self):
        """各頂点の当たり判定に使う「芯」（点・線・三角形）を選ぶ（ベクトル化）。
        芯の最近点から半径 r が見た目の表面。周りを全部ボクセルに囲まれた頂点（内部）は判定しない。"""
        from . import shape
        n = self.n_vox
        cen = self.voxel_centers()
        P = self.pos.reshape(n, 8, 3)
        cells = np.asarray(self.cells, dtype=np.int64)
        # 頂点のまわり8セルの空き具合 → 外向き
        lo = cells.min(0) - 2
        span = cells.max(0) - lo + 3

        def enc(x):
            x = x - lo
            return (x[..., 0] * span[1] + x[..., 1]) * span[2] + x[..., 2]
        occ = np.sort(enc(cells))
        corner = cells[:, None, :] + CORNER_BITS.astype(np.int64)[None]       # (n,8,3)
        outward = np.zeros((n, 8, 3))
        exposed = np.zeros((n, 8), bool)
        for o in CORNER_BITS.astype(np.int64):
            cc = corner - 1 + o
            e = enc(cc)
            pos = np.searchsorted(occ, e)
            found = (pos < len(occ)) & (occ[np.minimum(pos, len(occ) - 1)] == e)
            empty = ~found
            exposed |= empty
            outward += empty[..., None] * (o.astype(float) - 0.5)
        frames = self.voxel_frames()
        outward = np.einsum("nkj,nji->nki", outward, frames)
        # 候補（ボクセル, 種類, 芯のボクセル3つ）
        S, T = self.topo["segs"], self.topo["tris"]
        cv = [np.arange(n)]
        ct = [np.zeros(n, np.int64)]
        ci = [np.stack([np.arange(n)] * 3, 1)]
        if len(S):
            for col in (0, 1):
                cv.append(S[:, col]); ct.append(np.ones(len(S), np.int64))
                ci.append(np.stack([S[:, 0], S[:, 1], S[:, 1]], 1))
        if len(T):
            for col in (0, 1, 2):
                cv.append(T[:, col]); ct.append(np.full(len(T), 2, np.int64))
                ci.append(T)
        cv, ct, ci = np.concatenate(cv), np.concatenate(ct), np.concatenate(ci)
        V = P[cv]                                                             # (C,8,3)
        A, B, Cc = cen[ci[:, 0]][:, None], cen[ci[:, 1]][:, None], cen[ci[:, 2]][:, None]
        D = np.zeros((len(cv), 8))
        K = np.zeros((len(cv), 8, 3))
        m = ct == 0
        D[m] = np.linalg.norm(V[m] - A[m], axis=-1)
        K[m] = np.broadcast_to(A[m], V[m].shape)
        m = ct == 1
        if m.any():
            D[m], K[m] = shape.dist_segments(V[m], A[m], B[m])
        m = ct == 2
        if m.any():
            D[m], K[m] = shape.dist_triangles(V[m], A[m], B[m], Cc[m])
        dirv = V - K
        score = np.einsum("cki,cki->ck", dirv, outward[cv]) / np.maximum(np.linalg.norm(dirv, axis=-1), 1e-9)
        key = np.round(D, 4) - 1e-6 * score
        ctype = np.full(8 * n, -1, np.int64)
        cidx = np.zeros((8 * n, 3), np.int64)
        for k in range(8):
            order = np.lexsort((key[:, k], cv))
            first = np.ones(len(order), bool)
            first[1:] = cv[order][1:] != cv[order][:-1]
            pick = order[first]
            vox = cv[pick]
            ok = exposed[vox, k]
            ctype[8 * vox[ok] + k] = ct[pick][ok]
            cidx[8 * vox[ok] + k] = ci[pick][ok]
        self.ctype, self.cidx = ctype, cidx

    def contact_cores(self):
        """各頂点に対応する芯の最近点（この点から半径 r が見た目の表面）。"""
        if FAST:
            return fastphys.contact_cores(self.pos, self.ctype, self.cidx)
        from . import shape
        cen = self.voxel_centers()
        A, B, C = cen[self.cidx[:, 0]], cen[self.cidx[:, 1]], cen[self.cidx[:, 2]]
        K = A.copy()
        K[self.ctype < 0] = np.nan  # 内部の頂点は判定しない
        m1 = self.ctype == 1
        if m1.any():
            _, K[m1] = shape.dist_segments(self.pos[m1], A[m1], B[m1])
        m2 = self.ctype == 2
        if m2.any():
            _, K[m2] = shape.dist_triangles(self.pos[m2], A[m2], B[m2], C[m2])
        return K

    def _collide_planet(self, pl, collect=False):
        """地面との当たり判定：頂点ごとに対応する芯の点を、半径 r の球として地面と判定する。
        （見た目の球・カプセル・斜面と同じ形で接地する）"""
        K = self.contact_cores()
        r = self.size * 0.5
        rel = K - pl.center
        with np.errstate(invalid="ignore"):
            near = np.einsum("ij,ij->i", rel, rel) < (pl.bound + r) ** 2
        if not near.any():
            return
        # 地面から十分高い頂点は調べない（地面の高さは傾きの上限で見積もる）
        com = self.pos.mean(0) - pl.center
        cn = float(np.linalg.norm(com))
        if cn > 1e-6:
            up = com / cn
            gc = getattr(self, "_ground_cache", None)
            if gc is not None and gc[0] is pl and float(np.abs(gc[1] - up).max()) * pl.radius < 0.5:
                Rs = gc[2]
            else:
                Rs = float(pl.surface_radius(up[None])[0])
                self._ground_cache = (pl, up, Rs)
            with np.errstate(invalid="ignore"):
                rad = np.sqrt(np.einsum("ij,ij->i", rel, rel))
                along = rel @ up
                lat = np.sqrt(np.maximum(rad * rad - along * along, 0.0))
                near &= (rad - Rs) < r + 0.5 + lat / max(pl.lipschitz, 0.2)
            if not near.any():
                return
        idx = np.nonzero(near)[0]
        d, g = pl.sdf_grad(K[idx])
        d = d - r
        if self.mode == "rigid" or self.chunk_size:
            # 剛体のまま速くぶつかりそうなら、ぶつかる前にブロックごとの計算に戻す（壊れ方を残すため）
            vn = -np.einsum("ij,ij->i", self.vel[idx], g)
            lim = self.IMPACT_SPEED if self.mode == "rigid" else self.CHUNK_IMPACT_SPEED
            coming = (vn > lim) & (d < vn * 0.12 + 0.05)
            if np.any(coming):
                if self.mode == "rigid":
                    self._want_soft = True
                else:
                    # チャンク化した大型構築物：ぶつかる所のチャンクだけブロックごとに戻す
                    vox = np.unique(idx[coming] // 8)
                    ch = self.chunk_of[vox]
                    self._pending_dissolve = getattr(self, "_pending_dissolve", set()) | set(ch[ch >= 0].tolist())
        hit = d < 0
        if not hit.any():
            return
        hi = idx[hit]
        n = g[hit]
        if collect:
            return hi, -d[hit], n
        # 押し戻しを少しやわらげる（衝撃が結合に一度にかかってちぎれるのを防ぐ）
        self.pos[hi] -= n * (d[hit] * CONTACT_SOFT)[:, None]
        disp = self.pos[hi] - self._prev[hi]
        tang = disp - n * np.einsum("ij,ij->i", disp, n)[:, None]
        fr = np.clip(self.fric[hi], 0, 1)
        self.pos[hi] -= tang * fr[:, None]

    def _collide_boxes(self, bs, h, collect=False):
        """街の建物（向き付きの箱）との当たり判定。ぶつかった速さは建物の損傷に、押した分は動く街の反動に使う。"""
        from .cities import box_sdf
        city, C, A, E, idx = bs
        K = self.contact_cores()
        r = self.size * 0.5
        rb = np.linalg.norm(E, axis=1)
        lo = (C - rb[:, None]).min(0) - r
        hi = (C + rb[:, None]).max(0) + r
        with np.errstate(invalid="ignore"):
            sel = np.nonzero(np.all(K > lo, axis=1) & np.all(K < hi, axis=1))[0]
        if not len(sel):
            return None
        d, n, k = box_sdf(K[sel], C, A, E)
        d = d - r
        hit = d < 0
        if not hit.any():
            return None
        hi_ = sel[hit]
        n = n[hit]
        parts = idx[k[hit]]
        moving = city.is_moving()
        vs = city.point_velocity(self.pos[hi_]) if moving else np.zeros((len(hi_), 3))
        vn = -np.einsum("ij,ij->i", self.vel[hi_] - vs, n)
        # 建物の損傷（いちばん強く当たった速さ）
        if (vn >= 7.0).any():
            best = {}
            for pa, v in zip(parts.tolist(), vn.tolist()):
                if v > best.get(pa, 0.0):
                    best[pa] = v
            self._city_events.append((city, best, 0.0, None, None))
        if collect:
            return hi_, -d[hit], n, vs
        corr = -n * d[hit][:, None]
        self.pos[hi_] += corr
        disp = self.pos[hi_] - self._prev[hi_] - vs * h
        tang = disp - n * np.einsum("ij,ij->i", disp, n)[:, None]
        fr = np.clip(self.fric[hi_], 0, 1)
        self.pos[hi_] -= tang * fr[:, None]
        if moving:
            J = (corr - tang * fr[:, None]) * (self.mass[hi_] / h)[:, None]
            self._city_events.append((city, None, 0.0, -J, self.pos[hi_]))
        return None

    def substep_end(self, h):
        if not self.n_vox:
            return False
        if self.mode == "rigid":
            self._rigid_fit_pos()
            self.vel = (self.pos - self._prev) / h
            return False
        if self.mode == "art":
            self._art_end(h)
            return False
        self.vel = (self.pos - self._prev) / h
        self._plasticity(h)
        broke = self._check_breaks()
        if self.chunk_size and not broke:
            pend = getattr(self, "_pending_dissolve", None)
            if pend:
                self._pending_dissolve = set()
                self._dissolve_chunks(pend)
            self._check_chunk_strain()
        return broke

    def _dissolve_chunks(self, chunks):
        """指定したチャンクをブロックごとの計算に戻す。"""
        chunks = {int(k) for k in chunks if k >= 0}
        if not chunks or not self.chunk_size:
            return
        self._dissolved = getattr(self, "_dissolved", set())
        for k in chunks:
            v = np.nonzero(self.chunk_of == k)[0]
            if len(v):
                self._dissolved.add(tuple(int(x) for x in self._chunk_key[v[0]]))
        # 戻したブロックの基準形を今の形に合わせる（急に元の形へ戻ろうとしないように）
        P8 = self.pos.reshape(-1, 8, 3)
        F = self.voxel_frames()
        for k in chunks:
            for i in np.nonzero(self.chunk_of == k)[0]:
                P = P8[i] - P8[i].mean(0)
                self.rest[i] = (P @ F[i].T) / self.act_scale[i]
        self._setup_chunks(keep_broken=True)

    def _ensure_lastR(self):
        if not hasattr(self, "_last_R") or len(self._last_R) != self.n_vox:
            self._last_R = np.tile(np.eye(3), (self.n_vox, 1, 1))
        if getattr(self, "_allvox", None) is None or len(self._allvox) != self.n_vox:
            self._allvox = np.arange(self.n_vox, dtype=np.int64)
        if getattr(self, "_quat", None) is None or len(self._quat) != self.n_vox:
            self._quat = np.zeros((self.n_vox, 4))
            self._quat[:, 0] = 1.0

    def _shape_match(self):
        if FAST:
            self._ensure_lastR()
            fn = fastphys.shape_match_par if self.n_vox >= fastphys.PARALLEL_MIN_VOXELS and not self._in_pool \
                else fastphys.shape_match
            fn(self.pos, self.rest, self.act_scale, self.v_stiff, self._allvox, self._last_R, self._quat)
            return
        P = self.pos.reshape(-1, 8, 3)
        c = P.mean(axis=1, keepdims=True)
        X = P - c
        Q = self.rest * self.act_scale[:, None, :]
        Q = Q - Q.mean(axis=1, keepdims=True)
        A = np.einsum("nki,nkj->nij", X, Q)
        U, _, Vt = np.linalg.svd(A)
        det = np.linalg.det(U @ Vt)
        U[det < 0, :, 2] *= -1
        R = U @ Vt
        goal = c + np.einsum("nij,nkj->nki", R, Q)
        P += self.v_stiff[:, None, None] * (goal - P)
        self._last_R = R

    def _solve_joints(self):
        if not len(self.ja):
            return
        if FAST:
            fastphys.solve_joints(self.pos, self.ja, self.jb, self.jr, self.jk, self.inv_mass, self.jcount)
            return
        pa, pb = self.pos[self.ja], self.pos[self.jb]
        d = pb - pa
        L = np.linalg.norm(d, axis=1)
        C = L - self.jr
        n = d / np.maximum(L, 1e-9)[:, None]
        wa, wb = self.inv_mass[self.ja], self.inv_mass[self.jb]
        s = (C * self.jk / (wa + wb))[:, None] * n
        nv = len(self.pos)
        delta = np.zeros((nv, 3))
        for ax in range(3):
            delta[:, ax] += np.bincount(self.ja, weights=s[:, ax] * wa, minlength=nv)
            delta[:, ax] -= np.bincount(self.jb, weights=s[:, ax] * wb, minlength=nv)
        self.pos += delta / self.jcount[:, None]

    def _rotor_targets(self, r):
        """回転ジョイント: アクチュエーター出力面の4頂点を軸まわりに回した位置。"""
        pairs = JOINT_PAIRS[r["d"]]
        ia, ib = r["a"], r["b"]
        A = 8 * ia + pairs[:, 0]
        B = 8 * ib + pairs[:, 1]
        pa = self.pos[A]
        c = pa.mean(0)
        n = c - self.pos[8 * ia:8 * ia + 8].mean(0)
        n /= max(np.linalg.norm(n), 1e-9)
        th = np.radians(float(r["dev"]["state"].get("angle", 0.0)))
        v = pa - c
        cs, sn = np.cos(th), np.sin(th)
        rot = v * cs + fcross(n, v) * sn + np.outer(v @ n, n) * (1 - cs)
        r["_n"], r["_th"] = n, th
        return A, B, c + rot

    def _solve_rotors(self):
        for r in self.rotors:
            A, B, target = self._rotor_targets(r)
            err = self.pos[B] - target
            # 並進成分（重さや押す力）は両側で分け合い、回転・変形成分は出力側だけを動かす
            # （モーターの反トルクは土台に伝えない：安定性のため）
            mean = err.mean(axis=0)
            wa, wb = self.inv_mass[A[0]], self.inv_mass[B[0]]
            ka = wa / (wa + wb)
            self.pos[B] -= (err - mean) + mean * (1 - ka)
            self.pos[A] += mean * ka

    # ------------------------------------------------------------------
    # 軽量化 1：剛体化（加速度が小さく、形が変わっていない構築物は1つの剛体として動かす）
    # ------------------------------------------------------------------
    RIGID_ACC_IN = 25.0      # 全体の加速度（重力以外）がこれ以下なら剛体化を考える (m/s²)
    RIGID_ACC_OUT = 35.0     # これを超えたらブロックごとの計算に戻す
    RIGID_DEFORM_V = 0.05    # 剛体の動きからのずれの速さがこれ以下 (m/s)
    RIGID_CALM_T = 0.3       # 上の条件がこの時間続いたら剛体化 (s)
    IMPACT_SPEED = 2.0       # 剛体のままこの速さで地面に近づいたらブロックごとに戻す (m/s)

    def _rigid_allowed(self):
        """剛体化してよい構築物か（液体・粘土など柔らかい素材、動作中のアクチュエーターは不可）。"""
        if self.n_vox < 2:
            return False
        if np.any(self.v_stiff < 0.5) or np.any(self.v_rebond):
            return False
        from . import actuator as act
        for d in self.devs:
            if d and (self.reg.device(d["def"]) or {}).get("type") == "actuator" and act.busy(d["state"]):
                return False
        return True

    def body_acceleration(self):
        """重力以外の力による全体の加速度 (m/s²)。"""
        return float(np.linalg.norm(self.force.sum(0))) / max(self.total_mass(), 1e-9)

    def _rigid_motion(self):
        """(重心, 重心速度, 角速度, 慣性テンソル)。"""
        m = self.mass
        M = m.sum()
        x = (self.pos * m[:, None]).sum(0) / M
        v = (self.vel * m[:, None]).sum(0) / M
        r = self.pos - x
        L = (fcross(r, self.vel - v) * m[:, None]).sum(0)
        rr = np.einsum("ij,ij->i", r, r)
        I = np.eye(3) * (m * rr).sum() - np.einsum("i,ij,ik->jk", m, r, r)
        w = np.linalg.solve(I + np.eye(3) * 1e-9 * max(np.trace(I), 1e-9), L)
        return x, v, w, I, r

    def calm_for_rigid(self):
        """形がほとんど変わっていない（剛体として動いている）か。"""
        x, v, w, I, r = self._rigid_motion()
        dev = self.vel - (v + fcross(w, r))
        ext = float(np.sqrt(np.einsum("ij,ij->i", r, r).max()))
        return (float(np.abs(dev).max()) < self.RIGID_DEFORM_V
                and float(w @ w) * ext < self.RIGID_ACC_IN and self.body_acceleration() < self.RIGID_ACC_IN)

    def enter_rigid(self):
        if self.chunk_size or getattr(self, "_dissolved", None):
            self._setup_chunks()      # 落ち着いたので、戻していたチャンクを作り直す
        x, v, w, I, r = self._rigid_motion()
        self.rb = {"local": r.copy(), "R": np.eye(3), "x": x, "v": v, "w": w, "Ib": I,
                   "ext": float(np.sqrt(np.einsum("ij,ij->i", r, r).max()))}
        self.vel = v + fcross(w, r)
        self.mode = "rigid"
        self.rb_epoch += 1
        self._want_soft = False

    def leave_rigid(self):
        self.mode = "soft"
        self.rb = None
        self._calm_t = 0.0
        self._want_soft = False

    def _rigid_begin(self, h, gravity_fn, planets):
        acc = gravity_fn(self.pos) + self.force * self.inv_mass[:, None]
        self.vel += acc * h
        for pl in planets:
            if pl.has_water and self._near_planet(pl):
                self._buoyancy(pl, h, gravity_fn)
        # 速度を剛体の動き（並進＋回転）に揃える（運動量・角運動量は保つ）
        rb = self.rb
        m = self.mass
        M = m.sum()
        x = (self.pos * m[:, None]).sum(0) / M
        v = (self.vel * m[:, None]).sum(0) / M
        r = self.pos - x
        L = (fcross(r, self.vel - v) * m[:, None]).sum(0)
        R = rb["R"]
        Iw = R @ rb["Ib"] @ R.T
        w = np.linalg.solve(Iw + np.eye(3) * 1e-9 * max(np.trace(Iw), 1e-9), L)
        self.vel = v + fcross(w, r)
        self._prev = self.pos.copy()
        self.pos += self.vel * h
        near = [pl for pl in planets if self._near_planet(pl)]
        if getattr(self, "_full_rigid", False):
            self._rigid_ground_contacts(near, h)
            return
        for pl in near:
            self._collide_planet(pl)
        for bs in getattr(self, "_city_boxes", ()):
            self._collide_boxes(bs, h)

    def _rigid_ground_contacts(self, near, h=1 / 180):
        """完全剛体モードの地面・建物との接触：剛体として（並進＋回転で）めり込みを解消し、摩擦をかける。
        頂点ごとに押し戻して形に合わせ直すやり方だと、押し戻しが全体に薄まって跳ね返ってしまうため。"""
        P, D, N, VS, CI = [], [], [], [], []
        for pl in near:
            res = self._collide_planet(pl, collect=True)
            if res:
                P.append(res[0]); D.append(res[1]); N.append(res[2])
                VS.append(np.zeros((len(res[0]), 3))); CI.append(np.full(len(res[0]), -1))
        boxes = list(getattr(self, "_city_boxes", ()))
        for bi, bs in enumerate(boxes):
            res = self._collide_boxes(bs, h, collect=True)
            if res:
                P.append(res[0]); D.append(res[1]); N.append(res[2])
                VS.append(res[3]); CI.append(np.full(len(res[0]), bi))
        if not P:
            return
        P, D, N = np.concatenate(P), np.concatenate(D), np.concatenate(N)
        VS, CI = np.concatenate(VS), np.concatenate(CI)
        order = np.argsort(-D)[:32]                   # 深い所から（多すぎる接触点は間引く）
        P, D, N, VS, CI = P[order], D[order], N[order], VS[order], CI[order]
        m = self.mass
        M = m.sum()
        x = (self.pos * m[:, None]).sum(0) / M
        r_all = self.pos - x
        rr = np.einsum("ij,ij->i", r_all, r_all)
        I = np.eye(3) * (m * rr).sum() - np.einsum("i,ij,ik->jk", m, r_all, r_all)
        Iinv = np.linalg.inv(I + np.eye(3) * 1e-6 * max(np.trace(I), 1e-6))
        rk = r_all[P]
        rn = fcross(rk, N)
        wk = 1.0 / M + np.einsum("ij,ij->i", rn, rn @ Iinv.T)
        # めり込み（法線方向）：接点ごとの押し戻し量 λ≥0 を、全接点が同時に地面の上に出るように一度に解く
        from scipy.optimize import nnls
        A = (N @ N.T) / M + rn @ Iinv @ rn.T
        try:
            lam_n, _ = nnls(A, D, maxiter=200)
        except Exception:  # noqa: BLE001
            lam_n = np.zeros(len(P))
            lam_n[0] = D[0] / wk[0]
        dx = (N * lam_n[:, None]).sum(0) / M
        dth = Iinv @ (rn * lam_n[:, None]).sum(0)
        # 動いている街（ステーション）を押した分の反動
        for bi in np.unique(CI[CI >= 0]):
            m_ = (CI == bi) & (lam_n > 0)
            if m_.any():
                J = N[m_] * (lam_n[m_] / h)[:, None]
                self._city_events.append((boxes[bi][0], None, 0.0, -J, self.pos[P[m_]]))
        # 摩擦（接点の横すべりを、押し戻した量に比例する上限まで止める）。動いている建物とは相対的に
        base = self.pos[P] - self._prev[P] - VS * h
        fr = np.clip(self.fric[P], 0, 1)
        for _ in range(2):
            for k in range(len(P)):
                if lam_n[k] <= 0:
                    continue
                disp = base[k] + dx + np.cross(dth, rk[k])
                t = disp - N[k] * (N[k] @ disp)
                tl = float(np.linalg.norm(t))
                if tl < 1e-9:
                    continue
                tn = t / tl
                rt = np.cross(rk[k], tn)
                wt = 1.0 / M + rt @ (Iinv @ rt)
                lam = min(tl * fr[k] / wt, lam_n[k] * 1.5)
                dx -= tn * (lam / M)
                dth -= Iinv @ rt * lam
        th = float(np.linalg.norm(dth))
        if th > 1e-12:
            k_ = dth / th
            Kx = np.array([[0, -k_[2], k_[1]], [k_[2], 0, -k_[0]], [-k_[1], k_[0], 0]])
            Rw = np.eye(3) + np.sin(th) * Kx + (1 - np.cos(th)) * (Kx @ Kx)
            r_all = r_all @ Rw.T
        self.pos = x + dx + r_all

    def _rigid_fit_pos(self):
        """頂点を剛体の形に戻す（重み付きの最適な回転・並進）。"""
        rb = self.rb
        m = self.mass
        M = m.sum()
        x = (self.pos * m[:, None]).sum(0) / M
        H = (rb["local"] * m[:, None]).T @ (self.pos - x)
        U, _, Vt = np.linalg.svd(H)
        R = Vt.T @ U.T
        if np.linalg.det(R) < 0:
            Vt[2] *= -1
            R = Vt.T @ U.T
        rb["R"], rb["x"] = R, x
        self.pos = x + rb["local"] @ R.T

    def update_mode(self, dt, world):
        """毎ティック：ブロックごと ⇔ 剛体 ⇔ レール（遠方の軌道計算）を切り替える。"""
        if not self.n_vox or self.asleep:
            return
        phys = world.cfg.get("physics", {})
        self._full_rigid = bool(world.settings.get("full_rigid", False))
        if self._full_rigid:
            return self._update_full_rigid(world, phys, dt)
        if self.mode == "art":                      # 完全剛体モードを切ったとき
            self.mode = "soft"
            self.art = None
        acc = self.body_acceleration()
        if self.mode in ("rigid", "rails"):
            out = (self._want_soft or acc > self.RIGID_ACC_OUT or not self._rigid_allowed())
            if not out and self.mode == "rigid":
                _, _, w, _, _ = self._rigid_motion()
                out = float(w @ w) * self.rb["ext"] > self.RIGID_ACC_OUT
            if out:
                if self.mode == "rails":
                    self.leave_rails()
                self.leave_rigid()
                return
        if self.mode == "soft":
            if not phys.get("rigid_enabled", True) or self._want_soft:
                self._want_soft = False
                self._calm_t = 0.0
                return
            if self._rigid_allowed() and self.calm_for_rigid():
                self._calm_t += dt
                if self._calm_t >= self.RIGID_CALM_T:
                    self.enter_rigid()
            else:
                self._calm_t = 0.0
        # 軽量化 2：プレイヤーから遠い構築物は「レール」に乗せる
        if self.mode == "rigid" and phys.get("rails_enabled", True):
            if self._rails_ok(world, float(phys.get("rails_distance", 400.0))):
                self.enter_rails()
        elif self.mode == "rails":
            if not self._rails_ok(world, float(phys.get("rails_distance", 400.0)) * 0.9):
                self.leave_rails()

    def _actuator_busy(self):
        from . import actuator as act
        for d in self.devs:
            if d and (self.reg.device(d["def"]) or {}).get("type") == "actuator" and act.busy(d["state"]):
                return True
        return False

    ART_HOLD = 1.0              # アクチュエーターが止まってからこの時間（秒）は関節つきのまま（細かく切り替えない）

    def _update_full_rigid(self, world, phys, dt=0.0):
        """完全剛体モード（ワールド設定）：衝突・速度・加速度にかかわらず、構築物は丸ごと1つの剛体。
        アクチュエーターが動いている間は、アクチュエーターの前後で分けた部分ごとの剛体を関節でつないで動かし
        （mode "art"）、止まればすぐ（今の形のまま）1つの剛体に戻る。ぶつかっても壊れない。
        ブロックの追加・削除のあとも、そのときの形で剛体に戻る。"""
        self._want_soft = False          # 衝突などによる「ブロックごとに戻す」要求は無視する
        if self._actuator_busy():
            self._art_hold = self.ART_HOLD
            if self.mode == "rails":
                self.leave_rails()
            if self.mode != "art":
                self.enter_art()
            return
        if self.mode == "art" and getattr(self, "_art_hold", 0.0) > 0:
            self._art_hold -= dt
            return
        if self.mode in ("soft", "art"):
            self.enter_rigid()
        if self.mode == "rigid" and phys.get("rails_enabled", True):
            if self._rails_ok(world, float(phys.get("rails_distance", 400.0))):
                self.enter_rails()
        elif self.mode == "rails":
            if not self._rails_ok(world, float(phys.get("rails_distance", 400.0)) * 0.9):
                self.leave_rails()

    # ------------------------------------------------------------------
    # 完全剛体モード＋アクチュエーター：アクチュエーターの前後で分けた部分ごとの剛体を関節でつなぐ
    # ・部分 = アクチュエーターの出力面の結合を除いてつながっているブロックのまとまり
    # ・関節 = 出力面の4隅を回転角だけ回した位置に、出力側の部分の4隅を合わせる（fastphys.art_solve）
    # ・押し出し = アクチュエーター自身の出力面の4頂点を、押し出した分だけ部分の中でずらす
    # ・電力・信号・配管は構築物全体で共有なので、部分に分けても今までどおり通る
    # ------------------------------------------------------------------
    ART_ITERS = 6               # 1サブステップあたりの関節・接触の反復回数
    ART_CONTACTS = 24           # 部分ごとの接触点の上限（深い所から）
    MOTOR_STEP_X = 10.0         # 今回進める角度の何倍まで、1サブステップで直してよいか（念のための上限）
    MOTOR_HOLD_DEG = 20.0       # それとは別に1サブステップで直せる角度（数値が暴れたときの念のための上限）

    def _art_groups(self):
        n = self.n_vox
        parent = np.arange(n)

        def find(x):
            while parent[x] != x:
                parent[x] = parent[parent[x]]
                x = parent[x]
            return x
        rotor_keys = {r["key"] for r in self.rotors}
        for key in list(self.links) + list(self.extras):
            if key in rotor_keys:
                continue
            ia, ib = self.index.get(key[0]), self.index.get(key[1])
            if ia is None or ib is None:
                continue
            ra, rb = find(ia), find(ib)
            if ra != rb:
                parent[ra] = rb
        roots = np.array([find(i) for i in range(n)])
        _, gid = np.unique(roots, return_inverse=True)
        return gid.ravel().astype(np.int64)

    def _art_build(self):
        from . import actuator as act
        gid = self._art_groups()
        G = int(gid.max()) + 1 if len(gid) else 0
        vg = np.repeat(gid, 8)
        m = self.mass
        M = np.bincount(vg, weights=m, minlength=G)
        X = np.stack([np.bincount(vg, weights=m * self.pos[:, k], minlength=G) for k in range(3)], 1) / M[:, None]
        vl = self.pos - X[vg]
        Ib = np.zeros((G, 3, 3))
        rr = np.einsum("ij,ij->i", vl, vl)
        for i in range(3):
            for j in range(3):
                Ib[:, i, j] = np.bincount(vg, weights=m * ((rr if i == j else 0.0) - vl[:, i] * vl[:, j]), minlength=G)
        tr = np.trace(Ib, axis1=1, axis2=2)
        IinvB = np.linalg.inv(Ib + np.eye(3)[None] * (1e-6 * np.maximum(tr, 1e-6))[:, None, None])
        exts = []
        for i, d in enumerate(self.devs):
            if d and act.is_actuator(self, i):
                pairs = JOINT_PAIRS[int(d.get("dir", 2))]
                rows = 8 * i + pairs[:, 0]
                nb = self.pos[rows].mean(0) - self.pos[8 * i:8 * i + 8].mean(0)
                nb /= max(np.linalg.norm(nb), 1e-9)
                exts.append((i, rows, nb, float(d["state"].get("ext", 0.0))))
        ra, rb, A, B, C, devs = [], [], [], [], [], []
        for r in self.rotors:
            a, b = int(gid[r["a"]]), int(gid[r["b"]])
            if a == b:
                continue                 # 輪になっていて回せない（固定されたまま）
            pairs = JOINT_PAIRS[r["d"]]
            ra.append(a); rb.append(b)
            A.append(8 * r["a"] + pairs[:, 0]); B.append(8 * r["b"] + pairs[:, 1])
            C.append(np.arange(8 * r["a"], 8 * r["a"] + 8))
            devs.append(r["dev"])
        self.art = {"version": self.version, "n": self.n_vox, "gid": gid, "vg": vg, "G": G,
                    "vl0": vl.copy(), "vl": vl, "invM": 1.0 / M, "IinvB": IinvB, "Ib": Ib,
                    "X": X, "R": np.tile(np.eye(3), (G, 1, 1)), "exts": exts,
                    "ra": np.array(ra, np.int64), "rb": np.array(rb, np.int64),
                    "A": np.array(A, np.int64).reshape(-1, 4), "B": np.array(B, np.int64).reshape(-1, 4),
                    "C": np.array(C, np.int64).reshape(-1, 8), "devs": devs,
                    "applied": np.array([float(d["state"].get("angle", 0.0)) for d in devs]),
                    "ext_applied": [e[3] for e in exts],
                    "gv": np.arange(len(vg), dtype=np.int64)}

    def enter_art(self):
        self._art_build()
        self.mode = "art"
        self.rb = None
        self._want_soft = False

    def _art_fit(self):
        """今の頂点の位置に、部分ごとの最適な姿勢（重み付きの回転・並進）を合わせる。"""
        a = self.art
        vg, G, vl = a["vg"], a["G"], a["vl"]
        m = self.mass
        M = 1.0 / a["invM"]
        X = np.stack([np.bincount(vg, weights=m * self.pos[:, k], minlength=G) for k in range(3)], 1) / M[:, None]
        d = self.pos - X[vg]
        H = np.zeros((G, 3, 3))
        for i in range(3):
            for j in range(3):
                H[:, i, j] = np.bincount(vg, weights=m * vl[:, i] * d[:, j], minlength=G)
        U, _, Vt = np.linalg.svd(H)
        V = np.transpose(Vt, (0, 2, 1))
        Ut = np.transpose(U, (0, 2, 1))
        R = V @ Ut
        neg = np.linalg.det(R) < 0
        if neg.any():
            V[neg, :, 2] *= -1
            R = V @ Ut
        a["X"], a["R"] = X, R

    def _art_write(self):
        a = self.art
        if FAST:
            fastphys.art_write(self.pos, a["X"], a["R"], a["gv"], a["vg"], a["vl"])
        else:
            self.pos = a["X"][a["vg"]] + np.einsum("nij,nj->ni", a["R"][a["vg"]], a["vl"])

    def _art_begin(self, h, gravity_fn, planets):
        a = getattr(self, "art", None)
        if a is None or a["version"] != self.version or a["n"] != self.n_vox:
            self._art_build()
            a = self.art
        acc = gravity_fn(self.pos) + self.force * self.inv_mass[:, None]
        self.vel += acc * h
        for pl in planets:
            if pl.has_water and self._near_planet(pl):
                self._buoyancy(pl, h, gravity_fn)
        # 押し出し：アクチュエーターの出力面を押し出した分だけずらす（部分の形の中での変形）
        self._art_fit()
        a["defo"] = []
        if a["exts"]:
            vl = a["vl0"].copy()
            moved = False
            for k, (i, rows, nb, e0) in enumerate(a["exts"]):
                st = self.devs[i]["state"] if self.devs[i] else {}
                # 伸びもティックごとに進むので、サブステップごとに伸びる速さで追いかける（動きが速度として残るように）
                want = float(st.get("ext", e0))
                spd = max(float(st.get("ext_speed", 0.5)), 0.01) * h * 1.05
                e = a["ext_applied"][k] = a["ext_applied"][k] + float(np.clip(want - a["ext_applied"][k], -spd, spd))
                if e != e0:
                    vl[rows] += nb * ((e - e0) * self.size)
                    moved = True
            if moved:
                # 部分の重心が基準点（X）のままになるように、部分ごとに中心を合わせ直す
                vg, G = a["vg"], a["G"]
                m = self.mass
                M = 1.0 / a["invM"]
                lbar = np.stack([np.bincount(vg, weights=m * vl[:, k], minlength=G) for k in range(3)], 1) / M[:, None]
                vl -= lbar[vg]
            d = vl - a["vl"]
            rows = np.nonzero(np.abs(d).max(1) > 0)[0]
            if len(rows):
                a["vl"] = vl
                a["defo"].append((rows, d[rows]))
        # 部分ごとの速度（運動量・角運動量を保つ）
        vg, G = a["vg"], a["G"]
        m = self.mass
        M = 1.0 / a["invM"]
        X, R = a["X"], a["R"]
        v = np.stack([np.bincount(vg, weights=m * self.vel[:, k], minlength=G) for k in range(3)], 1) / M[:, None]
        r = self.pos - X[vg]
        Lm = fcross(r, self.vel - v[vg]) * m[:, None]
        Lg = np.stack([np.bincount(vg, weights=Lm[:, k], minlength=G) for k in range(3)], 1)
        Iinv_w = R @ a["IinvB"] @ np.transpose(R, (0, 2, 1))
        w = np.einsum("gij,gj->gi", Iinv_w, Lg)
        self._prev = self.pos.copy()
        a["X"] = X + v * h
        for g in range(G):
            phi = w[g] * h
            th = float(np.linalg.norm(phi))
            if th > 1e-14:
                k_ = phi / th
                Kx = np.array([[0, -k_[2], k_[1]], [k_[2], 0, -k_[0]], [-k_[1], k_[0], 0]])
                R[g] = (np.eye(3) + np.sin(th) * Kx + (1 - np.cos(th)) * (Kx @ Kx)) @ R[g]
        self._art_write()
        # 接触の候補（予測した位置で集め、反復の中では姿勢の変化から深さを見積もる）
        near = [pl for pl in planets if self._near_planet(pl)]
        P, D, N, VS, CI = [], [], [], [], []
        for pl in near:
            res = self._collide_planet(pl, collect=True)
            if res:
                P.append(res[0]); D.append(res[1]); N.append(res[2])
                VS.append(np.zeros((len(res[0]), 3))); CI.append(np.full(len(res[0]), -1))
        boxes = list(getattr(self, "_city_boxes", ()))
        for bi, bs in enumerate(boxes):
            res = self._collide_boxes(bs, h, collect=True)
            if res:
                P.append(res[0]); D.append(res[1]); N.append(res[2])
                VS.append(res[3]); CI.append(np.full(len(res[0]), bi))
        if P:
            P, D, N = np.concatenate(P), np.concatenate(D), np.concatenate(N)
            VS, CI = np.concatenate(VS), np.concatenate(CI)
            cg = vg[P]
            order = np.lexsort((-D, cg))                 # 部分ごとに深い順
            P, D, N, VS, CI, cg = P[order], D[order], N[order], VS[order], CI[order], cg[order]
            first = np.searchsorted(cg, cg)
            keep = (np.arange(len(cg)) - first) < self.ART_CONTACTS
            P, D, N, VS, CI, cg = P[keep], D[keep], N[keep], VS[keep], CI[keep], cg[keep]
        else:
            P = np.zeros(0, np.int64); D = np.zeros(0); N = np.zeros((0, 3)); VS = np.zeros((0, 3))
            CI = np.zeros(0, np.int64); cg = np.zeros(0, np.int64)
        X, R = a["X"], a["R"]
        cl = np.einsum("nji,nj->ni", R[cg], self.pos[P] - X[cg]) if len(P) else np.zeros((0, 3))
        nr = len(a["ra"])
        vl = a["vl"]
        LA = vl[a["A"]] if nr else np.zeros((0, 4, 3))
        LB = vl[a["B"]] if nr else np.zeros((0, 4, 3))
        LC = vl[a["C"]].mean(1) if nr else np.zeros((0, 3))
        # 回転角はティックごとに進むので、サブステップごとに少しずつ（回転の速さで）追いかける（動きが速度として残るように）
        if nr:
            want = np.array([float(d["state"].get("angle", 0.0)) for d in a["devs"]])
            spd = np.array([max(float(d["state"].get("rot_speed", 90.0)), 1.0) for d in a["devs"]]) * h * 1.05
            step = np.clip(want - a["applied"], -spd, spd)
            a["applied"] = a["applied"] + step
            ang = np.radians(a["applied"])
            # モーターが1サブステップで回せる角度の上限（今回進める角度＋重さを支える分）。
            # 地面などに押し返されて回れないときに、無限の力で押し続けて構築物ごと跳ね飛ばさないように
            cap = np.radians(np.abs(step) * self.MOTOR_STEP_X + self.MOTOR_HOLD_DEG)
        else:
            ang = np.zeros(0)
            cap = np.zeros(0)
        lamN = np.zeros(len(P))
        fastphys.art_solve(X, R, a["invM"], a["IinvB"], a["ra"], a["rb"], np.ascontiguousarray(LA),
                           np.ascontiguousarray(LB), np.ascontiguousarray(LC), ang, cap, cg.astype(np.int64),
                           np.ascontiguousarray(cl), np.ascontiguousarray(N, dtype=np.float64),
                           np.ascontiguousarray(D, dtype=np.float64), self.pos[P].copy(), self._prev[P].copy(),
                           np.ascontiguousarray(VS, dtype=np.float64), np.clip(self.fric[P], 0, 1),
                           float(h), int(self.ART_ITERS), lamN)
        self._art_write()
        a["contacts"] = (cg.astype(np.int64), np.ascontiguousarray(cl), np.ascontiguousarray(N, dtype=np.float64), lamN)
        # 動いている街（ステーション）を押した分の反動
        if len(P) and (CI >= 0).any():
            for bi in np.unique(CI[CI >= 0]):
                m_ = (CI == bi) & (lamN > 0)
                if m_.any():
                    J = N[m_] * (lamN[m_] / h)[:, None]
                    self._city_events.append((boxes[bi][0], None, 0.0, -J, self.pos[P[m_]]))

    def _art_end(self, h):
        """ほかの構築物に押された分なども部分ごとの剛体の動きに直してから、速度を求める。"""
        a = self.art
        self._art_fit()
        self._art_write()
        self.vel = (self.pos - self._prev) / h
        # 押し出しによる形の変化は速度に含めない（含めると、部分全体の運動量として毎サブステップ積み上がってしまう）
        for rows, d in a.get("defo") or ():
            g = a["vg"][rows]
            self.vel[rows] -= np.einsum("nij,nj->ni", a["R"][g], d) / h
        a["defo"] = []
        ct = a.get("contacts")
        if ct is not None and len(ct[0]) and (ct[3] > 0).any():
            # 部分ごとの速度（並進・回転）に直し、地面から離れる向きの余計な速度を消してから頂点の速度に戻す
            vg, G = a["vg"], a["G"]
            m = self.mass
            M = 1.0 / a["invM"]
            X, R = a["X"], a["R"]
            V = np.stack([np.bincount(vg, weights=m * self.vel[:, k], minlength=G) for k in range(3)], 1) / M[:, None]
            r = self.pos - X[vg]
            Lm = fcross(r, self.vel - V[vg]) * m[:, None]
            Lg = np.stack([np.bincount(vg, weights=Lm[:, k], minlength=G) for k in range(3)], 1)
            W = np.einsum("gij,gj->gi", R @ a["IinvB"] @ np.transpose(R, (0, 2, 1)), Lg)
            fastphys.art_contact_vel(V, W, X, R, a["invM"], a["IinvB"], ct[0], ct[1], ct[2], ct[3], 2)
            self.vel = V[vg] + fcross(W[vg], r)
        a["contacts"] = None

    # ------------------------------------------------------------------
    # 軽量化 2：レール（遠方の構築物は重心の軌道と回転だけを計算する）
    # ------------------------------------------------------------------
    def _rails_ok(self, world, dist):
        x = self.rb["x"] if self.rb is not None else self.center_of_mass()
        if np.linalg.norm(x - world.player.pos) < dist:
            return False
        if np.any(self.force):
            return False
        # 地面の近くでは当たり判定が必要なのでレールに乗せない
        pl = world.nearest_planet(x)
        if pl is not None:
            alt = float(pl.sdf(x[None])[0])
            ext = self.rb["ext"] if self.rb is not None else 10.0
            if alt < ext + 15.0:
                return False
        return True

    def enter_rails(self):
        self.mode = "rails"
        x, v, w, I, r = self._rigid_motion()
        self.rb["x"], self.rb["v"], self.rb["w"] = x, v, w
        self._rails_sync()

    def leave_rails(self):
        self._rails_sync()
        self.mode = "rigid"

    def rails_step(self, dt, gravity_fn, sub=4):
        """重心は重力だけで（速度ベルレ法）、回転は一定の角速度で進める。"""
        rb = self.rb
        h = dt / sub
        x, v = rb["x"], rb["v"]
        a = gravity_fn(x[None])[0]
        for _ in range(sub):
            v = v + a * (h * 0.5)
            x = x + v * h
            a = gravity_fn(x[None])[0]
            v = v + a * (h * 0.5)
        rb["x"], rb["v"] = x, v
        w = rb["w"]
        wn = float(np.linalg.norm(w))
        if wn > 1e-9:
            k = w / wn
            th = wn * dt
            Kx = np.array([[0, -k[2], k[1]], [k[2], 0, -k[0]], [-k[1], k[0], 0]])
            Rw = np.eye(3) + np.sin(th) * Kx + (1 - np.cos(th)) * (Kx @ Kx)
            rb["R"] = Rw @ rb["R"]
        self._rails_sync()

    def _rails_sync(self):
        rb = self.rb
        r = rb["local"] @ rb["R"].T
        self.pos = rb["x"] + r
        self.vel = rb["v"] + fcross(rb["w"], r)

    def render_transform(self):
        """描画用：剛体・レール・静止中なら (R, x, 版)。ブロックごとに動いている間は None。"""
        if self.mode in ("rigid", "rails") and self.rb is not None:
            return self.rb["R"], self.rb["x"], ("rb", self.rb_epoch, self.version)
        if getattr(self, "asleep", False):
            if getattr(self, "_sleep_anchor", (None,))[0] != self.sleep_epoch:
                self._sleep_anchor = (self.sleep_epoch, self.pos[0].copy())
            return np.eye(3), self._sleep_anchor[1], ("sleep", self.sleep_epoch, self.version)
        return None

    # ------------------------------------------------------------------
    # 軽量化 3：大型構築物はチャンク（4×4×4 ブロック）ごとの剛体を結合でつなぐ
    # ------------------------------------------------------------------
    CHUNK_EDGE = 4
    CHUNK_IMPACT_SPEED = 4.0  # チャンクのままこの速さで地面に近づいたら、ぶつかる所のチャンクを戻す (m/s)
    CHUNK_STRAIN = 0.7       # 境目の結合がちぎれる距離のこの割合まで伸びたら、そのチャンクはブロックごとに戻す
    chunk_min_voxels = 300   # これ以上のブロック数でチャンク化（world が設定で上書き）

    def _restore_joints(self):
        """チャンク内で省いていた結合を元に戻す。"""
        if self.chunk_size and getattr(self, "_ja_all", None) is not None:
            self.ja, self.jb = self._ja_all, self._jb_all
            self.jr, self.jk, self.jbrk = self._jr_all, self._jk_all, self._jbrk_all
            self.jkey = self._jkey_all
            nv = 8 * self.n_vox
            cnt = np.bincount(self.ja, minlength=nv) + np.bincount(self.jb, minlength=nv)
            self.jcount = np.maximum(cnt, 1).astype(np.float64)
        self.chunk_size = 0

    def _setup_chunks(self, keep_broken=False, fresh=False):
        if not fresh:
            self._restore_joints()     # 結合はいったん全部に戻してから分け直す
        n = self.n_vox
        self.chunk_size = 0
        self.chunk_of = np.full(n, -1, np.int64)
        self._chunk_rest = None
        self._jactive = None
        if not keep_broken:
            self._dissolved = set()
        if n < max(int(self.chunk_min_voxels), 8) or not hasattr(self, "v_stiff"):
            return
        cells = np.asarray(self.cells, dtype=np.int64)
        key = np.floor_divide(cells, self.CHUNK_EDGE)
        excl = (self.v_stiff < 0.5) | self.v_rebond
        for r in self.rotors:
            excl[r["a"]] = True
            excl[r["b"]] = True
        for i, d in enumerate(self.devs):
            if d and (self.reg.device(d["def"]) or {}).get("type") == "actuator":
                excl[i] = True
        dissolved = getattr(self, "_dissolved", set()) if keep_broken else set()
        _, inv = np.unique(key, axis=0, return_inverse=True)
        inv = inv.ravel()
        for k in dissolved:
            excl |= (key == np.asarray(k)).all(1)
        cid = np.where(excl, -1, inv)
        # 1ブロックだけのチャンクは普通のブロック扱い
        ids, cnt = np.unique(cid[cid >= 0], return_counts=True)
        small = set(ids[cnt < 2].tolist())
        if small:
            cid = np.where(np.isin(cid, list(small)), -1, cid)
        uniq = np.unique(cid[cid >= 0])
        if not len(uniq):
            return
        remap = -np.ones(int(cid.max()) + 1, np.int64)
        remap[uniq] = np.arange(len(uniq))
        self.chunk_of = np.where(cid >= 0, remap[np.maximum(cid, 0)], -1)
        self._chunk_key = key
        self.chunk_size = self.CHUNK_EDGE
        self.n_chunks = len(uniq)
        vox_in = np.nonzero(self.chunk_of >= 0)[0]
        self._cv = (8 * vox_in[:, None] + np.arange(8)[None]).ravel()          # チャンクに入っている頂点
        self._cv_chunk = np.repeat(self.chunk_of[vox_in], 8)
        self._single = np.nonzero(self.chunk_of < 0)[0]
        # チャンク内の結合は解かない（剛体なので）
        ca = self.chunk_of[self.ja // 8]
        cb = self.chunk_of[self.jb // 8]
        self._jactive = ~((ca >= 0) & (ca == cb))
        self._ja_all, self._jb_all = self.ja, self.jb
        self._jr_all, self._jk_all, self._jbrk_all = self.jr, self.jk, self.jbrk
        self._jkey_all = self.jkey
        act = self._jactive
        self.ja, self.jb = self.ja[act], self.jb[act]
        self.jr, self.jk, self.jbrk = self.jr[act], self.jk[act], self.jbrk[act]
        self.jkey = [k for k, a in zip(self.jkey, act) if a]
        nv = 8 * n
        cnt = np.bincount(self.ja, minlength=nv) + np.bincount(self.jb, minlength=nv)
        self.jcount = np.maximum(cnt, 1).astype(np.float64)

    def _chunk_rest_shape(self):
        """チャンクの基準形（作った時点の形をそのまま使う：それまでの変形も残る）。"""
        P = self.pos[self._cv]
        cc = self._cv_chunk
        C = self.n_chunks
        cnt = np.bincount(cc, minlength=C).astype(float)
        cen = np.stack([np.bincount(cc, weights=P[:, k], minlength=C) for k in range(3)], 1) / cnt[:, None]
        self._chunk_rest = P - cen[cc]

    def _shape_match_chunks(self):
        if self._chunk_rest is None:
            self._chunk_rest_shape()
        self._ensure_lastR()
        if FAST:
            if len(self._single):
                fastphys.shape_match(self.pos, self.rest, self.act_scale, self.v_stiff, self._single, self._last_R,
                                     self._quat)
            fastphys.shape_match_chunks(self.pos, self._cv, self._cv_chunk, self._chunk_rest, self.n_chunks)
            return
        # チャンクに入っていないブロックは今までどおり
        if len(self._single):
            sub = self._single
            P = self.pos.reshape(-1, 8, 3)[sub]
            c = P.mean(axis=1, keepdims=True)
            X = P - c
            Q = self.rest[sub] * self.act_scale[sub][:, None, :]
            Q = Q - Q.mean(axis=1, keepdims=True)
            A = np.einsum("nki,nkj->nij", X, Q)
            U, _, Vt = np.linalg.svd(A)
            det = np.linalg.det(U @ Vt)
            U[det < 0, :, 2] *= -1
            R = U @ Vt
            goal = c + np.einsum("nij,nkj->nki", R, Q)
            P += self.v_stiff[sub][:, None, None] * (goal - P)
            self.pos.reshape(-1, 8, 3)[sub] = P
            self._last_R[sub] = R
        # チャンク：まとめて最適な回転を求め、基準形に合わせる
        cv, cc, Q = self._cv, self._cv_chunk, self._chunk_rest
        C = self.n_chunks
        P = self.pos[cv]
        cnt = np.bincount(cc, minlength=C).astype(float)
        cen = np.stack([np.bincount(cc, weights=P[:, k], minlength=C) for k in range(3)], 1) / cnt[:, None]
        X = P - cen[cc]
        A = np.zeros((C, 3, 3))
        for i in range(3):
            for j in range(3):
                A[:, i, j] = np.bincount(cc, weights=X[:, i] * Q[:, j], minlength=C)
        U, _, Vt = np.linalg.svd(A)
        det = np.linalg.det(U @ Vt)
        U[det < 0, :, 2] *= -1
        R = U @ Vt
        self.pos[cv] = cen[cc] + np.einsum("nij,nj->ni", R[cc], Q)

    def _check_chunk_strain(self):
        """境目の結合が大きく伸びたチャンクは、ブロックごとの計算に戻す（そこで曲がったり折れたりできる）。"""
        if not len(self.ja) or not self.chunk_size:
            return
        L = np.linalg.norm(self.pos[self.jb] - self.pos[self.ja], axis=1) - self.jr
        hot = L > self.jbrk * self.CHUNK_STRAIN
        if not hot.any():
            return
        vox = np.unique(np.concatenate([self.ja[hot] // 8, self.jb[hot] // 8]))
        chunks = set(self.chunk_of[vox][self.chunk_of[vox] >= 0].tolist())
        if chunks:
            self._dissolve_chunks(chunks)


    def _plasticity(self, h):
        if not hasattr(self, "_last_R") or not np.any(self.v_plast > 0) or len(self._last_R) != self.n_vox:
            return
        P = self.pos.reshape(-1, 8, 3)
        X = P - P.mean(axis=1, keepdims=True)
        local = np.einsum("nji,nkj->nki", self._last_R, X) / self.act_scale[:, None, :]
        local -= local.mean(axis=1, keepdims=True)
        diff = local - self.rest
        strain = np.linalg.norm(diff, axis=2).max(axis=1) / self.size
        over = (strain > self.v_yield) & (self.v_plast > 0)
        if self.chunk_size:
            over &= self.chunk_of < 0      # チャンク内は変形しない
        if over.any():
            rate = np.clip(self.v_plast[over] * 20.0 * h, 0, 1)[:, None, None]
            self.rest[over] += diff[over] * rate
            # 潰れすぎ・伸びすぎを防ぐ
            lim = self.size * 1.2
            self.rest = np.clip(self.rest, -lim, lim)

    def _check_breaks(self):
        broke_rotor = False
        for r in getattr(self, "rotors", []):
            A, B, target = self._rotor_targets(r)
            if np.linalg.norm(self.pos[B] - target, axis=1).max() > r["br"]:
                self.links.discard(r["key"])
                broke_rotor = True
        if not len(self.ja):
            if broke_rotor:
                self.rebuild()
            return broke_rotor
        L = np.linalg.norm(self.pos[self.jb] - self.pos[self.ja], axis=1)
        broken = np.nonzero(L > self.jbrk)[0]
        if not len(broken):
            if broke_rotor:
                self.rebuild()
            return broke_rotor
        for j in broken:
            key = self.jkey[j]
            if key[0] == "x":
                self.extras.pop(key[1:], None)
            else:
                self.links.discard(key)
        self.rebuild()
        return True

    # ------------------------------------------------------------------
    # 編集
    # ------------------------------------------------------------------
    def add_voxel(self, cell, mat, dev, positions):
        """positions: (8,3) 新しいボクセルの頂点位置。隣接ボクセルと近ければ結合する。"""
        cell = tuple(int(x) for x in cell)
        if cell in self.index:
            return False
        self.cells.append(cell)
        self.mats.append(mat)
        self.devs.append(self._init_device(dev) if dev else None)
        self.pos = np.vstack([self.pos, positions])
        v = np.zeros((8, 3))
        if self.n_vox > 1:
            ctr = positions.mean(0)
            centers = self.voxel_centers()[:-1]
            near = np.linalg.norm(centers - ctr, axis=1).argmin()
            v[:] = self.vel[8 * near:8 * near + 8].mean(0)
        self.vel = np.vstack([self.vel, v])
        self.rest = np.concatenate([self.rest, (REST_UNIT * self.size)[None]], axis=0)
        self.act_scale = np.vstack([self.act_scale, np.ones((1, 3))])
        i = self.n_vox - 1
        for d in range(6):
            nb = (cell[0] + DIR_TUPLES[d][0], cell[1] + DIR_TUPLES[d][1], cell[2] + DIR_TUPLES[d][2])
            j = self.index.get(nb)
            if j is None:
                continue
            pairs = JOINT_PAIRS[d]
            dist = np.linalg.norm(positions[pairs[:, 0]] - self.pos[8 * j + pairs[:, 1]], axis=1).max()
            if dist < self.size * 0.35:
                self.links.add(link_key(cell, nb))
        self.force = np.zeros((self.n_vert, 3))
        self.rebuild()
        return i

    def remove_voxels(self, idxs):
        keep = np.ones(self.n_vox, bool)
        keep[list(idxs)] = False
        removed = [self.cells[i] for i in idxs]
        self._apply_keep(keep)
        rs = set(removed)
        self.links = {k for k in self.links if k[0] not in rs and k[1] not in rs}
        self.rebuild()
        return removed

    def _apply_keep(self, keep):
        kv = np.repeat(keep, 8)
        self.cells = [c for c, k in zip(self.cells, keep) if k]
        self.mats = [m for m, k in zip(self.mats, keep) if k]
        self.devs = [d for d, k in zip(self.devs, keep) if k]
        self.pos = self.pos[kv]
        self.vel = self.vel[kv]
        self.rest = self.rest[keep]
        self.act_scale = self.act_scale[keep]
        self.force = np.zeros((int(kv.sum()), 3))

    def components(self):
        """結合で繋がっているボクセル群に分ける。"""
        n = self.n_vox
        parent = list(range(n))

        def find(x):
            while parent[x] != x:
                parent[x] = parent[parent[x]]
                x = parent[x]
            return x
        for key in list(self.links) + list(self.extras.keys()):
            a, b = self.index.get(key[0]), self.index.get(key[1])
            if a is None or b is None:
                continue
            ra, rb = find(a), find(b)
            if ra != rb:
                parent[ra] = rb
        groups = {}
        for i in range(n):
            groups.setdefault(find(i), []).append(i)
        return list(groups.values())

    def split(self):
        """ちぎれた部分を別の構築物に分離する。新しい構築物のリストを返す。"""
        comps = self.components()
        if len(comps) <= 1:
            return []
        comps.sort(key=len, reverse=True)
        out = []
        for comp in comps[1:]:
            sub = Construct(self.reg, size=self.size, name=self.name + "の破片")
            idx = np.array(comp)
            kv = (8 * idx[:, None] + np.arange(8)).ravel()
            sub.cells = [self.cells[i] for i in comp]
            sub.mats = [self.mats[i] for i in comp]
            sub.devs = [self.devs[i] for i in comp]
            sub.pos = self.pos[kv].copy()
            sub.vel = self.vel[kv].copy()
            sub.rest = self.rest[idx].copy()
            sub.act_scale = self.act_scale[idx].copy()
            cs = set(sub.cells)
            sub.links = {k for k in self.links if k[0] in cs and k[1] in cs}
            sub.extras = {k: v for k, v in self.extras.items() if k[0] in cs and k[1] in cs}
            sub.force = np.zeros((sub.n_vert, 3))
            sub.rebuild()
            out.append(sub)
        keep = np.zeros(self.n_vox, bool)
        keep[comps[0]] = True
        self._apply_keep(keep)
        cs = set(self.cells)
        self.links = {k for k in self.links if k[0] in cs and k[1] in cs}
        self.rebuild()
        return out

    def absorb(self, other, cell_map, xform=None):
        """別の構築物を取り込む。cell_map: other のセル → このセルの対応。
        xform: {ボクセル番号: (頂点の並べ替え perm, 旧ローカル→新ローカルの回転 M)}（90°単位で向きを揃える時）"""
        for c in other.cells:
            if cell_map[c] in self.index:
                raise ValueError("セルが重なっています")
        if xform:
            other._reorient(xform)
        self.cells += [cell_map[c] for c in other.cells]
        self.mats += list(other.mats)
        self.devs += list(other.devs)
        self.pos = np.vstack([self.pos, other.pos])
        self.vel = np.vstack([self.vel, other.vel])
        self.rest = np.concatenate([self.rest, other.rest], axis=0)
        self.act_scale = np.vstack([self.act_scale, other.act_scale])
        for a, b in other.links:
            self.links.add(link_key(cell_map[a], cell_map[b]))
        for (a, b), lst in other.extras.items():
            self.extras[link_key(cell_map[a], cell_map[b])] = list(lst)
        for k, v in other.storage.items():
            self.storage[k] = self.storage.get(k, 0.0) + v
        for k, v in other.cavity_air.items():
            self.cavity_air[cell_map.get(k, k)] = v
        self.force = np.zeros((self.n_vert, 3))
        self.rebuild()

    def _reorient(self, xform):
        """ボクセルの頂点番号と向きを、指定の回転で並べ替える（格子に揃えるため）。"""
        pmap = {}
        for k, (perm, M) in xform.items():
            sl = slice(8 * k, 8 * k + 8)
            P, V = self.pos[sl].copy(), self.vel[sl].copy()
            rest = self.rest[k].copy()
            for old in range(8):
                self.pos[8 * k + perm[old]] = P[old]
                self.vel[8 * k + perm[old]] = V[old]
                self.rest[k, perm[old]] = M @ rest[old]
            self.act_scale[k] = np.abs(M) @ self.act_scale[k]
            pmap[self.cells[k]] = perm
            d = self.devs[k]
            if d:
                v = M @ DIRS[d["dir"]].astype(float)
                d["dir"] = dir_index(np.round(v).astype(int))
        for key, lst in self.extras.items():
            pa, pb = pmap.get(key[0]), pmap.get(key[1])
            self.extras[key] = [(int(pa[a]) if pa is not None else a, int(pb[b]) if pb is not None else b, r)
                                for a, b, r in lst]

    def try_rebond(self):
        """再結合する素材同士が近づいていたら結合を追加。"""
        if not self.v_rebond.any():
            return False
        from scipy.spatial import cKDTree
        idx = np.nonzero(self.v_rebond)[0]
        centers = self.voxel_centers()[idx]
        tree = cKDTree(centers)
        pairs = tree.query_pairs(self.size * 1.15, output_type="ndarray")
        added = False
        for a, b in pairs:
            ia, ib = idx[a], idx[b]
            key = link_key(self.cells[ia], self.cells[ib])
            if key in self.links or key in self.extras:
                continue
            ia, ib = self.index[key[0]], self.index[key[1]]
            Pa, Pb = self.pos[8 * ia:8 * ia + 8], self.pos[8 * ib:8 * ib + 8]
            D = np.linalg.norm(Pa[:, None] - Pb[None], axis=2)
            lim = max(self.v_rebond_dist[ia], self.v_rebond_dist[ib])
            order = np.argsort(D, axis=None)[:4]
            lst = []
            for o in order:
                ca, cb = divmod(int(o), 8)
                if D[ca, cb] < lim:
                    lst.append((ca, cb, float(D[ca, cb]) * 0.5))
            if lst:
                self.extras[key] = lst
                added = True
        if added:
            self.rebuild()
        return added

    # ------------------------------------------------------------------
    # 保存
    # ------------------------------------------------------------------
    def rigid_fit(self):
        """現在の頂点位置にもっとも合う剛体変換 (R, t)：pos ≒ R @ local + t"""
        local = ((np.asarray(self.cells, dtype=np.float64)[:, None, :] + CORNER_BITS[None]) * self.size).reshape(-1, 3)
        lc, pc = local.mean(0), self.pos.mean(0)
        H = (local - lc).T @ (self.pos - pc)
        U, _, Vt = np.linalg.svd(H)
        R = Vt.T @ U.T
        if np.linalg.det(R) < 0:
            Vt[2] *= -1
            R = Vt.T @ U.T
        t = pc - R @ lc
        return R, t

    def angular_velocity(self):
        com = self.center_of_mass()
        r = self.pos - com
        v = self.vel - self.velocity()
        Lm = (fcross(r, v) * self.mass[:, None]).sum(0)
        I = (self.mass[:, None, None] * (np.einsum("ij,ij->i", r, r)[:, None, None] * np.eye(3)
                                          - r[:, :, None] * r[:, None, :])).sum(0)
        try:
            return np.linalg.solve(I + np.eye(3) * 1e-6, Lm)
        except np.linalg.LinAlgError:
            return np.zeros(3)

    def to_dict(self):
        return {
            "id": self.id, "name": self.name, "voxel_size": self.size,
            "voxels": [[c[0], c[1], c[2], m, _clean_dev(d)] for c, m, d in zip(self.cells, self.mats, self.devs)],
            "links": [[list(a), list(b)] for a, b in sorted(self.links)],
            "extras": [[list(k[0]), list(k[1]), v] for k, v in self.extras.items()],
            "storage": self.storage,
            "cavity_air": [[list(k), v] for k, v in self.cavity_air.items()],
            # 現在の形（回転したアーム・伸びたアクチュエーター・変形）を保存
            "vertices": np.round((self.pos - self.rigid_fit()[1]) @ self.rigid_fit()[0], 4).tolist(),
            "rest": np.round(self.rest, 4).tolist(),
            **({"anchor": self.anchor["city"]} if getattr(self, "anchor", None) else {}),
        }

    def spawn_dict(self):
        R, t = self.rigid_fit()
        return {"position": t.tolist(), "rotation": R.tolist(), "velocity": self.velocity().tolist(),
                "angular_velocity": self.angular_velocity().tolist()}

    @classmethod
    def from_dict(cls, registry, data, spawn):
        voxels = [((v[0], v[1], v[2]), v[3], v[4]) for v in data["voxels"]]
        links = None
        if "links" in data:
            links = {link_key(tuple(a), tuple(b)) for a, b in data["links"]}
        c = cls.from_voxels(registry, voxels, spawn.get("position", [0, 0, 0]), spawn.get("rotation"),
                            spawn.get("velocity", [0, 0, 0]), spawn.get("angular_velocity", [0, 0, 0]),
                            size=data.get("voxel_size", 1.0), links=links, id_=data.get("id"),
                            name=data.get("name", "構築物"))
        for a, b, lst in data.get("extras", []):
            c.extras[link_key(tuple(a), tuple(b))] = [tuple(x) for x in lst]
        c.storage = dict(data.get("storage", {}))
        c.cavity_air = {tuple(k): v for k, v in data.get("cavity_air", [])}
        verts = data.get("vertices")
        if verts is not None and len(verts) == c.n_vert:
            R = np.eye(3) if spawn.get("rotation") is None else np.asarray(spawn["rotation"], dtype=np.float64)
            c.pos = np.asarray(verts, dtype=np.float64) @ R.T + np.asarray(spawn.get("position", [0, 0, 0]))
        rest = data.get("rest")
        if rest is not None and len(rest) == c.n_vox:
            c.rest = np.asarray(rest, dtype=np.float64).reshape(-1, 8, 3)
        c.rebuild()
        if data.get("anchor"):
            c.anchor = {"city": data["anchor"]}    # 軌道上の街に固定（位置は読み込み後に合わせる）
        return c

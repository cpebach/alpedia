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


def _clean_dev(d, reg=None):
    """保存する形の機器の情報（計算中だけの項目を除く）。"""
    if not d:
        return None
    dd = reg.device(d["def"]) if reg is not None else None
    st = devstate.to_saved(dd["type"], d["state"]) if dd else \
        {k: v for k, v in d["state"].items() if not k.startswith("_")}
    return {"def": d["def"], "dir": d["dir"], "name": d["name"], "state": st}


def link_key(a, b):
    a, b = tuple(a), tuple(b)
    return (a, b) if a <= b else (b, a)


CONTACT_SOFT = 1.0

from . import devstate, fastphys  # noqa: E402
from .planet import fcross  # noqa: E402


def _native_planets(planets):
    """Rust に渡す惑星のリスト（Rust の地形が使えない惑星があれば None）。"""
    pls = []
    for pl in planets:
        nat = pl._nat()
        if nat is None:
            return None
        pls.append((nat, float(pl.bound), float(pl.lipschitz), float(pl.gm), float(pl.radius), bool(pl.has_water)))
    return pls


def native_core():
    from . import native
    return native.core


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
        self.ext_colliders = []      # 拡張（街など）が出した当たり判定の箱（ワールドがティックごとに入れる）
        self.ext_events = []         # それに当たった記録（拡張が after_physics で処理する）
        self.anchor = None           # 動く物体に固定：{"to": 相手のID}（中身は拡張が決める）

    # ------------------------------------------------------------------
    # 構築
    # ------------------------------------------------------------------
    @classmethod
    def from_voxels(cls, registry, voxels, origin, rotation=None, velocity=(0, 0, 0),
                    angular_velocity=(0, 0, 0), size=1.0, links=None, defer=False, **kw):
        """voxels: [(cell(tuple), material_id, device_dict|None), ...]
        defer=True：形の作り直し（rebuild）と回転の速度は呼んだ側がする（読み込みで2回作らないように）。"""
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
        if defer:
            return c
        c.rebuild()
        w = np.asarray(angular_velocity, dtype=np.float64)
        if np.any(w):
            com = c.center_of_mass()
            c.vel += np.cross(w, c.pos - com)
        return c

    def _init_device(self, dev):
        """機器の情報を決まった形にする（状態は devstate の表のとおりにそろえる）。"""
        from .actuator import migrate_device
        dev = migrate_device(dev)                # 古い「アクチュエーター」で押し出していた物はピストンに
        dd = self.reg.device(dev["def"])
        out = {"def": dev["def"], "dir": int(dev.get("dir", 2)), "name": dev.get("name") or (dd["name"] if dd else dev["def"])}
        if dd:
            out["state"] = devstate.normalize(dd["type"], dd.get("params", {}), dev.get("state"),
                                              where=f"（{self.name}/{out['name']}）")
        else:
            out["state"] = dict(dev.get("state") or {})      # 定義が無い（MOD を外した）：そのまま残す
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
        # 結合（ジョイント）：まとめて numpy で
        keys = list(self.links)
        idx = self.index
        pa = np.array([idx.get(k[0], -1) for k in keys], np.int64)
        pb = np.array([idx.get(k[1], -1) for k in keys], np.int64)
        cells_arr = np.asarray(self.cells, np.int64).reshape(-1, 3)
        ok = (pa >= 0) & (pb >= 0)
        dcode = np.full(len(keys), -1, np.int64)
        if ok.any():
            delta = cells_arr[pb[ok]] - cells_arr[pa[ok]]
            code = (delta[:, 0] + 1) * 9 + (delta[:, 1] + 1) * 3 + (delta[:, 2] + 1)
            lut = np.full(27, -1, np.int64)
            for d_, t_ in enumerate(DIR_TUPLES):
                lut[(t_[0] + 1) * 9 + (t_[1] + 1) * 3 + (t_[2] + 1)] = d_
            inrange = (np.abs(delta) <= 1).all(1)
            dc = np.full(len(delta), -1, np.int64)
            dc[inrange] = lut[code[inrange]]
            dcode[np.nonzero(ok)[0]] = dc
        good = dcode >= 0
        self.links = {keys[q] for q in np.nonzero(good)[0]}
        is_rotor = np.array([keys[q] in rotor_of for q in range(len(keys))], bool) if rotor_of else np.zeros(len(keys), bool)
        for q in np.nonzero(good & is_rotor)[0]:
            key = keys[q]
            ia, ib = int(pa[q]), int(pb[q])
            i_act, d_act, dv = rotor_of[key]
            i_out = ib if ia == i_act else ia
            self.v_plast[i_out] = 0.0
            self.rotors.append({"a": i_act, "b": i_out, "d": d_act, "dev": dv, "key": key,
                                "br": max(min(self.v_break[ia], self.v_break[ib]) * 3, 0.3 * self.size)})
        sel = np.nonzero(good & ~is_rotor)[0]
        A, B, D = pa[sel], pb[sel], dcode[sel]
        JP = JOINT_PAIRS[D] if len(sel) else np.zeros((0, 4, 2), np.int64)
        ja = (8 * A[:, None] + JP[:, :, 0]).ravel()
        jb = (8 * B[:, None] + JP[:, :, 1]).ravel()
        k_ = np.minimum(self.v_jstiff[A], self.v_jstiff[B]) if len(sel) else np.zeros(0)
        br_ = np.minimum(self.v_break[A], self.v_break[B]) if len(sel) else np.zeros(0)
        jr = np.zeros(len(ja))
        jk = np.repeat(k_, 4)
        jbrk = np.repeat(br_, 4)
        jkey = [keys[q] for q in sel for _ in range(4)]
        xa, xb, xr, xk, xbr = [], [], [], [], []
        for key, lst in list(self.extras.items()):
            ia, ib = idx.get(key[0]), idx.get(key[1])
            if ia is None or ib is None:
                del self.extras[key]
                continue
            k = min(self.v_jstiff[ia], self.v_jstiff[ib])
            br = min(self.v_break[ia], self.v_break[ib])
            for ca, cb, rest in lst:
                xa.append(8 * ia + ca); xb.append(8 * ib + cb); xr.append(rest)
                xk.append(k); xbr.append(br + rest); jkey.append(("x",) + key)
        self.ja = np.concatenate([ja, np.array(xa, np.int64)]).astype(np.int64)
        self.jb = np.concatenate([jb, np.array(xb, np.int64)]).astype(np.int64)
        self.jr = np.concatenate([jr, np.array(xr, np.float64)])
        self.jk = np.concatenate([jk, np.array(xk, np.float64)])
        self.jbrk = np.concatenate([jbrk, np.array(xbr, np.float64)])
        self.jkey = jkey
        nv = 8 * n
        cnt = np.bincount(self.ja, minlength=nv) + np.bincount(self.jb, minlength=nv)
        self.jcount = np.maximum(cnt, 1).astype(np.float64)
        # 表面（描画・OBJ出力用）：結合していない向きの面
        linked = np.zeros((n, 6), bool)
        gl = np.nonzero(good)[0]
        if len(gl):
            linked[pa[gl], dcode[gl]] = True
            linked[pb[gl], dcode[gl] ^ 1] = True          # 反対の向き（+X↔-X など）
        fv, fd = np.nonzero(~linked)
        self.face_vox = fv.astype(np.int64)
        self.face_dir = fd.astype(np.int64)
        self.face_idx = (8 * self.face_vox[:, None] + FACE_QUADS[self.face_dir]) if len(fv) else np.zeros((0, 4), np.int64)
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

    # ------------------------------------------------------------------
    # アンカー：アンカーのブロックがある構築物は惑星（ワールド）に固定。物理計算しない（当たり判定はある）。
    # アクチュエーターの先の部分は動ける（アンカーのある部分だけが固定）。アンカーを失えば普通に動く
    # ------------------------------------------------------------------
    def anchor_index(self):
        """アンカーのブロックの番号の配列（形が変わったときだけ数え直す）。"""
        cache = getattr(self, "_anchor_cache", None)
        if cache is None or cache[0] != (self.version, self.n_vox):
            idx = [i for i, d in enumerate(self.devs)
                   if d and (self.reg.device(d["def"]) or {}).get("type") == "anchor"]
            cache = self._anchor_cache = ((self.version, self.n_vox), np.array(idx, dtype=np.int64))
        return cache[1]

    @property
    def anchored(self):
        return len(self.anchor_index()) > 0

    def actuator_locked(self, i):
        """アクチュエーター i の両側（本体の部分と出力の先の部分）が、どちらもアンカーで固定されている（動かせない）か。"""
        anc = self.anchor_index()
        if not len(anc) or not self.devs[i]:
            return False
        from .actuator import output_cell
        j = self.index.get(output_cell(self, i))
        if j is None:
            return False
        self.part_of(i)                                  # 部分の番号を最新に
        label = self._part_cache[1]
        fixed = set(label[anc].tolist())
        return int(label[i]) in fixed and int(label[j]) in fixed

    def _pose_key(self):
        """形と置き場所が変わっていなければ同じになる値（止まっている物の計算の使い回しに）。"""
        if not self.n_vox:
            return (self.version, 0)
        p = self.pos
        return (self.version, self.n_vox, self.mode, float(p[0, 0]), float(p[0, 1]), float(p[-1, 2]), float(p[-1, 0]))

    def centers_cached(self):
        """ブロックの中心。止まっている物（アンカーで固定・眠っている）は計算を使い回す。"""
        if not (self.is_static or self.asleep):
            return self.voxel_centers()
        k = self._pose_key()
        cache = getattr(self, "_ctr_cache", None)
        if cache is None or cache[0] != k:
            cache = self._ctr_cache = (k, self.voxel_centers())
        return cache[1]

    def frames_cached(self):
        if not (self.is_static or self.asleep):
            return self.voxel_frames()
        k = self._pose_key()
        cache = getattr(self, "_frm_cache", None)
        if cache is None or cache[0] != k:
            cache = self._frm_cache = (k, self.voxel_frames())
        return cache[1]

    def bounds(self):
        """外側の球 (中心, 半径)。止まっている物は使い回す。"""
        k = self._pose_key() if (self.is_static or self.asleep) else None
        cache = getattr(self, "_bnd_cache", None)
        if k is not None and cache is not None and cache[0] == k:
            return cache[1]
        if not self.n_vox:
            b = (np.zeros(3), 0.0)
        else:
            lo, hi = self.pos.min(0), self.pos.max(0)
            b = ((lo + hi) * 0.5, float(np.linalg.norm(hi - lo)) * 0.5)
        self._bnd_cache = (k, b)
        return b

    PASSIVE_DEVICES = {"anchor", "pipe", "tank", "valve", "battery", "seat", "generator", "antenna", "computer"}

    # 固定された物では、ときどき（0.25 秒ごと）まとめて処理すれば足りる機器（ライト：点いているか・電気の消費だけ）
    SLOW_DEVICES = {"light"}

    def is_quiet(self, slow_ok=False):
        """止まっていて、毎ティック処理する機器が無い（固定された建物の看板・ソーラーパネルなど。動いているプログラムも無い）。
        slow_ok：ライトのように、ときどきまとめて処理すれば足りる機器だけなら True。"""
        if not self.is_static:
            return False
        cache = getattr(self, "_quiet_cache", None)
        if cache is None or cache[0] != (self.version, self.n_vox):
            types = [(i, (self.reg.device(d["def"]) or {}).get("type")) for i, d in enumerate(self.devs) if d]
            passive = all(t in self.PASSIVE_DEVICES for _, t in types)
            slow = all(t in self.PASSIVE_DEVICES or t in self.SLOW_DEVICES for _, t in types)
            comps = [i for i, t in types if t == "computer"]
            cache = self._quiet_cache = ((self.version, self.n_vox), passive, comps, slow)
        if not (cache[3] if slow_ok else cache[1]):
            return False
        return not any(self.devs[i]["state"].get("running") for i in cache[2])

    @property
    def has_devices(self):
        cache = getattr(self, "_hasdev", None)
        if cache is None or cache[0] != (self.version, self.n_vox):
            cache = self._hasdev = ((self.version, self.n_vox), any(d for d in self.devs))
        return cache[1]

    def lattice_pose(self):
        """(R, t)：格子の座標（セル × 大きさ）→ ワールド。剛体なら剛体の姿勢、止まっている物は使い回す。"""
        if self.mode in ("rigid", "rails") and self.rb is not None and not (self.is_static or self.asleep):
            return self.rigid_fit()
        k = self._pose_key() if (self.is_static or self.asleep) else None
        cache = getattr(self, "_lat_cache", None)
        if k is not None and cache is not None and cache[0] == k:
            return cache[1]
        out = self.rigid_fit()
        self._lat_cache = (k, out)
        return out

    LOD_CELL = 4                 # 遠くで見せる箱：この数のブロックごとにまとめる

    def lod_boxes(self):
        """遠くから見せる「それっぽい色の箱」：(中心 (k,3), 半分の大きさ (k,3), 色 (k,3))。格子の座標（セル単位）。
        LOD_CELL ブロック角ごとに、中にあるブロックをちょうど囲む箱を1つ、いちばん多い色で。形が変わったときだけ作り直す。"""
        cache = getattr(self, "_lod_cache", None)
        if cache is not None and cache[0] == (self.version, self.n_vox):
            return cache[1]
        if not self.n_vox:
            out = (np.zeros((0, 3)), np.zeros((0, 3)), np.zeros((0, 3)))
        else:
            cells = np.asarray(self.cells, dtype=np.int64)
            cols = []
            for m, d in zip(self.mats, self.devs):
                if d:
                    cols.append((self.reg.device(d["def"]) or {}).get("color", [0.6, 0.6, 0.6]))
                else:
                    cols.append((self.reg.material(m) or {}).get("color", [0.6, 0.6, 0.6]))
            cols = np.asarray(cols, dtype=np.float64)
            key = np.floor_divide(cells, self.LOD_CELL)
            _, inv = np.unique(key, axis=0, return_inverse=True)
            inv = inv.ravel()
            G = int(inv.max()) + 1
            lo = np.full((G, 3), np.iinfo(np.int64).max)
            hi = np.full((G, 3), np.iinfo(np.int64).min)
            np.minimum.at(lo, inv, cells)
            np.maximum.at(hi, inv, cells + 1)
            # 色：素材ごとの数でいちばん多いもの（同数なら平均）
            mid = np.array([hash((m, (d or {}).get("def"))) for m, d in zip(self.mats, self.devs)])
            col = np.zeros((G, 3))
            for g in range(G):
                sel = np.nonzero(inv == g)[0]
                u, cnt = np.unique(mid[sel], return_counts=True)
                best = u[cnt == cnt.max()]
                col[g] = cols[sel[np.isin(mid[sel], best)]].mean(0)
            out = ((lo + hi) * 0.5, (hi - lo) * 0.5, col)
        self._lod_cache = ((self.version, self.n_vox), out)
        return out

    FIXED_INV = 1e-12             # 固定のブロックの、押し出しでの質量の逆数（ほぼ動かない。0 だと割り算できない）

    def collision_inv(self):
        """構築物どうしの押し出しに使う、ブロックごとの質量の逆数。アンカーで固定されたブロックはほぼ 0。"""
        v = self.inv_mass[::8]
        if not self.anchored:
            return v
        v = v.copy()
        a = getattr(self, "art", None)
        if self.mode == "art" and a is not None and a.get("n") == self.n_vox and a["fixed"].any():
            v[a["fixed"][a["gid"]]] = self.FIXED_INV
        else:
            v[:] = self.FIXED_INV
        return v

    @property
    def is_static(self):
        """物理計算をせず、その場に止まっている（アンカーで固定・アクチュエーターも止まっている）。"""
        return self.anchored and self.mode != "art"

    def voxel_center(self, i):
        """ブロック i の中心（ワールド座標）。"""
        return self.pos[8 * i:8 * i + 8].mean(0)

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

    def substep(self, h, iters, gravity_fn, planets, gscale=1.0):
        """単独で1サブステップ進める（テスト用）。"""
        self.substep_begin(h, iters, gravity_fn, planets, gscale)
        return self.substep_end(h)

    def substep_begin(self, h, iters, gravity_fn, planets, gscale=1.0):
        """1サブステップの前半（Rust）。gscale：重力倍率（重力は惑星から Rust で計算する。gravity_fn は昔の名残り）。"""
        if not self.n_vox:
            return
        if gscale is None:
            gscale = 1.0
        if self.mode == "art":
            self._native_art_begin(h, planets, gscale)
        else:
            self._native_begin(h, iters, planets, gscale)

    def substep_end(self, h):
        if not self.n_vox:
            return False
        if self.mode == "art":
            if getattr(self, "_nb_art", False):
                self._nb_art = False
                self._native_art_end(h)
            return False
        if getattr(self, "_nb_used", False):
            self._nb_used = False
            return self._native_end(h)
        return False

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


    def _build_contacts(self):
        """各頂点の当たり判定に使う「芯」（点・線・三角形）を選ぶ（Rust の build_contacts）。
        芯の最近点から半径 r が見た目の表面。周りを全部ボクセルに囲まれた頂点（内部）は判定しない。"""
        self.ctype, self.cidx = native_core().build_contacts(
            np.asarray(self.cells, np.int64).reshape(-1, 3), np.ascontiguousarray(self.pos, dtype=np.float64),
            np.ascontiguousarray(self.voxel_frames(), dtype=np.float64),
            np.ascontiguousarray(self.voxel_centers(), dtype=np.float64),
            np.ascontiguousarray(self.topo["segs"], dtype=np.int64).reshape(-1, 2),
            np.ascontiguousarray(self.topo["tris"], dtype=np.int64).reshape(-1, 3))

    def contact_cores(self):
        """各頂点に対応する芯の最近点（この点から半径 r が見た目の表面）。"""
        return fastphys.contact_cores(self.pos, self.ctype, self.cidx)


    def _ground_ref(self, pl):
        """地面から高い頂点を省くための (上向き, その方向の地表半径)。重心が惑星の中心なら (None, 0)。"""
        com = self.pos.mean(0) - pl.center
        cn = float(np.linalg.norm(com))
        if cn <= 1e-6:
            return None, 0.0
        up = com / cn
        gc = getattr(self, "_ground_cache", None)
        if gc is not None and gc[0] is pl and float(np.abs(gc[1] - up).max()) * pl.radius < 0.5:
            return up, gc[2]
        Rs = float(pl.surface_radius(up[None])[0])
        self._ground_cache = (pl, up, Rs)
        return up, Rs

    def _collide_planet(self, pl, collect=False):
        """地面との当たり判定（Rust）：頂点ごとに対応する芯の点を、半径 r の球として地面と判定する。
        collect なら押し戻さずに (頂点, めり込み, 法線) を返す。"""
        nat = pl._nat()
        K = self.contact_cores()
        r = self.size * 0.5
        up, Rs = self._ground_ref(pl)
        if not (self.pos.flags.c_contiguous and self.pos.flags.writeable):
            self.pos = np.ascontiguousarray(self.pos)
        c_ = np.ascontiguousarray
        want, hi, dep, n = nat.collide(self.pos, c_(self._prev), c_(self.vel), c_(K), c_(self.fric), r, float(pl.bound),
                                       float(pl.lipschitz), None if up is None else [float(x) for x in up], Rs,
                                       self.mode == "rigid", float(self.IMPACT_SPEED), float(CONTACT_SOFT),
                                       not collect)
        if want:
            self._want_soft = True
        if collect and len(hi):
            return hi, dep, n
        return None

    def _collide_boxes(self, bs, h, collect=False):
        """拡張が出した向き付きの箱（街の建物など）との当たり判定。
        強くぶつかった速さと、動いている物体を押した力積は ext_events に記録する（拡張が after_physics で使う）。"""
        from .geom import box_sdf
        owner, C, A, E, idx = bs
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
        moving = owner.is_moving()
        vs = owner.point_velocity(self.pos[hi_]) if moving else np.zeros((len(hi_), 3))
        vn = -np.einsum("ij,ij->i", self.vel[hi_] - vs, n)
        # 建物の損傷（いちばん強く当たった速さ）
        if (vn >= 7.0).any():
            best = {}
            for pa, v in zip(parts.tolist(), vn.tolist()):
                if v > best.get(pa, 0.0):
                    best[pa] = v
            self.ext_events.append((owner, best, None, None))
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
            self.ext_events.append((owner, None, -J, self.pos[hi_]))
        return None


    # ------------------------------------------------------------------
    # Rust で1サブステップをまとめて計算する（native/alpedia_core の Body）
    # ------------------------------------------------------------------
    def _native_body(self):
        """この構築物の Rust の Body（形が変わったら作り直す）。"""
        core = native_core()
        from . import actuator as act
        hk = np.array([list(act.hinge_of(r["dev"], self.reg) or (-1, -1)) for r in self.rotors], np.int64).reshape(-1, 2)
        key = (self.version, id(core), self.n_vox, hk.tobytes())     # ヒンジの側が変わっても作り直す
        nb = getattr(self, "_nb_cache", None)
        if nb is not None and nb[0] == key:
            return nb[1]
        i64 = lambda a: np.ascontiguousarray(a, dtype=np.int64)  # noqa: E731
        f64 = lambda a: np.ascontiguousarray(a, dtype=np.float64)  # noqa: E731
        R = len(self.rotors)
        ra = np.zeros(R, np.int64)
        rva = np.zeros((R, 4), np.int64)
        rvb = np.zeros((R, 4), np.int64)
        rbr = np.zeros(R)
        for q, r in enumerate(self.rotors):
            pairs = JOINT_PAIRS[r["d"]]
            ra[q] = r["a"]
            rva[q] = 8 * r["a"] + pairs[:, 0]
            rvb[q] = 8 * r["b"] + pairs[:, 1]
            rbr[q] = r["br"]
        body = core.Body(float(self.size), i64(self.ja), i64(self.jb), f64(self.jr), f64(self.jk), f64(self.jbrk),
                         f64(self.jcount), f64(self.inv_mass), f64(self.mass), f64(self.damp), f64(self.fric),
                         f64(self.v_stiff), f64(self.v_plast), f64(self.v_yield), f64(self.v_density),
                         i64(self.ctype), i64(self.cidx), ra, rva, rvb, rbr, hk)
        self._nb_cache = (key, body)
        return body

    def native_proxy(self):
        """構築物どうしの押し出しに使う Rust の Proxy（形が変わったら作り直す）。"""
        vinv = self.collision_inv()
        key = (self.version, self.n_vox, id(native_core()), vinv.tobytes() if self.anchored else None)
        px = getattr(self, "_nproxy", None)
        if px is not None and px[0] == key:
            return px[1]
        W = self.proxW.tocsr()
        W.sort_indices()
        n = self.n_vox
        pairs = []
        for key_ in list(self.links) + list(self.extras):
            ia, ib = self.index.get(key_[0]), self.index.get(key_[1])
            if ia is not None and ib is not None:
                pairs.append((ia, ib))
        links = np.asarray(pairs, np.int64).reshape(-1, 2)
        inv = np.ascontiguousarray(W @ vinv, dtype=np.float64)
        obj = native_core().Proxy(n, float(self.size), np.ascontiguousarray(W.indptr, dtype=np.int64),
                                  np.ascontiguousarray(W.indices, dtype=np.int64),
                                  np.ascontiguousarray(W.data, dtype=np.float64), inv, links,
                                  np.ascontiguousarray(self.mass, dtype=np.float64))
        self._nproxy = (key, obj)
        return obj

    def _rotor_angles(self):
        return [float(np.radians(float(r["dev"]["state"].get("angle", 0.0)))) for r in self.rotors]

    def _contig(self, *names):
        """Rust に渡す配列を、連続した書き込める float64 にそろえる。"""
        for k in names:
            a = getattr(self, k)
            if not (isinstance(a, np.ndarray) and a.dtype == np.float64 and a.flags.c_contiguous and a.flags.writeable):
                setattr(self, k, np.array(a, dtype=np.float64, order="C"))

    def _native_begin(self, h, iters, planets, gscale):
        """substep_begin（ブロックごと・剛体）を Rust で。"""
        body = self._native_body()
        pls = _native_planets(planets)
        rigid = self.mode == "rigid"
        full = rigid and getattr(self, "_full_rigid", False)
        if getattr(self, "_prev", None) is None or self._prev.shape != self.pos.shape:
            self._prev = self.pos.copy()
        self._contig("pos", "vel", "_prev", "force")
        cav = self._cavity_neighbors() if any(p[5] for p in pls) else None
        cav = None if cav is None else np.ascontiguousarray(cav[0], dtype=np.int64)
        if rigid:
            rb = self.rb
            defer = bool(full and self.ext_colliders)
            args = (self.pos, self.vel, self._prev, self.force, np.ascontiguousarray(rb["R"], dtype=np.float64),
                    np.ascontiguousarray(rb["Ib"], dtype=np.float64), pls, gscale, h, cav, bool(full),
                    float(self.IMPACT_SPEED), float(CONTACT_SOFT))
            want = body.rigid_begin(*args, True) if defer else body.rigid_begin(*args)
            if want:
                self._want_soft = True
            if defer:
                # 完全剛体で MOD の箱（街の建物など）に触れうるとき：箱との接触は MOD のプログラムで集め、地面と一緒に解く
                self._native_full_contacts(body, pls, h)
                self._nb_used = True
                return True
        else:
            self._ensure_lastR()
            self._contig("rest", "act_scale", "_last_R", "_quat")
            par = self.n_vox >= fastphys.PARALLEL_MIN_VOXELS and not self._in_pool
            body.soft_begin(self.pos, self.vel, self._prev, self.force, self.rest, self.act_scale, self._last_R,
                            self._quat, pls, gscale, h, int(iters), self._rotor_angles(), bool(par), cav,
                            float(CONTACT_SOFT))
        for bs in self.ext_colliders:
            self._collide_boxes(bs, h)
        self._nb_used = True
        return True

    # ---- 1ティックの物理をまとめて Rust で（world._native_tick） ----
    def tick_job(self, groups, planets_ok):
        """world._native_tick に渡す、この構築物の配列（dict）。まとめて計算できないときは None
        （MOD の箱の持ち主が動いている など）。
        groups：MOD の箱のまとまりの番号を決める {id(C): 番号} と、そのリスト（ここで足す）。"""
        if not planets_ok or not self.n_vox or self.mode not in ("soft", "rigid", "art"):
            return None
        body = self._native_body()
        if body is None:
            return None
        idx, lst = groups
        boxes = []
        for owner, C, A, E, part in self.ext_colliders:
            if owner.is_moving():
                return None                      # 動いている街：サブステップごとに進めて押した反動を拡張に渡す
            k = idx.get(id(C))
            if k is None:
                k = idx[id(C)] = len(lst)
                lst.append((np.ascontiguousarray(C, dtype=np.float64), np.ascontiguousarray(A, dtype=np.float64),
                            np.ascontiguousarray(E, dtype=np.float64), np.ascontiguousarray(part, dtype=np.int64)))
            boxes.append(k)
        if getattr(self, "_prev", None) is None or self._prev.shape != self.pos.shape:
            self._prev = self.pos.copy()
        self._contig("pos", "vel", "_prev", "force")
        job = {"body": body, "proxy": self.native_proxy(), "pos": self.pos, "vel": self.vel, "prev": self._prev,
               "force": self.force, "boxes": boxes, "cav": None,
               "parallel": bool(self.n_vox >= fastphys.PARALLEL_MIN_VOXELS)}
        if self.mode == "art":
            a = getattr(self, "art", None)
            if a is None or a["version"] != self.version or a["n"] != self.n_vox:
                self._art_build()
                a = self.art
            if a.get("nat") is None:
                return None
            self._art_contig()
            ex = a["exts"]
            st = [self.devs[e[0]]["state"] if self.devs[e[0]] else {} for e in ex]
            job.update(kind=3, topo=a["nat"], x=a["X"], r=a["R"], vl=a["vl"], vl0=a["vl0"], applied=a["applied"],
                       ext_applied=a["ext_applied"],
                       want=[float(dv["state"].get("angle", 0.0)) for dv in a["devs"]],
                       rot_speed=[float(dv["state"].get("rot_speed", 90.0)) for dv in a["devs"]],
                       ext_want=[float(s_.get("ext", e[3])) for s_, e in zip(st, ex)],
                       ext_speed=[float(s_.get("ext_speed", 0.5)) for s_ in st])
        elif self.mode == "rigid":
            rb = self.rb
            if rb is None:
                return None
            for k in ("R", "Ib", "local", "x"):
                v = rb[k]
                if not (isinstance(v, np.ndarray) and v.dtype == np.float64 and v.flags.c_contiguous and v.flags.writeable):
                    rb[k] = np.array(v, dtype=np.float64, order="C")
            full = bool(getattr(self, "_full_rigid", False))
            job.update(kind=2 if full else 1, rb_r=rb["R"], rb_ib=rb["Ib"], local=rb["local"], rb_x=rb["x"])
        else:
            self._ensure_lastR()
            self._contig("rest", "act_scale", "_last_R", "_quat")
            job.update(kind=0, rest=self.rest, act_scale=self.act_scale, last_r=self._last_R, quat=self._quat,
                       angles=self._rotor_angles())
        return job

    def tick_broken(self, broken, rot):
        """Rust でまとめて計算した間にちぎれた結合を外す（_native_end と同じ）。"""
        for q in list(rot):
            self.links.discard(self.rotors[q]["key"])
        for j in list(broken):
            key = self.jkey[j]
            if key[0] == "x":
                self.extras.pop(key[1:], None)
            else:
                self.links.discard(key)
        self.rebuild()

    # ---- 関節つき剛体（完全剛体モードでアクチュエーターが動く間）の Rust 版 ----
    def _native_art_topo(self):
        core = native_core()
        a = self.art
        i64 = lambda x: np.ascontiguousarray(x, dtype=np.int64)  # noqa: E731
        f64 = lambda x: np.ascontiguousarray(x, dtype=np.float64)  # noqa: E731
        ex = a["exts"]
        return core.ArtTopo(i64(a["vg"]), f64(a["invM"]), f64(a["IinvB"]), i64(a["ra"]), i64(a["rb"]),
                            i64(a["A"]).reshape(-1, 4), i64(a["B"]).reshape(-1, 4), i64(a["C"]).reshape(-1, 8),
                            i64([e[1] for e in ex]).reshape(-1, 4), f64([e[2] for e in ex]).reshape(-1, 3),
                            f64([e[3] for e in ex]), int(self.ART_ITERS), int(self.ART_CONTACTS),
                            float(self.MOTOR_STEP_X), float(self.MOTOR_HOLD_DEG),
                            i64(a["HK"]).reshape(-1, 2),
                            **({"fixed": i64(a["fixed"]), "xfix": f64(a["Xfix"])} if a["fixed"].any() else {}))

    def _art_contig(self):
        a = self.art
        for k in ("X", "R", "vl", "vl0", "applied", "ext_applied"):
            v = a[k]
            if not (isinstance(v, np.ndarray) and v.dtype == np.float64 and v.flags.c_contiguous and v.flags.writeable):
                a[k] = np.array(v, dtype=np.float64, order="C")

    def _native_art_begin(self, h, planets, gscale):
        """関節つき剛体の1サブステップの前半（Rust）。MOD の箱との接触だけ Python で集めて渡す（MOD のプログラムを呼ぶため）。"""
        a = getattr(self, "art", None)
        if a is None or a["version"] != self.version or a["n"] != self.n_vox:
            self._art_build()
            a = self.art
        nat, body = a.get("nat"), self._native_body()
        pls = _native_planets(planets)
        if getattr(self, "_prev", None) is None or self._prev.shape != self.pos.shape:
            self._prev = self.pos.copy()
        self._contig("pos", "vel", "_prev", "force")
        self._art_contig()
        cav = self._cavity_neighbors() if any(p[5] for p in pls) else None
        cav = None if cav is None else np.ascontiguousarray(cav[0], dtype=np.int64)
        ex = a["exts"]
        st = [self.devs[e[0]]["state"] if self.devs[e[0]] else {} for e in ex]
        ext_want = [float(s_.get("ext", e[3])) for s_, e in zip(st, ex)]
        ext_speed = [float(s_.get("ext_speed", 0.5)) for s_ in st]
        rows, d = nat.predict(body, self.pos, self.vel, self._prev, self.force, a["X"], a["R"], a["vl"], a["vl0"],
                              a["ext_applied"], ext_want, ext_speed, pls, gscale, h, cav)
        a["defo"] = [(rows, d)] if len(rows) else []
        # MOD の箱（街の建物など）との接触は、MOD のプログラムを呼ぶので Python で集める
        boxes = list(self.ext_colliders)
        P, D, N, VS, CI = [], [], [], [], []
        for bi, bs in enumerate(boxes):
            res = self._collide_boxes(bs, h, collect=True)
            if res:
                P.append(res[0]); D.append(res[1]); N.append(res[2]); VS.append(res[3])
                CI.append(np.full(len(res[0]), bi))
        cat = lambda L, sh, dt: (np.ascontiguousarray(np.concatenate(L), dtype=dt) if L  # noqa: E731
                                 else np.zeros(sh, dt))
        want = [float(dv["state"].get("angle", 0.0)) for dv in a["devs"]]
        spd = [float(dv["state"].get("rot_speed", 90.0)) for dv in a["devs"]]
        cg, cl, cn, lam, cp, ci = nat.solve(body, self.pos, self._prev, a["X"], a["R"], a["vl"], a["applied"], want,
                                            spd, pls, h, cat(P, 0, np.int64), cat(D, 0, np.float64),
                                            cat(N, (0, 3), np.float64), cat(VS, (0, 3), np.float64),
                                            cat(CI, 0, np.int64))
        a["contacts"] = (cg, cl, cn, lam)
        # 動いている街（ステーション）を押した分の反動
        if len(ci) and (ci >= 0).any():
            for bi in np.unique(ci[ci >= 0]):
                m_ = (ci == bi) & (lam > 0)
                if m_.any():
                    J = cn[m_] * (lam[m_] / h)[:, None]
                    self.ext_events.append((boxes[bi][0], None, -J, self.pos[cp[m_]]))
        self._nb_art = True
        return True

    def _native_art_end(self, h):
        a = self.art
        self._contig("pos", "vel", "_prev")
        self._art_contig()
        defo = a.get("defo") or []
        rows = defo[0][0] if defo else np.zeros(0, np.int64)
        d = defo[0][1] if defo else np.zeros((0, 3))
        ct = a.get("contacts")
        if ct is None:
            ct = (np.zeros(0, np.int64), np.zeros((0, 3)), np.zeros((0, 3)), np.zeros(0))
        a["nat"].end(self._native_body(), self.pos, self.vel, self._prev, a["X"], a["R"], a["vl"], rows, d, *ct, h)
        a["defo"] = []
        a["contacts"] = None

    def _native_full_contacts(self, body, pls, h):
        boxes = list(self.ext_colliders)
        P, D, N, VS, CI = [], [], [], [], []
        for bi, bs in enumerate(boxes):
            res = self._collide_boxes(bs, h, collect=True)
            if res:
                P.append(res[0]); D.append(res[1]); N.append(res[2]); VS.append(res[3])
                CI.append(np.full(len(res[0]), bi))
        cat = lambda L, sh, dt: (np.ascontiguousarray(np.concatenate(L), dtype=dt) if L  # noqa: E731
                                 else np.zeros(sh, dt))
        want, cp, cn, lam, ci, pts = body.rigid_contacts(
            self.pos, self._prev, self.vel, pls, h, float(self.IMPACT_SPEED), float(CONTACT_SOFT),
            cat(P, 0, np.int64), cat(D, 0, np.float64), cat(N, (0, 3), np.float64), cat(VS, (0, 3), np.float64),
            cat(CI, 0, np.int64))
        if want:
            self._want_soft = True
        # 動いている街（ステーション）を押した分の反動
        if len(ci) and (ci >= 0).any():
            for bi in np.unique(ci[ci >= 0]):
                m_ = (ci == bi) & (lam > 0)
                if m_.any():
                    J = cn[m_] * (lam[m_] / h)[:, None]
                    self.ext_events.append((boxes[bi][0], None, -J, pts[m_]))

    def _native_end(self, h):
        if self.mode == "rigid":
            rb = self.rb
            rb["local"] = np.ascontiguousarray(rb["local"], dtype=np.float64)
            self._contig("pos", "vel", "_prev")
            R, x = native_core().rigid_end(self.pos, self.vel, self._prev, rb["local"], np.ascontiguousarray(self.mass), h)
            rb["R"], rb["x"] = R, x
            return False
        body = self._native_body()
        self._contig("rest")
        lastR = self._last_R if hasattr(self, "_last_R") else np.zeros(0)
        broken, rot = body.soft_end(self.pos, self.vel, self._prev, self.rest, self.act_scale, lastR, h,
                                    self._rotor_angles())
        broke = False
        for q in rot.tolist():
            self.links.discard(self.rotors[q]["key"])
            broke = True
        for j in broken.tolist():
            key = self.jkey[j]
            if key[0] == "x":
                self.extras.pop(key[1:], None)
            else:
                self.links.discard(key)
            broke = True
        if broke:
            self.rebuild()
        return broke

    def _ensure_lastR(self):
        if not hasattr(self, "_last_R") or len(self._last_R) != self.n_vox:
            self._last_R = np.tile(np.eye(3), (self.n_vox, 1, 1))
        if getattr(self, "_allvox", None) is None or len(self._allvox) != self.n_vox:
            self._allvox = np.arange(self.n_vox, dtype=np.int64)
        if getattr(self, "_quat", None) is None or len(self._quat) != self.n_vox:
            self._quat = np.zeros((self.n_vox, 4))
            self._quat[:, 0] = 1.0


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


    def update_mode(self, dt, world):
        """毎ティック：ブロックごと ⇔ 剛体 ⇔ レール（遠方の軌道計算）を切り替える。"""
        if not self.n_vox or self.asleep:
            return
        phys = world.cfg.get("physics", {})
        # 完全剛体モードでも soft_ok の構築物（物語の見せ場の落石・木箱など）は、曲がる・ちぎれる
        self._full_rigid = bool(world.settings.get("full_rigid", False)) and not getattr(self, "soft_ok", False)
        if self.anchored:                    # アンカーで固定：柔らかいブロックのモードでも剛体（＋関節）として扱う
            self._full_rigid = True
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

    def dev_ids(self, kind=None):
        """機器のあるブロックの番号（kind が種類なら、その種類だけ）。形が変わったときだけ数え直す。"""
        key = (self.version, len(self.cells))
        cache = self.__dict__.get("_devid_cache")
        if cache is None or cache[0] != key:
            by = {None: []}
            for i, d in enumerate(self.devs):
                if d:
                    by[None].append(i)
                    by.setdefault((self.reg.device(d["def"]) or {}).get("type"), []).append(i)
            cache = self._devid_cache = (key, by)
        return cache[1].get(kind, ())

    def actuator_indices(self):
        """アクチュエーター（ヒンジなども）の番号（形が変わったときだけ数え直す）。"""
        cache = self.__dict__.get("_act_cache")
        key = (self.version, len(self.cells))
        if cache is None or cache[0] != key:
            cache = self._act_cache = (key, [i for i, d in enumerate(self.devs)
                                             if d and (self.reg.device(d["def"]) or {}).get("type") == "actuator"])
        return cache[1]

    def _actuator_busy(self):
        from . import actuator as act
        devs = self.devs
        for i in self.actuator_indices():
            if act.busy(devs[i]["state"]):
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
        if self.anchored:
            if self.mode == "rails":
                self.leave_rails()
            return
        if self.mode == "rigid" and phys.get("rails_enabled", True):
            if self._rails_ok(world, float(phys.get("rails_distance", 400.0))):
                self.enter_rails()
        elif self.mode == "rails":
            if not self._rails_ok(world, float(phys.get("rails_distance", 400.0)) * 0.9):
                self.leave_rails()

    # ------------------------------------------------------------------
    # 完全剛体モード＋アクチュエーター：アクチュエーターの前後で分けた部分ごとの剛体を関節でつなぐ
    # ・部分 = アクチュエーターの出力面の結合を除いてつながっているブロックのまとまり
    # ・関節 = 出力面の4隅を回転角だけ回した位置に、出力側の部分の4隅を合わせる（Rust の ArtTopo）
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
        # アンカーのある部分は固定（動かない）。固定どうしをつなぐアクチュエーターは動かせない
        fixed = np.zeros(G, bool)
        anc = self.anchor_index()
        if len(anc):
            fixed[np.unique(gid[anc])] = True
        ra, rb, A, B, C, devs, HK = [], [], [], [], [], [], []
        for r in self.rotors:
            a, b = int(gid[r["a"]]), int(gid[r["b"]])
            if a == b or (fixed[a] and fixed[b]):
                continue                 # 輪になっていて回せない（固定されたまま）
            pairs = JOINT_PAIRS[r["d"]]
            ra.append(a); rb.append(b)
            A.append(8 * r["a"] + pairs[:, 0]); B.append(8 * r["b"] + pairs[:, 1])
            C.append(np.arange(8 * r["a"], 8 * r["a"] + 8))
            devs.append(r["dev"])
            hp = act.hinge_of(r["dev"], self.reg)
            HK.append(list(hp) if hp is not None else [-1, -1])
        self.art = {"version": self.version, "n": self.n_vox, "gid": gid, "vg": vg, "G": G,
                    "vl0": vl.copy(), "vl": vl, "invM": 1.0 / M, "IinvB": IinvB, "Ib": Ib,
                    "X": X, "R": np.tile(np.eye(3), (G, 1, 1)), "exts": exts,
                    "ra": np.array(ra, np.int64), "rb": np.array(rb, np.int64),
                    "A": np.array(A, np.int64).reshape(-1, 4), "B": np.array(B, np.int64).reshape(-1, 4),
                    "C": np.array(C, np.int64).reshape(-1, 8), "devs": devs,
                    "applied": np.array([float(d["state"].get("angle", 0.0)) for d in devs]),
                    "ext_applied": np.array([e[3] for e in exts], dtype=np.float64),
                    "gv": np.arange(len(vg), dtype=np.int64),
                    "HK": np.array(HK, np.int64).reshape(-1, 2), "fixed": fixed}
        if fixed.any():
            # 固定の部分は質量・慣性を無限大として解く（関節は動く側だけを動かす）。今の姿勢をそのまま保つ
            self.art["invM_s"] = np.where(fixed, 0.0, self.art["invM"])
            self.art["IinvB_s"] = np.where(fixed[:, None, None], 0.0, IinvB)
            self.art["Xfix"] = X.copy()
        self.art["nat"] = self._native_art_topo()

    def enter_art(self):
        self._art_build()
        self.mode = "art"
        self.rb = None
        self._want_soft = False


    # ------------------------------------------------------------------
    # 軽量化 2：レール（遠方の構築物は重心の軌道と回転だけを計算する）
    # ------------------------------------------------------------------
    def _rails_ok(self, world, dist):
        x = self.rb["x"] if self.rb is not None else self.center_of_mass()
        if world.player_distance(x) < dist:
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

    # ------------------------------------------------------------------
    # 外から位置を決める（拡張が、動く物体に固定した構築物を一緒に動かす）
    # ------------------------------------------------------------------
    def hold_in_frame(self, R, x, v, w):
        """座標系 (R, x) に今の形のまま固定する（形が変わったときだけ取り直す）。物理計算は止まる（mode="rails"）。"""
        f = getattr(self, "_frame_hold", None)
        if f is None or f["ver"] != self.version or self.rb is None or self.mode != "rails":
            local = (self.pos - x) @ R
            self._frame_hold = {"ver": self.version}
            self.mode = "rails"
            self.rb = {"local": local, "R": np.array(R, float), "x": np.array(x, float), "v": np.array(v, float),
                       "w": np.array(w, float), "Ib": np.eye(3), "ext": float(np.linalg.norm(local, axis=1).max())}
            self.rb_epoch += 1

    def move_with_frame(self, R, x, v, w):
        """固定した座標系が (R, x, 速度 v, 角速度 w) に動いた：一緒に動かす。"""
        self.hold_in_frame(R, x, v, w)
        rb = self.rb
        rb["R"], rb["x"], rb["v"], rb["w"] = (np.array(R, float), np.array(x, float), np.array(v, float),
                                              np.array(w, float))
        self._rails_sync()
        self.asleep = False
        self.sleep_epoch += 1

    def release_frame(self):
        """固定をやめて、ふつうの物理計算に戻す。"""
        self._frame_hold = None
        if self.mode == "rails":
            self.mode, self.rb = "soft", None

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
            if getattr(self, "soft_ok", False):
                sub.soft_ok = True              # 破片も柔らかいまま（完全剛体モードの中の見せ場）
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

    def snap_to_lattice(self, keep):
        """ブロック keep（番号の配列）の今の位置から格子（ローカル座標）の置き方を求め、残りのブロックを
        その格子の位置に置き直す（アンカーで固定された側に、固定されていない側を合わせる）。
        格子から離れた所に取り込んだブロック（格子がずれていた結合）はそのまま。"""
        keep = np.asarray(keep, dtype=np.int64)
        if not len(keep) or len(keep) == self.n_vox:
            return
        fit = keep
        anc = np.intersect1d(self.anchor_index(), keep)
        if len(anc):                    # 固定されている部分（アクチュエーターの先で回っている所は除く）で格子を求める
            fit = np.unique(np.concatenate([self.part_of(int(i)) for i in anc]))
        R, t = self.rigid_fit(fit)
        cells = np.asarray(self.cells, dtype=np.float64)
        span = np.abs(cells[keep]).max() + 50
        others = np.setdiff1d(np.arange(self.n_vox), keep)
        others = others[np.abs(cells[others]).max(1) < span]
        if not len(others):
            return
        local = ((cells[others][:, None, :] + CORNER_BITS[None]) * self.size).reshape(-1, 3)
        rows = (8 * others[:, None] + np.arange(8)[None]).ravel()
        self.pos[rows] = local @ R.T + t
        self.vel[rows] = 0.0
        if getattr(self, "_prev", None) is not None and self._prev.shape == self.pos.shape:
            self._prev[rows] = self.pos[rows]
        self.wake(soft=True)

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
        idx = np.nonzero(self.v_rebond)[0]
        centers = np.ascontiguousarray(self.voxel_centers()[idx], dtype=np.float64)
        pa, pb = native_core().pairs_within(centers, self.size * 1.15)
        added = False
        for a, b in zip(pa.tolist(), pb.tolist()):
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
    def part_of(self, i):
        """ブロック i と同じ「部分」（アクチュエーターの出力面の結合をたどらずにつながっているブロック）の番号の配列。
        変形する構築物で、コンピューターの付いている部分の姿勢を測るのに使う。"""
        cache = getattr(self, "_part_cache", None)
        if cache is None or cache[0] != (self.version, self.n_vox):
            n = self.n_vox
            parent = list(range(n))

            def find(x):
                while parent[x] != x:
                    parent[x] = parent[parent[x]]
                    x = parent[x]
                return x
            joints = {r["key"] for r in self.rotors}
            for key in list(self.links) + list(getattr(self, "extras", {}).keys()):
                if key in joints:
                    continue
                ia, ib = self.index.get(key[0]), self.index.get(key[1])
                if ia is not None and ib is not None:
                    parent[find(ia)] = find(ib)
            label = np.array([find(k) for k in range(n)], dtype=np.int64)
            cache = self._part_cache = ((self.version, self.n_vox), label)
        label = cache[1]
        return np.nonzero(label == label[i])[0]

    def rigid_fit(self, idx=None):
        """現在の頂点位置にもっとも合う剛体変換 (R, t)：pos ≒ R @ local + t。idx：その番号のブロックだけで合わせる"""
        cells = np.asarray(self.cells, dtype=np.float64)
        pos = self.pos
        if idx is not None:
            idx = np.asarray(idx, dtype=np.int64)
            cells = cells[idx]
            pos = self.pos.reshape(-1, 8, 3)[idx].reshape(-1, 3)
        local = ((cells[:, None, :] + CORNER_BITS[None]) * self.size).reshape(-1, 3)
        lc, pc = local.mean(0), pos.mean(0)
        H = (local - lc).T @ (pos - pc)
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
            "voxels": [[c[0], c[1], c[2], m, _clean_dev(d, self.reg)] for c, m, d in zip(self.cells, self.mats, self.devs)],
            "links": [[list(a), list(b)] for a, b in sorted(self.links)],
            "extras": [[list(k[0]), list(k[1]), v] for k, v in self.extras.items()],
            "storage": self.storage,
            "cavity_air": [[list(k), v] for k, v in self.cavity_air.items()],
            # 現在の形（回転したアーム・伸びたアクチュエーター・変形）を保存
            "vertices": np.round((self.pos - self.rigid_fit()[1]) @ self.rigid_fit()[0], 4).tolist(),
            "rest": np.round(self.rest, 4).tolist(),
            **({"anchor": self.anchor["to"]} if getattr(self, "anchor", None) else {}),
            **({"soft_ok": True} if getattr(self, "soft_ok", False) else {}),
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
                            name=data.get("name", "構築物"), defer=True)
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
        w = np.asarray(spawn.get("angular_velocity", [0, 0, 0]), dtype=np.float64)
        if np.any(w):
            c.vel += np.cross(w, c.pos - c.center_of_mass())
        if data.get("anchor"):
            c.anchor = {"to": data["anchor"]}      # 動く物体（軌道上の街など）に固定。中身は拡張が決める
        if data.get("soft_ok"):
            c.soft_ok = True
        return c

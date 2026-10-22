"""機器の動作：電力（配線不要・同一構築物で共有）、配管（連続したボクセルが必要）とバルブ、
推進機（点火・燃焼・発電）・アクチュエータ・精錬/加工機（物資の受け渡し）・通気口・気密。

推進機：点火のときだけ電気を使い（ignite_energy）、燃えている間は燃料を使って推力と電気（power_output）を出す。
  タンクと直接（配管で）つながっていると、点火したら燃料が尽きるまで全開で燃える（固体ロケットのように止められない）。
  あいだにバルブを入れると、バルブの開き具合で燃料の量（＝推力）を変えられ、閉じると消える。
精錬機・加工機：原料は配管でつながった所（タンク・ほかの機械の作った物）から受け取り、作った物は自分の中にためる。
  タンクは、つながった機械が作った燃料などの流れる物を受け取る。バルブを閉じると、その先へは流れない。"""
import numpy as np

from . import airtight, actuator
from .construct import DIR_TUPLES


class Networks:
    """構築物ごとのキャッシュ：配管のつながり（区画とバルブ）・空洞。

    配管の区画（segment）：配管につながる機器（タンク・配管・推進機・精錬機・加工機・通気口）が
    面で接して続いているひとかたまり。同じ区画の中では、燃料も物資も好きなだけ（すぐに）流れる。
    バルブは区画と区画の間に入り、開き具合 × 最大流量（kg/秒）までしか通さない。閉じると通さない。"""

    def __init__(self):
        self.pipe_of = {}      # voxel idx -> 区画の番号（バルブは入らない）
        self.pipes = []        # [[voxel idx...]] 区画ごとの機器
        self.valve_adj = {}    # バルブの voxel idx -> となりの区画・バルブ [("s", 区画) / ("v", idx)]
        self.seg_adj = {}      # 区画 -> となりのバルブ [idx]
        self.cavities = []     # [{"cells": set, "rep": cell, "volume": m3}]
        self.cavity_of = {}    # cell -> cavity index


def dev_def(c, i):
    d = c.devs[i]
    return c.reg.device(d["def"]) if d else None


def rebuild_networks(c):
    net = Networks()
    n = c.n_vox
    kind = []
    for i in range(n):
        dd = dev_def(c, i)
        if dd and dd.get("pipe_port"):
            kind.append("v" if dd["type"] == "valve" else "p")
        else:
            kind.append(None)
    parent = list(range(n))

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x
    pairs = []
    for a, b in c.links:
        ia, ib = c.index.get(a), c.index.get(b)
        if ia is None or ib is None or kind[ia] is None or kind[ib] is None:
            continue
        if kind[ia] == "p" and kind[ib] == "p":
            ra, rb = find(ia), find(ib)
            if ra != rb:
                parent[ra] = rb
        else:
            pairs.append((ia, ib))
    groups = {}
    for i in range(n):
        if kind[i] == "p":
            groups.setdefault(find(i), []).append(i)
    for k, members in enumerate(groups.values()):
        net.pipes.append(members)
        net.seg_adj[k] = []
        for m in members:
            net.pipe_of[m] = k
    for i in range(n):
        if kind[i] == "v":
            net.valve_adj[i] = []
    for ia, ib in pairs:                   # バルブとそのとなり（区画か、別のバルブ）
        for v, o in ((ia, ib), (ib, ia)):
            if kind[v] != "v":
                continue
            node = ("v", o) if kind[o] == "v" else ("s", net.pipe_of[o])
            if node not in net.valve_adj[v]:
                net.valve_adj[v].append(node)
            if node[0] == "s" and v not in net.seg_adj[node[1]]:
                net.seg_adj[node[1]].append(v)
    # 空洞
    old_density = {}
    old = getattr(c, "_net", None)
    if old:
        for cav in old.cavities:
            air = c.cavity_air.get(cav["rep"], 0.0)
            dens = air / max(cav["volume"], 1e-9)
            for cell in cav["cells"]:
                old_density[cell] = dens
    cavs = airtight.find_cavities(c.cells)
    new_air = {}
    vol1 = c.size ** 3
    for k, cells in enumerate(cavs):
        rep = min(cells)
        volume = len(cells) * vol1
        if old_density:
            air = sum(old_density.get(cell, 0.0) for cell in cells) * vol1
        else:
            air = c.cavity_air.get(rep, 0.0)
        new_air[rep] = air
        net.cavities.append({"cells": cells, "rep": rep, "volume": volume})
        for cell in cells:
            net.cavity_of[cell] = k
    c.cavity_air = new_air
    c._net = net
    c.dirty = False
    return net


# ---------------------------------------------------------------------------
# 配管の中の流れ（燃料・空気・水・物資）
# ---------------------------------------------------------------------------
def is_fluid(reg, res):
    r = reg.defs.get("resources", {}).get(res)
    return bool(r.get("fluid")) if r else res in ("fuel", "air", "water")


def valve_opening(st):
    v = st.get("open", 1.0)
    return float(min(max(float(v), 0.0), 1.0))


def proc_outputs(dd):
    """精錬機・加工機が出す物（作る物のうち、自分の原料ではないもの）。"""
    rs = dd["params"].get("recipes", [])
    ins = {k for r in rs for k in r.get("in", {})}
    return {k for r in rs for k in r.get("out", {})} - ins


def proc_inputs(dd):
    rs = dd["params"].get("recipes", [])
    out = {}
    for r in rs:
        for k, v in r.get("in", {}).items():
            out[k] = max(out.get(k, 0.0), float(v))
    return out


class Flow:
    """1ティックの流れ。バルブごとに、このティックで通せる残りの量を持つ。"""

    def __init__(self, c, dt):
        self.c = c
        self.net = c._net
        self.rem = {}
        for v in self.net.valve_adj:
            d = c.devs[v]
            dd = dev_def(c, v)
            mf = float(dd["params"].get("max_flow", 6.0)) if dd else 6.0
            self.rem[v] = valve_opening(d["state"]) * mf * dt
        self.opened = {v for v, r in self.rem.items() if r > 0}

    def _reach(self, i, by_open=False):
        """機器 i から届く区画を近い順に：[(区画, 通ったバルブのリスト)]"""
        net = self.net
        if i in net.pipe_of:
            starts = [(("s", net.pipe_of[i]), [])]
        elif i in net.valve_adj:
            starts = [(("v", i), [])]
        else:
            return []
        seen = {starts[0][0]}
        out = []
        queue = list(starts)
        while queue:
            node, path = queue.pop(0)
            if node[0] == "s":
                out.append((node[1], path))
                nbrs = [("v", v) for v in net.seg_adj.get(node[1], [])]
            else:
                if node[1] not in path:
                    path = path + [node[1]]
                if (node[1] not in self.opened) if by_open else (self.rem.get(node[1], 0.0) <= 1e-12):
                    continue                    # 閉じたバルブ（このティックはもう通さない）の先へは行かない
                nbrs = net.valve_adj.get(node[1], [])
            for nb in nbrs:
                if nb not in seen:
                    seen.add(nb)
                    queue.append((nb, path))
        return out

    def valves_to(self, i, res):
        """機器 i に res を届けるのに通るバルブ（res のあるいちばん近い所までの道の）。"""
        for seg, path in self._reach(i):
            if any(True for _ in self._sources(seg, res, ("tank", "proc"), i)):
                return set(path)
        return set()

    def _cap(self, path):
        return min((self.rem[v] for v in path), default=float("inf"))

    def _use(self, path, amount):
        for v in path:
            self.rem[v] -= amount

    def _sources(self, seg, res, kinds, exclude):
        c = self.c
        for i in self.net.pipes[seg]:
            if i == exclude:
                continue
            dd = dev_def(c, i)
            if not dd:
                continue
            st = c.devs[i]["state"]
            if dd["type"] == "tank" and "tank" in kinds:
                cont = st.setdefault("contents", {})
                if cont.get(res, 0.0) > 0:
                    yield cont
            elif dd["type"] == "processor" and "proc" in kinds and res in proc_outputs(dd):
                store = st.setdefault("store", {})
                if store.get(res, 0.0) > 0:
                    yield store

    def pull(self, i, res, amount, kinds=("tank", "proc"), dry=False):
        """機器 i へ、届く所から res を amount まで持ってくる。持ってこられた量を返す。
        dry：数えるだけ（開いているバルブの先まで、このティックに通した量は気にせず見る。点火できるかの確かめ用）。"""
        if amount <= 0:
            return 0.0
        got = 0.0
        for seg, path in self._reach(i, by_open=dry):
            cap = (amount - got) if dry else min(self._cap(path), amount - got)
            if cap <= 1e-12:
                continue
            took = 0.0
            for box in self._sources(seg, res, kinds, i):
                t = min(box.get(res, 0.0), cap - took)
                if t > 0:
                    if not dry:
                        box[res] = box.get(res, 0.0) - t
                    took += t
                if took >= cap - 1e-12:
                    break
            if took > 0:
                if not dry:
                    self._use(path, took)
                got += took
            if got >= amount - 1e-12:
                break
        return got

    def put_tanks(self, i, res, amount):
        """機器 i から、届くタンクの空きへ res を入れる。入れられた量を返す。"""
        c = self.c
        put = 0.0
        for seg, path in self._reach(i):
            cap = min(self._cap(path), amount - put)
            if cap <= 1e-12:
                continue
            done = 0.0
            for k in self.net.pipes[seg]:
                dd = dev_def(c, k)
                if k == i or not dd or dd["type"] != "tank":
                    continue
                cont = c.devs[k]["state"].setdefault("contents", {})
                free = float(dd["params"].get("capacity", 0)) - sum(cont.values())
                add = min(free, cap - done)
                if add > 0:
                    cont[res] = cont.get(res, 0.0) + add
                    done += add
                if done >= cap - 1e-12:
                    break
            if done > 0:
                self._use(path, done)
                put += done
            if put >= amount - 1e-12:
                break
        return put


# 古い関数（区画の中のタンクだけ。バルブの先は見ない）
def tanks_in(c, net_id):
    out = []
    if net_id is None:
        return out
    for i in c._net.pipes[net_id]:
        dd = dev_def(c, i)
        if dd and dd["type"] == "tank":
            out.append((i, c.devs[i]["state"]["contents"], float(dd["params"].get("capacity", 0))))
    return out


def fluid_amount(c, net_id, res):
    return sum(cont.get(res, 0.0) for _, cont, _ in tanks_in(c, net_id))


def storage_total(c):
    """構築物の精錬機・加工機に入っている物の合計（表示・スクリプトの api.storage 用）。"""
    out = dict(c.storage)
    for d in c.devs:
        if d and "store" in d["state"]:
            for k, v in d["state"]["store"].items():
                if v > 1e-9:
                    out[k] = out.get(k, 0.0) + v
    return out


# ---------------------------------------------------------------------------
# 毎ティック
# ---------------------------------------------------------------------------
def world_axis(c, i, d, frames=None):
    F = c.voxel_frames()[i] if frames is None else frames[i]
    axis, sign = d // 2, (1.0 if d % 2 == 0 else -1.0)
    return F[axis] * sign


def update(c, dt, input_keys=frozenset(), seated=False, ambient_kpa=0.0):
    if c.dirty or not hasattr(c, "_net"):
        rebuild_networks(c)
    net = c._net
    c.force[:] = 0.0
    if not c.n_vox:
        return
    frames = None
    by_type = {}
    for i, d in enumerate(c.devs):
        if d:
            dd = c.reg.device(d["def"])
            if dd:
                by_type.setdefault(dd["type"], []).append((i, d, dd))
    if c.storage and by_type.get("processor"):
        # 古いセーブ：構築物で共有していた資源は、最初の精錬機・加工機に入れる
        st0 = by_type["processor"][0][1]["state"].setdefault("store", {})
        for k, v in c.storage.items():
            st0[k] = st0.get(k, 0.0) + v
        c.storage = {}
    flow = Flow(c, dt)

    # ---- 電力（手回し・太陽光） ----
    crank_energy = 0.0
    for i, d, dd in by_type.get("crank", []):
        crank_energy += d["state"].pop("pending", 0.0)
    batteries = [(i, d, dd) for i, d, dd in by_type.get("battery", [])]
    stored = sum(d["state"].get("charge", 0.0) for _, d, _ in batteries)
    capacity = sum(float(dd["params"].get("capacity", 0)) for _, _, dd in batteries)
    production = 0.0
    for i, d, dd in by_type.get("generator", []):          # 太陽光パネル（燃料を使う発電機はなくなった）
        st = d["state"]
        on = st.get("enabled", True)
        st["active"] = bool(on)
        if on:
            production += float(dd["params"].get("output", 0))

    # ---- 推進機：点火（電気を使う）→ 燃焼（燃料を使い、推力と電気を出す） ----
    ignite_cost = 0.0
    pool = stored + crank_energy + production * dt
    point_forces = []
    burning = []
    for i, d, dd in by_type.get("thruster", []):
        st = d["state"]
        p = dd["params"]
        fuel = p.get("fuel", "fuel")
        want = bool(st.pop("ignite", False)) or bool(seated and p.get("key") and p["key"] in input_keys)
        if want and not st.get("lit"):
            e = float(p.get("ignite_energy", 0.0))
            if flow.pull(i, fuel, 1e-6, dry=True) <= 0:
                st["error"] = "燃料が届いていません（タンクとつなぐ・バルブを開ける）"
            elif pool - ignite_cost < e:
                st["error"] = "電気が足りず点火できません（バッテリーなど）"
            else:
                ignite_cost += e
                st["lit"] = True
                st.pop("error", None)
        st["firing"] = 0.0
        st["throttle"] = 0.0
        if st.get("lit"):
            burning.append((i, d, dd, float(p.get("fuel_rate", 0)) * dt))
    # 同じバルブの先にいくつも推進機があるときは、バルブを通る量を欲しい量に比べて分ける
    through = {}
    for i, d, dd, need in burning:
        vs = flow.valves_to(i, dd["params"].get("fuel", "fuel"))
        d["_vs"] = vs
        for v in vs:
            through[v] = through.get(v, 0.0) + need
    rem0 = dict(flow.rem)
    for i, d, dd, need in burning:
        st = d["state"]
        p = dd["params"]
        quota = need
        for v in d.pop("_vs", ()):
            quota = min(quota, rem0.get(v, 0.0) * need / max(through[v], 1e-12))
        got = flow.pull(i, p.get("fuel", "fuel"), quota) if quota > 0 else 0.0
        frac = got / need if need > 0 else 0.0
        if frac <= 1e-6:
            st["lit"] = False                   # 燃料が来なくなった（バルブを閉じた・空）：消える
            continue
        if frames is None:
            frames = c.voxel_frames()
        f = world_axis(c, i, d["dir"], frames) * float(p.get("thrust", 0)) * frac
        point_forces.append((i, f))
        st["firing"] = st["throttle"] = frac
        production += float(p.get("power_output", 0.0)) * frac
    if point_forces:
        apply_point_forces(c, point_forces)

    # ---- 電力の配分 ----
    consumers = []
    for t in ("computer", "actuator", "processor", "air_vent", "antenna"):
        for i, d, dd in by_type.get(t, []):
            if dd.get("power") == "electric" and _wants_power(c, i, d, dd, t):
                consumers.append((i, d, dd))
    demand = sum(_power_use(c, d, dd) for _, d, dd in consumers)
    avail = production * dt + stored + crank_energy - ignite_cost
    ratio = 1.0 if demand <= 0 else min(1.0, max(avail, 0.0) / (demand * dt))
    net_energy = production * dt + crank_energy - demand * dt * ratio - ignite_cost
    _distribute(batteries, net_energy)
    stored = sum(d["state"].get("charge", 0.0) for _, d, _ in batteries)
    c.powered = stored > 1.0 or production > 0
    c.status = {"production": production, "demand": demand, "stored": stored,
                "capacity": capacity, "ratio": ratio}
    powered = {i for i, _, _ in consumers} if ratio > 0 else set()
    # アンテナ：電力が来ていて ON のときだけ電波を出せる・受けられる（コンピューターの通信に必要）
    for i, d, dd in by_type.get("antenna", []):
        d["state"]["active"] = i in powered

    # ---- アクチュエータ ----
    for i, d, dd in by_type.get("actuator", []):
        actuator.step(c, i, dd, dt, i in powered, ratio)

    # ---- 精錬機・加工機：原料を配管から受け取り、作った物は自分の中にためる ----
    for i, d, dd in by_type.get("processor", []):
        st = d["state"]
        store = st.setdefault("store", {})
        cap = float(dd["params"].get("capacity", 400.0))
        for r, amt in proc_inputs(dd).items():
            room = cap - sum(store.values())
            want = min(amt * 2 - store.get(r, 0.0), room)
            if want > 0:
                got = flow.pull(i, r, want)
                if got > 0:
                    store[r] = store.get(r, 0.0) + got
    for i, d, dd in by_type.get("processor", []):
        st = d["state"]
        store = st.setdefault("store", {})
        if i not in powered:
            st["working"] = False
            continue
        recipe = _ready_recipe(c, dd, st)
        if recipe is None:
            st["working"] = False
            st["progress"] = 0.0
            continue
        st["working"] = True
        st["progress"] = st.get("progress", 0.0) + dt * ratio
        if st["progress"] >= float(recipe.get("time", 1.0)):
            st["progress"] = 0.0
            # 原料が何種類も入っているときは順番に（燃料も鉄も少しずつできる）
            rs = dd["params"].get("recipes", [])
            st["next"] = (rs.index(recipe) + 1) % max(len(rs), 1)
            for r, a in recipe["in"].items():
                store[r] = store.get(r, 0.0) - a
            for r, a in recipe["out"].items():
                store[r] = store.get(r, 0.0) + a

    # ---- タンク：届く精錬機・加工機の作った燃料など（流れる物）を受け取る ----
    for i, d, dd in by_type.get("tank", []):
        cont = d["state"].setdefault("contents", {})
        free = float(dd["params"].get("capacity", 0)) - sum(cont.values())
        if free <= 1e-9:
            continue
        for r in _fluid_outputs(c, by_type):
            got = flow.pull(i, r, free, kinds=("proc",))
            if got > 0:
                cont[r] = cont.get(r, 0.0) + got
                free -= got
            if free <= 1e-9:
                break

    # ---- 通気口・気密 ----
    for i, d, dd in by_type.get("air_vent", []):
        st = d["state"]
        cav = _adjacent_cavity(c, i)
        st["cavity"] = cav is not None
        if cav is None or i not in powered:
            continue
        p = dd["params"]
        info = net.cavities[cav]
        rep = info["rep"]
        air = c.cavity_air.get(rep, 0.0)
        target = airtight.air_for_pressure(float(p.get("target_kpa", 101.3)), info["volume"])
        rate = float(p.get("rate", 1.0)) * dt * ratio
        if st.get("mode", "fill") == "fill" and air < target:
            got = flow.pull(i, "air", min(rate, target - air), kinds=("tank",))
            c.cavity_air[rep] = air + got
        elif st.get("mode") == "drain" and air > 0:
            mv = min(rate, air)
            flow.put_tanks(i, "air", mv)  # 入りきらない分は外へ排気
            c.cavity_air[rep] = air - mv


def _fluid_outputs(c, by_type):
    out = []
    for _, _, dd in by_type.get("processor", []):
        for r in proc_outputs(dd):
            if r not in out and is_fluid(c.reg, r):
                out.append(r)
    return out


LOCAL_FRACTION = 0.15


def apply_point_forces(c, forces, local=LOCAL_FRACTION):
    """機器の力を構築物全体に配分する。
    一部（local）はその機器のボクセルに直接かけて構造に応力を生じさせ、
    残りは剛体として並進＋回転に配分する（巨大な力で局所的にちぎれすぎないように）。"""
    m = c.mass
    M = m.sum()
    com = (c.pos * m[:, None]).sum(0) / M
    r = c.pos - com
    I = (m[:, None, None] * (np.einsum("ij,ij->i", r, r)[:, None, None] * np.eye(3)
                             - r[:, :, None] * r[:, None, :])).sum(0) + np.eye(3) * 1e-6
    Iinv = np.linalg.inv(I)
    lin = np.zeros(3)
    tau = np.zeros(3)
    for i, f in forces:
        c.force[8 * i:8 * i + 8] += f * local / 8.0
        fr = f * (1 - local)
        lin += fr
        p = c.pos[8 * i:8 * i + 8].mean(0)
        tau += np.cross(p - com, fr)
    alpha = Iinv @ tau
    c.force += m[:, None] * (lin / M + np.cross(alpha, r))


def _wants_power(c, i, d, dd, t):
    st = d["state"]
    if t == "computer":
        return bool(st.get("running"))
    if t == "actuator":
        return actuator.busy(st)
    if t == "processor":
        return st.get("enabled", True) and _ready_recipe(c, dd, st) is not None
    if t == "air_vent":
        return st.get("enabled", True) and _adjacent_cavity(c, i) is not None
    if t == "antenna":
        return st.get("enabled", True)
    return False


def _power_use(c, d, dd):
    """消費電力（W）。精錬機などはレシピごとに "power" で上書きできる（水→燃料は多め）。"""
    if dd["type"] == "processor":
        r = _ready_recipe(c, dd, d["state"])
        if r is not None and "power" in r:
            return float(r["power"])
    return float(dd.get("power_use", 0))


def _ready_recipe(c, dd, st=None):
    """材料がそろっていて、作った物を入れる空きもあるレシピ（その機器の中の物で決める）。"""
    rs = dd["params"].get("recipes", [])
    st = st or {}
    store = st.get("store", {})
    cap = float(dd["params"].get("capacity", 400.0))
    start = int(st.get("next", 0)) % max(len(rs), 1)
    for k in range(len(rs)):
        r = rs[(start + k) % len(rs)]
        if not all(store.get(k_, 0.0) >= v for k_, v in r.get("in", {}).items()):
            continue
        after = sum(store.values()) - sum(r.get("in", {}).values()) + sum(r.get("out", {}).values())
        if after <= cap + 1e-9:
            return r
    return None


def _adjacent_cavity(c, i):
    cell = c.cells[i]
    for d in DIR_TUPLES:
        nb = (cell[0] + d[0], cell[1] + d[1], cell[2] + d[2])
        k = c._net.cavity_of.get(nb)
        if k is not None:
            return k
    return None


def _distribute(batteries, energy):
    if not batteries or abs(energy) < 1e-12:
        return
    if energy > 0:
        for _, d, dd in batteries:
            cap = float(dd["params"].get("capacity", 0))
            ch = d["state"].get("charge", 0.0)
            add = min(cap - ch, energy)
            d["state"]["charge"] = ch + add
            energy -= add
            if energy <= 0:
                break
    else:
        need = -energy
        for _, d, _ in batteries:
            ch = d["state"].get("charge", 0.0)
            take = min(ch, need)
            d["state"]["charge"] = ch - take
            need -= take
            if need <= 0:
                break


def cavity_report(c):
    if c.dirty or not hasattr(c, "_net"):
        rebuild_networks(c)
    out = []
    for cav in c._net.cavities:
        out.append((cav["rep"], len(cav["cells"]),
                    airtight.pressure_kpa(c.cavity_air.get(cav["rep"], 0.0), cav["volume"])))
    return out


def interact(c, i, player=None):
    """E キーで機器を操作。メッセージを返す。"""
    d = c.devs[i]
    if not d:
        return None
    dd = c.reg.device(d["def"])
    if not dd:
        return f"未定義の機器: {d['def']}"
    st = d["state"]
    t = dd["type"]
    if t == "valve":
        o = valve_opening(st)
        if o > 0:
            st["set"] = o
            st["open"] = 0.0
        else:
            st["open"] = float(st.get("set", 1.0)) or 1.0
        return f"{d['name']}: " + ("閉" if st["open"] <= 0 else f"開（{st['open']:.0%}。開き具合は P で）")
    if t == "crank":
        e = float(dd["params"].get("energy_per_use", 5000))
        st["pending"] = st.get("pending", 0.0) + e
        return f"{d['name']}: {e/1000:.1f} kJ 発電"
    if t == "computer":
        st["running"] = not st.get("running", False)
        st.pop("error", None)
        return f"{d['name']}: {'起動' if st['running'] else '停止'}（{st.get('script') or 'スクリプト未設定'}）"
    if t in ("generator", "processor", "air_vent", "antenna"):
        if t == "air_vent" and st.get("enabled", True):
            st["mode"] = "drain" if st.get("mode", "fill") == "fill" else "fill"
            return f"{d['name']}: モード {st['mode']}"
        st["enabled"] = not st.get("enabled", True)
        return f"{d['name']}: {'ON' if st['enabled'] else 'OFF'}"
    if t == "thruster":
        if st.get("lit"):
            return f"{d['name']}: 燃焼中（止める・出力を変えるには、つながるバルブで）"
        st["ignite"] = True
        return f"{d['name']}: 点火"
    if t == "tank":
        return f"{d['name']}: " + (", ".join(f"{k} {v:.0f}kg" for k, v in st["contents"].items()) or "空")
    if t == "battery":
        return f"{d['name']}: {st.get('charge', 0)/1e6:.2f} MJ"
    return d["name"]

"""機器の動作：電力（配線不要・同一構築物で共有）、配管（連続したボクセルが必要）、
推進機・アクチュエータ・精錬/加工機・通気口・気密。"""
import numpy as np

from . import airtight, actuator
from .construct import DIR_TUPLES


class Networks:
    """構築物ごとのキャッシュ：配管ネットワーク・空洞。"""

    def __init__(self):
        self.pipe_of = {}      # voxel idx -> network id
        self.pipes = []        # [[voxel idx...]]
        self.cavities = []     # [{"cells": set, "rep": cell, "volume": m3}]
        self.cavity_of = {}    # cell -> cavity index


def dev_def(c, i):
    d = c.devs[i]
    return c.reg.device(d["def"]) if d else None


def rebuild_networks(c):
    net = Networks()
    n = c.n_vox
    ports = []
    for i in range(n):
        dd = dev_def(c, i)
        if dd and dd.get("pipe_port"):
            if dd["type"] == "valve" and not c.devs[i]["state"].get("open", True):
                ports.append(False)
            else:
                ports.append(True)
        else:
            ports.append(False)
    parent = list(range(n))

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x
    for a, b in c.links:
        ia, ib = c.index.get(a), c.index.get(b)
        if ia is not None and ib is not None and ports[ia] and ports[ib]:
            ra, rb = find(ia), find(ib)
            if ra != rb:
                parent[ra] = rb
    groups = {}
    for i in range(n):
        if ports[i]:
            groups.setdefault(find(i), []).append(i)
    for k, members in enumerate(groups.values()):
        net.pipes.append(members)
        for m in members:
            net.pipe_of[m] = k
    # 閉じたバルブも自分自身だけのネットワークを持つ（表示用）
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
# 配管内の流体
# ---------------------------------------------------------------------------
def tanks_in(c, net_id):
    out = []
    for i in c._net.pipes[net_id]:
        dd = dev_def(c, i)
        if dd and dd["type"] == "tank":
            out.append((i, c.devs[i]["state"]["contents"], float(dd["params"].get("capacity", 0))))
    return out


def fluid_amount(c, net_id, res):
    return sum(cont.get(res, 0.0) for _, cont, _ in tanks_in(c, net_id))


def draw_fluid(c, net_id, res, amount):
    if amount <= 0 or net_id is None:
        return 0.0
    got = 0.0
    for _, cont, _ in tanks_in(c, net_id):
        take = min(cont.get(res, 0.0), amount - got)
        if take > 0:
            cont[res] = cont.get(res, 0.0) - take
            got += take
        if got >= amount:
            break
    return got


def put_fluid(c, net_id, res, amount):
    if amount <= 0 or net_id is None:
        return 0.0
    put = 0.0
    for _, cont, cap in tanks_in(c, net_id):
        free = cap - sum(cont.values())
        add = min(free, amount - put)
        if add > 0:
            cont[res] = cont.get(res, 0.0) + add
            put += add
        if put >= amount:
            break
    return put


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

    # ---- 電力 ----
    crank_energy = 0.0
    for i, d, dd in by_type.get("crank", []):
        crank_energy += d["state"].pop("pending", 0.0)
    batteries = [(i, d, dd) for i, d, dd in by_type.get("battery", [])]
    stored = sum(d["state"].get("charge", 0.0) for _, d, _ in batteries)
    capacity = sum(float(dd["params"].get("capacity", 0)) for _, _, dd in batteries)

    consumers = []
    for t in ("computer", "actuator", "processor", "air_vent", "antenna"):
        for i, d, dd in by_type.get(t, []):
            if dd.get("power") == "electric" and _wants_power(c, i, d, dd, t):
                consumers.append((i, d, dd))
    demand = sum(_power_use(c, d, dd) for _, d, dd in consumers)
    # 燃料を使う発電機は、電気の使い道があるとき（消費中かバッテリーに空きがある）だけ燃料を燃やす
    need_fuel_power = demand > 0 or stored < capacity * 0.999
    production = 0.0
    for i, d, dd in by_type.get("generator", []):
        st = d["state"]
        if not st.get("enabled", True):
            st["active"] = False
            continue
        p = dd["params"]
        rate = float(p.get("fuel_rate", 0))
        if rate > 0 and not need_fuel_power:
            st["active"] = False
            continue
        need = rate * dt
        got = draw_fluid(c, net.pipe_of.get(i), p.get("fuel", "fuel"), need) if need > 0 else 0.0
        frac = got / need if need > 0 else 1.0
        st["active"] = frac > 0
        production += float(p.get("output", 0)) * frac
    avail = production * dt + stored + crank_energy
    ratio = 1.0 if demand <= 0 else min(1.0, avail / (demand * dt))
    net_energy = production * dt + crank_energy - demand * dt * ratio
    _distribute(batteries, net_energy)
    stored = sum(d["state"].get("charge", 0.0) for _, d, _ in batteries)
    c.powered = stored > 1.0 or production > 0
    c.status = {"production": production, "demand": demand, "stored": stored,
                "capacity": capacity, "ratio": ratio}
    powered = {i for i, _, _ in consumers} if ratio > 0 else set()
    # アンテナ：電力が来ていて ON のときだけ電波を出せる・受けられる（コンピューターの通信に必要）
    for i, d, dd in by_type.get("antenna", []):
        d["state"]["active"] = i in powered

    # ---- 推進機 ----
    point_forces = []
    for i, d, dd in by_type.get("thruster", []):
        st = d["state"]
        p = dd["params"]
        thr = float(np.clip(st.get("throttle", 0.0), 0, 1))
        if seated and p.get("key") and p["key"] in input_keys:
            thr = 1.0
        st["firing"] = 0.0
        if thr <= 0:
            continue
        need = float(p.get("fuel_rate", 0)) * thr * dt
        frac = 1.0
        if need > 0:
            got = draw_fluid(c, net.pipe_of.get(i), p.get("fuel", "fuel"), need)
            frac = got / need
        if frac <= 0:
            continue
        if frames is None:
            frames = c.voxel_frames()
        f = world_axis(c, i, d["dir"], frames) * float(p.get("thrust", 0)) * thr * frac
        point_forces.append((i, f))
        st["firing"] = thr * frac

    if point_forces:
        apply_point_forces(c, point_forces)

    # ---- アクチュエータ ----
    for i, d, dd in by_type.get("actuator", []):
        actuator.step(c, i, dd, dt, i in powered, ratio)

    # ---- 精錬機・加工機 ----
    for i, d, dd in by_type.get("processor", []):
        st = d["state"]
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
                c.storage[r] = c.storage.get(r, 0.0) - a
            for r, a in recipe["out"].items():
                c.storage[r] = c.storage.get(r, 0.0) + a

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
        nid = net.pipe_of.get(i)
        if st.get("mode", "fill") == "fill" and air < target:
            got = draw_fluid(c, nid, "air", min(rate, target - air))
            c.cavity_air[rep] = air + got
        elif st.get("mode") == "drain" and air > 0:
            mv = min(rate, air)
            put_fluid(c, nid, "air", mv)  # 入りきらない分は外へ排気
            c.cavity_air[rep] = air - mv


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
    rs = dd["params"].get("recipes", [])
    start = int((st or {}).get("next", 0)) % max(len(rs), 1)
    for k in range(len(rs)):
        r = rs[(start + k) % len(rs)]
        if all(c.storage.get(k_, 0.0) >= v for k_, v in r.get("in", {}).items()):
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
        st["open"] = not st.get("open", True)
        c.dirty = True
        return f"{d['name']}: {'開' if st['open'] else '閉'}"
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
        st["throttle"] = 0.0 if st.get("throttle", 0) > 0 else 1.0
        return f"{d['name']}: スロットル {st['throttle']:.0%}"
    if t == "tank":
        return f"{d['name']}: " + ", ".join(f"{k} {v:.0f}kg" for k, v in st["contents"].items())
    if t == "battery":
        return f"{d['name']}: {st.get('charge', 0)/1e6:.2f} MJ"
    return d["name"]

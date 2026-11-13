"""機器の動作：電力（配線不要・同一構築物で共有）、配管（連続したボクセルが必要）とバルブ、
アクチュエーターも配管と同じく燃料・物資を通す（結合している出力面の先へも。分離すると通らない）、
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
    # 結合（アクチュエーターで格子がずれてつないだ所 extras も含む）：配管の機器どうしなら流れる
    for a, b in list(c.links) + list(getattr(c, "extras", {}).keys()):
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
    key = (c.version, len(c.cells))
    cache = c.__dict__.get("_bytype_cache")
    if cache is None or cache[0] != key:                 # 種類ごとの機器の番号（形が変わったときだけ数え直す）
        idx = {}
        for i, d in enumerate(c.devs):
            if d:
                dd = c.reg.device(d["def"])
                if dd:
                    idx.setdefault(dd["type"], []).append(i)
        cache = c._bytype_cache = (key, idx)
    devs = c.devs
    by_type = {}
    for t, ids in cache[1].items():
        lst = []
        for i in ids:
            d = devs[i] if i < len(devs) else None
            dd = c.reg.device(d["def"]) if d else None
            if dd:
                lst.append((i, d, dd))
        if lst:
            by_type[t] = lst
    if c.storage and by_type.get("processor"):
        # 古いセーブ：構築物で共有していた資源は、最初の精錬機・加工機に入れる
        st0 = by_type["processor"][0][1]["state"].setdefault("store", {})
        for k, v in c.storage.items():
            st0[k] = st0.get(k, 0.0) + v
        c.storage = {}
    plan = _native_plan(c, net, by_type)
    _native_update(c, plan, net, by_type, dt, input_keys, seated)


# ---------------------------------------------------------------------------
# 機器の1ティックは Rust（alpedia_core.DevPlan）でまとめて計算する。アクチュエーターだけ Python
# ---------------------------------------------------------------------------
def _native_plan(c, net, by_type):
    """この構築物の機器のまとめ（形が変わったときだけ作り直す）。"""
    from . import native
    core = native.require()
    key = (c.version, len(c.cells), id(net))
    cache = c.__dict__.get("_devplan")
    if cache is not None and cache[0] == key:
        return cache[1]
    devs = []
    order = []
    for t, lst in by_type.items():
        for i, d, dd in lst:
            order.append(i)
    order.sort()
    outs_all = []
    for i in order:
        d = c.devs[i]
        dd = c.reg.device(d["def"])
        p = dd.get("params", {})
        e = {"vox": i, "type": dd["type"], "name": d.get("name", ""), "dir": int(d.get("dir", 0)),
             "electric": dd.get("power") == "electric", "power_use": float(dd.get("power_use", 0)),
             "capacity": float(p.get("capacity", 400.0 if dd["type"] == "processor" else 0)),
             "output": float(p.get("output", 0)), "fuel": p.get("fuel", "fuel"), "fuel_rate": float(p.get("fuel_rate", 0)),
             "thrust": float(p.get("thrust", 0)), "power_output": float(p.get("power_output", 0.0)),
             "ignite_energy": float(p.get("ignite_energy", 0.0)), "max_flow": float(p.get("max_flow", 6.0)),
             "key": p.get("key") or None, "rate": float(p.get("rate", 1.0)), "target_kpa": float(p.get("target_kpa", 101.3))}
        if dd["type"] == "processor":
            e["recipes"] = [(list((k, float(v)) for k, v in r.get("in", {}).items()),
                             list((k, float(v)) for k, v in r.get("out", {}).items()),
                             float(r.get("time", 1.0)), float(r["power"]) if "power" in r else None)
                            for r in p.get("recipes", [])]
            e["inputs"] = list(proc_inputs(dd).items())
            e["outputs"] = sorted(proc_outputs(dd))
            outs_all += e["outputs"]
        if dd["type"] == "air_vent":
            e["cavity"] = _adjacent_cavity(c, i)
        devs.append(e)
    fluids = sorted({r for r in outs_all if is_fluid(c.reg, r)} | {"fuel", "air", "water"})
    va = [(v, [(k == "v", x) for k, x in adj]) for v, adj in net.valve_adj.items()]
    plan = core.DevPlan(devs, dict(net.pipe_of), [list(m) for m in net.pipes], va,
                        {k: list(v) for k, v in net.seg_adj.items()}, fluids,
                        [float(cv["volume"]) for cv in net.cavities])
    c._devplan = (key, plan, order)
    return plan


def _native_update(c, plan, net, by_type, dt, input_keys, seated):
    order = c._devplan[2]
    states = [c.devs[i]["state"] for i in order]
    reps = [cv["rep"] for cv in net.cavities]
    cav = np.array([float(c.cavity_air.get(r, 0.0)) for r in reps], dtype=np.float64)
    c._contig("pos", "force")
    mass = c.mass if (c.mass.flags.c_contiguous and c.mass.dtype == np.float64) else np.ascontiguousarray(c.mass, np.float64)
    production, demand, stored, capacity, ratio, powered, notes = plan.update(
        states, float(dt), [str(k) for k in input_keys], bool(seated), c.pos, mass, c.force, cav)
    for r, a in zip(reps, cav.tolist()):
        c.cavity_air[r] = a
    c.powered = stored > 1.0 or production > 0
    c.status = {"production": production, "demand": demand, "stored": stored, "capacity": capacity, "ratio": ratio}
    for n in notes:
        _note(c, n)
    pw = set(powered)
    for i, d, dd in by_type.get("actuator", []):
        actuator.step(c, i, dd, dt, i in pw, ratio)


def _adjacent_cavity(c, i):
    cell = c.cells[i]
    for d in DIR_TUPLES:
        nb = (cell[0] + d[0], cell[1] + d[1], cell[2] + d[2])
        k = c._net.cavity_of.get(nb)
        if k is not None:
            return k
    return None


def _note(c, text):
    """画面に出す知らせ（世界が次のティックでメッセージにする）。"""
    notes = c.__dict__.setdefault("notes", [])
    if text not in notes:
        notes.append(text)


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
    if t == "anchor":
        return (f"{d['name']}: {c.name} を惑星に固定しています（ブロック {c.n_vox} 個。"
                "アクチュエーターの先は動けます。アンカーとはなれた部分は動きます）")
    if t == "crank":
        e = float(dd["params"].get("energy_per_use", 5000))
        st["pending"] = st.get("pending", 0.0) + e
        return f"{d['name']}: {e/1000:.1f} kJ 発電"
    if t == "computer":
        st["running"] = not st.get("running", False)
        st.pop("error", None)
        return f"{d['name']}: {'起動' if st['running'] else '停止'}（{st.get('script') or 'スクリプト未設定'}）"
    if t in ("generator", "processor", "air_vent", "antenna", "light", "custom"):
        if t == "air_vent" and st.get("enabled", True):
            st["mode"] = "drain" if st.get("mode", "fill") == "fill" else "fill"
            return f"{d['name']}: モード {st['mode']}"
        st["enabled"] = not st.get("enabled", True)
        return f"{d['name']}: {'ON' if st['enabled'] else 'OFF'}"
    if t == "thruster":
        if st.get("lit"):
            return f"{d['name']}: 燃焼中（止める・出力を変えるには、つながるバルブで）"
        st["ignite"] = True
        st["_ask"] = True                   # 結果（点火した・できない理由）は次のティックで知らせる
        return None
    if t == "tank":
        return f"{d['name']}: " + (", ".join(f"{k} {v:.0f}kg" for k, v in st["contents"].items()) or "空")
    if t == "battery":
        return f"{d['name']}: {st.get('charge', 0)/1e6:.2f} MJ"
    return d["name"]

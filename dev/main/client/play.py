"""手の操作（狙っている物の説明・置く場所の枠）。今までの画面（game.py）と Rust の画面（client_rs）の両方で使う。"""
import numpy as np

from ..construct import DIR_NAMES


def aim(w, h, look_vectors):
    """狙っている物 h（world.raycast の結果）の、枠と説明。
    戻り値 {"highlight": (中心, 軸 (3,3), 大きさ (3,)) か None, "ghost": 置く場所の枠 か None, "text": 説明}。座標はワールド。"""
    out = {"highlight": None, "ghost": None, "text": ""}
    if not h:
        return out
    if h["kind"] == "voxel":
        from .game import device_status_text
        c, i = h["construct"], h["index"]
        F = c.voxel_frames()[i]
        ctr = c.pos[8 * i:8 * i + 8].mean(0)
        out["highlight"] = (ctr, F, np.full(3, c.size * 1.02))
        axis, sign = h["face"] // 2, (1 if h["face"] % 2 == 0 else -1)
        out["ghost"] = (ctr + F[axis] * sign * c.size, F, np.full(3, c.size * 0.98))
        d = c.devs[i]
        m = c.reg.material(c.mats[i])
        info = f"{c.name}  [{m['name']}]"
        if c.anchor:
            info += "  ［固定中：機器以外のブロックで E を押すと切り離し］"
        if d:
            dd = c.reg.device(d["def"]) or {}
            t_ = dd.get("type")
            info += f"\n{d['name']}"
            if dd.get("directional"):
                info += f"  向き {DIR_NAMES[d['dir']]}"
            same = sum(1 for x in c.devs if x and x["name"] == d["name"])
            if same > 1:
                info += f"  （同じ名前が {same} 個：まとめて動く）"
            surv = not w.creative
            hint = {"computer": "E 起動/停止・P プログラム", "thruster": "E 点火・止めるのはバルブで",
                    "valve": "E 開閉・P 開き具合", "actuator": "E 実行・P 動作の設定",
                    "tank": "E 手持ちの燃料を入れる" if surv else "E 中身の切り替え",
                    "processor": "E 原料を入れる・できた物を受け取る" if surv else "P 設定",
                    "seat": "E 座る"}.get(t_, "P 名前の設定")
            info += f"\n［{hint}］"
            txt = device_status_text(c, i, d, dd)
            if txt:
                info += "\n" + txt
        s = c.status
        if s:
            info += (f"\n電力 {s.get('stored', 0)/1e6:.2f}/{s.get('capacity', 0)/1e6:.1f} MJ  発電 "
                     f"{s.get('production', 0)/1e3:.1f} kW  消費 {s.get('demand', 0)/1e3:.1f} kW")
        from ..devices import cavity_report, storage_total
        cav = cavity_report(c)
        if cav:
            info += "\n気密空洞: " + ", ".join(f"{n}セル {p_:.1f}kPa" for _, n, p_ in cav)
        tot = storage_total(c)
        if tot:
            info += "\n機械の中の物: " + c.reg.res_text(tot, ", ")
        out["text"] = info
    elif h["kind"] in ("terrain", "water"):
        size = float(w.settings.get("voxel_size", 0.5))
        g = h["point"] + h["normal"] * size * 0.55
        fwd, right, up, _ = look_vectors
        out["ghost"] = (g, np.stack([right, h["normal"], fwd]), np.full(3, size))
        if h["kind"] == "water":
            out["text"] = "水面" + ("" if w.creative else "（左クリックで水をくむ：いくらくんでも減りません）")
        elif w.creative:
            out["text"] = f"{h['planet'].name} の地面（左クリックで掘る）"
        else:
            res = w.registry.res_name(h["planet"].resource_at(h["point"]))
            out["text"] = f"{h['planet'].name} の地面：{res}（左クリックで掘る）"
    return out


def item_text(w, item, sel, dir_index):
    """アイテムバーの選んでいる枠の説明。"""
    if item is None:
        return f"[{sel + 1}] （空）"
    kind, id_ = item
    if kind == "material":
        m = w.registry.material(id_) or {}
        return f"[{sel + 1}] 素材: {m.get('name', id_)}"
    dd = w.registry.device(id_) or {}
    extra = f"  向き {DIR_NAMES[dir_index]}" if dd.get("directional") else ""
    return f"[{sel + 1}] 機器: {dd.get('name', id_)}{extra}"

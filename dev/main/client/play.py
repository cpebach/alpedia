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


def camera_clip(w, eye, offset):
    """三人称のカメラが壁・地面・建物の向こうに入らないように、手前で止める（乗っている乗り物は通り抜ける）。"""
    n = float(np.linalg.norm(offset))
    if n < 1e-6:
        return eye
    d = offset / n
    own = w.player.seated[0] if w.player.seated else None
    start, left = np.asarray(eye, dtype=np.float64), n
    for _ in range(4):
        try:
            h = w.raycast(start, d, left)
        except Exception:  # noqa: BLE001
            return eye + offset
        if h is None:
            return eye + offset
        if h.get("kind") == "voxel" and own is not None and h["construct"].id == own:
            step = float(h["t"]) + 0.05       # 自分の乗り物：その先から調べ直す
            start, left = start + d * step, left - step
            if left <= 0:
                return eye + offset
            continue
        t = float(np.linalg.norm(start + d * float(h["t"]) - eye))
        return eye + d * max(t - 0.4, 0.3)
    return eye + offset


def seat_follow(w, addons):
    """座っている席が遠隔操作なら、操っている構築物を後ろから映す：(重心, 距離) か None。MOD の seat_camera() が決める。"""
    p = w.player
    if not p.seated:
        return None
    c = w.constructs.get(p.seated[0])
    if c is None:
        return None
    for a in addons:
        fn = getattr(a, "seat_camera", None)
        t = fn(c) if fn else None
        if t is not None and t.n_vox:
            ext = float(np.linalg.norm(t.pos.max(0) - t.pos.min(0)))
            return t.center_of_mass(), float(np.clip(ext * 2.0, 4.0, 30.0))
    return None


QUIET_DEVICES = {"pipe", "battery", "anchor"}         # 近くの物の一覧に出さない（E で触るほどの物ではない）


def nearby_targets(w, addons, radius=2.6):
    """近くにある、触れるもの（使う順）：[{"label", "hit", "kind": "e", "settings": 設定を開けるか, "icon", "dist"}]
    機器 1 つにボタン 1 つ（設定はボタンの歯車・P）。配管・バッテリーは出さない。MOD の nearby_priority で並べ替える。"""
    p = w.player
    if p.seated:
        return []
    eye = p.eye()
    out = []
    surv = not w.creative
    names = {"seat": "座る", "computer": "起動・停止", "tank": "燃料を入れる" if surv else "中身を切り替える",
             "actuator": "動かす", "processor": "原料を入れる・受け取る" if surv else "ON/OFF",
             "thruster": "点火", "valve": "開閉", "crank": "回す"}
    for c in w.constructs_near(eye, radius):
        if not c.n_vox or not c.has_devices:
            continue
        for i, d in enumerate(c.devs):
            if not d:
                continue
            dd = c.reg.device(d["def"]) or {}
            t_ = dd.get("type")
            if t_ in QUIET_DEVICES:
                continue
            ctr = c.pos[8 * i:8 * i + 8].mean(0)
            dist = float(np.linalg.norm(ctr - eye))
            if dist > radius:
                continue
            hit = {"kind": "voxel", "t": dist, "construct": c, "index": i, "face": 2, "point": ctr}
            out.append({"label": f"{names.get(t_, '操作')}（{d['name']}）", "hit": hit, "kind": "e", "dist": dist,
                        "icon": "interact", "settings": t_ != "seat"})
    for a in addons:
        try:
            out += a.nearby(p.pos, radius) or []
        except Exception:  # noqa: BLE001
            pass

    def rank(t):
        pr = 0.0
        for a in addons:
            fn = getattr(a, "nearby_priority", None)
            if fn is not None:
                try:
                    pr += float(fn(t) or 0.0)
                except Exception:  # noqa: BLE001
                    pass
        return (pr, t["dist"])
    out.sort(key=rank)
    return out[:4]

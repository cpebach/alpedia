"""アクチュエーター：回転・押し出し・引き込み・分離・結合。

アクチュエーターは向き（出力面）を持つブロック。
- 回転   : 出力面の先につながったブロック（とその先の部分）を、向きを軸に回す
- 押し出し: 自分が伸びて、出力面の先を押し出す（最大でブロック1つ分）
- 引き込み: 伸びた分を引き戻す
- 分離   : 出力面の結合を切り離す（先の部分は別の構築物になる）
- 結合   : 出力面の先にあるブロック（別の構築物でも可）とつなぐ

操作方法は2通り:
- コンピューターから: スクリプトで actuator.rotate(90) / .extend(1.0) / .detach() など
- 手動: P で動作設定メニューを開き、動作の種類・量・速度を決めて閉じ、E で実行
"""
import numpy as np

from .construct import DIR_TUPLES, JOINT_PAIRS, link_key

ACTIONS = ["rotate", "extend", "retract", "detach", "attach"]
ACTION_NAMES = {"rotate": "回転", "extend": "押し出し", "retract": "引き込み", "detach": "分離", "attach": "結合"}
DEFAULT_PRESET = {"action": "rotate", "amount": 90.0, "speed": 90.0}


def is_actuator(c, i):
    d = c.devs[i]
    return bool(d) and (c.reg.device(d["def"]) or {}).get("type") == "actuator"


def init_state(st, params):
    # 旧形式（target）からの移行
    if "target" in st and "ext_target" not in st:
        st["ext_target"] = float(st.pop("target"))
    st.setdefault("ext", 0.0)
    st.setdefault("ext_target", st["ext"])
    st.setdefault("ext_speed", float(params.get("speed", 0.5)))
    st.setdefault("angle", 0.0)
    st.setdefault("angle_target", st["angle"])
    st.setdefault("rot_speed", float(params.get("rot_speed", 90.0)))
    st.setdefault("preset", dict(DEFAULT_PRESET))
    return st


def stroke(c, i):
    dd = c.reg.device(c.devs[i]["def"]) or {}
    return float(dd.get("params", {}).get("stroke", 1.0))


def output_cell(c, i):
    d = c.devs[i]["dir"]
    cell = c.cells[i]
    return (cell[0] + DIR_TUPLES[d][0], cell[1] + DIR_TUPLES[d][1], cell[2] + DIR_TUPLES[d][2])


def is_attached(c, i):
    return link_key(c.cells[i], output_cell(c, i)) in c.links


def busy(st):
    return (abs(st.get("ext_target", 0) - st.get("ext", 0)) > 1e-4
            or abs(st.get("angle_target", 0) - st.get("angle", 0)) > 1e-3)


# ---------------------------------------------------------------------------
# 命令
# ---------------------------------------------------------------------------
def cmd_rotate(c, i, angle, speed=None, absolute=False):
    st = c.devs[i]["state"]
    st["angle_target"] = float(angle) if absolute else float(st.get("angle_target", 0.0)) + float(angle)
    if speed is not None:
        st["rot_speed"] = max(1.0, min(float(speed), 3600.0))
    c.wake(soft=True)


def cmd_extend(c, i, amount=1.0, speed=None, absolute=False):
    st = c.devs[i]["state"]
    mx = stroke(c, i)
    t = float(amount) if absolute else float(st.get("ext_target", 0.0)) + float(amount)
    st["ext_target"] = min(max(t, 0.0), mx)
    if speed is not None:
        st["ext_speed"] = max(0.01, min(float(speed), 10.0))
    c.wake(soft=True)


def cmd_retract(c, i, amount=1.0, speed=None):
    cmd_extend(c, i, -float(amount), speed)


def cmd_detach(c, i):
    """出力面の結合を外す。外れたら True。（分かれた部分の処理は world 側で行う）"""
    key = link_key(c.cells[i], output_cell(c, i))
    found = key in c.links or key in c.extras
    c.links.discard(key)
    c.extras.pop(key, None)
    # 出力面の頂点につながる追加結合も外す
    face = set(int(x) for x in JOINT_PAIRS[c.devs[i]["dir"]][:, 0])
    for k in list(c.extras):
        if c.cells[i] in k:
            lst = [e for e in c.extras[k] if not (
                (k[0] == c.cells[i] and e[0] in face) or (k[1] == c.cells[i] and e[1] in face))]
            if len(lst) != len(c.extras[k]):
                found = True
                if lst:
                    c.extras[k] = lst
                else:
                    del c.extras[k]
    if found:
        c.rebuild()
    return found


def face_info(c, i):
    """出力面の中心・法線・4頂点の位置。"""
    d = c.devs[i]["dir"]
    corners = 8 * i + JOINT_PAIRS[d][:, 0]
    pa = c.pos[corners]
    ctr = pa.mean(0)
    n = ctr - c.pos[8 * i:8 * i + 8].mean(0)
    n /= max(np.linalg.norm(n), 1e-9)
    return ctr, n, pa


def cmd_attach(world, c, i):
    """出力面の先にあるブロックと結合する。メッセージを返す。"""
    if is_attached(c, i):
        return "既に結合しています"
    ctr, n, pa = face_info(c, i)
    want = ctr + n * c.size * 0.5          # つながるべきブロックの中心
    best = None
    for other in world.constructs.values():
        if not other.n_vox:
            continue
        cen = other.voxel_centers()
        dist = np.linalg.norm(cen - want, axis=1)
        if other is c:
            dist[i] = np.inf
        k = int(np.argmin(dist))
        lim = max(c.size, other.size) * 0.75
        if dist[k] < lim and (best is None or dist[k] < best[0]):
            best = (float(dist[k]), other, k)
    if best is None:
        return "出力面の先に結合できるブロックがありません"
    _, other, k = best
    target_cell = None
    if other is not c:
        cell_map, xform = _merge_map(c, other)
        target_cell = cell_map[other.cells[k]]
        world.merge_constructs(c, other, cell_map, xform)
    else:
        target_cell = c.cells[k]
    j = c.index[target_cell]
    out = output_cell(c, i)
    pairs = JOINT_PAIRS[c.devs[i]["dir"]]
    if target_cell == out:
        # 格子の隣どうし → 通常の結合（回転・押し出しも使える）
        c.links.add(link_key(c.cells[i], out))
    else:
        # 格子がずれている → 近い頂点どうしを結ぶ
        Pb = c.pos[8 * j:8 * j + 8]
        lst = []
        used = set()
        for ca in pairs[:, 0]:
            dd = np.linalg.norm(Pb - c.pos[8 * i + ca], axis=1)
            for cb in np.argsort(dd):
                if int(cb) not in used:
                    used.add(int(cb))
                    rest = float(dd[cb]) if dd[cb] > 0.15 * c.size else 0.0
                    lst.append((int(ca), int(cb), rest))
                    break
        key = link_key(c.cells[i], target_cell)
        if key[0] != c.cells[i]:
            lst = [(b, a, r) for a, b, r in lst]
        c.extras[key] = lst
    c.rebuild()
    if target_cell == out:
        sync_angle(c, i)
    c.wake(soft=True)
    return "結合しました" + ("" if target_cell == out else "（格子がずれているため回転・押し出しは伝わりません）")


def _merge_map(c, other):
    """other のセルを c の格子に対応させる。
    戻り値 (cell_map, xform)。各ボクセルが c の格子に（90°単位の回転も含めて）揃っていれば
    xform に頂点の並べ替えを入れて格子に合わせる。揃っていなければ離れたセル番号に置き、
    結合は頂点どうしで行う。"""
    R, t = c.rigid_fit()
    cen = other.voxel_centers()
    local = (cen - t) @ R / c.size - 0.5
    rnd = np.round(local)
    if abs(other.size - c.size) < 1e-6 and np.abs(local - rnd).max() < 0.2:
        F = other.voxel_frames()
        M = np.einsum("ij,njk->nik", R.T, np.transpose(F, (0, 2, 1)))  # 旧ローカル→c ローカル
        Mr = np.round(M)
        if np.abs(M - Mr).max() < 0.25 and np.all(np.abs(np.linalg.det(Mr) - 1) < 1e-6):
            m = {cell: tuple(int(x) for x in r) for cell, r in zip(other.cells, rnd)}
            if len(set(m.values())) == len(m) and not any(v in c.index for v in m.values()):
                ok = all(tuple(int(x) for x in np.subtract(m[b], m[a])) in DIR_TUPLES for a, b in other.links)
                if ok:
                    xf = {}
                    for k in range(other.n_vox):
                        if np.allclose(Mr[k], np.eye(3)):
                            continue
                        P = other.pos[8 * k:8 * k + 8]
                        q = (P - P.mean(0)) @ R
                        bits = (q > 0).astype(int)
                        perm = bits[:, 0] + 2 * bits[:, 1] + 4 * bits[:, 2]
                        if len(set(perm.tolist())) != 8:
                            break
                        xf[k] = (perm, Mr[k])
                    else:
                        return m, xf
    off = np.asarray(c.cells).max(0) - np.asarray(other.cells).min(0) + 1000
    return {cell: tuple(int(x) for x in np.add(cell, off)) for cell in other.cells}, {}


def output_angle(c, i, world=None):
    """出力側ブロックの向き（度）。アクチュエーターの軸まわりに、
    軸が横向きなら「真上」から、軸が縦向きなら「構築物の +Z 方向」から測った角度（右ねじの向きが +）。"""
    j = c.index.get(output_cell(c, i))
    if j is None or not is_attached(c, i):
        return 0.0
    ctr, n, _ = face_info(c, i)
    F = c.voxel_frames()[j]
    d = c.devs[i]["dir"]
    axis = d // 2
    # 出力ブロックの、軸に垂直なローカル軸（横軸なら Y、縦軸なら Z）
    r = F[2] if axis == 1 else F[1]
    if world is not None and world.nearest_planet(ctr) is not None:
        up = world.nearest_planet(ctr).up(ctr)
    else:
        up = np.array([0.0, 1.0, 0.0])
    if abs(float(np.dot(n, up))) > 0.7:
        R, _ = c.rigid_fit()
        w = R[:, 2]
    else:
        w = up
    r = r - n * np.dot(r, n)
    w = w - n * np.dot(w, n)
    if np.linalg.norm(r) < 1e-6 or np.linalg.norm(w) < 1e-6:
        return 0.0
    return float(np.degrees(np.arctan2(np.dot(n, np.cross(w, r)), np.dot(w, r))))


def measure_twist(c, i):
    """出力面とその先のブロックの、軸まわりのねじれ角（度）。"""
    j = c.index.get(output_cell(c, i))
    if j is None:
        return None
    pairs = JOINT_PAIRS[c.devs[i]["dir"]]
    pa = c.pos[8 * i + pairs[:, 0]]
    pb = c.pos[8 * j + pairs[:, 1]]
    ctr, n, _ = face_info(c, i)
    va = pa - pa.mean(0)
    vb = pb - pb.mean(0)
    s = float(np.einsum("ij,ij->", np.cross(va, vb), np.broadcast_to(n, va.shape)))
    co = float(np.einsum("ij,ij->", va, vb))
    return float(np.degrees(np.arctan2(s, co)))


def sync_angle(c, i):
    """結合した直後に、今のねじれ角を回転角として取り込む（急に回らないように）。"""
    tw = measure_twist(c, i)
    if tw is None:
        return
    st = c.devs[i]["state"]
    cur = float(st.get("angle", 0.0))
    ang = cur + ((tw - cur + 180.0) % 360.0 - 180.0)
    st["angle"] = st["angle_target"] = ang


def execute_preset(world, c, i):
    """P メニューで設定した動作を実行（E キー・操縦席のキー）。"""
    st = c.devs[i]["state"]
    p = st.get("preset") or dict(DEFAULT_PRESET)
    a = p.get("action", "rotate")
    name = c.devs[i]["name"]
    if a == "rotate":
        cmd_rotate(c, i, p.get("amount", 90.0), p.get("speed"))
        return f"{name}: {p.get('amount', 90):+.0f}° 回転（{p.get('speed', 90):.0f}°/秒）"
    if a == "extend":
        before = st["ext_target"]
        cmd_extend(c, i, p.get("amount", 1.0), p.get("speed"))
        if st["ext_target"] == before:
            return f"{name}: これ以上押し出せません（最大 {stroke(c, i):.1f} ブロック）"
        return f"{name}: {p.get('amount', 1):.2f} ブロック押し出し"
    if a == "retract":
        before = st["ext_target"]
        cmd_retract(c, i, p.get("amount", 1.0), p.get("speed"))
        if st["ext_target"] == before:
            return f"{name}: これ以上引き込めません"
        return f"{name}: {p.get('amount', 1):.2f} ブロック引き込み"
    if a == "detach":
        ok = world.actuator_detach(c, i)
        return f"{name}: " + ("分離しました" if ok else "結合しているものがありません")
    if a == "attach":
        return f"{name}: " + cmd_attach(world, c, i)
    return f"{name}: 不明な動作 {a}"


def preset_text(st):
    p = st.get("preset") or DEFAULT_PRESET
    a = p.get("action", "rotate")
    if a == "rotate":
        return f"{ACTION_NAMES[a]} {p.get('amount', 90):+.0f}° / {p.get('speed', 90):.0f}°/秒"
    if a in ("extend", "retract"):
        return f"{ACTION_NAMES[a]} {p.get('amount', 1):.2f}ブロック / {p.get('speed', 0.5):.2f}ブロック/秒"
    return ACTION_NAMES.get(a, a)


# ---------------------------------------------------------------------------
# 毎ティック
# ---------------------------------------------------------------------------
def step(c, i, dd, dt, powered, ratio):
    st = c.devs[i]["state"]
    init_state(st, dd["params"])
    moving = False
    if powered:
        e, et = st["ext"], st["ext_target"]
        if abs(et - e) > 1e-4:
            stp = st["ext_speed"] * dt * ratio
            st["ext"] = e + float(np.clip(et - e, -stp, stp))
            moving = True
        a, at = st["angle"], st["angle_target"]
        if abs(at - a) > 1e-3:
            stp = st["rot_speed"] * dt * ratio
            st["angle"] = a + float(np.clip(at - a, -stp, stp))
            moving = True
    if moving:
        c.wake(soft=True)
    axis = c.devs[i]["dir"] // 2
    c.act_scale[i] = 1.0
    c.act_scale[i, axis] = 1.0 + st["ext"]
    st["attached"] = is_attached(c, i)

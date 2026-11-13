"""アクチュエーター：回転・押し出し・引き込み・分離・結合（計算は Rust：alpedia_core の game/actuator.rs。結合だけ Python）。

アクチュエーターは向き（出力面）を持つブロック。
- 回転   : 出力面の先につながったブロック（とその先の部分）を、向きを軸に回す
           ヒンジ（別の機器）は、ヒンジのブロックの中心を軸に -90〜+90° 振る（扉・腕の関節など。曲がる側は置き方で決まる）
- 押し出し: 自分が伸びて、出力面の先を押し出す（最大でブロック1つ分）
- 引き込み: 伸びた分を引き戻す
- 分離   : 出力面の結合を切り離す（先の部分は別の構築物になる）
- 結合   : 出力面の先にあるブロック（別の構築物でも可）とつなぐ

操作方法は2通り:
- コンピューターから: スクリプトで actuator.rotate(90) / .extend(1.0) / .detach() など
- 手動: P で動作設定メニューを開き、動作の種類・量・速度を決めて閉じ、E で実行
"""
import numpy as np

from . import native
from .construct import DIR_TUPLES, JOINT_PAIRS, link_key

ACTIONS = ["rotate", "extend", "retract", "detach", "attach", "none"]
ACTION_NAMES = {"rotate": "回転", "extend": "押し出し", "retract": "引き込み", "detach": "分離", "attach": "結合",
                "none": "なし"}
# "none"（なし）：E・操縦席のキー（X）では動かさない。コンピューターのプログラムだけで動かすアクチュエーターに
HINGE_ACTIONS = ["rotate", "detach", "attach", "none"]       # ヒンジは押し出さない（rotate は「曲げる」）
# アクチュエーターの種類（定義の params.motion）と、できる動作。分離・結合はどれでもできる
#   rotor（ローター。機器 "actuator"）：回す / piston（ピストン）：押し出す・引き込む / hinge（ヒンジ）：曲げる
#   motion の無い MOD の機器は、昔のアクチュエーターと同じく全部できる（"both"）
MOTION_ACTIONS = {"rotor": ["rotate", "detach", "attach", "none"], "piston": ["extend", "retract", "detach", "attach", "none"],
                  "hinge": HINGE_ACTIONS, "both": ACTIONS}
MOTION_NAMES = {"rotor": "ローター", "piston": "ピストン", "hinge": "ヒンジ", "both": "アクチュエーター"}
DEFAULT_PRESET = {"action": "rotate", "amount": 90.0, "speed": 90.0}
PISTON_PRESET = {"action": "extend", "amount": 1.0, "speed": 0.5}


_C = native.require()
MotionError = _C.MotionError

HINGE_MAX = 90.0            # ヒンジで曲げられる角度（-90〜+90°。0 がまっすぐ）
HINGE_AMOUNTS = [-90.0, -60.0, -45.0, -30.0, -15.0, 0.0, 15.0, 30.0, 45.0, 60.0, 90.0]   # P メニューで選べる角度

motion = _C.act_motion                      # アクチュエーターの種類："rotor" / "piston" / "hinge" / "both"
actions_for = _C.act_actions_for
default_preset = _C.act_default_preset
migrate_device = _C.act_migrate_device      # 古いセーブの「アクチュエーター」で押し出していた物はピストンに
check_action = _C.act_check_action          # その動作ができなければ MotionError
hinge_sides = _C.act_hinge_sides
default_hinge_side = _C.act_default_hinge_side
hinge_pair = _C.act_hinge_pair
is_hinge_dev = _C.act_is_hinge_dev
hinge_side = _C.act_hinge_side
hinge_of = _C.act_hinge_of                  # ヒンジなら軸になる辺（出力面の4隅のうち2つ）。回転なら None
is_actuator = _C.act_is_actuator
stroke = _C.act_stroke
output_cell = _C.act_output_cell
is_attached = _C.act_is_attached
busy = _C.act_busy
cmd_detach = _C.act_cmd_detach              # 出力面の結合を外す（外れたら True）
face_info = _C.act_face_info                # 出力面の中心・法線・4頂点の位置
measure_twist = _C.act_measure_twist
sync_angle = _C.act_sync_angle
output_angle = _C.act_output_angle          # 出力側ブロックの向き（度）
execute_preset = _C.act_execute_preset      # P メニューで設定した動作を実行（E キー・操縦席のキー）
preset_text = _C.act_preset_text
step = _C.act_step                          # 毎ティック


def side_toward(frame_rows, d, look):
    """置くとき：出力の軸に垂直な4つの向きのうち、見ている向き look にいちばん近いもの（その側へ曲がる）。"""
    return _C.act_side_toward(np.ascontiguousarray(frame_rows, dtype=np.float64).reshape(3, 3), int(d),
                              [float(x) for x in look])


def mode_name(st, dev=None):
    return _C.act_mode_name(st, dev)


def init_state(st, params):
    """足りない項目を既定値で埋める（形は devstate の表）。"""
    from . import devstate
    return devstate.fill("actuator", params, st)


def cmd_rotate(c, i, angle, speed=None, absolute=False):
    _C.act_cmd_rotate(c, i, angle, speed, absolute)


def cmd_extend(c, i, amount=1.0, speed=None, absolute=False):
    _C.act_cmd_extend(c, i, amount, speed, absolute)


def cmd_retract(c, i, amount=1.0, speed=None):
    cmd_extend(c, i, -float(amount), speed)


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



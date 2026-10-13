"""初回起動時にサンプルワールド「demo」を作る。"""
import json

import numpy as np

from . import paths, log
from .construct import Construct
from .planet import make_planet
from .registry import Registry
from .world import default_bodies

L = log.get("demo")

HOVER_SCRIPT = '''"""ホバー制御スクリプト（サンプル）

コンピューターを E で起動すると、高度 target m で浮くように推進機を調整します。
四隅の推進機の出力差で姿勢（傾き）も保ちます。
操縦席に座っている間は W/S で目標高度、矢印キーで傾けて移動できます。
api の使い方は scripts/README.txt を参照。
"""


def tick(api):
    target = api.memory.get("target", 12.0)
    keys = api.input
    if "w" in keys:
        target += 0.2
    if "s" in keys:
        target = max(2.0, target - 0.2)
    api.memory["target"] = target

    # 高度：重力を打ち消す基本スロットル + PD 制御
    err = target - api.altitude
    vs = api.vertical_speed
    base = 0.5 + err * 0.03 - vs * 0.2

    # 姿勢：ローカルY軸を惑星の上方向へ向ける
    ux, uy, uz = api.up_local
    wx, wy, wz = api.angular_velocity_local
    # 矢印キーで少し傾ける（その方向へ移動）
    lean_x = (0.15 if "right arrow" in keys else 0) - (0.15 if "left arrow" in keys else 0)
    lean_z = (0.15 if "up arrow" in keys else 0) - (0.15 if "down arrow" in keys else 0)
    # キーを押していない時は横方向の速度を打ち消すように傾ける
    vx, vy, vz = api.velocity_local
    if lean_x == 0:
        lean_x = max(-0.1, min(0.1, -vx * 0.04))
    if lean_z == 0:
        lean_z = max(-0.1, min(0.1, -vz * 0.04))
    ex, ez = ux + lean_x, uz + lean_z
    # 重心の偏りを打ち消す積分項
    ix = api.memory.get("ix", 0.0) + ex * 0.05
    iz = api.memory.get("iz", 0.0) + ez * 0.05
    ix, iz = max(-1, min(1, ix)), max(-1, min(1, iz))
    api.memory["ix"], api.memory["iz"] = ix, iz
    tx = (ez * 2.0 + iz * 1.0) - wx * 1.2
    tz = -(ex * 2.0 + ix * 1.0) - wz * 1.2

    thrusters = api.devices("thruster")
    for t in thrusters:
        ox, oy, oz = t.offset
        adj = -oz * tx + ox * tz
        t.throttle = min(max(base + adj * 0.4, 0.0), 1.0)

    n = api.memory.get("n", 0) + 1
    api.memory["n"] = n
    if n % 60 == 0:
        print(f"高度 {api.altitude:.1f} m / 目標 {target:.1f} m / 傾き {(1 - uy) * 100:.1f}%")
'''

SCRIPT_README = '''ゲーム内 Python スクリプト
========================

ここ（saves/<ワールド>/scripts/）に .py を置き、コンピューター機器の state.script に
ファイル名を設定すると、その構築物を制御できます（E キーで起動/停止）。
ゲーム内ではコンピューターを見て P キーを押すと、このフォルダのスクリプトを表示・編集できます。
コンピューターは電力を使います。電力が無いと停止します。

スクリプトには tick(api) 関数を書きます。一定間隔（既定 0.05 秒）で呼ばれます。
import できるのは math / random / statistics のみです。print() はゲーム画面とログに出ます。

api で使えるもの
----------------
api.time              ワールド時刻（秒）
api.position          重心位置 [x,y,z]
api.velocity          速度 [x,y,z]
api.speed             速さ
api.altitude          最寄り惑星の地表からの高度
api.vertical_speed    上下方向の速度
api.up                上方向ベクトル
api.up_local          上方向（構築物ローカル座標。直立なら [0,1,0]）
api.angular_velocity_local  角速度（ローカル座標）
api.velocity_local    速度（ローカル座標）
api.mass              質量 kg
api.power             {"production", "demand", "stored", "capacity", "ratio"}
api.storage           構築物の固形資源
api.input             操縦席に座っている時に押されているキーの集合
api.memory            スクリプト用の保存領域（dict、セーブされる）
api.orientation()     回転行列
api.cavities()        気密空洞の一覧と気圧
api.devices(type=None, name=None)   機器の一覧（type 例: "thruster"）
api.device(name)      名前で機器を取得
api.log(...)          メッセージ表示
api.display(text, face=None, width=None, color=None, background=None)
                      このコンピューターの表面に文字を表示（停止しても残るので看板になる）
                      face: "+X" "-X" "+Y" "-Y" "+Z" "-Z" "all"（省略時はブロックの向き）
                      width: 表示板の幅 m（省略時はブロックの大きさ）
                      color / background: (r,g,b) 0〜1 または 0〜255
api.clear_display()   表示を消す
api.stop()            このコンピューターを停止
api.planet()          最寄り惑星 {"name","center","radius","mu","surface_gravity"}
api.orbit()           軌道 {"altitude","apoapsis","periapsis","eccentricity","period",
                      "horizontal_speed","vertical_speed","orbital_speed","time_to_apoapsis"}

無線通信（構築物にアンテナが必要。アンテナが無い・電力が無い・OFF のときは使うとエラー）
------------------------------------------------------------------------------------
api.send(channel, data)     チャンネルに情報を送る。data は数値・文字・True/False/None と、
                            それらのリスト・辞書（4096 文字まで）。送ったものは 2 秒間届く
api.receive(channel)        前回呼んだ後に届いた情報のリスト（古い順）。各要素は
                            {"data": 情報, "from": 送った構築物の名前, "distance": 距離m, "age": 秒}
api.latest(channel, default=None, max_age=1.0)
                            届いている一番新しい情報（max_age 秒より古ければ default）。消えない
api.antenna                 アンテナが使えるか（True/False）
api.antenna_range           届く距離 m（送る側と受ける側の短いほうまで届く。基本のアンテナは 3000 m）
自分の構築物が送ったものは自分では受け取りません。遠隔操作の例は drone.py / drone_remote.py。

機器（devices() の要素）
------------------------
.name .type .kind .direction .state .cell .offset（重心からのローカル位置）
推進機:         .throttle = 0.0〜1.0
バルブ:         .open = True/False
発電機/精錬機/加工機/通気口: .enabled = True/False
タンク:         .contents（読み取り）
バッテリー:     .charge（読み取り）
コンピューター: .display(text, ...) / .clear_display()（他のコンピューターに表示）
アクチュエーター（出力面の先の部分を動かす）:
  .rotate(角度, speed=度/秒)        相対回転      .rotate_to(角度, speed)   絶対角度へ
  .extend(量, speed=ブロック/秒)    押し出し（最大ブロック1つ分）
  .retract(量, speed)               引き込み      .extend_to(位置 0〜1, speed)
  .detach()                         分離          .attach()  出力面の先のブロックと結合
  .execute()                        P メニューで設定した動作を実行
  .angle .extension .attached .busy（読み取り）
  .output_angle                     出力側ブロックの世界から見た向き（度・横軸なら真上が 0）
'''


def _surface(planet, xz, extra=0.0):
    p = np.array([xz[0], planet.radius, xz[1]], dtype=np.float64)
    up = p / np.linalg.norm(p)
    r = float(planet.surface_radius(up[None])[0])
    return up * (r + extra)


def _rot_for(up):
    up = up / np.linalg.norm(up)
    fwd = np.cross([1.0, 0, 0], up)
    fwd /= np.linalg.norm(fwd)
    right = np.cross(up, fwd)
    return np.stack([right, up, fwd], axis=1)


def build_rocket():
    V = []
    S = "aluminum"
    # 1段目: 四隅に推進機、それ以外は配管
    for x in range(3):
        for z in range(3):
            if (x, z) in ((0, 0), (2, 0), (0, 2), (2, 2)):
                V.append(((x, 0, z), "steel", {"def": "thruster", "dir": 2, "name": f"推進機{len(V)}"}))
            else:
                V.append(((x, 0, z), S, {"def": "pipe", "dir": 2}))
    # 2〜3段目: 燃料タンク
    for y in (1, 2):
        for x in range(3):
            for z in range(3):
                V.append(((x, y, z), "steel", {"def": "tank", "dir": 2,
                                               "state": {"contents": {"fuel": 120.0}}}))
    # 4段目: 発電機・バッテリー・コンピューター・手回し発電機など
    top = {(0, 0): {"def": "generator", "dir": 2, "state": {"enabled": False}},
           (1, 0): {"def": "battery", "dir": 2},
           (2, 0): {"def": "computer", "dir": 2, "name": "ホバー制御", "state": {"script": "hover.py"}},
           (0, 1): {"def": "crank", "dir": 2},
           (1, 1): {"def": "seat", "dir": 2, "name": "操縦席"},
           (2, 1): {"def": "valve", "dir": 2, "name": "燃料バルブ"},
           }
    for x in range(3):
        for z in range(3):
            d = top.get((x, z))
            V.append(((x, 3, z), S if d is None else "steel", d))
    return V


def build_cabin():
    """4x4x4 の殻（中が 2x2x2 の空洞）＋ 通気口・空気タンク・バッテリー。"""
    V = []
    for x in range(4):
        for y in range(4):
            for z in range(4):
                if 1 <= x <= 2 and 1 <= y <= 2 and 1 <= z <= 2:
                    continue
                mat = "glass" if (y in (1, 2) and z == 0 and x in (1, 2)) else "steel"
                V.append(((x, y, z), mat, None))
    # 壁の一部を機器に
    def setdev(cell, dev):
        for k, v in enumerate(V):
            if v[0] == cell:
                V[k] = (cell, "steel", dev)
    setdev((3, 1, 1), {"def": "air_vent", "dir": 1, "name": "通気口"})
    V.append(((4, 1, 1), "steel", {"def": "tank", "dir": 2, "name": "空気タンク", "state": {"contents": {"air": 50.0}}}))
    V.append(((4, 0, 1), "steel", {"def": "battery", "dir": 2}))
    return V


def build_tower():
    V = []
    for y in range(8):
        for x in range(2):
            for z in range(2):
                V.append(((x, y, z), "wood", None))
    return V


def build_blob(mat, n=3):
    return [((x, y, z), mat, None) for x in range(n) for y in range(n) for z in range(n)]


def pyramid_obj():
    """インポート例：四角錐の OBJ（素材割当て表だけを付ける）。"""
    v = [(-2, 0, -2), (2, 0, -2), (2, 0, 2), (-2, 0, 2), (0, 3, 0)]
    f = [(1, 2, 3), (1, 3, 4), (1, 5, 2), (2, 5, 3), (3, 5, 4), (4, 5, 1)]
    lines = ["# 四角錐", "o pyramid"] + [f"v {a} {b} {c}" for a, b, c in v]
    lines += ["usemtl base"] + [f"f {a} {b} {c}" for a, b, c in f[:2]]
    lines += ["usemtl stone"] + [f"f {a} {b} {c}" for a, b, c in f[2:]]
    return "\n".join(lines) + "\n"


def create_demo(name="demo", planet="terra"):
    wdir = paths.world_dir(name)
    if wdir.exists():
        return wdir
    L.info("サンプルワールド '%s' を作成します", name)
    for sub in ("scripts", "exports", "imports"):
        (wdir / sub).mkdir(parents=True, exist_ok=True)
    reg = Registry().load()
    terra = make_planet(reg.get("planets", planet), [0, 0, 0])
    size = 0.5
    settings = {
        "name": name, "creative": True, "voxel_size": size, "gravity_scale": 1.0, "time": 0.0,
        "cleanup": {"enabled": True, "distance": 1500.0, "seconds": 60.0, "max_vertices": 400, "check_interval": 5.0},
        "player": "default",
        "planets": [{"def": planet, "position": [0, 0, 0], "craters": []},
                    {"def": "luna", "position": [0, 0, 3200], "craters": []}] + default_bodies(planet, False)[1:],
    }
    builds = [
        ("ロケット", build_rocket(), (6, 4)),
        ("気密キャビン", build_cabin(), (-6, 6)),
        ("木の塔", build_tower(), (0, 12)),
        ("水の塊", build_blob("water", 3), (8, 12)),
        ("粘土の塊", build_blob("clay", 3), (-8, 12)),
    ]
    constructs, spawns = [], {}
    for title, vox, xz in builds:
        base = _surface(terra, xz, 0.05)
        R = _rot_for(base)
        c = Construct.from_voxels(reg, vox, base, R, size=size, name=title)
        if title == "水の塊":
            c = Construct.from_voxels(reg, vox, base + R[:, 1] * 4.0, R, size=size, name=title)
        constructs.append(c.to_dict())
        spawns[c.id] = c.spawn_dict()
    (wdir / "world.json").write_text(json.dumps(settings, ensure_ascii=False, indent=1), encoding="utf-8")
    (wdir / "constructs.json").write_text(json.dumps(constructs, ensure_ascii=False, indent=1), encoding="utf-8")
    (wdir / "spawn.json").write_text(json.dumps(spawns, ensure_ascii=False, indent=1), encoding="utf-8")
    (wdir / "mods.json").write_text(json.dumps({"enabled": ["example_mod"]}, ensure_ascii=False, indent=1),
                                    encoding="utf-8")
    (wdir / "scripts" / "hover.py").write_text(HOVER_SCRIPT, encoding="utf-8")
    (wdir / "scripts" / "README.txt").write_text(SCRIPT_README, encoding="utf-8")
    (wdir / "imports" / "pyramid.obj").write_text(pyramid_obj(), encoding="utf-8")
    (wdir / "imports" / "pyramid.alp.json").write_text(json.dumps({
        "format": "alpedia-construct", "version": 1, "name": "ピラミッド", "obj": "pyramid.obj",
        "voxel_size": 0.5, "scale": 1.0, "materials": {"stone": "rock", "base": "steel"},
        "default_material": "rock"}, ensure_ascii=False, indent=1), encoding="utf-8")
    return wdir

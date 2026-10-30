"""迎撃砲塔・レーダー・ミサイル発射機の、スクリプトとブロックプログラムの口。

スクリプト（Python）：
    d = api.devices("turret")[0];  d.aim([x, y, z]);  d.fire()     砲塔を矢印の向きに向けて撃つ（ビームは一瞬で届く）
    d.ready                                                         撃てるか（冷えているか）
    api.devices("launcher")[0].launch([x, y, z])                    矢印の先の地点へミサイルを撃つ（構築物の重心から）
    api.nearest_missile()   いちばん近いミサイルへの矢印 [x,y,z]（無ければ [0,0,0]）。レーダーが要る
    api.missile_count()     レーダーに映っているミサイルの数
    api.missiles()          [{"position", "velocity", "distance"}]（近い順）
"""
import numpy as np

from main import blocks as B
from main import extensions as ext


def _weapons(port):
    return port.world.addon("weapons")


def _vec(v):
    try:
        return [float(v[0]), float(v[1]), float(v[2])]
    except (TypeError, ValueError, IndexError):
        raise ValueError("矢印は [x, y, z] で渡してください")


# --- 砲塔 ------------------------------------------------------------------
def turret_aim(h, vec):
    h.data["aim"] = _vec(vec)


def turret_fire(h):
    h.data["fire"] = float(h.port.time)


def turret_ready(h):
    w = _weapons(h.port)
    return bool(w is not None and w.turret_ready(h.port.construct, h.index))


# --- ミサイル発射機 --------------------------------------------------------
def launcher_launch(h, vec):
    w = _weapons(h.port)
    if w is None:
        return False
    com = h.port.pose()[0]
    target = np.asarray(com, float) + np.asarray(_vec(vec), float)
    return w.launch_from(h.port.construct, h.index, target)


# --- レーダー --------------------------------------------------------------
def _contacts(api):
    port = api._p
    w = _weapons(port)
    if w is None:
        return []
    return w.radar_contacts(port.construct)


def api_missiles(api):
    return [{"position": list(p), "velocity": list(v), "distance": d} for p, v, d in _contacts(api)]


def api_missile_count(api):
    return len(_contacts(api))


def api_nearest_missile(api):
    cs = _contacts(api)
    if not cs:
        return [0.0, 0.0, 0.0]
    com = np.asarray(api._p.pose()[0], float)
    return (np.asarray(cs[0][0], float) - com).tolist()


def api_nearest_distance(api):
    cs = _contacts(api)
    return float(cs[0][2]) if cs else 0.0


ext.register_device("turret", methods={"aim": turret_aim, "fire": turret_fire}, props={"ready": turret_ready})
ext.register_device("launcher", methods={"launch": launcher_launch})
ext.register_api("missiles", api_missiles)
ext.register_api("missile_count", api_missile_count)
ext.register_api("nearest_missile", api_nearest_missile)
ext.register_api("nearest_missile_distance", api_nearest_distance)

B.register_ext(
    categories=[("weapon", "迎撃・ミサイル", (0.85, 0.35, 0.30))],
    statements={
        "turret_aim": dict(cat="weapon", label="砲塔 {dev} を {vec} に向ける", slots={"dev": "dev:turret", "vec": "vec"},
                           code=["for d in _devs(api, {dev}, 'turret'):", "    d.aim({vec})"]),
        "turret_fire": dict(cat="weapon", label="砲塔 {dev} で撃つ", slots={"dev": "dev:turret"},
                            code=["for d in _devs(api, {dev}, 'turret'):", "    d.fire()"]),
        "launcher_fire": dict(cat="weapon", label="発射機 {dev} で {vec} の先へミサイルを撃つ",
                              slots={"dev": "dev:launcher", "vec": "vec"},
                              code=["for d in _devs(api, {dev}, 'launcher'):", "    d.launch({vec})"]),
    },
    defaults={
        "turret_aim": {"dev": "*", "vec": {"type": "vsensor", "name": "nearest_missile"}},
        "turret_fire": {"dev": "*"},
        "launcher_fire": {"dev": "*", "vec": {"type": "vec", "frame": "船から見た", "x": {"type": "num", "v": 0},
                                               "y": {"type": "num", "v": 0}, "z": {"type": "num", "v": 100}}},
    },
    sensors={"missile_count": ("ミサイルの数", "api.missile_count()"),
             "missile_dist": ("いちばん近いミサイルまでの距離", "api.nearest_missile_distance()")},
    vsensors={"nearest_missile": ("いちばん近いミサイルへの矢印", "_V(api.nearest_missile())")},
    devbools={"撃てる": "ready"},
)

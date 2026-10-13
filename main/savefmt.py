"""セーブデータの決まった形（保存形式）。

saves/<ワールド名>/ の各ファイルに入れてよい項目と型をここで決める。
読み込み時に check_*() で調べ、形が違う所はログに出す（読める所は読む）。
機器の状態（voxels の5番目の state）の中身は devstate.py で決める。

形式を変えたら FORMAT を上げ、古い形式の読み替えを書く。
"""

FORMAT = 1

# world.json（コアの項目）。拡張（街パックなど）の項目は、拡張の settings_keys で足す
WORLD_KEYS = {
    "format": int, "name": str, "creative": bool, "voxel_size": (int, float), "gravity_scale": (int, float),
    "cleanup": dict, "player": str, "planets": list, "time": (int, float), "full_rigid": bool, "tutorial": bool,
}
CLEANUP_KEYS = {"enabled": bool, "distance": (int, float), "seconds": (int, float), "max_vertices": int,
                "check_interval": (int, float)}
PLANET_KEYS = {"def": str, "position": list, "craters": list, "override": dict}

# constructs.json の1つ分
CONSTRUCT_REQUIRED = {"id": str, "name": str, "voxel_size": (int, float), "voxels": list}
CONSTRUCT_OPTIONAL = {"links": list, "extras": list, "storage": dict, "cavity_air": list, "vertices": list,
                      "rest": list, "anchor": str}
DEVICE_KEYS = {"def": str, "dir": int, "name": str, "state": dict}

# spawn.json の1つ分（{構築物ID: これ}）
SPAWN_KEYS = {"position": list, "rotation": list, "velocity": list, "angular_velocity": list}

# player.json
PLAYER_KEYS = {"position": list, "velocity": list, "inventory": dict, "seated": (list, type(None)), "jetpack": bool,
               "def": str, "starter_given": bool, "credits_given": bool, "hotbar": (list, type(None))}


def _keys(d, spec, where, required=(), extra=()):
    bad = []
    if not isinstance(d, dict):
        return [f"{where}: 辞書ではありません"]
    for k in required:
        if k not in d:
            bad.append(f"{where}: '{k}' がありません")
    for k, v in d.items():
        t = spec.get(k)
        if t is None:
            if k not in extra:
                bad.append(f"{where}: 知らない項目 '{k}'")
            continue
        if isinstance(v, bool) and t in (int, float, (int, float)):
            bad.append(f"{where}: '{k}' の型が違います（{type(v).__name__}）")
        elif not isinstance(v, t):
            bad.append(f"{where}: '{k}' の型が違います（{type(v).__name__}）")
    return bad


def check_world(s, extra_keys=()):
    bad = _keys(s, WORLD_KEYS, "world.json", required=("planets",), extra=extra_keys)
    if isinstance(s.get("cleanup"), dict):
        bad += _keys(s["cleanup"], CLEANUP_KEYS, "world.json cleanup")
    for i, p in enumerate(s.get("planets") or []):
        bad += _keys(p, PLANET_KEYS, f"world.json planets[{i}]", required=("def",))
    return bad


def check_construct(d):
    where = f"構築物 {d.get('id', '?') if isinstance(d, dict) else '?'}"
    bad = _keys(d, {**CONSTRUCT_REQUIRED, **CONSTRUCT_OPTIONAL}, where, required=tuple(CONSTRUCT_REQUIRED))
    for j, v in enumerate(d.get("voxels") or [] if isinstance(d, dict) else []):
        if not (isinstance(v, list) and len(v) == 5 and all(isinstance(x, int) for x in v[:3])
                and isinstance(v[3], str)):
            bad.append(f"{where} voxels[{j}]: [x, y, z, 素材, 機器] の形ではありません")
            continue
        if v[4] is not None:
            bad += _keys(v[4], DEVICE_KEYS, f"{where} voxels[{j}] 機器", required=tuple(DEVICE_KEYS))
    return bad


def check_spawn(sp):
    bad = []
    for cid, d in (sp or {}).items():
        bad += _keys(d, SPAWN_KEYS, f"spawn.json {cid}")
    return bad


def check_player(d):
    return _keys(d, PLAYER_KEYS, "player.json")

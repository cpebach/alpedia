"""ワールド：読み込み・保存・シミュレーション・編集操作。本体は Rust（alpedia_core.World）。

saves/<ワールド名>/
    world.json       ワールド設定（惑星・自動削除・クリエイティブ等）
    constructs.json  構築物のデータ（ボクセル・機器・結合・資源）
    spawn.json       構築物の初期位置・初期速度・向き
    mods.json        このワールドに適用するMOD
    player.json      プレイヤーの状態
    scripts/         コンピューター用 Python スクリプト
    exports/         OBJ 出力先
    imports/         OBJ 読み込み元
ここは既定の設定（表）と Rust の入り口だけ。
"""
from . import log, native

_c = native.require()
L = log.get("world")

DEFAULT_SETTINGS = {
    "voxel_size": 0.5,
    "gravity_scale": 1.0,
    "cleanup": {"enabled": True, "distance": 1500.0, "seconds": 60.0, "max_vertices": 400,
                "check_interval": 5.0},
    "player": "default",
    "planets": [{"def": "terra", "position": [0, 0, 0]}],
    # 完全剛体モード（構築物は丸ごと1つの剛体）が既定：やわらかいブロックの計算は重いので、ローエンドでも遊べるように
    "full_rigid": True,
}

# 新しいワールドの天体：選んだ惑星＋月（ルナ）＋小惑星3つ。月・小惑星・洞窟のまわりでは星鉱石が採れる
MOON = {"def": "luna", "position": [0, 0, 4000], "craters": []}
ASTEROIDS = [
    {"def": "asteroid", "position": [5200, 1500, -3000], "craters": [], "override": {"name": "小惑星A", "seed": 21}},
    {"def": "asteroid", "position": [-5800, -800, 2600], "craters": [],
     "override": {"name": "小惑星B", "seed": 22, "radius": 20}},
    {"def": "asteroid", "position": [2500, 3500, 6500], "craters": [],
     "override": {"name": "小惑星C", "seed": 23, "radius": 36}},
]

default_bodies = _c.world_default_bodies
_read_json = _c.world_read_json
_write_json = _c.world_write_json
_drop_fuel_generators = _c.world_drop_fuel_generators
missing_references = _c.world_missing_references
_limit_dv = _c.world_limit_dv
World = _c.World

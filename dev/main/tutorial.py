"""チュートリアルワールドの作成（ストーリー仕立て）。

物語・舞台・目標はストーリー MOD（mods/story）が持つ。ここではワールドの入れ物
（惑星テラ・月ルナ・小惑星、MOD「story」を有効、ストーリーの印）だけを作り、
最初に読み込んだときに MOD が地面の跡・構築物・スクリプトを作る。
台本は mods/story/code/script.py、舞台は mods/story/code/sites.py（docs/ストーリー台本.md にまとめ）。
"""
import json

from . import paths, log
from .world import default_bodies

L = log.get("tutorial")

STORY_MOD = "story"


def create_tutorial(name="tutorial"):
    wdir = paths.world_dir(name)
    if wdir.exists():
        raise FileExistsError(f"ワールド「{name}」は既にあります")
    L.info("チュートリアルワールド '%s' を作成します", name)
    for sub in ("scripts", "exports", "imports"):
        (wdir / sub).mkdir(parents=True, exist_ok=True)
    override = {"noise_amp": 1.5}        # なだらかなテラ（舞台の配置はこの地形で決めてある）
    settings = {
        "name": name, "format": 2, "voxel_size": 0.5, "gravity_scale": 1.0, "time": 0.0,
        "tutorial": True,
        "cleanup": {"enabled": False, "distance": 5000.0, "seconds": 120.0, "max_vertices": 400, "check_interval": 5.0},
        "player": "default",
        "planets": [{"def": "terra", "position": [0, 0, 0], "craters": [], "override": override}]
        + default_bodies("terra", True)[1:],
        "story": {"built": False},
    }
    (wdir / "world.json").write_text(json.dumps(settings, ensure_ascii=False, indent=1), encoding="utf-8")
    (wdir / "constructs.json").write_text("[]", encoding="utf-8")
    (wdir / "spawn.json").write_text("{}", encoding="utf-8")
    (wdir / "mods.json").write_text(json.dumps({"enabled": [STORY_MOD]}, ensure_ascii=False, indent=1),
                                    encoding="utf-8")
    return wdir


def is_story_world(name):
    """物語のチュートリアルのワールドか（前の版の、看板のチュートリアルではないか）。"""
    try:
        s = json.loads((paths.world_dir(name) / "world.json").read_text(encoding="utf-8"))
    except Exception:  # noqa: BLE001
        return False
    return isinstance(s.get("story"), dict)

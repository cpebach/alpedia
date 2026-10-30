"""新しいワールドを作る（空のワールド・MOD が形を決めるワールド）。

シナリオ（物語のチュートリアル）は「シナリオモード」の MOD（mods/story）として遊ぶ。
新しいワールドを作るときにその MOD を選ぶと、MOD の mod.json の "world" の形（惑星・ワールドの設定）で作り、
最初に読み込んだときに MOD が舞台（構築物・スクリプト）を作る。始め方はこの1つだけ
（前の版の「チュートリアル」ボタン・テンプレートはなくなった。前の版のチュートリアルのワールドはそのまま続きから遊べる）。
"""
import json

from . import log, modmeta, paths
from .world import DEFAULT_SETTINGS, default_bodies

L = log.get("newworld")

STORY_MOD = "story"
SCENARIO_WORLD = "scenario"          # 初めて起動したときに作るシナリオのワールドの名前


def create_world(name, mods=(), settings_patch=None, planet="terra"):
    """ワールドを作る。mods：選んだ MOD（依存する MOD はここで足す）。
    選んだ MOD に "world"（ワールドの形）を持つものがあれば、その形で作る（惑星の選択は使わない）。"""
    from .content import SCRIPT_README
    from .savefmt import FORMAT
    wdir = paths.world_dir(name)
    if wdir.exists():
        raise FileExistsError(f"ワールド「{name}」は既にあります")
    mods, added, missing = modmeta.resolve(list(mods))
    if missing:
        L.warning("依存する MOD が見つかりません: %s", missing)
    for sub in ("scripts", "exports", "imports"):
        (wdir / sub).mkdir(parents=True, exist_ok=True)
    s = json.loads(json.dumps(DEFAULT_SETTINGS))
    s["name"] = name
    s["format"] = FORMAT
    s["planets"] = default_bodies(planet)
    patch = dict(settings_patch or {})
    pre_id, preset = modmeta.world_preset(mods)
    if preset:
        L.info("MOD '%s' のワールドの形で作ります", pre_id)
        if isinstance(preset.get("planets"), list):
            s["planets"] = json.loads(json.dumps(preset["planets"]))
        else:
            s["planets"] = default_bodies(preset.get("main", "terra"), preset.get("moon", True),
                                          preset.get("asteroids", True))
            if isinstance(preset.get("main_override"), dict):
                s["planets"][0]["override"] = dict(preset["main_override"])
        for k in ("voxel_size", "gravity_scale"):
            patch.pop(k, None)                           # 舞台はこの大きさで作ってある
        cl = patch.pop("cleanup", None)
        s.update(json.loads(json.dumps(preset.get("settings", {}))))
        if cl and not (preset.get("settings") or {}).get("cleanup"):
            s["cleanup"].update(cl)
    else:
        if "cleanup" in patch:
            s["cleanup"].update(patch.pop("cleanup"))
    s.update(patch)
    s["cleanup"] = {**DEFAULT_SETTINGS["cleanup"], **s.get("cleanup", {})}
    (wdir / "world.json").write_text(json.dumps(s, ensure_ascii=False, indent=1), encoding="utf-8")
    (wdir / "constructs.json").write_text("[]", encoding="utf-8")
    (wdir / "spawn.json").write_text("{}", encoding="utf-8")
    (wdir / "mods.json").write_text(json.dumps({"enabled": list(mods)}, ensure_ascii=False, indent=1), encoding="utf-8")
    (wdir / "scripts" / "README.txt").write_text(SCRIPT_README, encoding="utf-8")
    L.info("ワールド作成: %s（MOD %s）", name, mods)
    return wdir


def create_tutorial(name=SCENARIO_WORLD):
    """シナリオモードのワールドを作る（テスト・初回起動用）。"""
    return create_world(name, [STORY_MOD])


def is_story_world(name):
    """シナリオ（物語）のワールドか。"""
    try:
        s = json.loads((paths.world_dir(name) / "world.json").read_text(encoding="utf-8"))
    except Exception:  # noqa: BLE001
        return False
    return isinstance(s.get("story"), dict)

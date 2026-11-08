"""本体だけで作れる「はじめの設計図」（sauce/blueprints/starter_*.alp.json）を作る。

    python tools/make_starter_blueprints.py

どれも本体の素材・機器だけでできている（MOD は要らない）。クリエイティブで新しく始めたワールドには、
出現地点のそばに「はじめのロケット」が1台置かれる。
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
sys.path.insert(0, str(Path(__file__).resolve().parent))

from main import paths  # noqa: E402

UP, FWD = 2, 4          # 機器の向き：+Y（上）・+Z（前）


def dev(def_id, name, d=UP, **state):
    return {"def": def_id, "dir": d, "name": name, **({"state": state} if state else {})}


def rocket():
    """1列のロケット：下から 推進機・バルブ・タンク・タンク・バッテリー・操縦席。足は4枚のアルミのひれ。
    座って Space で点火、バルブの開き具合（P）で推力を変える。"""
    V = []
    col = [("thruster", "推進機"), ("valve", "バルブ"), ("tank", "燃料タンク"), ("tank", "燃料タンク"),
           ("battery", "バッテリー")]
    for y, (d, n) in enumerate(col):
        st = {"contents": {"fuel": 800.0}} if d == "tank" else ({"open": 0.75} if d == "valve" else {})
        V.append([1, y, 1, "steel", dev(d, n, **st)])
    V.append([1, len(col), 1, "steel", dev("seat", "操縦席", FWD)])
    for x, z in ((0, 1), (2, 1), (1, 0), (1, 2)):           # ひれ（下の2段）
        V.append([x, 0, z, "aluminum", None])
        V.append([x, 1, z, "aluminum", None])
    return ("starter_rocket", "はじめのロケット",
            "推進機・バルブ・燃料タンク・バッテリー・操縦席だけの小さなロケット。座って Space で点火、"
            "バルブ（P）で推力を変える。止めるときはバルブを閉じる", V)


def factory():
    """精錬機 → 加工機、精錬機 → バルブ → タンク。電気は太陽光パネルとバッテリー。"""
    V = []
    for x in range(5):
        for z in range(3):
            V.append([x, 0, z, "steel", None])                  # 床
    V += [[0, 1, 1, "steel", dev("refiner", "精錬機")],
          [1, 1, 1, "steel", dev("fabricator", "加工機")],
          [0, 1, 0, "steel", dev("valve", "燃料のバルブ", open=1.0)],
          [0, 1, 2, "steel", dev("battery", "バッテリー")],
          [1, 1, 0, "steel", dev("tank", "燃料タンク")],
          [3, 1, 1, "steel", dev("crank", "手回し発電機")]]
    for x in range(2, 5):
        V.append([x, 1, 0, "steel", dev("solar_panel", "太陽光パネル")])
    return ("starter_factory", "はじめの工場",
            "精錬機（鉱石 → 鉄・ケイ砂、硫黄 → 燃料）と加工機（鉄 → 機械部品）。燃料はバルブの先のタンクへ。"
            "電気は太陽光パネル・バッテリー・手回し発電機", V)


def room():
    """気密室：鋼鉄の箱（中は 3×2×3 の空洞）、ガラスの窓、通気口で与圧する。"""
    V = []
    for x in range(5):
        for y in range(4):
            for z in range(5):
                inner = 1 <= x <= 3 and 1 <= y <= 2 and 1 <= z <= 3
                if inner:
                    continue
                m = "glass" if (z == 0 and y == 2 and 1 <= x <= 3) else "steel"
                V.append([x, y, z, m, None])
    for v in V:
        if v[:3] == [4, 1, 2]:
            v[4] = dev("air_vent", "通気口", 1)
        if v[:3] == [0, 1, 2]:
            v[4] = dev("tank", "空気タンク", contents={"air": 800.0})
        if v[:3] == [2, 3, 2]:
            v[4] = dev("solar_panel", "太陽光パネル")
    return ("starter_room", "はじめの気密室",
            "完全に囲まれた空洞は自動で気密になる。通気口（E で与圧／減圧）で空気タンクの空気を入れる。"
            "宇宙の基地の練習に", V)


def story_helpers():
    """ブロックを作る道具（mods/story/code/programs.py。MOD としては読み込まず、ファイルとして使う）。"""
    import importlib.util
    spec = importlib.util.spec_from_file_location("_programs", paths.MODS / "story" / "code" / "programs.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def transformer():
    """変形ロボット（ロケット ⇄ 人型）：形・結合（部分の間はアクチュエーターだけ）・ブロックプログラム。"""
    from main import blocks as B
    import transformer_robot as TR
    V = TR.voxels()
    vox = [[c[0], c[1], c[2], m, d] for c, m, d, _ in V]
    prog = TR.program(story_helpers())
    scripts = {"transformer.blocks.json": json.dumps(prog, ensure_ascii=False, indent=1),
               "transformer.py": B.to_python(prog, "transformer.blocks.json")}
    return "transformer_robot", TR.NAME, TR.DESC, vox, TR.links(V), scripts


def main():
    out = paths.SAUCE / "blueprints"
    out.mkdir(parents=True, exist_ok=True)
    items = [(*x, None, {}) for x in (rocket(), factory(), room())] + [transformer()]
    for fname, name, desc, vox, links, scripts in items:
        meta = {"format": "alpedia-construct", "version": 1, "name": name, "description": desc, "requires": [],
                "voxel_size": 0.5, "scale": 1.0, "default_material": "steel", "voxels": vox, "storage": {},
                "scripts": scripts, **({"links": links} if links else {})}
        (out / f"{fname}.alp.json").write_text(json.dumps(meta, ensure_ascii=False), encoding="utf-8")
        print(f"{fname}.alp.json  {name}  ボクセル {len(vox)}")


if __name__ == "__main__":
    main()

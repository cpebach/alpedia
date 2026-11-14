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


FERRY_L, FERRY_W, FERRY_H = 24, 12, 6      # 船体：長さ 12 m・幅 6 m・高さ 3 m（0.5 m のブロック）
FERRY_BOW = {20: (1, 10), 21: (2, 9), 22: (3, 8), 23: (4, 7)}     # 船首：前ほど細く（z: x の範囲）


def _ferry_cells():
    """船体の床の形（x, z）"""
    out = set()
    for z in range(FERRY_L):
        lo, hi = FERRY_BOW.get(z, (0, FERRY_W - 1))
        for x in range(lo, hi + 1):
            out.add((x, z))
    return out


def ferry_program(P):
    """操縦席に座っている間：W/S で前・後、A/D で左・右へ回る（後ろの左右の推進機の出力の差）。
    回る速さに合わせて差を弱めて、回りすぎないようにする。降りたらバルブを閉じて止まる。"""
    from main import blocks as B
    N, V, S, key, seated = P.N, P.V, P.S, P.key, P.seated
    setv, iff, if_else, stmt, valve = P.setv, P.iff, P.if_else, P.stmt, P.valve
    add, sub, mul, cmp, show, join, rnd, T = P.add, P.sub, P.mul, P.cmp, P.show, P.join, P.rnd, P.T
    pos = lambda x: P.fn("マイナスなら0", x)      # noqa: E731
    KEEP = 0.02                              # 走っている間のいちばん小さい開き具合（火を消さない）
    p = B.new_program()
    start = B.new_stmt("on_start")
    start["body"] = [setv("出力", N(0.8))]
    tick = B.new_stmt("on_tick")
    tick["body"] = [
        if_else(seated(), [
            setv("前後", sub(key("w"), key("s"))),
            # 右へ回る：左の推進機を強く。回っている速さ（度/秒）の分だけ戻す
            setv("回す", sub(mul(sub(key("d"), key("a")), N(0.35)), mul(S("yaw_rate"), N(0.02)))),
            iff(key("space"), [setv("出力", N(1.0))]),
            iff(key("left shift"), [setv("出力", N(0.35))]),
            setv("前", mul(pos(V("前後")), V("出力"))),
            setv("後", mul(pos(sub(N(0), V("前後"))), V("出力"))),
            valve("バルブ 左", add(V("前"), V("回す"), N(KEEP))),
            valve("バルブ 右", add(sub(V("前"), V("回す")), N(KEEP))),
            valve("バルブ 後退", add(V("後"), N(KEEP))),
            stmt("ignite", dev="*"),
            show(join(T("フェリー\n速さ "), rnd(S("speed")), T(" m/s\nW/S 前後・A/D 回る\nSpace 全速・Shift ゆっくり"))),
        ], [
            valve("バルブ 左", N(0)), valve("バルブ 右", N(0)), valve("バルブ 後退", N(0)),
            show(T("フェリー\n操縦席に座ると動きます")),
        ]),
    ]
    p["scripts"] += [start, tick]
    return p


def ferry():
    """木のフェリー：船体（中は空気の入った空洞）・甲板・船室（ガラスの窓・操縦席・コンピューター）・客席・手すり。
    後ろに前へ押す推進機が左右に2つ（操縦で出力の差をつけて曲がる）と、後ろへ押す推進機が1つ。それぞれにバルブと燃料タンク。"""
    V = {}

    def put(x, y, z, m, d=None):
        V[(x, y, z)] = [x, y, z, m, d]

    foot = _ferry_cells()
    edge = {(x, z) for (x, z) in foot if any((x + dx, z + dz) not in foot for dx, dz in ((1, 0), (-1, 0), (0, 1), (0, -1)))}
    for (x, z) in foot:
        put(x, 0, z, "wood")                              # 船底
        put(x, FERRY_H - 1, z, "wood")                    # 甲板
        if (x, z) in edge:
            for y in range(1, FERRY_H - 1):
                put(x, y, z, "wood")                      # 船べり（中は空気の空洞）
    # 推進機（後ろの壁）・バルブ・燃料タンク（船体の中）。左右と後退で別々につなぐ（となりどうしにしない）
    for x, side, d, nm in ((2, "左", 4, "推進機 左"), (9, "右", 4, "推進機 右"), (6, "後退", 5, "推進機 後退")):
        put(x, 1, 0, "steel", dev("thruster", nm, d))
        put(x, 1, 1, "steel", dev("valve", f"バルブ {side}", open=0.0))
        put(x, 1, 2, "steel", dev("tank", f"燃料タンク {side}", contents={"fuel": 800.0}))
        put(x, 1, 3, "steel", dev("tank", f"燃料タンク {side}", contents={"fuel": 800.0}))
    put(4, 1, 6, "steel", dev("battery", "バッテリー"))
    # 重し：竜骨に沿って船底の上に鋼鉄（重心を下げて、ひっくり返りにくくする）
    for z in range(8, 19):
        for x in (5, 6):
            put(x, 1, z, "steel")
    # 船室（甲板の上、前寄り）：木の壁にガラスの窓、屋根。後ろに出入り口
    y0 = FERRY_H
    cx0, cx1, cz0, cz1 = 3, 8, 12, 17
    for x in range(cx0, cx1 + 1):
        for z in range(cz0, cz1 + 1):
            wall = x in (cx0, cx1) or z in (cz0, cz1)
            for y in range(y0, y0 + 4):
                if not wall:
                    continue
                if z == cz0 and 5 <= x <= 6 and y < y0 + 3:
                    continue                              # 出入り口
                win = y == y0 + 2 and not (x in (cx0, cx1) and z in (cz0, cz1))     # 窓は1段（ガラスは重い）
                put(x, y, z, "glass" if win else "wood")
            put(x, y0 + 4, z, "wood")                     # 屋根
    put(5, y0, cz1 - 1, "steel", dev("seat", "操縦席", FWD))
    put(6, y0, cz1 - 1, "steel", dev("computer", "操船のコンピューター", FWD, script="ferry.py",
                                      blocks="ferry.blocks.json", running=True))
    # 客席（後ろの甲板）と手すり（乗り降りの所はあける）
    for z in (3, 5, 7, 9):
        for x in (1, 2, 3, 8, 9, 10):
            put(x, y0, z, "wood")
    for (x, z) in edge:
        if 8 <= z <= 10 or z >= FERRY_L - 2 or (cx0 <= x <= cx1 and cz0 <= z <= cz1):
            continue
        put(x, y0, z, "aluminum")
    vox = list(V.values())
    from main import blocks as B
    prog = ferry_program(story_helpers())
    scripts = {"ferry.blocks.json": json.dumps(prog, ensure_ascii=False, indent=1),
               "ferry.py": B.to_python(prog, "ferry.blocks.json")}
    return ("starter_ferry", "はじめのフェリー",
            "木の船体（中は空気。竜骨に鋼鉄の重し）で水に浮かぶフェリー。ガイアの海や池に置く。船室の操縦席に座ると、W/S で前後、"
            "A/D で回る（後ろの左右の推進機の出力の差）。Space で全速、Left Shift でゆっくり。燃料は推進機ごとのタンクに",
            vox, None, scripts)


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
    items = [(*x, None, {}) for x in (rocket(), factory(), room())] + [transformer(), ferry()]
    for fname, name, desc, vox, links, scripts in items:
        meta = {"format": "alpedia-construct", "version": 1, "name": name, "description": desc, "requires": [],
                "voxel_size": 0.5, "scale": 1.0, "default_material": "steel", "voxels": vox, "storage": {},
                "scripts": scripts, **({"links": links} if links else {})}
        (out / f"{fname}.alp.json").write_text(json.dumps(meta, ensure_ascii=False), encoding="utf-8")
        print(f"{fname}.alp.json  {name}  ボクセル {len(vox)}")


if __name__ == "__main__":
    main()

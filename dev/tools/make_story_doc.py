"""docs/ストーリー台本.md を mods/story/code/script.py から作り直す。  python tools/make_story_doc.py"""
import importlib.util
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
spec = importlib.util.spec_from_file_location("script", ROOT / "mods/story/code/script.py")
S = importlib.util.module_from_spec(spec)
spec.loader.exec_module(S)

COND = {
    "look": "見ている向き（水平）の変化の合計が 150° 以上", "walk": "救難ビーコンから 3.6 m 以内",
    "beacon": "ビーコンのコンピューターが動いている", "sakura": "サクラに話しかけた（E）",
    "rocks": "谷に幅 1 m・高さ 1.9 m の通り道がある（または谷を抜けた）", "dig": "谷で地面を 3 回掘った",
    "ledge": "段差の 3 m 先まで行った", "valve": "電源のバルブが開いている", "engine": "発電用の推進機 2 つが燃えている",
    "lights": "電源の発電が 0 より大きい（基地の明かりが点く）", "terminal": "明かりの点いた後に本館の端末を E",
    "door": "「待つ」ブロックのある手順で動かしている間に、扉が 2 回開いて閉じた",
    "trips": "同じく、クレーンが 180° と 0° を 3 往復した（最後に風で荷物が落ちる）",
    "model": "お手本の装置から 4.5 m 以内", "rod": "課題のプログラムで棒が 5 秒続けて ±5° 以内",
    "console": "ドローンの操縦台に座っている", "ring": "ドローンの重心が光る輪の中",
    "ore": "採掘場（灰色）で 3 回掘った（鉱石 +20 ずつ）", "sulfur": "採掘場（黄色）で 3 回掘った（硫黄鉱石 +20 ずつ）",
    "load": "精錬機を E（持ち物の鉱石・硫黄鉱石を入れる）。すぐ後にキンギのドローンが配管を切りに来る",
    "pipe": "切られた所に配管がある", "fuel": "工場のタンクに燃料 80 以上", "parts": "加工機に機械部品 4 つ以上",
    "gate": "遮断機のモーターが 90° 前後", "ride": "ホバーバイクに座っている（黒いドローンが追ってくる）",
    "reach": "ルミナス号から 14 m 以内", "hull": "外壁の穴が全部ふさがっている", "thrusters": "下の四隅に推進機",
    "fuel_ship": "燃料タンクが全部 790 以上", "launch": "航法コンピューターが moon.py で動いている",
    "seat": "ルミナス号の操縦席に座っている", "land": "月の発着場の上 2 m 以内・速さ 0.4 m/s 未満",
    "mikaga": "ミカガに話しかけた", "kensu": "ケンスに話しかけた（ミサイルが来はじめる）",
    "intercept_prog": "迎撃の制御が「いちばん近いミサイルへの矢印」に向けて撃つプログラムで動いている",
    "intercept": "迎撃砲塔でミサイルを 12 発撃ち落とした", "board": "ロボットの操縦席に座っている",
    "robot_walk": "ロボットが 25 m 動いた（キンギがミサイルを撃ちはじめる）", "dodge": "ミサイルを 8 発かわした（当たらずに爆発・時間切れ）",
    "clash": "キンギのロボットから 9 m 以内に合わせて 2 秒", "modules": "作業台の四隅に推進モジュール",
    "thrust": "4 基が燃えていて、バルブがどれも 0.6 以上・差 0.05 以内",
    "navi": "再加速の航法が、看板の地点から 10 m 以内の「地点への矢印」で動いている", "rise": "上の 3 つが済んでから 12 秒（途中でキンギが倒れ、ドナチが代わる）",
}


def lines(xs):
    return "\n".join(f"- {w}「{t}」" if w != S.SYS else f"- （{t}）" for w, t in xs)


out = ["# シナリオ台本：それでも", "",
       "シナリオモード（MOD「story」）の物語と、章ごとの目標・判定・セリフ・ムービーの一覧。",
       "ゲームの中身は `dev/mods/story`（台本は `code/script.py`、判定は `code/addon.py`）にあり、この文書は",
       "`python tools/make_story_doc.py` で script.py から作っている。**移植・更新のときは `dev/tests/test_story.py` と見比べる。**",
       "", "## 始め方", "",
       "新規作成 →「MOD を選ぶ…」で「シナリオモード（それでも）」を入れて作る（必要な MOD は一緒に入る）。初めて起動したときは `scenario` ができる。",
       "", "## あらすじ", "", S.__doc__.split("\n\n")[1].strip(), "", "## ムービー", "",
       "| 名前（mods/story/movies/） | 章 |", "|---|---|"]
out += [f"| `{k}.mp4` | {v} |" for k, v in S.MOVIES.items()]
out += ["", "## 章と目標", ""]
for c in S.CHAPTERS:
    out += [f"### {c['title']}（`{c['id']}`）", "", f"場所：{c['place']}", ""]
    if c.get("movie_in"):
        out += ["始めのムービー：" + "、".join(S.MOVIES[m] for m in c["movie_in"]), ""]
    out += ["| 目標 ID | 表示 | 済む条件 | 目印 |", "|---|---|---|---|"]
    out += [f"| `{g}` | {t} | {COND.get(g, '')} | {k or '―'} |" for g, t, k in c["goals"]]
    if c["intro"]:
        out += ["", "始め：", lines(c["intro"])]
    if c.get("hints"):
        out += ["", "ヒント（75 秒済まないとき）：", lines(c["hints"].values())]
    if c.get("events"):
        out += ["", "出来事：", lines(c["events"].values())]
    out += ["", "終わり：", lines(c["outro"])]
    if c.get("movie_out"):
        out += ["", "終わりのムービー：" + "、".join(S.MOVIES[m] for m in c["movie_out"])]
    out.append("")
out += ["## クリアのあと", "", lines(S.ENDING), "", "目標の欄：", ""] + [f"- {x}" for x in S.FREE_PLAY] + [""]
(ROOT.parent / "docs" / "ストーリー台本.md").write_text("\n".join(out), encoding="utf-8")
print("書き出し: docs/ストーリー台本.md")

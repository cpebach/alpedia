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
    "ore": "採掘場（灰色）で 3 回掘った", "sulfur": "採掘場（黄色）で 3 回掘った",
    "load": "精錬機を E（持ち物の鉱石・硫黄鉱石を入れる）。すぐ後にキンギのドローンが配管を切りに来る",
    "pipe": "切られた所に配管がある", "fuel": "工場のタンクに燃料 80 以上", "parts": "加工機に機械部品 4 つ以上",
    "gate": "遮断機のモーターが 90° 前後", "ride": "ホバーバイクに座っている（黒いドローンが追ってくる）",
    "reach": "ルミナス号から 14 m 以内", "hull": "外壁の穴が全部ふさがっている", "thrusters": "下の四隅に推進機",
    "hose": "給油車の配管の口から給油口まで、光る枠 2 つに配管", "couple": "給油口（アクチュエーター）が結合している",
    "fuel_ship": "船の燃料タンク 9 つが全部 790 以上（給油車の燃料ポンプで送る。給油口を通る）", "launch": "航法コンピューターが moon.py で動いている",
    "seat": "ルミナス号の操縦席に座っている", "land": "月の発着場の上 2 m 以内・速さ 0.4 m/s 未満",
    "mikaga": "ミカガに話しかけた", "record": "ミカガに会った後、机の横の端末（ポッドの記録）を E", "kensu": "ケンスに話しかけた（ミサイルが来はじめる）",
    "intercept_prog": "迎撃の制御が「いちばん近いミサイルへの矢印」に向けて撃つプログラムで動いている",
    "intercept": "迎撃砲塔でミサイルを 12 発撃ち落とした", "board": "ロボットの操縦席に座っている",
    "robot_walk": "ロボットが 25 m 動いた（キンギがミサイルを撃ちはじめる）", "dodge": "ミサイルを 8 発かわした（当たらずに爆発・時間切れ）",
    "clash": "キンギのロボットから 9 m 以内に合わせて 2 秒（済むと一騎打ちのムービー）",
    "choice": "ネムリの前で「撃つ／話す」を選んだ（どちらでもトワは撃たない。選んだ方は flags.choice に残る）", "modules": "作業台の四隅に推進モジュール",
    "thrust": "4 基が燃えていて、バルブがどれも 0.6 以上・差 0.05 以内",
    "navi": "再加速の航法が、看板の地点から 10 m 以内の「地点への矢印」で動いている", "rise": "上の 3 つが済んでから 12 秒（途中でキンギが倒れ、ドナチが代わる）",
    "talk3": "広場のみんなのうち 3 人に話しかけた", "photo": "カメラモードで「写真を保存」した（みんなはカメラの方を向いて手をふる）",
    "embark": "発射台の乗り口を E（話して写真を撮るまでは乗れない）。座ると 5 秒のカウントダウン",
    "orbit": "まっすぐ上へ 4 秒 → 24 秒で水平に倒しながら軌道の速さへ（13 秒でブースター分離）→ 高さ 150 m の円い軌道",
}


def lines(xs):
    return "\n".join(f"- {w}「{t}」" if w != S.SYS else f"- （{t}）" for w, t in xs)


out = ["# シナリオ台本：ストーリー vol.1", "",
       "シナリオモード（MOD「story」）の物語と、章ごとの目標・判定・セリフ・ムービーの一覧。",
       "ゲームの中身は `dev/mods/story`（台本は `code/script.py`、判定は `code/addon.py`）にあり、この文書は",
       "`python tools/make_story_doc.py` で script.py から作っている。**移植・更新のときは `dev/tests/test_story.py` と見比べる。**",
       "", "## 始め方", "",
       "メニューの「ストーリー」で始める（「詳しく設定して作る…」→「MOD を選ぶ…」で「シナリオモード（ストーリー vol.1）」を入れて作ってもよい。必要な MOD は一緒に入る）。初めて起動したときは `scenario` ができる。",
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
    if c.get("event_movies"):
        out += ["", "出来事のムービー：" + "、".join(f"{k} → " + "、".join(S.MOVIES[m] for m in v)
                                         for k, v in c["event_movies"].items())]
    if c["id"] == "port":
        out += ["", "ポッドの記録（端末を E）：", lines(S.RECORD_LINES)]
    if c["id"] == "robot":
        out += ["", f"選ぶ（組みついてムービーのあと）：{S.CHOICE['title']}"]
        for k, label, ls in S.CHOICE["options"]:
            out += [f"- 「{label}」（`{k}`）", "  " + lines(ls).replace(chr(10), chr(10) + "  ")]
    out += ["", "終わり：", lines(c["outro"])]
    if c.get("movie_out"):
        out += ["", "終わりのムービー：" + "、".join(S.MOVIES[m] for m in c["movie_out"])]
    out.append("")
out += ["## エピローグで話しかけたとき", ""] + [f"- {v}" for v in S.DEPART_LINES.values()] + [""]
out += ["## エンドロール（低い軌道に乗ったら）", "", "それから："] + [f"- {w}：{t}" for w, t in S.CREDITS_AFTER]
out += ["", "制作："] + [f"- {r}：{n}" for r, n in S.CREDITS_STAFF]
out += ["", "お世話になったもの（`mods/story/credits/<名前>.png` があればロゴの絵を出す）："]
out += [f"- {n}（`{k}.png`）：{t}" for k, n, t in S.CREDITS_THANKS]
out += ["", "最後にタイトル（Alpedia・ストーリー vol.1）を出して、ワールドの選択（メニュー）へ戻る。", ""]
out += ["## クリアのあと", "", "目標の欄：", ""] + [f"- {x}" for x in S.FREE_PLAY] + [""]
(ROOT.parent / "docs" / "ストーリー台本.md").write_text("\n".join(out), encoding="utf-8")
print("書き出し: docs/ストーリー台本.md")

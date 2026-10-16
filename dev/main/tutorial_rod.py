"""チュートリアルの「棒の姿勢制御」パート（アクチュエーター＋ブロックプログラミング）。

揺らしモーターで左右に揺れ続ける台の上に、姿勢モーターと棒がある。
コンピューターのブロックプログラムで姿勢モーターを回し、台が揺れても棒が同じ向きを保つようにする。
ワールドの作成は tutorial.create_tutorial（ひとつのチュートリアルにまとめてある）。
"""
from . import blocks as B

SWAY_SCRIPT = '''"""揺らし役：揺らしモーターで台を左右に揺らし続ける（自動）

AMPLITUDE（揺れの大きさ・度）や SPEED（速さ）を変えると、制御の難しさが変わります。
"""
import math

AMPLITUDE = 20.0
SPEED = 0.8


def tick(api):
    a = api.device("揺らしモーター")
    a.rotate_to(AMPLITUDE * math.sin(api.time * SPEED), speed=90)
    api.display("揺らし役\\n台を自動で揺らす", width=1.3, color=(1.0, 0.8, 0.5))
'''

SIGNS = [
    """■ 棒の向きを保とう（姿勢制御）

右の装置は、揺れる台の上に棒が立っています。
台は「揺らしモーター」で左右に揺れ続けます。
台の上の「姿勢モーター」を回して、台が揺れても
棒がいつも同じ向きを向くようにするのが目標です。
（カメラのジンバルや、ロケットの姿勢制御と同じ考え方）

まずはお手本：台が傾いても棒はまっすぐ上のまま。
右端の「お手本」コンピューター（緑の文字）を P で開くと
ブロックで作ったプログラムを見られます。ブロック数個だけ！""",
    """■ ステップ1：手で回してみよう（右の課題装置）
姿勢モーターを見て P → 動作「回転」・量・速さを決めて閉じる → E で実行
→ 揺れに合わせて手で合わせ続けるのは大変…

■ ステップ2：棒の向きを測る
「課題」コンピューター（右端）を P で開くと、
棒の向きを表示するだけのブロックプログラムが入っています。
「姿勢モーター の 出力の向き」は世界から見た棒の角度
（真上が 0°、傾くと ±）。台が揺れると数字が変わります。""",
    """■ ステップ3：自動でまっすぐに保つ（課題コンピューター）
考え方：ずれ（誤差）＝ 目標 − 今の向き。そのぶん反対に回せばよい。

1.「ずっと」の「＋ここに追加」を選び、変数の「□ を □ にする」を押す
　左の枠 → 誤差、右の枠 → 「□ − □」
　中身 a → 変数「目標」、b → 「機器の値…」
　　→ 機器を「姿勢モーター」に、種類 ⟳ で「出力の向き」に
2. アクチュエーターの「□ の角度を □ 度にする」を押す
　機器 → 姿勢モーター、角度 → 「□ + □」
　　a → 機器の値（姿勢モーター の 角度）、b → 変数「誤差」
　速さ → 180
3. Ctrl+R（保存して実行）→ 棒がまっすぐになれば成功！""",
    """■ 姿勢制御の発展

・「起動したとき」の 目標 を 30 にすると、棒が 30° 傾いたまま保たれます
・課題装置の操縦席に座ると、キーが使えます：
　「キー up arrow を押したとき」→「目標 を 10 ずつ変える」
・誤差に × 0.3 をかけると、ゆっくり追いかける動きになります（比例制御）
・「Python をコピー」で、ブロックが Python でどう書かれるか見てみよう
・揺らし役（sway.py）の AMPLITUDE や SPEED を変えて、
　どこまで速い揺れについていけるか試そう

この考え方は、この先のロケットの誘導（orbit.py）でも使われています""",
]


def _devval(prop):
    return {"type": "devval", "dev": "姿勢モーター", "prop": prop}


def _display_stmt(prefix):
    d = B.new_stmt("display")
    d["text"] = {"type": "join", "a": {"type": "text", "v": prefix},
                 "b": {"type": "func", "fn": "四捨五入", "a": _devval("出力の向き")}}
    d["width"] = {"type": "num", "v": 1.3}
    return d


def starter_program():
    """課題：棒の向きを表示するだけ。"""
    p = B.new_program()
    st = B.new_stmt("on_start")
    a = B.new_stmt("set_var")
    a["var"], a["value"] = "目標", {"type": "num", "v": 0}
    st["body"].append(a)
    tk = B.new_stmt("on_tick")
    tk["body"].append(_display_stmt("課題  棒の向き "))
    p["scripts"] += [st, tk]
    return p


def solution_program():
    """お手本：誤差のぶんだけ姿勢モーターを回す。"""
    p = B.new_program()
    st = B.new_stmt("on_start")
    a = B.new_stmt("set_var")
    a["var"], a["value"] = "目標", {"type": "num", "v": 0}
    st["body"].append(a)
    tk = B.new_stmt("on_tick")
    e = B.new_stmt("set_var")
    e["var"] = "誤差"
    e["value"] = {"type": "op", "op": "-", "a": {"type": "var", "name": "目標"}, "b": _devval("出力の向き")}
    r = B.new_stmt("act_rotate_to")
    r["dev"] = "姿勢モーター"
    r["angle"] = {"type": "op", "op": "+", "a": _devval("角度"), "b": {"type": "var", "name": "誤差"}}
    r["speed"] = {"type": "num", "v": 180}
    tk["body"] += [e, r, _display_stmt("お手本  棒の向き ")]
    p["scripts"] += [st, tk]
    return p


def apparatus(ctrl_name, blocks_file, script_file):
    V = [((x, 0, z), "rock", None) for x in range(5) for z in range(3)]
    V.append(((2, 1, 0), "steel", {"def": "battery", "dir": 2}))
    V.append(((2, 2, 0), "steel", {"def": "actuator", "dir": 4, "name": "揺らしモーター"}))
    V.append(((0, 1, 0), "steel", {"def": "computer", "dir": 5, "name": "揺らし役",
                                   "state": {"script": "sway.py", "running": True}}))
    V.append(((4, 1, 0), "steel", {"def": "computer", "dir": 5, "name": ctrl_name,
                                   "state": {"script": script_file, "blocks": blocks_file, "running": True}}))
    V.append(((4, 1, 2), "steel", {"def": "seat", "dir": 2, "name": "操縦席"}))
    # 揺れる台（揺らしモーターの出力）
    V += [((x, 2, 1), "aluminum", None) for x in (1, 2, 3)]
    # 姿勢モーター（台の上、出力は +Z）と棒
    V.append(((2, 3, 1), "steel", {"def": "actuator", "dir": 4, "name": "姿勢モーター",
                                   "state": {"preset": {"action": "rotate", "amount": 15.0, "speed": 45.0}}}))
    V += [((2, y, 2), "wood", None) for y in range(3, 8)]   # 軽い木の棒（速く振っても折れにくい）
    V.append(((2, 8, 2), "rubber", None))
    return V

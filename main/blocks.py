"""ブロック（ノード）プログラミング：データ構造・ブロック定義・Python コードへの変換。

プログラムは JSON で保存する（scripts/<名前>.blocks.json）。保存時に同名の .py を生成し、
コンピューターはその .py を実行する（実行の仕組みは Python で書いたスクリプトと同じ）。

ノードの形:
  文 :  {"type": "if", "cond": 式, "body": [文...], "else": [文...]}
  式 :  {"type": "num", "v": 1.0} / {"type": "op", "op": "+", "a": 式, "b": 式} など
"""
import copy
import json

FORMAT = "alpedia-blocks"

# ---------------------------------------------------------------------------
# カテゴリ
# ---------------------------------------------------------------------------
CATEGORIES = [
    ("event", "イベント", (0.80, 0.62, 0.15)),
    ("control", "制御", (0.85, 0.45, 0.15)),
    ("device", "機器", (0.25, 0.50, 0.85)),
    ("actuator", "アクチュエーター", (0.45, 0.35, 0.80)),
    ("look", "表示", (0.65, 0.35, 0.75)),
    ("var", "変数", (0.85, 0.35, 0.20)),
    ("op", "演算", (0.30, 0.65, 0.35)),
    ("sensor", "センサー", (0.20, 0.62, 0.70)),
    ("radio", "通信", (0.80, 0.30, 0.55)),
]
CAT_COLOR = {k: c for k, _, c in CATEGORIES}

KEYS = ["space", "w", "a", "s", "d", "q", "e", "x", "z", "c", "up arrow", "down arrow", "left arrow",
        "right arrow", "left shift", "left control", "1", "2", "3", "4"]
PRESET_VARS = ["x", "y", "z", "目標", "誤差", "状態", "カウンター", "速さ", "角度"]

# ---------------------------------------------------------------------------
# 文（命令）ブロック
#   label の {名前} が入力欄。slots: 名前 → 種類（num/bool/text/key/var/dev:<種類>/choice:<a>|<b>）
# ---------------------------------------------------------------------------
STATEMENTS = {
    # イベント（帽子ブロック）
    "on_start": dict(cat="event", hat=True, label="起動したとき"),
    "on_tick": dict(cat="event", hat=True, label="ずっと（0.05秒ごと）"),
    "on_key": dict(cat="event", hat=True, label="キー {key} を押している間", slots={"key": "key"}),
    "on_key_down": dict(cat="event", hat=True, label="キー {key} を押したとき", slots={"key": "key"}),
    "on_when": dict(cat="event", hat=True, label="{cond} になったとき", slots={"cond": "bool"}),
    # 制御
    "if": dict(cat="control", label="もし {cond} なら", slots={"cond": "bool"}, body=True),
    "if_else": dict(cat="control", label="もし {cond} なら", slots={"cond": "bool"}, body=True, orelse=True),
    "repeat": dict(cat="control", label="{n} 回くり返す", slots={"n": "num"}, body=True),
    "reset_timer": dict(cat="control", label="タイマーをリセット"),
    "stop": dict(cat="control", label="プログラムを止める"),
    # 機器
    "throttle": dict(cat="device", label="推進機 {dev} の出力を {value} にする",
                     slots={"dev": "dev:thruster", "value": "num"}),
    "valve": dict(cat="device", label="バルブ {dev} を {state}", slots={"dev": "dev:valve", "state": "choice:開く|閉じる"}),
    "enable": dict(cat="device", label="機器 {dev} を {state}", slots={"dev": "dev:*", "state": "choice:ON|OFF"}),
    # アクチュエーター
    "act_rotate": dict(cat="actuator", label="{dev} を {angle} 度回す（速さ {speed} 度/秒）",
                       slots={"dev": "dev:actuator", "angle": "num", "speed": "num"}),
    "act_rotate_to": dict(cat="actuator", label="{dev} の角度を {angle} 度にする（速さ {speed}）",
                          slots={"dev": "dev:actuator", "angle": "num", "speed": "num"}),
    "act_extend": dict(cat="actuator", label="{dev} を {amount} ブロック押し出す（速さ {speed}）",
                       slots={"dev": "dev:actuator", "amount": "num", "speed": "num"}),
    "act_retract": dict(cat="actuator", label="{dev} を {amount} ブロック引き込む（速さ {speed}）",
                        slots={"dev": "dev:actuator", "amount": "num", "speed": "num"}),
    "act_detach": dict(cat="actuator", label="{dev} を分離する", slots={"dev": "dev:actuator"}),
    "act_attach": dict(cat="actuator", label="{dev} を結合する", slots={"dev": "dev:actuator"}),
    "act_execute": dict(cat="actuator", label="{dev} の設定動作を実行", slots={"dev": "dev:actuator"}),
    # 表示
    "display": dict(cat="look", label="表面に {text} と表示（幅 {width} m）", slots={"text": "text", "width": "num"}),
    "clear_display": dict(cat="look", label="表示を消す"),
    "print": dict(cat="look", label="メッセージ {text} を出す", slots={"text": "text"}),
    # 変数
    "set_var": dict(cat="var", label="{var} を {value} にする", slots={"var": "var", "value": "num"}),
    "change_var": dict(cat="var", label="{var} を {value} ずつ変える", slots={"var": "var", "value": "num"}),
    # 通信（アンテナが必要）
    "radio_send": dict(cat="radio", label="チャンネル {ch} に {value} を送る", slots={"ch": "text", "value": "num"}),
}

STMT_DEFAULTS = {
    "on_key": {"key": "space"}, "on_key_down": {"key": "x"},
    "on_when": {"cond": {"type": "cmp", "op": ">", "a": {"type": "sensor", "name": "altitude"},
                         "b": {"type": "num", "v": 10}}},
    "if": {"cond": {"type": "cmp", "op": ">", "a": {"type": "sensor", "name": "altitude"},
                    "b": {"type": "num", "v": 10}}},
    "if_else": {"cond": {"type": "key", "key": "space"}},
    "repeat": {"n": {"type": "num", "v": 3}},
    "throttle": {"dev": "*", "value": {"type": "num", "v": 1.0}},
    "valve": {"dev": "*", "state": "開く"},
    "enable": {"dev": "*", "state": "ON"},
    "act_rotate": {"dev": "*", "angle": {"type": "num", "v": 90}, "speed": {"type": "num", "v": 45}},
    "act_rotate_to": {"dev": "*", "angle": {"type": "num", "v": 0}, "speed": {"type": "num", "v": 45}},
    "act_extend": {"dev": "*", "amount": {"type": "num", "v": 1.0}, "speed": {"type": "num", "v": 0.5}},
    "act_retract": {"dev": "*", "amount": {"type": "num", "v": 1.0}, "speed": {"type": "num", "v": 0.5}},
    "act_detach": {"dev": "*"}, "act_attach": {"dev": "*"}, "act_execute": {"dev": "*"},
    "display": {"text": {"type": "text", "v": "Hello"}, "width": {"type": "num", "v": 1.0}},
    "print": {"text": {"type": "text", "v": "Hello"}},
    "set_var": {"var": "x", "value": {"type": "num", "v": 0}},
    "change_var": {"var": "x", "value": {"type": "num", "v": 1}},
    "radio_send": {"ch": {"type": "text", "v": "ch1"}, "value": {"type": "num", "v": 1}},
}

# ---------------------------------------------------------------------------
# 式（値）ブロック
# ---------------------------------------------------------------------------
SENSORS = {
    "altitude": ("高度", "api.altitude"),
    "vspeed": ("上下の速さ", "api.vertical_speed"),
    "speed": ("速さ", "api.speed"),
    "hspeed": ("水平の速さ", "api.orbit()['horizontal_speed']"),
    "tilt": ("傾き(度)", "math.degrees(math.acos(max(-1.0, min(1.0, api.up_local[1]))))"),
    "time": ("時刻", "api.time"),
    "timer": ("タイマー", "(api.time - m['_timer'])"),
    "mass": ("質量", "api.mass"),
    "power": ("蓄電量(J)", "api.power['stored']"),
    "apoapsis": ("遠点", "api.orbit()['apoapsis']"),
    "periapsis": ("近点", "api.orbit()['periapsis']"),
    "orbital_speed": ("円軌道の速さ", "api.orbit()['orbital_speed']"),
    "time_to_ap": ("遠点までの時間", "api.orbit()['time_to_apoapsis']"),
    # 姿勢（構築物の前＝ローカル +Z、右＝ローカル +X。真上を向いていると 0）
    "pitch": ("前への傾き(度)", "math.degrees(math.atan2(-api.up_local[2], api.up_local[1]))"),
    "roll": ("右への傾き(度)", "math.degrees(math.atan2(-api.up_local[0], api.up_local[1]))"),
    "pitch_rate": ("前へ傾く速さ(度/秒)", "math.degrees(api.angular_velocity_local[0])"),
    "roll_rate": ("右へ傾く速さ(度/秒)", "-math.degrees(api.angular_velocity_local[2])"),
}
OPS = {"+": "+", "-": "-", "×": "*", "÷": "/", "余り": "%"}
CMPS = {"<": "<", ">": ">", "=": "==", "≦": "<=", "≧": ">=", "≠": "!="}
FUNCS = {"絶対値": "abs", "四捨五入": "round", "平方根": "math.sqrt", "sin": "_sin", "cos": "_cos"}
DEVPROPS = {"角度": "angle", "出力の向き": "output_angle", "伸び": "extension", "出力": "throttle",
            "燃料": "fuel", "蓄電": "charge"}
DEVBOOLS = {"動作中": "busy", "結合している": "attached", "開いている": "open"}

# パレットに並べる式ブロック（種類, 表示名, 作るノード）
EXPR_PALETTE = [
    ("op", "□ + □", {"type": "op", "op": "+", "a": {"type": "num", "v": 0}, "b": {"type": "num", "v": 1}}),
    ("op", "□ − □", {"type": "op", "op": "-", "a": {"type": "num", "v": 0}, "b": {"type": "num", "v": 1}}),
    ("op", "□ × □", {"type": "op", "op": "×", "a": {"type": "num", "v": 1}, "b": {"type": "num", "v": 2}}),
    ("op", "□ ÷ □", {"type": "op", "op": "÷", "a": {"type": "num", "v": 1}, "b": {"type": "num", "v": 2}}),
    ("op", "□ の余り □", {"type": "op", "op": "余り", "a": {"type": "num", "v": 5}, "b": {"type": "num", "v": 2}}),
    ("op", "□ > □", {"type": "cmp", "op": ">", "a": {"type": "num", "v": 0}, "b": {"type": "num", "v": 0}}),
    ("op", "□ < □", {"type": "cmp", "op": "<", "a": {"type": "num", "v": 0}, "b": {"type": "num", "v": 0}}),
    ("op", "□ = □", {"type": "cmp", "op": "=", "a": {"type": "num", "v": 0}, "b": {"type": "num", "v": 0}}),
    ("op", "◇ かつ ◇", {"type": "logic", "op": "and", "a": {"type": "bool", "v": True}, "b": {"type": "bool", "v": True}}),
    ("op", "◇ または ◇", {"type": "logic", "op": "or", "a": {"type": "bool", "v": True}, "b": {"type": "bool", "v": False}}),
    ("op", "◇ ではない", {"type": "not", "a": {"type": "bool", "v": True}}),
    ("op", "絶対値 □", {"type": "func", "fn": "絶対値", "a": {"type": "num", "v": -1}}),
    ("op", "□ から □ の乱数", {"type": "random", "a": {"type": "num", "v": 1}, "b": {"type": "num", "v": 10}}),
    ("op", "□ と □（文字をつなぐ）", {"type": "join", "a": {"type": "text", "v": "高度 "},
                                    "b": {"type": "sensor", "name": "altitude"}}),
    ("sensor", "キー □ が押されている", {"type": "key", "key": "space"}),
    ("sensor", "機器 □ の値", {"type": "devval", "dev": "*", "prop": "角度"}),
    ("sensor", "機器 □ が動作中", {"type": "devbool", "dev": "*", "prop": "動作中"}),
    ("radio", "チャンネル □ の受信値", {"type": "radio_val", "ch": {"type": "text", "v": "ch1"}}),
    ("radio", "チャンネル □ に届いている", {"type": "radio_has", "ch": {"type": "text", "v": "ch1"}}),
    ("radio", "アンテナが使える", {"type": "radio_ok"}),
] + [("sensor", name, {"type": "sensor", "name": k}) for k, (name, _) in SENSORS.items()]


def new_stmt(kind):
    spec = STATEMENTS[kind]
    node = {"type": kind}
    node.update(copy.deepcopy(STMT_DEFAULTS.get(kind, {})))
    if spec.get("hat") or spec.get("body"):
        node["body"] = []
    if spec.get("orelse"):
        node["else"] = []
    return node


def new_program():
    return {"format": FORMAT, "version": 1, "scripts": []}


def load(path):
    data = json.loads(path.read_text(encoding="utf-8"))
    if data.get("format") != FORMAT:
        raise ValueError("ブロックプログラムの形式ではありません")
    return data


def save(path, prog):
    path.write_text(json.dumps(prog, ensure_ascii=False, indent=1), encoding="utf-8")


def slot_kind(stmt_type, slot):
    return STATEMENTS[stmt_type].get("slots", {}).get(slot, "num")


def label_parts(label):
    """'もし {cond} なら' → [('text','もし '), ('slot','cond'), ('text',' なら')]"""
    out, i = [], 0
    while i < len(label):
        j = label.find("{", i)
        if j < 0:
            out.append(("text", label[i:]))
            break
        if j > i:
            out.append(("text", label[i:j]))
        k = label.index("}", j)
        out.append(("slot", label[j + 1:k]))
        i = k + 1
    return out


# ---------------------------------------------------------------------------
# 表示用の要約
# ---------------------------------------------------------------------------
def _fmt_num(v):
    try:
        f = float(v)
    except (TypeError, ValueError):
        return str(v)
    return str(int(f)) if f == int(f) and abs(f) < 1e12 else f"{f:g}"


def summary(node):
    if node is None:
        return "?"
    if not isinstance(node, dict):
        if node == "*":
            return "すべて"
        return str(node)
    t = node.get("type")
    if t == "num":
        return _fmt_num(node.get("v", 0))
    if t == "text":
        return "「" + str(node.get("v", "")).replace("\n", "⏎") + "」"
    if t == "bool":
        return "はい" if node.get("v") else "いいえ"
    if t == "var":
        return node.get("name", "x")
    if t == "sensor":
        return SENSORS.get(node.get("name"), ("?",))[0]
    if t == "op":
        return f"({summary(node.get('a'))} {node.get('op')} {summary(node.get('b'))})"
    if t == "cmp":
        return f"〈{summary(node.get('a'))} {node.get('op')} {summary(node.get('b'))}〉"
    if t == "logic":
        word = "かつ" if node.get("op") == "and" else "または"
        return f"〈{summary(node.get('a'))} {word} {summary(node.get('b'))}〉"
    if t == "not":
        return f"〈{summary(node.get('a'))} ではない〉"
    if t == "func":
        return f"{node.get('fn')}({summary(node.get('a'))})"
    if t == "random":
        return f"乱数({summary(node.get('a'))}〜{summary(node.get('b'))})"
    if t == "join":
        return f"({summary(node.get('a'))}＋{summary(node.get('b'))})"
    if t == "key":
        return f"〈キー {node.get('key')} が押されている〉"
    if t == "devval":
        return f"({summary(node.get('dev'))} の {node.get('prop')})"
    if t == "devbool":
        return f"〈{summary(node.get('dev'))} が {node.get('prop')}〉"
    if t == "radio_val":
        return f"(チャンネル {summary(node.get('ch'))} の受信値)"
    if t == "radio_has":
        return f"〈チャンネル {summary(node.get('ch'))} に届いている〉"
    if t == "radio_ok":
        return "〈アンテナが使える〉"
    return "?"


def expr_children(node):
    """式ノードの中の入力欄: [(キー, 種類)]"""
    t = node.get("type") if isinstance(node, dict) else None
    if t in ("op", "random"):
        return [("a", "num"), ("b", "num")]
    if t == "cmp":
        return [("a", "num"), ("b", "num")]
    if t == "logic":
        return [("a", "bool"), ("b", "bool")]
    if t == "not":
        return [("a", "bool")]
    if t == "func":
        return [("a", "num")]
    if t == "join":
        return [("a", "text"), ("b", "text")]
    if t in ("devval", "devbool"):
        return [("dev", "dev:*")]
    if t == "key":
        return [("key", "key")]
    if t in ("radio_val", "radio_has"):
        return [("ch", "text")]
    return []


# ---------------------------------------------------------------------------
# Python への変換
# ---------------------------------------------------------------------------
HEADER = '''"""このファイルはブロックプログラム {src} から自動生成されました。
ここを直接書き換えても、ブロック側には反映されません。
"""
import math
import random


def _devs(api, name, kind=None):
    if name == "*":
        return api.devices(kind)
    return api.devices(name=name)


def _val(api, name, prop, kind=None):
    ds = _devs(api, name, kind)
    if not ds:
        return 0
    d = ds[0]
    if prop == "fuel":
        return d.contents.get("fuel", 0)
    try:
        return getattr(d, prop)
    except Exception:
        return 0


def _div(a, b):
    return a / b if b else 0


def _mod(a, b):
    return a % b if b else 0


def _sin(a):
    return math.sin(math.radians(a))


def _cos(a):
    return math.cos(math.radians(a))


def _num(a):
    try:
        return float(a)
    except (TypeError, ValueError):
        return 0.0


def _radio(api, ch):
    v = api.latest(str(ch))
    if isinstance(v, bool):
        return 1.0 if v else 0.0
    try:
        return float(v)
    except (TypeError, ValueError):
        return 0.0


'''


def _q(s):
    return repr(str(s))


def gen_expr(node, kind="num"):
    if not isinstance(node, dict):
        return _q(node)
    t = node.get("type")
    if t == "num":
        return _fmt_num(node.get("v", 0)) if isinstance(node.get("v"), (int, float)) else f"_num({_q(node.get('v'))})"
    if t == "text":
        return _q(node.get("v", ""))
    if t == "bool":
        return "True" if node.get("v") else "False"
    if t == "var":
        return f"v.get({_q(node.get('name', 'x'))}, 0)"
    if t == "sensor":
        return SENSORS.get(node.get("name"), ("", "0"))[1]
    if t == "op":
        a, b = gen_expr(node.get("a")), gen_expr(node.get("b"))
        op = node.get("op", "+")
        if op == "÷":
            return f"_div({a}, {b})"
        if op == "余り":
            return f"_mod({a}, {b})"
        return f"({a} {OPS.get(op, '+')} {b})"
    if t == "cmp":
        return f"({gen_expr(node.get('a'))} {CMPS.get(node.get('op'), '==')} {gen_expr(node.get('b'))})"
    if t == "logic":
        op = "and" if node.get("op") == "and" else "or"
        return f"({gen_expr(node.get('a'), 'bool')} {op} {gen_expr(node.get('b'), 'bool')})"
    if t == "not":
        return f"(not {gen_expr(node.get('a'), 'bool')})"
    if t == "func":
        return f"{FUNCS.get(node.get('fn'), 'abs')}({gen_expr(node.get('a'))})"
    if t == "random":
        return f"random.uniform({gen_expr(node.get('a'))}, {gen_expr(node.get('b'))})"
    if t == "join":
        return f"(str({gen_expr(node.get('a'), 'text')}) + str({gen_expr(node.get('b'), 'text')}))"
    if t == "key":
        return f"({_q(node.get('key', 'space'))} in keys)"
    if t == "devval":
        return f"_val(api, {_q(node.get('dev', '*'))}, {_q(DEVPROPS.get(node.get('prop'), 'angle'))})"
    if t == "devbool":
        return f"bool(_val(api, {_q(node.get('dev', '*'))}, {_q(DEVBOOLS.get(node.get('prop'), 'busy'))}))"
    if t == "radio_val":
        return f"_radio(api, {gen_expr(node.get('ch'), 'text')})"
    if t == "radio_has":
        return f"(api.latest(str({gen_expr(node.get('ch'), 'text')})) is not None)"
    if t == "radio_ok":
        return "api.antenna"
    return "0"


def _dev_loop(node, kind, line):
    return [f"for d in _devs(api, {_q(node.get('dev', '*'))}, {_q(kind)}):", f"    {line}"]


def gen_stmt(node):
    """文 → Python の行のリスト（インデントなし）"""
    t = node.get("type")
    g = gen_expr
    if t in ("if", "if_else"):
        out = [f"if {g(node.get('cond'), 'bool')}:"] + _indent(gen_body(node.get("body", [])))
        if t == "if_else":
            out += ["else:"] + _indent(gen_body(node.get("else", [])))
        return out
    if t == "repeat":
        return [f"for _i in range(max(0, int({g(node.get('n'))}))):"] + _indent(gen_body(node.get("body", [])))
    if t == "reset_timer":
        return ["m['_timer'] = api.time"]
    if t == "stop":
        return ["api.stop()", "return"]
    if t == "throttle":
        return _dev_loop(node, "thruster", f"d.throttle = {g(node.get('value'))}")
    if t == "valve":
        return _dev_loop(node, "valve", f"d.open = {node.get('state') == '開く'}")
    if t == "enable":
        return _dev_loop(node, None, f"d.enabled = {node.get('state') == 'ON'}") if node.get("dev") != "*" else \
            ["for d in api.devices():", f"    d.enabled = {node.get('state') == 'ON'}"]
    if t == "act_rotate":
        return _dev_loop(node, "actuator", f"d.rotate({g(node.get('angle'))}, speed={g(node.get('speed'))})")
    if t == "act_rotate_to":
        return _dev_loop(node, "actuator", f"d.rotate_to({g(node.get('angle'))}, speed={g(node.get('speed'))})")
    if t == "act_extend":
        return _dev_loop(node, "actuator", f"d.extend({g(node.get('amount'))}, speed={g(node.get('speed'))})")
    if t == "act_retract":
        return _dev_loop(node, "actuator", f"d.retract({g(node.get('amount'))}, speed={g(node.get('speed'))})")
    if t == "act_detach":
        return _dev_loop(node, "actuator", "d.detach()")
    if t == "act_attach":
        return _dev_loop(node, "actuator", "d.attach()")
    if t == "act_execute":
        return _dev_loop(node, "actuator", "d.execute()")
    if t == "display":
        return [f"api.display(str({g(node.get('text'), 'text')}), width={g(node.get('width'))})"]
    if t == "clear_display":
        return ["api.clear_display()"]
    if t == "print":
        return [f"print({g(node.get('text'), 'text')})"]
    if t == "set_var":
        return [f"v[{_q(node.get('var', 'x'))}] = {g(node.get('value'))}"]
    if t == "change_var":
        name = _q(node.get("var", "x"))
        return [f"v[{name}] = _num(v.get({name}, 0)) + {g(node.get('value'))}"]
    if t == "radio_send":
        return [f"api.send(str({g(node.get('ch'), 'text')}), {g(node.get('value'))})"]
    return [f"pass  # 未対応のブロック {t}"]


def _indent(lines):
    return ["    " + ln for ln in lines]


def gen_body(body):
    out = []
    for n in body:
        out += gen_stmt(n)
    return out or ["pass"]


def to_python(prog, src_name="(ブロック)"):
    lines = [HEADER.format(src=src_name), "def tick(api):",
             "    m = api.memory",
             "    if api.time - m.get('_last', -99.0) > 0.5:   # 起動し直したら最初から",
             "        m.clear()",
             "    m['_last'] = api.time",
             "    v = m.setdefault('vars', {})",
             "    first = '_t0' not in m",
             "    if first:",
             "        m['_t0'] = api.time",
             "        m['_timer'] = api.time",
             "    keys = api.input",
             "    prev = set(m.get('_keys', []))",
             "    m['_keys'] = sorted(keys)",
             ]
    scripts = prog.get("scripts", [])
    order = {"on_start": 0}
    for k, s in enumerate(sorted(scripts, key=lambda s: order.get(s.get("type"), 1))):
        t = s.get("type")
        spec = STATEMENTS.get(t, {})
        lines.append(f"    # --- {spec.get('label', t).format(**{k2: summary(s.get(k2)) for k2 in spec.get('slots', {})})} ---")
        body = gen_body(s.get("body", []))
        if t == "on_start":
            lines += ["    if first:"] + _indent(_indent(body))
        elif t == "on_tick":
            lines += _indent(body)
        elif t == "on_key":
            lines += [f"    if {_q(s.get('key', 'space'))} in keys:"] + _indent(_indent(body))
        elif t == "on_key_down":
            key = _q(s.get("key", "x"))
            lines += [f"    if {key} in keys and {key} not in prev:"] + _indent(_indent(body))
        elif t == "on_when":
            flag = f"_w{k}"
            lines += [f"    _c = {gen_expr(s.get('cond'), 'bool')}",
                      f"    if _c and not m.get({_q(flag)}):"] + _indent(_indent(body)) + \
                     [f"    m[{_q(flag)}] = bool(_c)"]
    if not scripts:
        lines.append("    pass")
    return "\n".join(lines) + "\n"

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
    # 機器（推進機・バルブ）・アクチュエーター・MOD の機器（迎撃・ミサイルなど）は1つの種類「アクチュエーター」にまとめる
    ("actuator", "アクチュエーター", (0.45, 0.35, 0.80)),
    ("look", "表示", (0.65, 0.35, 0.75)),
    ("var", "変数", (0.85, 0.35, 0.20)),
    ("op", "演算", (0.30, 0.65, 0.35)),
    ("sensor", "センサー", (0.20, 0.62, 0.70)),
    ("radio", "通信", (0.80, 0.30, 0.55)),
    ("vec", "矢印", (0.60, 0.30, 1.00)),
]
CAT_COLOR = {k: c for k, _, c in CATEGORIES}
# 前の版・MOD の種類 → まとめた先（register_ext で MOD のブロックの種類も読み替える。元の種類は "orig_cat" に残す）
CAT_MERGE = {"device": "actuator", "weapon": "actuator"}

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
    "wait": dict(cat="control", label="{n} 秒待つ", slots={"n": "num"}),
    "wait_until": dict(cat="control", label="{cond} になるまで待つ", slots={"cond": "bool"}),
    "reset_timer": dict(cat="control", label="タイマーをリセット"),
    "stop": dict(cat="control", label="プログラムを止める"),
    # 機器
    "ignite": dict(cat="actuator", label="推進機 {dev} に点火する", slots={"dev": "dev:thruster"}),
    "valve_flow": dict(cat="actuator", label="バルブ {dev} の開き具合を {value} にする",
                       slots={"dev": "dev:valve", "value": "num"}),
    "valve": dict(cat="actuator", label="バルブ {dev} を {state}", slots={"dev": "dev:valve", "state": "choice:開く|閉じる"}),
    # 前の版のブロック（0 より大きいと点火するだけ。出力はバルブで変える）
    "throttle": dict(cat="actuator", label="推進機 {dev} の出力を {value} にする（点火のみ）",
                     slots={"dev": "dev:thruster", "value": "num"}, hidden=True),   # パレットには出さない
    "enable": dict(cat="actuator", label="機器 {dev} を {state}", slots={"dev": "dev:*", "state": "choice:ON|OFF"}),
    # アクチュエーター
    # 機器の欄 "dev:actuator.rotate" は、その動作ができるアクチュエーター（ローター・ヒンジ／ピストン）だけ
    "act_rotate": dict(cat="actuator", label="{dev} を {angle} 度回す・曲げる（速さ {speed} 度/秒）",
                       slots={"dev": "dev:actuator.rotate", "angle": "num", "speed": "num"}),
    "act_rotate_to": dict(cat="actuator", label="{dev} の角度を {angle} 度にする（速さ {speed}）",
                          slots={"dev": "dev:actuator.rotate", "angle": "num", "speed": "num"}),
    "act_extend": dict(cat="actuator", label="{dev} を {amount} ブロック押し出す（速さ {speed}）",
                       slots={"dev": "dev:actuator.extend", "amount": "num", "speed": "num"}),
    "act_retract": dict(cat="actuator", label="{dev} を {amount} ブロック引き込む（速さ {speed}）",
                        slots={"dev": "dev:actuator.retract", "amount": "num", "speed": "num"}),
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
    # 矢印（ベクトル）
    "set_vvar": dict(cat="vec", label="矢印の変数 {var} を {value} にする", slots={"var": "vvar", "value": "vec"}),
    "show_vec": dict(cat="vec", label="矢印 {value} を {color} で見せる（{scale} 倍）",
                     slots={"value": "vec", "color": "choice:黄|赤|緑|青|白|紫", "scale": "num"}),
}

STMT_DEFAULTS = {
    "on_key": {"key": "space"}, "on_key_down": {"key": "x"},
    "on_when": {"cond": {"type": "cmp", "op": ">", "a": {"type": "sensor", "name": "altitude"},
                         "b": {"type": "num", "v": 10}}},
    "if": {"cond": {"type": "cmp", "op": ">", "a": {"type": "sensor", "name": "altitude"},
                    "b": {"type": "num", "v": 10}}},
    "if_else": {"cond": {"type": "key", "key": "space"}},
    "repeat": {"n": {"type": "num", "v": 3}},
    "wait": {"n": {"type": "num", "v": 1}},
    "wait_until": {"cond": {"type": "cmp", "op": ">", "a": {"type": "sensor", "name": "timer"},
                            "b": {"type": "num", "v": 3}}},
    "throttle": {"dev": "*", "value": {"type": "num", "v": 1.0}},
    "ignite": {"dev": "*"},
    "valve_flow": {"dev": "*", "value": {"type": "num", "v": 0.5}},
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
    "set_vvar": {"var": "向き", "value": {"type": "vsensor", "name": "velocity"}},
    "show_vec": {"value": {"type": "vsensor", "name": "velocity"}, "color": "黄", "scale": {"type": "num", "v": 1}},
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
    "yaw_rate": ("右へ回る速さ(度/秒)", "math.degrees(api.angular_velocity_local[1])"),
    # 構築物から見た速さ（前＝ローカル +Z、右＝ローカル +X）と、その場所の重力
    "fwd_speed": ("前への速さ", "api.velocity_local[2]"),
    "right_speed": ("右への速さ", "api.velocity_local[0]"),
    "gravity": ("重力(m/s²)", "_gravity(api)"),
}
# 条件（はい／いいえ）のセンサー
BOOL_SENSORS = {
    "seated": ("座っている", "api.seated"),
}
OPS = {"+": "+", "-": "-", "×": "*", "÷": "/", "余り": "%"}
CMPS = {"<": "<", ">": ">", "=": "==", "≦": "<=", "≧": ">=", "≠": "!="}
FUNCS = {"絶対値": "abs", "四捨五入": "_round", "平方根": "math.sqrt", "sin": "_sin", "cos": "_cos",
         "マイナスなら0": "_pos"}
DEVPROPS = {"角度": "angle", "出力の向き": "output_angle", "伸び": "extension", "出力": "throttle", "開き具合": "open",
            "燃料": "fuel", "蓄電": "charge"}
DEVBOOLS = {"動作中": "busy", "結合している": "attached", "開いている": "open", "燃えている": "lit"}

# 矢印（ベクトル）：向きと長さを持つ値。中身はワールド座標の [x, y, z]
VSENSORS = {
    "velocity": ("速度", "_V(api.velocity)"),
    "gravity": ("重力", "_V(api.gravity)"),
    "up": ("上向き", "_V(api.up)"),
    "ship_fwd": ("船の前", "_V(api.orientation()[2])"),
    "ship_right": ("船の右", "_V(api.orientation()[0])"),
    "ship_up": ("船の上", "_V(api.orientation()[1])"),
    "to_planet": ("惑星の中心への矢印", "_to_planet(api)"),
    "position": ("位置", "_V(api.position)"),
}
VFRAMES = ["ワールド", "船から見た"]           # 矢印 (x, y, z)：船から見た＝(右, 上, 前)
VOPS = {"+": "_vadd", "-": "_vsub"}
VSCALES = {"×": "_vmul", "÷": "_vdiv"}
VCOMPS = {"前": "f", "右": "r", "上": "u", "x": "x", "y": "y", "z": "z"}
ARROW_COLORS = {"黄": (1.0, 0.85, 0.15), "赤": (1.0, 0.3, 0.25), "緑": (0.3, 0.95, 0.35), "青": (0.3, 0.6, 1.0),
                "白": (1.0, 1.0, 1.0), "紫": (0.75, 0.45, 1.0)}
PRESET_VVARS = ["向き", "行きたい速度", "ずれ"]


def _vs(name):
    return {"type": "vsensor", "name": name}

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
    ("op", "マイナスなら0 □", {"type": "func", "fn": "マイナスなら0", "a": {"type": "num", "v": -1}}),
] + [("sensor", name, {"type": "sensor", "name": k}) for k, (name, _) in SENSORS.items()] \
  + [("sensor", name, {"type": "bsensor", "name": k}) for k, (name, _) in BOOL_SENSORS.items()] \
  + [("vec", name, _vs(k)) for k, (name, _) in VSENSORS.items()] + [
    ("vec", "矢印 (□, □, □)", {"type": "vec", "frame": "船から見た", "x": {"type": "num", "v": 0},
                               "y": {"type": "num", "v": 1}, "z": {"type": "num", "v": 0}}),
    ("vec", "地点 (□, □, □) への矢印", {"type": "vto", "x": {"type": "num", "v": 0}, "y": {"type": "num", "v": 0},
                                     "z": {"type": "num", "v": 0}}),
    ("vec", "→ + →", {"type": "vop", "op": "+", "a": _vs("velocity"), "b": _vs("gravity")}),
    ("vec", "→ − →", {"type": "vop", "op": "-", "a": _vs("velocity"), "b": _vs("gravity")}),
    ("vec", "→ × □", {"type": "vscale", "op": "×", "a": _vs("velocity"), "b": {"type": "num", "v": 2}}),
    ("vec", "→ の向き", {"type": "vnorm", "a": _vs("velocity")}),
    ("vec", "→ の長さを □ までに", {"type": "vclamp", "a": _vs("velocity"), "b": {"type": "num", "v": 5}}),
    ("vec", "→ の → に垂直な部分", {"type": "vperp", "a": _vs("velocity"), "b": _vs("up")}),
    ("vec", "→ を → から □ 度以内に", {"type": "vtilt", "a": _vs("ship_up"), "b": _vs("up"),
                                   "n": {"type": "num", "v": 30}}),
    ("vec", "→ の長さ", {"type": "vlen", "a": _vs("velocity")}),
    ("vec", "→ の 前 成分", {"type": "vcomp", "axis": "前", "a": _vs("velocity")}),
    ("vec", "→ と → の内積", {"type": "vdot", "a": _vs("velocity"), "b": _vs("up")}),
    ("vec", "→ と → の角度", {"type": "vangle", "a": _vs("ship_up"), "b": _vs("up")}),
]

# 式ブロックの値の種類（入れられる入力欄）：num＝数値 / bool＝条件 / text＝文字 / vec＝矢印
NUM_TYPES = {"num", "op", "func", "random", "sensor", "var", "devval", "radio_val", "vlen", "vcomp", "vdot", "vangle"}
BOOL_TYPES = {"bool", "cmp", "logic", "not", "key", "devbool", "radio_has", "radio_ok", "bsensor"}
TEXT_TYPES = {"text", "join"}
VEC_TYPES = {"vsensor", "vvar", "vec", "vto", "vop", "vscale", "vnorm", "vclamp", "vperp", "vtilt"}


def value_kind(node):
    t = node.get("type") if isinstance(node, dict) else None
    if t in NUM_TYPES:
        return "num"
    if t in BOOL_TYPES:
        return "bool"
    if t in TEXT_TYPES:
        return "text"
    if t in VEC_TYPES:
        return "vec"
    return None


def fits(node, slot):
    """式ブロック node を、種類 slot（num/bool/text）の入力欄に入れられるか。
    条件は数値の欄にも入れられる（はい＝1、いいえ＝0 として計算）。文字の欄には何でも入る。
    矢印は矢印の欄にだけ入る（数にするには「長さ」「成分」などのブロックを使う）。"""
    k = value_kind(node)
    if k is None:
        return False
    if slot == "text":
        return True
    if k == "vec" or slot == "vec":
        return k == slot
    if slot == "num":
        return k in ("num", "bool")
    return k == slot


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
    if t == "bsensor":
        return f"〈{BOOL_SENSORS.get(node.get('name'), ('?',))[0]}〉"
    if t == "vsensor":
        return VSENSORS.get(node.get("name"), ("?",))[0]
    if t == "vvar":
        return node.get("name", "向き")
    if t == "vec":
        pre = "船から見た" if node.get("frame") == "船から見た" else ""
        return f"{pre}矢印({summary(node.get('x'))}, {summary(node.get('y'))}, {summary(node.get('z'))})"
    if t == "vto":
        return f"地点({summary(node.get('x'))}, {summary(node.get('y'))}, {summary(node.get('z'))})への矢印"
    if t in ("vop", "vscale"):
        return f"[{summary(node.get('a'))} {node.get('op')} {summary(node.get('b'))}]"
    if t == "vnorm":
        return f"[{summary(node.get('a'))} の向き]"
    if t == "vclamp":
        return f"[{summary(node.get('a'))} の長さを {summary(node.get('b'))} までに]"
    if t == "vperp":
        return f"[{summary(node.get('a'))} の {summary(node.get('b'))} に垂直な部分]"
    if t == "vtilt":
        return f"[{summary(node.get('a'))} を {summary(node.get('b'))} から {summary(node.get('n'))} 度以内に]"
    if t == "vlen":
        return f"({summary(node.get('a'))} の長さ)"
    if t == "vcomp":
        return f"({summary(node.get('a'))} の {node.get('axis')} 成分)"
    if t == "vdot":
        return f"({summary(node.get('a'))} と {summary(node.get('b'))} の内積)"
    if t == "vangle":
        return f"({summary(node.get('a'))} と {summary(node.get('b'))} の角度)"
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
    if t in ("vec", "vto"):
        return [("x", "num"), ("y", "num"), ("z", "num")]
    if t in ("vop", "vperp", "vdot", "vangle"):
        return [("a", "vec"), ("b", "vec")]
    if t in ("vscale", "vclamp"):
        return [("a", "vec"), ("b", "num")]
    if t in ("vnorm", "vlen", "vcomp"):
        return [("a", "vec")]
    if t == "vtilt":
        return [("a", "vec"), ("b", "vec"), ("n", "num")]
    return []


# ---------------------------------------------------------------------------
# ヒント：見本（MOD が渡す完成形）と比べて、次にすること（置くブロック・入れる式・入力する値）を1つ探す
# ---------------------------------------------------------------------------
_MENU_FIELDS = ("op", "fn", "frame", "axis", "prop")


def _slot_keys(stmt_type):
    spec = STATEMENTS.get(stmt_type, {})
    return [s for k, s in label_parts(spec.get("label", "")) if k == "slot"]


def _num_eq(a, b):
    try:
        a, b = float(a), float(b)
    except (TypeError, ValueError):
        return a == b
    return abs(a - b) <= 1e-6 + 1e-4 * max(abs(a), abs(b))


def _hint_expr(owner, key, kind, cv, sv):
    """入力欄 owner[key] の今の値 cv を見本 sv と比べる。違えば {"kind": "expr"/"value", ...}。"""
    if not isinstance(sv, dict):
        if cv != sv:
            return {"kind": "value", "owner": owner, "key": key, "slot": kind, "value": sv}
        return None
    st = sv.get("type")
    ct = cv.get("type") if isinstance(cv, dict) else None
    if st in ("num", "text", "bool"):
        same = ct == st and (_num_eq(cv.get("v"), sv.get("v")) if st == "num" else cv.get("v") == sv.get("v"))
        if not same:
            return {"kind": "value", "owner": owner, "key": key, "slot": kind, "value": sv.get("v"), "type": st}
        return None
    if ct != st or cv.get("name") != sv.get("name"):
        return {"kind": "expr", "owner": owner, "key": key, "slot": kind, "node": sv}
    for f in _MENU_FIELDS:                       # ▼ で選ぶ所（演算の種類・機器の値の種類など）
        if f in sv and cv.get(f) != sv.get(f):
            return {"kind": "value", "owner": cv, "key": f, "slot": "menu", "value": sv[f], "menu": True}
    for ck, ckind in expr_children(sv):
        h = _hint_expr(cv, ck, ckind, cv.get(ck), sv.get(ck))
        if h:
            return h
    return None


def _hint_stmt(cn, sn):
    for key in _slot_keys(sn["type"]):
        h = _hint_expr(cn, key, slot_kind(sn["type"], key), cn.get(key), sn.get(key))
        if h:
            return h
    spec = STATEMENTS.get(sn["type"], {})
    if spec.get("body") or spec.get("hat"):
        h = _hint_list(cn.setdefault("body", []), sn.get("body", []), cn)
        if h:
            return h
        if spec.get("orelse"):
            return _hint_list(cn.setdefault("else", []), sn.get("else", []), cn)
    return None


def _hint_list(cl, sl, owner):
    for k, sn in enumerate(sl):
        if k >= len(cl) or cl[k].get("type") != sn.get("type"):
            return {"kind": "insert", "list": cl, "index": k, "owner": owner, "type": sn["type"], "node": sn}
        h = _hint_stmt(cl[k], sn)
        if h:
            return h
    return None


def find_hint(cur, sol):
    """今のプログラム cur（エディタが編集中の物そのもの）を、見本 sol と比べて、最初の違いを返す（同じなら None）。
    {"kind": "insert", "list", "index", "type"}：list の index に type のブロックを置く
    {"kind": "expr", "owner", "key", "node"}：owner[key] の欄に node の形の式ブロックを入れる
    {"kind": "value", "owner", "key", "value"}：owner[key] の欄に value を入れる（数値・文字・機器の名前など）
    見本より多いブロックは気にしない。"""
    scripts = cur.setdefault("scripts", [])
    used = set()
    for s in sol.get("scripts", []):
        cands = [k for k, c in enumerate(scripts) if k not in used and c.get("type") == s.get("type")]
        same = [k for k in cands if all(json.dumps(scripts[k].get(key), sort_keys=True)
                                        == json.dumps(s.get(key), sort_keys=True) for key in _slot_keys(s["type"]))]
        match = (same or cands or [None])[0]
        if match is None:
            return {"kind": "insert", "list": scripts, "index": len(scripts), "owner": None, "type": s["type"],
                    "node": s}
        used.add(match)
        h = _hint_stmt(scripts[match], s)
        if h:
            return h
    return None


def hint_text(h):
    """ヒントの説明（画面の下に出す文）。"""
    if h is None:
        return ""
    if h["kind"] == "insert":
        spec = STATEMENTS.get(h["type"], {})
        label = "".join(("□" if t == "slot" else s) for t, s in label_parts(spec.get("label", h["type"])))
        return f"ヒント：光っているブロック「{label}」を、光っている所に置こう"
    if h["kind"] == "expr":
        return f"ヒント：光っている欄に「{summary(h['node'])}」を入れよう（欄を押してから、光っている値のブロックを押す）"
    v = h["value"]
    if h.get("type") == "num":
        return f"ヒント：光っている欄の数を {_fmt_num(v)} にしよう（欄を2回押すと入力できる）"
    if h.get("menu"):
        shown = {"and": "かつ", "or": "または"}.get(v, v)
        return f"ヒント：光っている ▼ を押して「{shown}」を選ぼう"
    if h.get("type") == "bool":
        return f"ヒント：光っている欄を「{'はい' if v else 'いいえ'}」にしよう"
    return f"ヒント：光っている欄を「{v}」にしよう"


# ---------------------------------------------------------------------------
# MOD が足すブロック（extensions.reset で元に戻す）
#   statements の "code"：Python の行のリスト。{入力欄の名前} にその欄の式が入る（機器は名前の文字列）
#   sensors / vsensors / bool_sensors：{ID: (表示名, Python の式)}　palette：[(カテゴリ, 表示名, ノード)]
# ---------------------------------------------------------------------------
_BASE = None


def _tables():
    return (STATEMENTS, STMT_DEFAULTS, SENSORS, BOOL_SENSORS, VSENSORS, DEVPROPS, DEVBOOLS)


def _keep_base():
    global _BASE
    if _BASE is None:
        import copy
        _BASE = ([copy.deepcopy(t) for t in _tables()], list(EXPR_PALETTE), [c for c in CATEGORIES])


def reset_ext():
    if _BASE is None:
        return
    tables, pal, cats = _BASE
    for t, base in zip(_tables(), tables):
        t.clear()
        t.update(base)
    EXPR_PALETTE[:] = pal
    CATEGORIES[:] = cats
    CAT_COLOR.clear()
    CAT_COLOR.update({k: c for k, _, c in CATEGORIES})


def register_ext(statements=None, defaults=None, sensors=None, bool_sensors=None, vsensors=None, palette=None,
                 devprops=None, devbools=None, categories=None):
    _keep_base()
    for cat in categories or []:
        if cat[0] in CAT_MERGE:
            continue                      # まとめた種類（機器・迎撃など）はアクチュエーターに入る
        if cat[0] not in CAT_COLOR:
            CATEGORIES.append(tuple(cat))
            CAT_COLOR[cat[0]] = cat[2]
    for kind, spec in (statements or {}).items():
        c = spec.get("cat")
        if c in CAT_MERGE:
            spec = dict(spec, cat=CAT_MERGE[c], orig_cat=c)
        STATEMENTS[kind] = spec
    STMT_DEFAULTS.update(defaults or {})
    SENSORS.update(sensors or {})
    BOOL_SENSORS.update(bool_sensors or {})
    VSENSORS.update(vsensors or {})
    DEVPROPS.update(devprops or {})
    DEVBOOLS.update(devbools or {})
    for k, (name, _) in (sensors or {}).items():
        EXPR_PALETTE.append(("sensor", name, {"type": "sensor", "name": k}))
    for k, (name, _) in (bool_sensors or {}).items():
        EXPR_PALETTE.append(("sensor", name, {"type": "bsensor", "name": k}))
    for k, (name, _) in (vsensors or {}).items():
        EXPR_PALETTE.append(("vec", name, {"type": "vsensor", "name": k}))
    EXPR_PALETTE.extend(palette or [])


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


def _round(a):
    # 無限大（脱出軌道の遠点など）や非数はそのまま（round では止まってしまうため）
    return round(a) if math.isfinite(a) else a


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


def _gravity(api):
    p = api.planet()
    if not p:
        return 0.0
    r2 = sum((api.position[i] - p["center"][i]) ** 2 for i in range(3))
    return p["mu"] / max(r2, 1.0)


def _pos(a):
    return a if a > 0 else 0.0


# --- 矢印（ベクトル）[x, y, z] ---
def _V(a):
    try:
        return [float(a[0]), float(a[1]), float(a[2])]
    except (TypeError, ValueError, IndexError, KeyError):
        return [0.0, 0.0, 0.0]


def _vadd(a, b):
    return [a[0] + b[0], a[1] + b[1], a[2] + b[2]]


def _vsub(a, b):
    return [a[0] - b[0], a[1] - b[1], a[2] - b[2]]


def _vmul(a, k):
    return [a[0] * k, a[1] * k, a[2] * k]


def _vdiv(a, k):
    return [a[0] / k, a[1] / k, a[2] / k] if k else [0.0, 0.0, 0.0]


def _vdot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def _vlen(a):
    return math.sqrt(_vdot(a, a))


def _vunit(a):
    n = _vlen(a)
    return _vmul(a, 1.0 / n) if n > 1e-9 else [0.0, 0.0, 0.0]


def _vclamp(a, n):
    k = _vlen(a)
    return _vmul(a, max(n, 0.0) / k) if k > max(n, 0.0) and k > 0 else list(a)


def _vperp(a, b):
    u = _vunit(b)
    return _vsub(a, _vmul(u, _vdot(a, u)))


def _vangle(a, b):
    na, nb = _vlen(a), _vlen(b)
    if na < 1e-9 or nb < 1e-9:
        return 0.0
    return math.degrees(math.acos(max(-1.0, min(1.0, _vdot(a, b) / (na * nb)))))


def _vtilt(a, b, deg):
    """矢印 a の向きを、b から deg 度以内にする（長さはそのまま）。"""
    n, u = _vlen(a), _vunit(b)
    if n < 1e-9 or _vlen(u) < 0.5:
        return list(a)
    d = _vmul(a, 1.0 / n)
    c, cmax = _vdot(d, u), math.cos(math.radians(max(0.0, min(180.0, deg))))
    if c >= cmax:
        return list(a)
    h = _vunit(_vsub(d, _vmul(u, c)))
    if _vlen(h) < 0.5:
        return _vmul(u, n) if deg < 90 else list(a)
    return _vmul(_vadd(_vmul(u, cmax), _vmul(h, math.sin(math.radians(deg)))), n)


def _vship(api, x, y, z):
    """船から見た (右, 上, 前) → ワールドの矢印。"""
    X, Y, Z = api.orientation()
    return [X[i] * x + Y[i] * y + Z[i] * z for i in range(3)]


def _vcomp(api, a, axis):
    if axis in "xyz":
        return a["xyz".index(axis)]
    X, Y, Z = api.orientation()
    return _vdot(a, {{"r": X, "u": Y, "f": Z}}[axis])


def _vget(v, name):
    x = v.get(name)
    return list(x) if isinstance(x, list) and len(x) == 3 else [0.0, 0.0, 0.0]


def _to_planet(api):
    p = api.planet()
    return _vsub(p["center"], api.position) if p else [0.0, 0.0, 0.0]


def _txt(a):
    if isinstance(a, list):
        return "(" + ", ".join("%.1f" % x for x in a) + ")"
    return str(a)


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
        return f"(_txt({gen_expr(node.get('a'), 'text')}) + _txt({gen_expr(node.get('b'), 'text')}))"
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
    if t == "bsensor":
        return f"bool({BOOL_SENSORS.get(node.get('name'), ('', 'False'))[1]})"
    # 矢印
    g = gen_expr
    if t == "vsensor":
        return VSENSORS.get(node.get("name"), ("", "[0.0, 0.0, 0.0]"))[1]
    if t == "vvar":
        return f"_vget(v, {_q(node.get('name', '向き'))})"
    if t == "vec":
        xyz = f"{g(node.get('x'))}, {g(node.get('y'))}, {g(node.get('z'))}"
        return f"_vship(api, {xyz})" if node.get("frame") == "船から見た" else f"[{xyz}]"
    if t == "vto":
        return f"_vsub([{g(node.get('x'))}, {g(node.get('y'))}, {g(node.get('z'))}], _V(api.position))"
    if t == "vop":
        return f"{VOPS.get(node.get('op'), '_vadd')}({g(node.get('a'), 'vec')}, {g(node.get('b'), 'vec')})"
    if t == "vscale":
        return f"{VSCALES.get(node.get('op'), '_vmul')}({g(node.get('a'), 'vec')}, {g(node.get('b'))})"
    if t == "vnorm":
        return f"_vunit({g(node.get('a'), 'vec')})"
    if t == "vclamp":
        return f"_vclamp({g(node.get('a'), 'vec')}, {g(node.get('b'))})"
    if t == "vperp":
        return f"_vperp({g(node.get('a'), 'vec')}, {g(node.get('b'), 'vec')})"
    if t == "vtilt":
        return f"_vtilt({g(node.get('a'), 'vec')}, {g(node.get('b'), 'vec')}, {g(node.get('n'))})"
    if t == "vlen":
        return f"_vlen({g(node.get('a'), 'vec')})"
    if t == "vcomp":
        return f"_vcomp(api, {g(node.get('a'), 'vec')}, {_q(VCOMPS.get(node.get('axis'), 'f'))})"
    if t == "vdot":
        return f"_vdot({g(node.get('a'), 'vec')}, {g(node.get('b'), 'vec')})"
    if t == "vangle":
        return f"_vangle({g(node.get('a'), 'vec')}, {g(node.get('b'), 'vec')})"
    return "[0.0, 0.0, 0.0]" if kind == "vec" else "0"


def _dev_loop(node, kind, line):
    return [f"for d in _devs(api, {_q(node.get('dev', '*'))}, {_q(kind)}):", f"    {line}"]


def _act_loop(node, action, line):
    """アクチュエーターの動作。「すべて」なら、その動作ができる種類だけ（ローターに押し出し、ピストンに回転はしない）。
    名前で選んだ機器はそのまま頼む（できない種類ならエラーで分かる）。"""
    if node.get("dev", "*") != "*":
        return _dev_loop(node, "actuator", line)
    return [f"for d in _devs(api, '*', 'actuator'):", f"    if d.can({_q(action)}):", f"        {line}"]


def gen_stmt(node):
    """文 → Python の行のリスト（インデントなし）"""
    t = node.get("type")
    g = gen_expr
    spec = STATEMENTS.get(t) or {}
    if spec.get("code"):                      # MOD が足したブロック（register_ext）
        vals = {}
        for sname, kind in spec.get("slots", {}).items():
            v = node.get(sname)
            if kind.startswith("dev:") or kind.startswith("choice:") or kind in ("var", "vvar", "key"):
                vals[sname] = _q(v if v is not None else "*")
            else:
                vals[sname] = g(v, kind if kind in ("num", "bool", "text", "vec") else "num")
        return [ln.format(**vals) for ln in spec["code"]]
    if t in ("wait", "wait_until"):
        return ["pass  # 待つブロックは、イベントのすぐ下（または「くり返す」の中）で使います"]
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
    if t == "ignite":
        return _dev_loop(node, "thruster", "d.ignite()")
    if t == "valve_flow":
        return _dev_loop(node, "valve", f"d.open = max(0.0, min(1.0, {g(node.get('value'))}))")
    if t == "enable":
        return _dev_loop(node, None, f"d.enabled = {node.get('state') == 'ON'}") if node.get("dev") != "*" else \
            ["for d in api.devices():", f"    d.enabled = {node.get('state') == 'ON'}"]
    if t == "act_rotate":
        return _act_loop(node, "rotate", f"d.rotate({g(node.get('angle'))}, speed={g(node.get('speed'))})")
    if t == "act_rotate_to":
        return _act_loop(node, "rotate", f"d.rotate_to({g(node.get('angle'))}, speed={g(node.get('speed'))})")
    if t == "act_extend":
        return _act_loop(node, "extend", f"d.extend({g(node.get('amount'))}, speed={g(node.get('speed'))})")
    if t == "act_retract":
        return _act_loop(node, "retract", f"d.retract({g(node.get('amount'))}, speed={g(node.get('speed'))})")
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
    if t == "set_vvar":
        return [f"v[{_q(node.get('var', '向き'))}] = {g(node.get('value'), 'vec')}"]
    if t == "show_vec":
        rgb = ARROW_COLORS.get(node.get("color"), ARROW_COLORS["黄"])
        return [f"api.show_arrow({g(node.get('value'), 'vec')}, color={rgb!r}, scale={g(node.get('scale'))})"]
    return [f"pass  # 未対応のブロック {t}"]


def _indent(lines):
    return ["    " + ln for ln in lines]


def gen_body(body):
    out = []
    for n in body:
        out += gen_stmt(n)
    return out or ["pass"]


# ---------------------------------------------------------------------------
# 待つブロック（順番に動かす）：イベントの下に「待つ」があると、上から順に1つずつ進める
#   ずっと：最後まで行ったら最初から（くり返す）　起動したとき：1回だけ
#   キーを押したとき・〜になったとき：そのたびに最初から（動いている間は始め直さない）
# ---------------------------------------------------------------------------
def has_wait(body):
    for n in body or []:
        t = n.get("type")
        if t in ("wait", "wait_until"):
            return True
        if t == "repeat" and has_wait(n.get("body")):
            return True
    return False


def _flatten(body, ops):
    buf = []

    def flush():
        if buf:
            ops.append(("do", gen_body(list(buf))))
            buf.clear()
    for n in body or []:
        t = n.get("type")
        if t == "wait":
            flush()
            ops.append(("wait", gen_expr(n.get("n"))))
        elif t == "wait_until":
            flush()
            ops.append(("until", gen_expr(n.get("cond"), "bool")))
        elif t == "repeat" and has_wait(n.get("body")):
            flush()
            k = len(ops)
            ops.append(("loop", gen_expr(n.get("n")), k))
            _flatten(n.get("body"), ops)
            ops.append(("end", k))
        else:
            buf.append(n)
    flush()
    return ops


def gen_sequence(body, k, restart):
    """順番に動かすプログラム（インデントなし）。restart：最後まで行ったら最初から。"""
    ops = _flatten(body, [])
    n = len(ops)
    key = _q(f"_sq{k}")
    out = [f"sq = m.setdefault({key}, {{'i': 0, 't': api.time, 'c': {{}}}})",
           "for _g in range(200):",
           "    _s = sq['i']"]
    for j, op in enumerate(ops):
        out.append(f"    if _s == {j}:")
        if op[0] == "do":
            out += ["        " + ln for ln in op[1]]
            out.append(f"        sq['i'] = {j + 1}; sq['t'] = api.time; continue")
        elif op[0] == "wait":
            out.append(f"        if api.time - sq['t'] < {op[1]}: break")
            out.append(f"        sq['i'] = {j + 1}; sq['t'] = api.time; continue")
        elif op[0] == "until":
            out.append(f"        if not ({op[1]}): break")
            out.append(f"        sq['i'] = {j + 1}; sq['t'] = api.time; continue")
        elif op[0] == "loop":
            out.append(f"        sq['c']['{op[2]}'] = int(max(0, {op[1]}))")
            end = next(i for i, x in enumerate(ops) if x[0] == "end" and x[1] == op[2])
            out.append(f"        sq['i'] = {j + 1} if sq['c']['{op[2]}'] > 0 else {end + 1}; sq['t'] = api.time; continue")
        elif op[0] == "end":
            start = next(i for i, x in enumerate(ops) if x[0] == "loop" and x[2] == op[1])
            out.append(f"        sq['c']['{op[1]}'] = sq['c'].get('{op[1]}', 1) - 1")
            out.append(f"        sq['i'] = {start + 1} if sq['c']['{op[1]}'] > 0 else {j + 1}; sq['t'] = api.time; continue")
    if restart:
        out.append("    sq['i'] = 0; sq['t'] = api.time")
    out.append("    break")
    return out, n


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
        if has_wait(s.get("body")):
            seq, n = gen_sequence(s.get("body"), k, restart=(t == "on_tick"))
            sk = _q(f"_sq{k}")
            if t in ("on_start", "on_tick"):
                lines += _indent(seq)
            elif t == "on_key":
                lines += [f"    if {_q(s.get('key', 'space'))} in keys:"] + _indent(_indent(seq))
            else:
                if t == "on_key_down":
                    key = _q(s.get("key", "x"))
                    trig = f"{key} in keys and {key} not in prev"
                else:
                    flag = _q(f"_w{k}")
                    lines += [f"    _c = {gen_expr(s.get('cond'), 'bool')}"]
                    trig = f"_c and not m.get({flag})"
                lines += [f"    if {sk} not in m:",
                          f"        m[{sk}] = {{'i': {n}, 't': api.time, 'c': {{}}}}",
                          f"    if {trig} and m[{sk}]['i'] >= {n}:",
                          f"        m[{sk}]['i'] = 0",
                          f"        m[{sk}]['t'] = api.time"]
                if t == "on_when":
                    lines += [f"    m[{flag}] = bool(_c)"]
                lines += _indent(seq)
            continue
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

"""ブロック（ノード）プログラミング：データ構造・ブロック定義・Python コードへの変換。
ブロックの表（MOD が足す）はここ。要約・ヒント・Python への変換などの計算は Rust（alpedia_core の game/blocks.rs）。

プログラムは JSON で保存する（scripts/<名前>.blocks.json）。保存時に同名の .py を生成し、
コンピューターはその .py を実行する（実行の仕組みは Python で書いたスクリプトと同じ）。

ノードの形:
  文 :  {"type": "if", "cond": 式, "body": [文...], "else": [文...]}
  式 :  {"type": "num", "v": 1.0} / {"type": "op", "op": "+", "a": 式, "b": 式} など
"""
import json

from . import native

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


def _core():
    return native.require()


def value_kind(node):
    return _core().blocks_value_kind(node)


def fits(node, slot):
    """式ブロック node を、種類 slot（num/bool/text）の入力欄に入れられるか。
    条件は数値の欄にも入れられる（はい＝1、いいえ＝0 として計算）。文字の欄には何でも入る。
    矢印は矢印の欄にだけ入る（数にするには「長さ」「成分」などのブロックを使う）。"""
    return _core().blocks_fits(node, slot)


def new_stmt(kind):
    return _core().blocks_new_stmt(kind)


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
    return _core().blocks_slot_kind(stmt_type, slot)


def label_parts(label):
    """'もし {cond} なら' → [('text','もし '), ('slot','cond'), ('text',' なら')]"""
    return _core().blocks_label_parts(label)


def _fmt_num(v):
    return _core().blocks_fmt_num(v)


def summary(node):
    """表示用の要約。"""
    return _core().blocks_summary(node)


def expr_children(node):
    """式ノードの中の入力欄: [(キー, 種類)]"""
    return _core().blocks_expr_children(node)


_MENU_FIELDS = ("op", "fn", "frame", "axis", "prop")


def find_hint(cur, sol):
    """今のプログラム cur（エディタが編集中の物そのもの）を、見本 sol と比べて、最初の違いを返す（同じなら None）。
    {"kind": "insert", "list", "index", "type"}：list の index に type のブロックを置く
    {"kind": "expr", "owner", "key", "node"}：owner[key] の欄に node の形の式ブロックを入れる
    {"kind": "value", "owner", "key", "value"}：owner[key] の欄に value を入れる（数値・文字・機器の名前など）
    見本より多いブロックは気にしない。"""
    return _core().blocks_find_hint(cur, sol)


def hint_text(h):
    """ヒントの説明（画面の下に出す文）。"""
    return _core().blocks_hint_text(h)


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
# Python への変換（Rust）
# ---------------------------------------------------------------------------
HEADER = native.require().BLOCKS_HEADER


def gen_expr(node, kind="num"):
    return _core().blocks_gen_expr(node, kind)


def gen_stmt(node):
    """文 → Python の行のリスト（インデントなし）"""
    return _core().blocks_gen_stmt(node)


def gen_body(body):
    return _core().blocks_gen_body(body)


def has_wait(body):
    return _core().blocks_has_wait(body)


def gen_sequence(body, k, restart):
    """順番に動かすプログラム（インデントなし）。restart：最後まで行ったら最初から。"""
    return _core().blocks_gen_sequence(body, k, restart)


def to_python(prog, src_name="(ブロック)"):
    return _core().blocks_to_python(prog, src_name)

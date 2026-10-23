"""ストーリーの乗り物のブロックプログラム（ホバーバイク・ドローン・ドローンの操縦台）。

ブロックの画面（コンピューターを P）で開いて、中身を見たり変えたりできる。
どれも同じ考え方：
  ・進みたい速さ − 今の速さ → 傾けたい角度（前・右）
  ・傾けたい角度 − 今の傾き → 前後・左右の推進機の出力の差（姿勢制御。棒の姿勢制御と同じ「ずれ × 係数」）
  ・浮きたい高さ − 今の高さ → 推進機みんなの出力（重さを支えるぶん ＋ ずれのぶん）
  ・推進機の出力は、それぞれの真上のバルブの開き具合で変える（後ろ左・後ろ右・前左・前右）
"""
from main import blocks as B

THRUST4 = 600000.0       # 推進機4つの最大推力 (N)
KEEP = 0.02              # 飛んでいる間のバルブのいちばん小さい開き具合（火を消さない）
VALVES = ("バルブ 後左", "バルブ 後右", "バルブ 前左", "バルブ 前右")


# --- ブロックを作る道具 ---------------------------------------------------
def N(v):
    return {"type": "num", "v": v}


def T(v):
    return {"type": "text", "v": v}


def V(name):
    return {"type": "var", "name": name}


def S(name):
    return {"type": "sensor", "name": name}


def op(o, a, b):
    return {"type": "op", "op": o, "a": a, "b": b}


def add(*xs):
    out = xs[0]
    for x in xs[1:]:
        out = op("+", out, x)
    return out


def sub(a, b):
    return op("-", a, b)


def mul(a, b):
    return op("×", a, b)


def div(a, b):
    return op("÷", a, b)


def cmp(o, a, b):
    return {"type": "cmp", "op": o, "a": a, "b": b}


def both(a, b):
    return {"type": "logic", "op": "and", "a": a, "b": b}


def key(k):
    return {"type": "key", "key": k}


def seated():
    return {"type": "bsensor", "name": "seated"}


def rnd(a):
    return {"type": "func", "fn": "四捨五入", "a": a}


def join(*parts):
    node = parts[-1]
    for p in reversed(parts[:-1]):
        node = {"type": "join", "a": p, "b": node}
    return node


def radio(ch):
    return {"type": "radio_val", "ch": T(ch)}


def radio_has(ch):
    return {"type": "radio_has", "ch": T(ch)}


def stmt(kind, **kw):
    st = B.new_stmt(kind)
    st.update(kw)
    return st


def setv(name, value):
    return stmt("set_var", var=name, value=value)


def chg(name, value):
    return stmt("change_var", var=name, value=value)


def iff(cond, body):
    return stmt("if", cond=cond, body=body)


def if_else(cond, body, other):
    return stmt("if_else", cond=cond, body=body, **{"else": other})


def clamp(name, lo, hi):
    """変数 name を lo〜hi に収める（もし ＞ なら …）。"""
    return [iff(cmp(">", V(name), N(hi)), [setv(name, N(hi))]),
            iff(cmp("<", V(name), N(lo)), [setv(name, N(lo))])]


def valve(name, value):
    return stmt("valve_flow", dev=name, value=value)


def show(text, width=1.1):
    return stmt("display", text=text, width=N(width))


# --- 浮いて進む乗り物の制御（バイクとドローンで共通） ----------------------
def hover_control():
    """変数 前後目標・左右目標（m/秒）と 目標高さ（m）に向けて、4つのバルブの開き具合を決めて点火する。"""
    out = [
        # 進みたい速さとのずれ → 傾けたい角度（度）
        setv("前傾き目標", mul(sub(V("前後目標"), S("fwd_speed")), N(6))),
        setv("右傾き目標", mul(sub(V("左右目標"), S("right_speed")), N(6))),
    ]
    out += clamp("前傾き目標", -20, 20) + clamp("右傾き目標", -20, 20)
    out += [
        # 姿勢制御：傾きのずれ × 0.02 − 傾く速さ × 0.012（後ろを強くすると前へ、左を強くすると右へ傾く）
        setv("前後", sub(mul(sub(V("前傾き目標"), S("pitch")), N(0.02)), mul(S("pitch_rate"), N(0.012)))),
        setv("左右", sub(mul(sub(V("右傾き目標"), S("roll")), N(0.02)), mul(S("roll_rate"), N(0.012)))),
        # 高さ：ずれ × 1.8 − 上下の速さ × 2.2（加速は ±4 m/s² まで）
        setv("加速", sub(mul(sub(V("目標高さ"), S("altitude")), N(1.8)), mul(S("vspeed"), N(2.2)))),
    ]
    out += clamp("加速", -4, 4)
    out += [
        # 重さを支える出力 ＝ 質量 ×（重力 ＋ 加速）÷ 推進機4つの推力（火を消さないために足す分を引いておく）
        setv("出力", sub(div(mul(S("mass"), add(S("gravity"), V("加速"))), N(THRUST4)), N(KEEP))),
        valve(VALVES[0], add(V("出力"), V("前後"), V("左右"), N(KEEP))),
        valve(VALVES[1], add(sub(add(V("出力"), V("前後")), V("左右")), N(KEEP))),
        valve(VALVES[2], add(sub(V("出力"), V("前後")), V("左右"), N(KEEP))),
        valve(VALVES[3], add(sub(sub(V("出力"), V("前後")), V("左右")), N(KEEP))),
        stmt("ignite", dev="*"),
    ]
    return out


def close_valves():
    return [valve("*", N(0))]


# --- ホバーバイク ----------------------------------------------------------
def bike_program():
    p = B.new_program()
    start = B.new_stmt("on_start")
    start["body"] = [setv("地面", S("altitude")), setv("浮く高さ", N(1.2))]
    tick = B.new_stmt("on_tick")
    tick["body"] = [
        if_else(seated(), [
            # 座っている：W/S で前・後、A/D で左・右（11 m/秒）、Space / Left Shift で高く・低く
            setv("前後目標", mul(sub(key("w"), key("s")), N(11))),
            setv("左右目標", mul(sub(key("d"), key("a")), N(11))),
            iff(key("space"), [chg("浮く高さ", N(0.15))]),
            iff(key("left shift"), [chg("浮く高さ", N(-0.15))]),
            iff(cmp("<", V("浮く高さ"), N(0.4)), [setv("浮く高さ", N(1.2))]),
        ], [
            # 降りた：止まってから、ゆっくり降りる
            setv("前後目標", N(0)), setv("左右目標", N(0)),
            iff(cmp("<", S("speed"), N(1.5)), [chg("浮く高さ", N(-0.05))]),
        ]),
        iff(cmp(">", V("浮く高さ"), N(12)), [setv("浮く高さ", N(12))]),
        iff(cmp("<", V("浮く高さ"), N(0)), [setv("浮く高さ", N(0))]),
        setv("目標高さ", add(V("地面"), V("浮く高さ"))),
        if_else(both(cmp("<", V("浮く高さ"), N(0.3)), cmp("<", S("altitude"), add(V("地面"), N(0.35)))),
                close_valves() + [show(T("ホバーバイク\n座ると動きます"))],
                hover_control() + [show(join(T("ホバーバイク\n速さ "), rnd(S("speed")), T(" m/s\n高さ "),
                                             rnd(sub(S("altitude"), V("地面"))), T(" m")))]),
    ]
    p["scripts"] += [start, tick]
    return p


# --- ドローン（無線で操縦台のキーを受け取る） --------------------------------
def drone_program():
    p = B.new_program()
    start = B.new_stmt("on_start")
    start["body"] = [setv("地面", S("altitude")), setv("目標高さ", S("altitude"))]
    tick = B.new_stmt("on_tick")
    tick["body"] = [
        if_else(radio_has("drone/前後"), [
            # 操縦台から届いた向き（前後・左右・上下は -1〜1、着陸は 0 か 1）
            setv("前後目標", mul(radio("drone/前後"), N(8))),
            setv("左右目標", mul(radio("drone/左右"), N(8))),
            chg("目標高さ", mul(radio("drone/上下"), N(0.15))),
            iff(cmp(">", radio("drone/着陸"), N(0.5)), [chg("目標高さ", N(-0.075))]),
            setv("状態", N(1)),
        ], [
            # 電波が切れた：その場でゆっくり降りる
            setv("前後目標", N(0)), setv("左右目標", N(0)), chg("目標高さ", N(-0.075)), setv("状態", N(0)),
        ]),
        iff(cmp("<", V("目標高さ"), V("地面")), [setv("目標高さ", V("地面"))]),
        iff(cmp(">", V("目標高さ"), add(S("altitude"), N(4))), [setv("目標高さ", add(S("altitude"), N(4)))]),
        stmt("radio_send", ch=T("drone/高さ"), value=sub(S("altitude"), V("地面"))),
        if_else(both(cmp("<", S("altitude"), add(V("地面"), N(0.3))),
                     cmp("<", V("目標高さ"), add(V("地面"), N(0.05)))),
                close_valves() + [show(T("ドローン\n着陸中"), 1.0)],
                hover_control() + [show(T("ドローン\n飛行中"), 1.0)]),
    ]
    p["scripts"] += [start, tick]
    return p


def remote_program():
    p = B.new_program()
    tick = B.new_stmt("on_tick")
    tick["body"] = [
        stmt("radio_send", ch=T("drone/前後"), value=sub(key("w"), key("s"))),
        stmt("radio_send", ch=T("drone/左右"), value=sub(key("d"), key("a"))),
        stmt("radio_send", ch=T("drone/上下"), value=sub(key("space"), key("left shift"))),
        stmt("radio_send", ch=T("drone/着陸"), value=V("着陸")),
        if_else(radio_has("drone/高さ"),
                [show(join(T("ドローン操縦台\n高さ "), rnd(radio("drone/高さ")), T(" m\nX で着陸／離陸")), 2.0)],
                [show(T("ドローン操縦台\n電波なし"), 2.0)]),
    ]
    land = B.new_stmt("on_key_down")
    land["key"] = "x"
    land["body"] = [setv("着陸", sub(N(1), V("着陸")))]
    p["scripts"] += [tick, land]
    return p


PROGRAMS = {"bike": bike_program, "drone": drone_program, "drone_remote": remote_program}

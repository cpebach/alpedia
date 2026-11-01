"""ストーリーの乗り物のブロックプログラム（ホバーバイク・ドローン・ドローンの操縦台・ルミナス号の航法）。

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


def say(text):
    return stmt("print", text=T(text))


def fn(name, a):
    return {"type": "func", "fn": name, "a": a}


# --- 矢印（ベクトル）のブロック ----------------------------------------------
def VS(name):
    return {"type": "vsensor", "name": name}


def VV(name):
    return {"type": "vvar", "name": name}


def vec(x, y, z, frame="ワールド"):
    return {"type": "vec", "frame": frame, "x": N(round(float(x), 3)), "y": N(round(float(y), 3)),
            "z": N(round(float(z), 3))}


def vto(p):
    return {"type": "vto", "x": N(round(float(p[0]), 3)), "y": N(round(float(p[1]), 3)), "z": N(round(float(p[2]), 3))}


def vadd(a, b):
    return {"type": "vop", "op": "+", "a": a, "b": b}


def vsub(a, b):
    return {"type": "vop", "op": "-", "a": a, "b": b}


def vmul(a, k):
    return {"type": "vscale", "op": "×", "a": a, "b": k}


def vnorm(a):
    return {"type": "vnorm", "a": a}


def vclamp(a, n):
    return {"type": "vclamp", "a": a, "b": n}


def vperp(a, b):
    return {"type": "vperp", "a": a, "b": b}


def vtilt(a, b, deg):
    return {"type": "vtilt", "a": a, "b": b, "n": N(deg)}


def vlen(a):
    return {"type": "vlen", "a": a}


def vcomp(a, axis):
    return {"type": "vcomp", "axis": axis, "a": a}


def vdot(a, b):
    return {"type": "vdot", "a": a, "b": b}


def setvv(name, value):
    return stmt("set_vvar", var=name, value=value)


def show_vec(value, color, scale=1.0):
    return stmt("show_vec", value=value, color=color, scale=N(scale))


# --- 浮いて進む乗り物の制御（バイクとドローンで共通） ----------------------
def hover_control(thrust4=None, valves=None):
    """変数 前後目標・左右目標（m/秒）と 目標高さ（m）に向けて、4つのバルブの開き具合を決めて点火する。"""
    thrust4 = THRUST4 if thrust4 is None else thrust4
    valves = VALVES if valves is None else valves
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
        setv("出力", sub(div(mul(S("mass"), add(S("gravity"), V("加速"))), N(thrust4)), N(KEEP))),
        valve(valves[0], add(V("出力"), V("前後"), V("左右"), N(KEEP))),
        valve(valves[1], add(sub(add(V("出力"), V("前後")), V("左右")), N(KEEP))),
        valve(valves[2], add(sub(V("出力"), V("前後")), V("左右"), N(KEEP))),
        valve(valves[3], add(sub(sub(V("出力"), V("前後")), V("左右")), N(KEEP))),
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
                [show(join(T("ドローン 高さ "), rnd(radio("drone/高さ")), T(" m\nSpace 上昇・Shift 下降\nX 自動着陸")), 1.2)],
                [show(T("ドローン操縦台\n電波なし"), 1.2)]),
    ]
    land = B.new_stmt("on_key_down")
    land["key"] = "x"
    land["body"] = [setv("着陸", sub(N(1), V("着陸")))]
    p["scripts"] += [tick, land]
    return p


# --- ルミナス号の航法（月への自動飛行） ---------------------------------------
# 5×5 の船の下の段：四隅と各辺の真ん中に推進機。それぞれの真上にバルブ（前 = +Z、右 = +X）
SHIP_VALVE_NAMES = {(0, 0): "バルブ 後左", (2, 0): "バルブ 後", (4, 0): "バルブ 後右", (0, 2): "バルブ 左",
                    (4, 2): "バルブ 右", (0, 4): "バルブ 前左", (2, 4): "バルブ 前", (4, 4): "バルブ 前右"}
SHIP_THRUST = 150000.0 * 8   # 推進機8つの最大推力 (N)
SHIP_KEEP = 0.01
CRUISE = 70.0                # 巡航の速さ (m/s)
BRAKE = 2.5                  # 減速の強さ (m/s²)
HOVER_ALT = 90.0             # 降下を始める、発着場の上の高さ (m)
LAND_H = 1.5                 # 着陸したとみなす、発着場の上面からの重心の高さ (m)
# 段階
WAIT, COUNT, ASCENT, CRUISING, DESCENT, LANDED = range(6)


def phase_is(k):
    return cmp("=", V("段階"), N(k))


def moon_program(pad, pad_up):
    """月の発着場 pad（ワールド座標）と、その上方向 pad_up に向けて飛ぶ。
    「行きたい速度 − 今の速度」の矢印のぶん加速する（重力のぶんも足す）。推力の向きに船の上を合わせる。"""
    pad = [float(x) for x in pad]
    up = [float(x) for x in pad_up]
    over = [p + u * HOVER_ALT for p, u in zip(pad, up)]
    PADUP = vec(*up)
    p = B.new_program()
    start = B.new_stmt("on_start")
    start["body"] = [setv("段階", N(WAIT)), setv("時刻0", S("time"))]
    tick = B.new_stmt("on_tick")
    body = [
        # 誰も座っていなければ待つ
        iff(both(cmp("<", V("段階"), N(ASCENT)), {"type": "not", "a": seated()}),
            [setv("段階", N(WAIT)), setv("時刻0", S("time"))]),
        setv("最大加速", div(N(SHIP_THRUST), S("mass"))),
        setvv("発着場へ", vto(pad)),
        setv("高さ", mul(vdot(VV("発着場へ"), PADUP), N(-1))),
        setvv("行きたい速度", vec(0, 0, 0)),
        iff(phase_is(WAIT), [iff(seated(), [setv("段階", N(COUNT))])]),
        iff(phase_is(COUNT), [
            setv("残り", sub(N(3), sub(S("time"), V("時刻0")))),
            iff(cmp("<", V("残り"), N(0.001)), [setv("段階", N(ASCENT)), stmt("act_detach", dev="給油口"),
                                                say("給油口を切り離しました。発進！")]),
        ]),
        iff(phase_is(ASCENT), [
            # まっすぐ上へ 18 m/s
            setvv("行きたい速度", vmul(VS("up"), N(18))),
            iff(cmp(">", S("altitude"), N(35)), [setv("段階", N(CRUISING)), say("巡航：月へ")]),
        ]),
        iff(phase_is(CRUISING), [
            # 降下を始める点（発着場の 90 m 上）へ。近づいたら止まれる速さ √(2×減速×距離) に落とす
            setvv("ずれ", vto(over)),
            setv("距離", vlen(VV("ずれ"))),
            setv("巡航の速さ", fn("平方根", mul(N(2 * BRAKE), V("距離")))),
            iff(cmp(">", V("巡航の速さ"), N(CRUISE)), [setv("巡航の速さ", N(CRUISE))]),
            setvv("行きたい速度", vmul(vnorm(VV("ずれ")), V("巡航の速さ"))),
            iff(both(cmp("<", V("距離"), N(6)), cmp("<", S("speed"), N(4))), [setv("段階", N(DESCENT)), say("降下")]),
        ]),
        iff(phase_is(DESCENT), [
            # 発着場への矢印のうち、発着場の上向きに垂直な部分 ＝ 横のずれ
            setvv("横のずれ", vperp(VV("発着場へ"), PADUP)),
            setv("降りる速さ", mul(sub(V("高さ"), N(LAND_H)), N(0.2))),
        ] + clamp("降りる速さ", 0.6, 4.0) + [
            setv("許すずれ", mul(V("高さ"), N(0.15))),
            iff(cmp("<", V("許すずれ"), N(1)), [setv("許すずれ", N(1))]),
            # ずれているときは高さを保って、真上に来てから降りる
            iff(cmp(">", vlen(VV("横のずれ")), V("許すずれ")), [setv("降りる速さ", N(0))]),
            setvv("行きたい速度", vadd(vmul(PADUP, mul(V("降りる速さ"), N(-1))),
                                     vclamp(vmul(VV("横のずれ"), N(0.35)), N(3)))),
            iff(both(cmp("<", V("高さ"), N(LAND_H)), cmp("<", S("speed"), N(1.2))),
                [setv("段階", N(LANDED)), say("着陸しました！ ようこそルナ・ポートへ")]),
        ]),
    ]
    # --- 推進機：行きたい加速 → 推力の向きと出力 ---
    fly = [
        setvv("重力", VS("gravity")),
        # 加速 ＝（行きたい速度 − 速度）× 0.8 − 重力（重力に負けないぶんも出す）
        setvv("加速", vsub(vmul(vsub(VV("行きたい速度"), VS("velocity")), N(0.8)), VV("重力"))),
        iff(phase_is(COUNT), [setvv("加速", vec(0, 0, 0))]),
        setv("出力", div(vlen(VV("加速")), V("最大加速"))),
        iff(cmp(">", V("出力"), N(1)), [setv("出力", N(1))]),
        if_else(cmp(">", V("出力"), N(0.001)), [setvv("推力の向き", vnorm(VV("加速")))],
                [setvv("推力の向き", VS("up"))]),
        # 上昇・降下：重力の反対を「上」にして、傾きすぎないようにする
        iff({"type": "logic", "op": "or", "a": phase_is(ASCENT), "b": phase_is(DESCENT)}, [
            setvv("上", vnorm(vmul(VV("重力"), N(-1)))),
            setv("上への加速", vdot(VV("加速"), VV("上"))),
            if_else(cmp("<", V("上への加速"), N(0.05)), [
                # 下へ速めたいときはほとんど噴かさない（重力で落ちる）。横のずれだけ直す
                setvv("横の加速", vperp(VV("加速"), VV("上"))),
                setvv("推力の向き", vtilt(vnorm(vadd(VV("横の加速"), VV("上"))), VV("上"), 25)),
                setv("出力", div(vlen(VV("横の加速")), V("最大加速"))),
                iff(cmp(">", V("出力"), N(0.02)), [setv("出力", N(0.02))]),
            ], [
                setvv("推力の向き", vtilt(VV("推力の向き"), VV("上"), 30)),
                setv("出力", div(div(V("上への加速"), vdot(VV("推力の向き"), VV("上"))), V("最大加速"))),
                iff(cmp(">", V("出力"), N(1)), [setv("出力", N(1))]),
            ]),
        ]),
        # 姿勢：船の上を推力の向きに合わせる。推力の向きを船から見た 右・前 の成分に分ける
        setv("右へ", vcomp(VV("推力の向き"), "右")),
        setv("前へ", vcomp(VV("推力の向き"), "前")),
        iff(cmp("<", vcomp(VV("推力の向き"), "上"), N(0.3)), [
            # 大きくずれている時は、向きを変えるのを先に（横の向きだけ見て、出力を下げる）
            setvv("横", vnorm(vperp(VV("推力の向き"), VS("ship_up")))),
            setv("右へ", vcomp(VV("横"), "右")),
            setv("前へ", vcomp(VV("横"), "前")),
            iff(cmp(">", V("出力"), N(0.25)), [setv("出力", N(0.25))]),
        ]),
        # 前後・左右の出力の差（ずれ × 2 − 傾く速さ × 0.042）
        setv("前後", sub(mul(V("前へ"), N(2)), mul(S("pitch_rate"), N(0.0419)))),
        setv("左右", add(mul(V("右へ"), N(-2)), mul(S("roll_rate"), N(0.0419)))),
    ]
    for (x, z), name in SHIP_VALVE_NAMES.items():
        ox, oz = (x - 2) * 0.5, (z - 2) * 0.5
        terms = [V("出力")]
        if oz:
            terms.append(mul(V("前後"), N(-oz * 0.5)))
        if ox:
            terms.append(mul(V("左右"), N(ox * 0.5)))
        fly.append(valve(name, add(fn("マイナスなら0", add(*terms)), N(SHIP_KEEP))))
    fly += [
        stmt("ignite", dev="*"),
        # 矢印を見せる：緑＝行きたい速度、黄＝推力の向き、赤＝重力
        show_vec(VV("行きたい速度"), "緑", 0.5),
        show_vec(VV("推力の向き"), "黄", 6),
        show_vec(VV("重力"), "赤", 1),
    ]
    body.append(if_else({"type": "logic", "op": "or", "a": phase_is(WAIT), "b": phase_is(LANDED)},
                        close_valves(), fly))
    msg = lambda text: show(join(T(f"ルミナス号 航法\n{text}"), T("\n速さ "), rnd(S("speed")), T(" m/s")), 1.3)  # noqa: E731
    body += [
        iff(phase_is(WAIT), [show(T("ルミナス号 航法\n操縦席に座ると発進します"), 1.3)]),
        iff(phase_is(COUNT), [show(join(T("ルミナス号 航法\n発進まで "), rnd(V("残り"))), 1.3)]),
        iff(phase_is(ASCENT), [msg("上昇")]),
        iff(phase_is(CRUISING), [show(join(T("ルミナス号 航法\n巡航  月まで "), rnd(V("距離")), T(" m\n速さ "),
                                           rnd(S("speed")), T(" m/s")), 1.3)]),
        iff(phase_is(DESCENT), [show(join(T("ルミナス号 航法\n降下  高さ "), rnd(V("高さ")), T(" m\n速さ "),
                                          rnd(S("speed")), T(" m/s")), 1.3)]),
        iff(phase_is(LANDED), [msg("着陸")]),
    ]
    tick["body"] = body
    p["scripts"] += [start, tick]
    return p


# --- 第3章：搬入の手順（見本：扉の開け閉めだけ。クレーンの手順はプレイヤーが足す） -------------
def crane_starter():
    p = B.new_program()
    start = B.new_stmt("on_start")
    start["body"] = [show(T("搬入の制御\n手順を動かしています"), 1.6)]
    tick = B.new_stmt("on_tick")
    tick["body"] = [
        stmt("act_rotate_to", dev="扉のモーター", angle=N(90), speed=N(60)),
        stmt("wait", n=N(2)),
        stmt("act_rotate_to", dev="扉のモーター", angle=N(0), speed=N(60)),
        stmt("wait", n=N(2)),
    ]
    p["scripts"] += [start, tick]
    return p


def crane_solution():
    """クレーンの手順を足したもの（テストと、ヒントの見本）。"""
    p = crane_starter()
    tick = p["scripts"][1]
    tick["body"] = [
        stmt("act_rotate_to", dev="扉のモーター", angle=N(90), speed=N(60)),
        stmt("wait", n=N(1.5)),
        stmt("act_rotate_to", dev="クレーンの旋回", angle=N(180), speed=N(60)),
        stmt("wait", n=N(3.5)),
        stmt("act_rotate_to", dev="クレーンの旋回", angle=N(0), speed=N(60)),
        stmt("wait", n=N(3.5)),
        stmt("act_rotate_to", dev="扉のモーター", angle=N(0), speed=N(60)),
        stmt("wait", n=N(1.5)),
    ]
    return p


# --- 第11章：迎撃（見本は砲塔を上に向けるだけ。ミサイルへの矢印と「撃つ」をプレイヤーが足す） --------
def intercept_starter():
    p = B.new_program()
    tick = B.new_stmt("on_tick")
    tick["body"] = [
        stmt("turret_aim", dev="*", vec=vec(0, 1, 0, "船から見た")),
        if_else(cmp(">", S("missile_count"), N(0)), [
            show(join(T("迎撃の制御\nミサイル "), S("missile_count"), T(" 発")), 1.6),
        ], [
            show(T("迎撃の制御\nミサイルなし"), 1.6),
        ]),
    ]
    p["scripts"].append(tick)
    return p


def intercept_solution():
    p = intercept_starter()
    tick = p["scripts"][0]
    tick["body"][0] = stmt("turret_aim", dev="*", vec=VS("nearest_missile"))
    tick["body"][1]["body"].insert(0, stmt("turret_fire", dev="*"))
    return p


# --- 第12章：ロボット（ホバーバイクと同じ浮き方。動いている間は腕を振る） -------------------------
ROBOT_THRUST4 = 160000.0
ROBOT_VALVES = ("バルブ 後左", "バルブ 後右", "バルブ 前左", "バルブ 前右")


def robot_program():
    p = B.new_program()
    start = B.new_stmt("on_start")
    start["body"] = [setv("地面", S("altitude")), setv("浮く高さ", N(0.8))]
    tick = B.new_stmt("on_tick")
    tick["body"] = [
        # コックピットの扉：座ると閉じる・降りると開く
        if_else(seated(), [stmt("act_rotate_to", dev="コックピット", angle=N(-90), speed=N(120))],
                [stmt("act_rotate_to", dev="コックピット", angle=N(0), speed=N(120))]),
        if_else(seated(), [
            setv("前後目標", mul(sub(key("w"), key("s")), N(9))),
            setv("左右目標", mul(sub(key("d"), key("a")), N(9))),
            iff(key("space"), [chg("浮く高さ", N(0.12))]),
            iff(key("left shift"), [chg("浮く高さ", N(-0.12))]),
            iff(cmp("<", V("浮く高さ"), N(0.4)), [setv("浮く高さ", N(0.8))]),
        ], [
            setv("前後目標", N(0)), setv("左右目標", N(0)),
            iff(cmp("<", S("speed"), N(1.0)), [chg("浮く高さ", N(-0.04))]),
        ]),
        iff(cmp(">", V("浮く高さ"), N(6)), [setv("浮く高さ", N(6))]),
        iff(cmp("<", V("浮く高さ"), N(0)), [setv("浮く高さ", N(0))]),
        setv("目標高さ", add(V("地面"), V("浮く高さ"))),
        # 腕：進んでいる間は前後に振る
        if_else(cmp(">", S("speed"), N(1.5)), [
            stmt("act_rotate_to", dev="左腕", angle=mul(fn("sin", mul(S("time"), N(360))), N(35)), speed=N(240)),
            stmt("act_rotate_to", dev="右腕", angle=mul(fn("sin", mul(S("time"), N(360))), N(35)), speed=N(240)),
        ], [
            stmt("act_rotate_to", dev="左腕", angle=N(0), speed=N(90)),
            stmt("act_rotate_to", dev="右腕", angle=N(0), speed=N(90)),
        ]),
        if_else(both(cmp("<", V("浮く高さ"), N(0.3)), cmp("<", S("altitude"), add(V("地面"), N(0.3)))),
                close_valves() + [show(T("ロボット\n座ると動きます"), 1.0)],
                hover_control(ROBOT_THRUST4, ROBOT_VALVES)
                + [show(join(T("ロボット\n速さ "), rnd(S("speed")), T(" m/s")), 1.0)]),
    ]
    p["scripts"] += [start, tick]
    return p


# --- 終章：再加速の航法（看板の地点への矢印を入れる） ------------------------------------------
def reaccel_starter():
    p = B.new_program()
    tick = B.new_stmt("on_tick")
    tick["body"] = [
        setvv("向き", vto([0.0, 0.0, 0.0])),
        show_vec(VV("向き"), "緑", 0.05),
        show(join(T("再加速の航法\n目標までの距離 "), rnd(vlen(VV("向き"))), T(" m")), 1.8),
    ]
    p["scripts"].append(tick)
    return p


def reaccel_solution(target):
    """看板の「目標の地点」を入れたもの（ヒントの見本）。"""
    p = reaccel_starter()
    p["scripts"][0]["body"][0] = setvv("向き", vto([float(x) for x in target]))
    return p


PROGRAMS = {"bike": bike_program, "drone": drone_program, "drone_remote": remote_program, "crane": crane_starter,
            "intercept": intercept_starter, "robot": robot_program, "reaccel": reaccel_starter}

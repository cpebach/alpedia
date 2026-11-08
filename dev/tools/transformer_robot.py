"""変形ロボット「ツバメ」：ロケット ⇄ 人型 に変形する乗り物（本体の素材・機器だけ）の形とブロックプログラム。

    tools/make_starter_blueprints.py が sauce/blueprints/transformer_robot.alp.json にする。

向き：右 = +X、上 = +Y、前 = +Z。ボクセルは 0.5 m。
推進機の向き（dir）は「押す向き」（噴射はその反対）。

人型（変形 = 0）
  ・足の裏の4つ（かかと・つま先 × 左右）が上へ押して浮く。傾きはこの4つの出力の差で保つ
  ・腰の左右のポッド（ヒンジ）は横向き。横へすばやく動く（左のポッドは右へ、右のポッドは左へ押す）
  ・背中のポッド（ヒンジ）は前へ押す。前へは、背中のポッドと少しの前傾きで、ゆっくり加速する
ロケット（変形 = 1）
  ・腕（ローター）を頭の上へ上げてひれに、腰と背中のポッド（ヒンジ）を下へ折って、推進機は全部下（後ろ）を向く
  ・足の4つ＋腰の2つ＋背中の2つ、8つ全部で下から押す。加速は速いが、曲がるのは推進機の出力の差だけ
    （傾ける速さに上限がある）なので、小回りはきかない
操縦席：胸の中。座っていないときは胸のハッチ（ヒンジ）が前へ開き、地面から乗り込める
"""
UP, DOWN, RIGHT, LEFT, FWD, BACK = 2, 3, 0, 1, 4, 5

FUEL = 800.0
VALVES_FEET = ("バルブ 後左", "バルブ 後右", "バルブ 前左", "バルブ 前右")


def dev(def_id, name, d=UP, **state):
    return {"def": def_id, "dir": d, "name": name, **({"state": state} if state else {})}


# アクチュエーターの設定の動作は「なし」：操縦席の X では動かさず、プログラムだけが動かす
PROG = {"action": "none", "amount": 0.0, "speed": 90.0}


def tank(name="燃料タンク"):
    return dev("tank", name, contents={"fuel": FUEL})


def valve(name):
    return dev("valve", name, open=0.0)


def voxels():
    """[(cell, 素材, 機器, 部分)]。部分：body（胴・脚）・hatch・arm_l・arm_r・hip_l・hip_r・back。
    部分どうしは、アクチュエーター（ローター・ヒンジ）の出力面でだけつながる（links）。"""
    V = []

    def put(cell, mat="aluminum", d=None, part="body"):
        V.append((tuple(cell), mat, d, part))

    # --- 足（y=0：かかと・つま先に推進機）・足首（y=1：バルブとタンク）・脚（y=2..3） ---
    for x, side in ((1, "左"), (3, "右")):
        put((x, 0, 0), "aluminum", dev("thruster", f"推進機 後{side}"))
        put((x, 0, 1))
        put((x, 0, 2), "aluminum", dev("thruster", f"推進機 前{side}"))
        put((x, 1, 0), "aluminum", valve(f"バルブ 後{side}"))
        put((x, 1, 1), "aluminum", tank())
        put((x, 1, 2), "aluminum", valve(f"バルブ 前{side}"))
        put((x, 2, 1), "aluminum", tank())
        put((x, 2, 0))                          # 脚は 1×3 の太さ（やわらかいブロックのワールドでも曲がりにくい）
        put((x, 2, 2))
        put((x, 3, 0))
        put((x, 3, 1))
        put((x, 3, 2))
    # --- 腰（y=4） ---
    for x in (1, 2, 3):
        put((x, 4, 0))
        put((x, 4, 1))
        put((x, 4, 2))
    # 腰のポッド：胴の下の段の左右のヒンジ（出力は外向き。+ の角度で下へ倒れる）の先に推進機
    # （人型では内向きに押す＝横へ動く。重心に近い高さ）
    put((0, 5, 1), "aluminum", dev("hinge", "左腰", LEFT, hinge=DOWN, preset=PROG))
    put((-1, 5, 1), "aluminum", dev("thruster", "推進機 腰左", RIGHT), "hip_l")
    put((4, 5, 1), "aluminum", dev("hinge", "右腰", RIGHT, hinge=DOWN, preset=PROG))
    put((5, 5, 1), "aluminum", dev("thruster", "推進機 腰右", LEFT), "hip_r")
    # --- 胴（y=5..9、x=1..3、z=0..2）。胸の中に操縦席、前はハッチ ---
    hatch = {(x, y, 2) for x in (1, 2, 3) for y in (6, 7)}
    special = {(1, 5, 1): ("aluminum", valve("バルブ 腰左")),           # 胴の下の段：腰のポッドのバルブとタンク
               (2, 5, 1): ("aluminum", tank()),
               (3, 5, 1): ("aluminum", valve("バルブ 腰右")),
               (2, 6, 1): ("aluminum", dev("seat", "操縦席", FWD)),
               (2, 7, 1): None,                                     # 操縦席の頭の上（空き）
               (1, 6, 0): ("aluminum", dev("battery", "バッテリー")),
               (3, 6, 1): ("aluminum", dev("computer", "変形ロボットの制御", BACK,
                                        script="transformer.py", blocks="transformer.blocks.json", running=True)),
               (3, 7, 0): ("aluminum", dev("battery", "バッテリー")),
               (2, 8, 2): ("aluminum", dev("hinge", "ハッチ", DOWN, hinge=FWD, preset=PROG)),
               (1, 8, 1): ("aluminum", dev("actuator", "左肩", LEFT, rot_speed=120.0, preset=PROG)),
               (3, 8, 1): ("aluminum", dev("actuator", "右肩", RIGHT, rot_speed=120.0, preset=PROG))}
    for x in (1, 2, 3):
        for y in range(5, 10):
            for z in (0, 1, 2):
                c = (x, y, z)
                if c in hatch:
                    continue
                if c in special:
                    if special[c] is not None:
                        put(c, special[c][0], special[c][1])
                    continue
                mat = "aluminum"
                put(c, mat)
    for x, y, z in sorted(hatch):
        put((x, y, z), "glass" if (x, y) == (2, 7) else "rubber", None, "hatch")
    # 頭
    put((2, 10, 1), "aluminum")
    put((2, 10, 2), "glass")
    put((2, 11, 1), "aluminum")
    put((2, 10, 0), "aluminum")
    # 腕：肩のローター（出力は外向き）の先に 3 ブロック下へ
    for x, part in ((0, "arm_l"), (4, "arm_r")):
        for y in (8, 7):
            put((x, y, 1), "rubber" if y == 8 else "aluminum", None, part)
        put((x, 6, 1), "rubber", None, part)                         # 手
    # --- 背中のポッド：腰の後ろのヒンジ（出力は後ろ。+ の角度で下へ）の先に タンク・バルブ・推進機（前へ押す） ---
    put((2, 5, -1), "aluminum", dev("hinge", "背中", BACK, hinge=DOWN, preset=PROG))
    put((2, 5, -2), "aluminum", tank(), "back")
    put((1, 5, -2), "aluminum", valve("バルブ 背左"), "back")
    put((3, 5, -2), "aluminum", valve("バルブ 背右"), "back")
    put((0, 5, -2), "aluminum", dev("thruster", "推進機 背左", FWD), "back")
    put((4, 5, -2), "aluminum", dev("thruster", "推進機 背右", FWD), "back")
    return V


# アクチュエーター → (出力の先の部分)
JOINTS = {(0, 5, 1): ((-1, 5, 1), "hip_l"), (4, 5, 1): ((5, 5, 1), "hip_r"), (2, 8, 2): ((2, 7, 2), "hatch"),
          (1, 8, 1): ((0, 8, 1), "arm_l"), (3, 8, 1): ((4, 8, 1), "arm_r"), (2, 5, -1): ((2, 5, -2), "back")}


def links(V):
    """同じ部分のとなりどうしをつなぎ、部分の間はアクチュエーターの出力面だけ。"""
    part = {v[0]: v[3] for v in V}
    out = set()
    for c, p in part.items():
        for d in ((1, 0, 0), (0, 1, 0), (0, 0, 1)):
            nb = (c[0] + d[0], c[1] + d[1], c[2] + d[2])
            if part.get(nb) == p:
                out.add((c, nb))
    for a, (b, _) in JOINTS.items():
        out.add((a, b) if a <= b else (b, a))
    return sorted([list(a), list(b)] for a, b in out)


# ---------------------------------------------------------------------------
# ブロックプログラム
# ---------------------------------------------------------------------------
THRUST = 150000.0        # 推進機1つの最大推力 (N)
KEEP = 0.02              # 飛んでいる間のバルブのいちばん小さい開き具合（火を消さない）


def program(P):
    """P：ブロックを作る道具（mods/story/code/programs.py と同じ関数：N・V・S・setv・iff など）。"""
    N, T, V, S = P.N, P.T, P.V, P.S
    add, sub, mul, div, cmp, both = P.add, P.sub, P.mul, P.div, P.cmp, P.both
    key, seated, setv, chg, iff, if_else, clamp, valve, show, stmt, fn, rnd, join = (
        P.key, P.seated, P.setv, P.chg, P.iff, P.if_else, P.clamp, P.valve, P.show, P.stmt, P.fn, P.rnd, P.join)
    B = P.B

    def rot(dev_name, angle, speed=120):
        return stmt("act_rotate_to", dev=dev_name, angle=N(angle) if isinstance(angle, (int, float)) else angle,
                    speed=N(speed))

    def busy(dev_name):
        return {"type": "devbool", "dev": dev_name, "prop": "動作中"}

    def either(a, b):
        return {"type": "logic", "op": "or", "a": a, "b": b}

    def feet(out, fb, lr):
        """足の4つ：出力 ＋ 前後（後ろを強く＝前へ傾く）＋ 左右（左を強く＝右へ傾く）。"""
        return [valve(VALVES_FEET[0], add(V(out), V(fb), V(lr), N(KEEP))),
                valve(VALVES_FEET[1], add(sub(add(V(out), V(fb)), V(lr)), N(KEEP))),
                valve(VALVES_FEET[2], add(sub(V(out), V(fb)), V(lr), N(KEEP))),
                valve(VALVES_FEET[3], add(sub(sub(V(out), V(fb)), V(lr)), N(KEEP)))]

    def attitude(gain_p, gain_d):
        """傾き目標 − 今の傾き → 前後・左右の出力の差（ずれ × 係数 − 傾く速さ × 係数 ＋ ずれのたまった分）。
        たまった分（前後の補正・左右の補正）は、重心が推進機のまん中からずれていても傾かないようにするため。"""
        out = [chg("前後の補正", mul(sub(V("前傾き目標"), S("pitch")), N(0.0006))),
               chg("左右の補正", mul(sub(V("右傾き目標"), S("roll")), N(0.0006)))]
        out += clamp("前後の補正", -0.25, 0.25) + clamp("左右の補正", -0.25, 0.25)
        return out + [
            setv("前後", add(sub(mul(sub(V("前傾き目標"), S("pitch")), N(gain_p)), mul(S("pitch_rate"), N(gain_d))),
                           V("前後の補正"))),
            setv("左右", add(sub(mul(sub(V("右傾き目標"), S("roll")), N(gain_p)), mul(S("roll_rate"), N(gain_d))),
                           V("左右の補正")))]

    def toward(name, target, step):
        """変数 name を target へ、1 回に step まで近づける（傾ける速さの上限）。"""
        return [setv("差", sub(target, V(name)))] + clamp("差", -step, step) + [chg(name, V("差"))]

    start = B.new_stmt("on_start")
    start["body"] = [setv("変形", N(0)), setv("地面", S("altitude")), setv("浮く高さ", N(0)),
                     setv("前傾き目標", N(0)), setv("右傾き目標", N(0)), setv("前後の補正", N(0)), setv("左右の補正", N(0))]
    tf = B.new_stmt("on_key_down")
    tf["key"] = "q"
    tf["body"] = [iff(seated(), [setv("変形", sub(N(1), V("変形"))),
                                 setv("前傾き目標", N(0)), setv("右傾き目標", N(0))])]

    tick = B.new_stmt("on_tick")
    body = [
        # 降りたら人型に戻る（着地して、ハッチが開く）
        iff({"type": "not", "a": seated()}, [setv("変形", N(0))]),
        # 操縦：W/S 前後、A/D 左右、Space 上・Shift 下（座っていないときは 0）
        if_else(seated(), [setv("前", sub(key("w"), key("s"))), setv("右", sub(key("d"), key("a"))),
                           setv("上", sub(key("space"), key("left shift"))),
                           setv("旋回目標", mul(sub(key("right arrow"), key("left arrow")), N(40)))],
                [setv("前", N(0)), setv("右", N(0)), setv("上", N(0)), setv("旋回目標", N(0))]),
        # 胸のハッチ：座ると閉じる・降りると前へ開く（地面から乗り込める）
        if_else(seated(), [rot("ハッチ", 0, 90)], [rot("ハッチ", 90, 90)]),
        # 変形：ロケット＝腕を上げてひれに、腰と背中のポッドを下へ折る。人型＝元に戻す
        if_else(cmp("=", V("変形"), N(1)),
                [rot("左肩", 180), rot("右肩", -180), rot("左腰", 90), rot("右腰", 90), rot("背中", 90)],
                [rot("左肩", 0), rot("右肩", 0), rot("左腰", 0), rot("右腰", 0), rot("背中", 0)]),
        setv("変形中", N(0)),
        iff(either(busy("左肩"), either(busy("背中"), busy("左腰"))), [setv("変形中", N(1))]),
    ]

    # --- 人型：足の4つで浮く。腰のポッドで横へ、背中のポッドと少しの前傾きで前へ（加速はゆっくり） ---
    humanoid = [
        iff(cmp(">", V("上"), N(0.5)), [chg("浮く高さ", N(0.12))]),
        iff(cmp("<", V("上"), N(-0.5)), [chg("浮く高さ", N(-0.12))]),
        iff(both(cmp("<", V("上"), N(0.5)), cmp("<", S("speed"), N(1.0))),     # 降りた：止まったらゆっくり下りる
            [iff({"type": "not", "a": seated()}, [chg("浮く高さ", N(-0.04))])]),
    ] + clamp("浮く高さ", 0, 8) + [
        setv("前後目標", mul(V("前"), N(6))),
        setv("左右目標", mul(V("右"), N(8))),
        # 前後：少しだけ傾ける（±6°）。横：傾けずに腰のポッドで押す
        setv("前傾き目標", mul(sub(V("前後目標"), S("fwd_speed")), N(2))),
        setv("右傾き目標", mul(sub(V("左右目標"), S("right_speed")), N(1.5))),
    ] + clamp("前傾き目標", -10, 6) + clamp("右傾き目標", -8, 8) + attitude(0.02, 0.012) + [
        # 高さ：目標の高さとのずれ → 上下の速さの目標（下りは 5 m/秒まで）→ 加速
        setv("上下目標", mul(sub(add(V("地面"), V("浮く高さ")), S("altitude")), N(1.0))),
    ] + clamp("上下目標", -5, 4) + [
        setv("加速", mul(sub(V("上下目標"), S("vspeed")), N(2.0))),
    ] + clamp("加速", -4, 4) + [
        setv("出力", sub(div(mul(S("mass"), add(S("gravity"), V("加速"))), N(4 * THRUST)), N(KEEP))),
    ] + feet("出力", "前後", "左右") + [
        # 横：腰のポッドで直接押す（左のポッドは右へ、右のポッドは左へ）
        setv("横", mul(sub(V("左右目標"), S("right_speed")), N(0.15))),
    ] + clamp("横", -0.6, 0.6) + [
        valve("バルブ 腰左", add(fn("マイナスなら0", V("横")), N(KEEP))),
        valve("バルブ 腰右", add(fn("マイナスなら0", sub(N(0), V("横"))), N(KEEP))),
        # 前：背中のポッド。左右の差で向きを保つ（← → で向きを変える）
        setv("背", mul(sub(V("前後目標"), S("fwd_speed")), N(0.08))),
    ] + clamp("背", 0, 0.35) + [
        setv("回転", mul(sub(S("yaw_rate"), V("旋回目標")), N(0.01))),
    ] + clamp("回転", -0.3, 0.3) + [
        valve("バルブ 背左", add(V("背"), fn("マイナスなら0", sub(N(0), V("回転"))), N(0.04))),
        valve("バルブ 背右", add(V("背"), fn("マイナスなら0", V("回転")), N(0.04))),
        stmt("ignite", dev="*"),
        show(join(T("人型  "), rnd(S("speed")), T(" m/s\nQ でロケットに")), 1.3),
    ]
    landed = [valve("*", N(0)), show(T("変形ロボット ツバメ\n座ると浮きます"), 1.3)]

    # --- ロケット：8つ全部で押す。曲がるのは出力の差だけ（傾ける速さは 15°/秒まで） ---
    rocket = [
        setv("浮く高さ", sub(S("altitude"), V("地面"))),
    ] + toward("前傾き目標", mul(V("前"), N(40)), 0.75) + toward("右傾き目標", mul(V("右"), N(40)), 0.75) + \
        attitude(0.03, 0.02) + [
        # 上下：Space で全開、何も押さなければ上下の速さを 0 に、Shift で下りる
        setv("加速", sub(mul(V("上"), N(3)), mul(S("vspeed"), N(1.5)))),
    ] + clamp("加速", -4, 4) + [
        setv("出力", div(mul(S("mass"), add(S("gravity"), V("加速"))),
                       mul(N(8 * THRUST), fn("cos", S("tilt"))))),
        # Space で全開（傾きを保つ分の余裕を残して 0.8）
        iff(cmp(">", V("上"), N(0.5)), [setv("出力", N(0.8))]),
    ] + clamp("出力", 0, 0.8) + feet("出力", "前後", "左右") + [
        valve("バルブ 腰左", add(V("出力"), V("左右"), N(KEEP))),
        valve("バルブ 腰右", add(sub(V("出力"), V("左右")), N(KEEP))),
        # 背中のポッド（折りたたむと重心の後ろ・下向き）：後ろの足と同じく、強くすると前へ傾く
        valve("バルブ 背左", add(V("出力"), V("前後"), N(KEEP))),
        valve("バルブ 背右", add(V("出力"), V("前後"), N(KEEP))),
        stmt("ignite", dev="*"),
        show(join(T("ロケット  "), rnd(S("speed")), T(" m/s\nQ で人型に")), 1.3),
    ]
    # 座ったら浮く（地面から 1.2 m）
    body.append(iff(both(seated(), cmp("<", V("浮く高さ"), N(0.6))), [setv("浮く高さ", N(1.2))]))
    body.append(if_else(both(cmp("=", V("変形"), N(1)), cmp("=", V("変形中"), N(0))), rocket,
                        [if_else(both(cmp("<", V("浮く高さ"), N(0.3)),
                                      cmp("<", S("altitude"), add(V("地面"), N(0.3)))),
                                 landed, humanoid)]))
    tick["body"] = body
    prog = B.new_program()
    prog["scripts"] += [start, tf, tick]
    return prog


NAME = "変形ロボット ツバメ"
DESC = ("ロケットと人型に変形するロボット（ブロックプログラムで動く）。座って Q で変形。"
        "人型：W/S/A/D・Space/Shift でホバリング、横へはすばやく、前へはゆっくり。"
        "ロケット：推進機が全部下を向いて加速は速いが、W/S/A/D で傾けて曲がるだけなので小回りはきかない（Space で全開）。"
        "座っていないときは胸のハッチが開いて、地面から乗り込める")

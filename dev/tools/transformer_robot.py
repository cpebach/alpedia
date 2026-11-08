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

    def clampv(name, lo, hi):
        """変数 name を lo〜hi（どちらもブロックの式）に収める。"""
        return [iff(cmp(">", V(name), hi), [setv(name, hi)]), iff(cmp("<", V(name), lo), [setv(name, lo)])]

    def feet(out, fb, lr):
        """足の4つ：出力 ＋ 前後（後ろを強く＝前へ傾く）＋ 左右（左を強く＝右へ傾く）。
        前後・左右の差は出力より大きくしない（重力の弱い所で、差のせいで全体の推力が増えて浮き上がらないように）。
        地上（縮尺 1）では 0.4 まで余分に付けてよい。"""
        lim = [setv("差", add(fn("絶対値", V(fb)), fn("絶対値", V(lr)))),
               # 上限：出力 ＋ 0.4 × 縮尺²（地上では 0.4 の余裕、重力の弱い所ではほぼ出力まで）
               setv("差の上限", add(fn("マイナスなら0", V(out)), mul(N(0.4), mul(V("縮尺"), V("縮尺"))))),
               iff(cmp(">", V("差"), V("差の上限")), [
                   setv("縮め", div(V("差の上限"), add(V("差"), N(1e-6)))),
                   setv(fb, mul(V(fb), V("縮め"))), setv(lr, mul(V(lr), V("縮め")))])]
        return lim + [valve(VALVES_FEET[0], add(V(out), V(fb), V(lr), N(KEEP))),
                valve(VALVES_FEET[1], add(sub(add(V(out), V(fb)), V(lr)), N(KEEP))),
                valve(VALVES_FEET[2], add(sub(V(out), V(fb)), V(lr), N(KEEP))),
                valve(VALVES_FEET[3], add(sub(sub(V(out), V(fb)), V(lr)), N(KEEP)))]

    def attitude(gain_p, gain_d):
        """傾き目標 − 今の傾き → 前後・左右の出力の差（ずれ × 係数 − 傾く速さ × 係数 ＋ ずれのたまった分）。
        たまった分（前後の補正・左右の補正）は、重心が推進機のまん中からずれていても傾かないようにするため。"""
        ki, lim = mul(N(0.0006), V("縮尺")), mul(N(0.25), V("縮尺"))
        out = [chg("前後の補正", mul(sub(V("前傾き目標"), S("pitch")), ki)),
               chg("左右の補正", mul(sub(V("右傾き目標"), S("roll")), ki))]
        out += clampv("前後の補正", mul(lim, N(-1)), lim) + clampv("左右の補正", mul(lim, N(-1)), lim)
        # 重力の弱い所では足の出力も小さく、傾きを戻す力も弱い：係数を重力に合わせて下げる（ゆっくり傾ける）
        p, d = mul(N(gain_p), V("縮尺")), mul(N(gain_d), V("縮尺の根"))
        return out + [
            setv("前後", add(sub(mul(sub(V("前傾き目標"), S("pitch")), p), mul(S("pitch_rate"), d)),
                           V("前後の補正"))),
            setv("左右", add(sub(mul(sub(V("右傾き目標"), S("roll")), p), mul(S("roll_rate"), d)),
                           V("左右の補正")))]

    def toward(name, target, step):
        """変数 name を target へ、1 回に step まで近づける（傾ける速さの上限）。"""
        return [setv("差", sub(target, V(name)))] + clamp("差", -step, step) + [chg(name, V("差"))]

    start = B.new_stmt("on_start")
    start["body"] = [setv("変形", N(0)), setv("着地", N(1)), setv("止まった", N(0)), setv("目標高さ", S("altitude")),
                     setv("上下を押した", N(0)),
                     setv("前傾き目標", N(0)), setv("右傾き目標", N(0)), setv("前後の補正", N(0)), setv("左右の補正", N(0))]
    tf = B.new_stmt("on_key_down")
    tf["key"] = "q"
    tf["body"] = [iff(seated(), [setv("変形", sub(N(1), V("変形"))), setv("目標高さ", S("altitude")),
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
        # 傾きを保つ係数の縮尺：推進機の出力（前のティック）が小さいと足の差も小さくしか付けられないので、係数も下げる
        # （重力の弱い所でふらつかないように。地上で浮いているときに 1）
        setv("縮尺", mul(fn("マイナスなら0", V("出力")), add(N(2), mul(V("変形"), N(2))))),
    ] + clamp("縮尺", 0.1, 1) + [
        setv("縮尺の根", fn("平方根", V("縮尺"))),
        # 腰・背中のポッドを消さずにおく少しの出力（重力の弱い所では、これだけでも流されるので小さく）
        setv("少し", mul(N(KEEP), V("縮尺"))),
    ] + clamp("少し", 0.002, KEEP) + [
        iff(either(busy("左肩"), either(busy("背中"), busy("左腰"))), [setv("変形中", N(1))]),
    ]

    # --- 人型：足の4つで浮く。腰のポッドで横へ、背中のポッドと少しの前傾きで前へ（加速はゆっくり） ---
    # 高さは「目標の高さ」との差で保つ（地面・甲板・船の上のどこでも同じ）。Space/Shift の間は目標が一緒に動く
    humanoid = [
        # Space/Shift を離したら、その高さ（＋0.3 m）で止まる（地面に押しつけたままにしない）
        if_else(cmp(">", fn("絶対値", V("上")), N(0.5)),
                [setv("目標高さ", add(S("altitude"), mul(V("上"), N(3)))), setv("上下を押した", N(1))],
                [iff(cmp("=", V("上下を押した"), N(1)),
                     [setv("目標高さ", add(S("altitude"), N(0.3))), setv("上下を押した", N(0))])]),
    ] + [
        setv("前後目標", mul(V("前"), N(6))),
        setv("左右目標", mul(V("右"), N(8))),
        # 前後：少しだけ傾ける（±6°）。横：傾けずに腰のポッドで押す
        setv("前傾き目標", mul(sub(V("前後目標"), S("fwd_speed")), N(4))),
        setv("右傾き目標", mul(sub(V("左右目標"), S("right_speed")), N(1.5))),
        # 傾ける上限：地上は前 6°・後ろ 10°。重力の弱い所（アマツ・月）では、傾けないと止まれないので 30° まで
        setv("傾き上限", div(N(98), add(S("gravity"), N(0.1)))),
    ] + clamp("傾き上限", 10, 30) + clampv("前傾き目標", mul(V("傾き上限"), N(-1)), mul(V("傾き上限"), N(0.6))) + \
        clamp("右傾き目標", -8, 8) + attitude(0.02, 0.012) + [
        # 高さ：目標の高さとのずれ → 上下の速さの目標（下りは 5 m/秒まで）→ 加速
        setv("上下目標", sub(V("目標高さ"), S("altitude"))),
        # 降りた：横の動きが止まるまでその高さで待ち、止まったら 3 m/秒でゆっくり下りる（滑りながら着くと転ぶ）。
        # 下りなくなったら（1 秒）着地
        iff({"type": "not", "a": seated()}, [
            if_else(cmp("<", add(fn("絶対値", S("fwd_speed")), fn("絶対値", S("right_speed"))), N(0.6)), [
                setv("上下目標", N(-3)),
                if_else(cmp("<", fn("絶対値", S("vspeed")), N(0.4)), [chg("止まった", N(1))], [setv("止まった", N(0))]),
                iff(cmp(">", V("止まった"), N(20)), [setv("着地", N(1))])],
                [setv("上下目標", N(0)), setv("止まった", N(0))])]),
    ] + clamp("上下目標", -5, 4) + [
        setv("加速", mul(sub(V("上下目標"), S("vspeed")), N(2.0))),
    ] + clamp("加速", -4, 4) + [
        setv("出力", sub(div(mul(S("mass"), add(S("gravity"), V("加速"))),
                            mul(N(4 * THRUST), fn("cos", S("tilt")))), N(KEEP))),
        # 下りるときも浮く力の半分は残す（足の差で傾きを保つため。重力の弱い所では 0 にすると転ぶ）
        setv("出力の下限", div(mul(S("mass"), S("gravity")), N(8 * THRUST))),
        iff(cmp("<", V("出力"), V("出力の下限")), [setv("出力", V("出力の下限"))]),
    ] + feet("出力", "前後", "左右") + [
        # 横：腰のポッドで直接押す（左のポッドは右へ、右のポッドは左へ）
        setv("横", mul(sub(V("左右目標"), S("right_speed")), N(0.15))),
        # 腰のポッドは重心より下で押すので、強すぎると足の差で傾きを保てずに転ぶ：足の出力の 1.2 倍まで（最大 0.6）
        setv("横の上限", mul(fn("マイナスなら0", V("出力")), N(1.2))),
    ] + clamp("横の上限", 0, 0.6) + clampv("横", mul(V("横の上限"), N(-1)), V("横の上限")) + [
        valve("バルブ 腰左", add(fn("マイナスなら0", V("横")), V("少し"))),
        valve("バルブ 腰右", add(fn("マイナスなら0", sub(N(0), V("横"))), V("少し"))),
        # 前：背中のポッド。左右の差で向きを保つ（← → で向きを変える）
        setv("背", mul(sub(V("前後目標"), S("fwd_speed")), N(0.08))),
    ] + clamp("背", 0, 0.35) + clampv("背", N(0), V("横の上限")) + [
        setv("回転", mul(sub(S("yaw_rate"), V("旋回目標")), N(0.01))),
    ] + clamp("回転", -0.3, 0.3) + [
        valve("バルブ 背左", add(V("背"), fn("マイナスなら0", sub(N(0), V("回転"))), V("少し"))),
        valve("バルブ 背右", add(V("背"), fn("マイナスなら0", V("回転")), V("少し"))),
        stmt("ignite", dev="*"),
        show(join(T("人型  "), rnd(S("speed")), T(" m/s\nQ でロケットに")), 1.3),
    ]
    landed = [valve("*", N(0)), show(T("変形ロボット ツバメ\n座ると浮きます"), 1.3)]

    # --- ロケット：8つ全部で押す。曲がるのは出力の差だけ（傾ける速さは 15°/秒まで） ---
    rocket = [
        setv("目標高さ", S("altitude")),
        # 傾けられるのは地上で 40°。重力の弱い所（アマツ・月）では 60° まで倒して、横へ飛べる
        setv("ロケットの傾き", div(N(392), add(S("gravity"), N(0.1)))),
    ] + clamp("ロケットの傾き", 40, 60) + \
        toward("前傾き目標", mul(V("前"), V("ロケットの傾き")), 0.75) + \
        toward("右傾き目標", mul(V("右"), V("ロケットの傾き")), 0.75) + \
        attitude(0.03, 0.02) + [
        # 上下：Space で全開、何も押さなければ上下の速さを 0 に、Shift で下りる
        setv("加速", sub(mul(V("上"), N(3)), mul(S("vspeed"), N(1.5)))),
    ] + clamp("加速", -4, 4) + [
        setv("出力", div(mul(S("mass"), add(S("gravity"), V("加速"))),
                       mul(N(8 * THRUST), fn("cos", S("tilt"))))),
        # Space で全開（傾きを保つ分の余裕を残して 0.8）
        iff(cmp(">", V("上"), N(0.5)), [setv("出力", N(0.8))]),
        # 下りるときも浮く力の半分は残す（傾きを保つため）
        setv("出力の下限", div(mul(S("mass"), S("gravity")), N(16 * THRUST))),
        iff(cmp("<", V("出力"), V("出力の下限")), [setv("出力", V("出力の下限"))]),
    ] + clamp("出力", 0, 0.8) + feet("出力", "前後", "左右") + [
        valve("バルブ 腰左", add(V("出力"), V("左右"), N(KEEP))),
        valve("バルブ 腰右", add(sub(V("出力"), V("左右")), N(KEEP))),
        # 背中のポッド（折りたたむと重心の後ろ・下向き）：後ろの足と同じく、強くすると前へ傾く
        valve("バルブ 背左", add(V("出力"), V("前後"), N(KEEP))),
        valve("バルブ 背右", add(V("出力"), V("前後"), N(KEEP))),
        stmt("ignite", dev="*"),
        show(join(T("ロケット  "), rnd(S("speed")), T(" m/s\nQ で人型に")), 1.3),
    ]
    # 着地しているときに座ったら浮く（今の高さ ＋ 1.2 m）
    body.append(iff(both(seated(), cmp("=", V("着地"), N(1))),
                    [setv("着地", N(0)), setv("止まった", N(0)), setv("目標高さ", add(S("altitude"), N(1.2))),
                     setv("前後の補正", N(0)), setv("左右の補正", N(0))]))
    body.append(if_else(both(cmp("=", V("変形"), N(1)), cmp("=", V("変形中"), N(0))), rocket,
                        [if_else(cmp("=", V("着地"), N(1)), landed, humanoid)]))
    tick["body"] = body
    prog = B.new_program()
    prog["scripts"] += [start, tf, tick]
    return prog


NAME = "変形ロボット ツバメ"
DESC = ("ロケットと人型に変形するロボット（ブロックプログラムで動く）。座って Q で変形。"
        "人型：W/S/A/D・Space/Shift でホバリング、横へはすばやく、前へはゆっくり。"
        "ロケット：推進機が全部下を向いて加速は速いが、W/S/A/D で傾けて曲がるだけなので小回りはきかない（Space で全開）。"
        "座っていないときは胸のハッチが開いて、地面から乗り込める")

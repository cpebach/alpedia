"""チュートリアルの「無線とドローン」パート。

操縦台（操縦席・コンピューター・アンテナ）に座って押したキーを無線で送り、
ドローン（推進機4つ・燃料タンク・コンピューター・アンテナ）がそれを受け取って飛ぶ。
ワールドの作成は tutorial.create_tutorial（ひとつのチュートリアルにまとめてある）。
"""

CHANNEL = "drone"

SIGNS = [
    """■ アンテナと無線通信

アンテナのある構築物どうしは、コンピューターで
情報をやり取りできます（電力が必要）。
　api.send("チャンネル", 情報)　送る
　api.receive("チャンネル")　　届いたものを受け取る
　api.latest("チャンネル")　　　いちばん新しいものを見る
アンテナがないと、この3つはエラーになります。
届く距離は 3000 m（相手のアンテナの距離の短いほう）。
ブロックプログラムでは「通信」のブロックを使います。""",
    """■ ドローンを遠隔操作しよう

右の操縦台の操縦席に座ると（E）、押したキーが
無線でドローンに送られます。
　W/S：前・後　A/D：左・右（操縦台の向きが前）
　Space：上昇　Left Shift：下降
　X：自動で着陸／もう一度 X で離陸
操縦台の画面に、高度・速さ・燃料・電波が出ます。
電波が切れるとドローンはその場でゆっくり降ります。
仕組みは drone.py（ドローン）と drone_remote.py（操縦台）""",
]

REMOTE_SCRIPT = '''"""ドローンの操縦台（送信側）

操縦席に座っている間に押しているキーを、操縦台の向き（前・右）と一緒に
チャンネル "drone" で送ります。ドローンから届く状態（"drone/status"）を画面に表示します。
この構築物にアンテナが必要です（無いと api.send がエラーになります）。
"""
CH = "drone"


def tick(api):
    X, Y, Z = api.orientation()            # 操縦台のローカル軸（ワールド座標）：前 = Z、右 = X
    m = api.memory
    if not api.antenna:
        api.display("ドローン操縦台\\nアンテナが使えません\\n（アンテナが無い・OFF・電力不足）", width=2.2,
                    color=(1.0, 0.6, 0.5))
        return
    keys = sorted(api.input)
    # X は押すたびに「着陸 ⇔ 飛行」を切り替える
    down = "x" in keys
    if down and not m.get("x_prev"):
        m["land"] = not m.get("land", False)
    m["x_prev"] = down
    api.send(CH, {"keys": keys, "fwd": Z, "right": X, "land": m.get("land", False)})

    got = api.receive(CH + "/status")
    if got:
        m["st"] = got[-1]["data"]
        m["dist"] = got[-1]["distance"]
        m["t"] = api.time
    st = m.get("st")
    if st is None or api.time - m.get("t", -99) > 1.0:
        radio = "電波：ドローンが見つかりません"
        body = "（ドローンのコンピューターとアンテナを確認）"
    else:
        radio = "電波：良好（距離 %.0f m）" % m["dist"]
        body = "高度 %.1f m　速さ %.1f m/s\\n燃料 %.0f kg　%s" % (st["alt"], st["speed"], st["fuel"], st["mode"])
    help_ = "W/S 前後  A/D 左右  Space 上昇  Shift 下降\\nX 着陸/離陸（座ると操作できます）"
    api.display("ドローン操縦台\\n" + radio + "\\n" + body + "\\n" + help_, width=2.2,
                color=(0.75, 1.0, 0.85))
'''

DRONE_SCRIPT = '''"""遠隔操作ドローン（受信側）

チャンネル "drone" で届くキー（操縦台から）に合わせて飛びます。
・前後左右のキー → その向きに進む（操縦台の向きが前）。離すとその場で止まる
・Space / Left Shift → 上昇・下降。何も押さなければ高さを保つ
・電波が切れたら、その場でゆっくり降りて着陸する
四隅の推進機の出力に差をつけて機体を傾け、傾いた向きに進みます。
自分の状態はチャンネル "drone/status" で送り返します。
"""
import math

CH = "drone"
THRUST = 150000.0      # 推進機1つの最大推力 (N)
MAX_SPEED = 8.0        # 水平の最高速度 (m/s)
CLIMB = 3.0            # 上昇・下降の速さ (m/s)
MAX_LEAN = 0.45        # 最大の傾き（約 24°）
DT = 0.05              # コンピューターの実行間隔 (秒)


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def flat(v, up):
    """上方向の成分を除いた水平のベクトル"""
    d = dot(v, up)
    return [v[0] - up[0] * d, v[1] - up[1] * d, v[2] - up[2] * d]


def unit(v):
    n = math.sqrt(dot(v, v)) or 1.0
    return [v[0] / n, v[1] / n, v[2] / n]


def tick(api):
    m = api.memory
    up = api.up
    alt = api.altitude
    if "ground" not in m:
        m["ground"] = alt                  # 起動したときの高さ＝地面に置いたときの高さ（地面からの重心の高さ）
        m["target"] = alt
    # アンテナが使えない（OFF・電力不足など）ときは、電波が切れたのと同じ扱い（その場で着陸）
    msg = api.latest(CH, max_age=0.5) if api.antenna else None
    link = msg is not None
    keys = set(msg["keys"]) if link else set()
    land = (not link) or bool(msg.get("land", False))   # 電波が切れたときも着陸する

    # --- 目標の高さ ---
    target = m["target"]
    vs = api.vertical_speed
    if "space" in keys and not land:
        target = alt + CLIMB * 1.3         # 少し上を目標にし続ける（＝ほぼ CLIMB の速さで上がる）
        m["climb"] = True
    elif "left shift" in keys or land:
        target = alt - (CLIMB if not land else 1.5) * 1.3   # 着陸・電波なしのときはゆっくり降りる
        m["climb"] = True
    elif m.get("climb"):
        target = alt + vs * 0.3            # キーを離したら、その辺りの高さで止まる
        m["climb"] = False
    target = max(target, m["ground"])
    m["target"] = target
    if alt < m["ground"] + 0.3 and target <= m["ground"] + 0.01:
        for t in api.devices("thruster"):
            t.throttle = 0.0               # 地面に着いたら止める
        report(api, alt, "着陸中" if link else "電波なし（着陸）")
        return

    # --- 進みたい速度（操縦台の向きで） ---
    fwd = unit(flat(msg["fwd"], up)) if link else [0, 0, 0]
    right = unit(flat(msg["right"], up)) if link else [0, 0, 0]
    sx = (1 if "d" in keys else 0) - (1 if "a" in keys else 0)
    sz = (1 if "w" in keys else 0) - (1 if "s" in keys else 0)
    want = [(right[i] * sx + fwd[i] * sz) * MAX_SPEED for i in range(3)]
    v = api.velocity
    vh = flat(v, up)
    err = [want[i] - vh[i] for i in range(3)]
    lean = [e * 0.12 for e in err]
    n = math.sqrt(dot(lean, lean))
    if n > MAX_LEAN:
        lean = [x * MAX_LEAN / n for x in lean]
    d = unit([up[i] + lean[i] for i in range(3)])      # 推力を向けたい方向（ワールド）

    # --- 姿勢：機体の上（ローカル Y）を d に向ける ---
    X, Y, Z = api.orientation()
    ex, ez = dot(d, X), dot(d, Z)
    wx, wy, wz = api.angular_velocity_local
    tx = ez * 3.0 - wx * 1.5
    tz = -ex * 3.0 - wz * 1.5

    # --- 高さ：目標の高さに近づける（重さを支える推力＋ずれの分） ---
    acc = max(-4.0, min(4.0, (target - alt) * 1.5 - vs * 2.0))
    pl = api.planet()
    g = pl["mu"] / max(dot([api.position[i] - pl["center"][i] for i in range(3)],
                           [api.position[i] - pl["center"][i] for i in range(3)]), 1.0) if pl else 9.8
    thrusters = api.devices("thruster")
    tilt = max(dot(Y, up), 0.5)
    base = api.mass * (g + acc) / (len(thrusters) * THRUST * tilt) if thrusters else 0.0
    for t in thrusters:
        ox, oy, oz = t.offset
        adj = -oz * tx + ox * tz
        t.throttle = min(max(base + adj * 0.4, 0.0), 1.0)
    report(api, alt, "飛行中" if link else "電波なし（降下中）")


def report(api, alt, mode):
    m = api.memory
    fuel = sum(t.contents.get("fuel", 0) for t in api.devices("tank"))
    if api.antenna:
        api.send(CH + "/status", {"alt": alt - m.get("ground", 0.0), "speed": api.speed, "fuel": fuel, "mode": mode})
    api.display("ドローン\\n" + mode, width=1.0, color=(1.0, 0.85, 0.5))
'''


def drone_voxels():
    """3×3×3 のドローン。下の段の四隅が推進機（上向きに押す）、真ん中の段は燃料タンク、上にアンテナ。"""
    V = []
    corners = {(0, 0): "推進機 左前", (2, 0): "推進機 右前", (0, 2): "推進機 左後", (2, 2): "推進機 右後"}
    for x in range(3):
        for z in range(3):
            if (x, z) in corners:
                V.append(((x, 0, z), "steel", {"def": "thruster", "dir": 2, "name": corners[(x, z)]}))
            else:
                V.append(((x, 0, z), "steel", {"def": "pipe", "dir": 2}))
    mid = {(1, 1): {"def": "tank", "dir": 2, "name": "燃料タンク", "state": {"contents": {"fuel": 800.0}}},
           (0, 1): {"def": "battery", "dir": 2},
           (2, 1): {"def": "computer", "dir": 0, "name": "ドローン制御",
                    "state": {"script": "drone.py", "running": True}}}
    for x in range(3):
        for z in range(3):
            d = mid.get((x, z))
            if d is None and (x == 1 or z == 1):
                d = {"def": "tank", "dir": 2, "state": {"contents": {"fuel": 800.0}}}
            V.append(((x, 1, z), "steel" if d else "aluminum", d))
    V.append(((1, 2, 1), "aluminum", {"def": "antenna", "dir": 2, "name": "アンテナ"}))
    return V


def remote_voxels():
    """操縦台：岩の台の上に操縦席（前 = +Z）、正面の台の上にコンピューターの画面、横にアンテナとバッテリー。"""
    V = [((x, 0, z), "rock", None) for x in range(3) for z in range(3)]
    V.append(((1, 1, 0), "steel", {"def": "seat", "dir": 4, "name": "操縦席"}))
    V.append(((1, 1, 2), "aluminum", None))                     # 画面の台
    V.append(((1, 2, 2), "steel", {"def": "computer", "dir": 5, "name": "送信機",
                                   "state": {"script": "drone_remote.py", "running": True}}))
    V.append(((0, 1, 2), "steel", {"def": "battery", "dir": 2}))
    V.append(((2, 1, 2), "aluminum", {"def": "antenna", "dir": 2, "name": "アンテナ"}))
    return V

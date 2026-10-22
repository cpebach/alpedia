"""ルミナス号の航法プログラム（月への自動飛行）のもと。

ワールドを作るときに、目標（月の発着場）の座標と惑星の値を書き込んで scripts/moon.py にする。
"""

MOON_SCRIPT = r'''"""ルミナス号 航法プログラム：月の街「ルナ・ポート」まで自動で飛ぶ

このコンピューターを E で起動してから、操縦席に E で座ります（座ると発進のカウントダウン）。
  待機 → カウントダウン → 上昇 → 巡航（月の発着場の上空へ）→ 降下 → 着陸
考え方は「行きたい速度 − 今の速度」のぶんだけ加速する、です（重力のぶんも足す）。
船の向き（上 = 推力の向き）は、推進機の出力に差をつけて合わせます（姿勢制御）。
推進機の出力は、それぞれの真上のバルブの開き具合で変えます（バルブを閉じると火が消えるので、点火し直す）。
数字を変えると、速さや降り方が変わります。
"""
import math

PAD = {pad}              # 月の発着場（ワールド座標）
PAD_UP = {pad_up}        # 発着場の上方向
BODIES = {bodies}        # 重力を出す天体 [(中心, mu)]
THRUST = 150000.0        # 推進機1つの推力 (N)
CRUISE = 70.0            # 巡航の速さ (m/s)
BRAKE = 2.5              # 減速の強さ (m/s²)
HOVER_ALT = 90.0         # 降下を始める、発着場の上の高さ (m)
LAND_H = 1.5             # 着陸したとみなす、発着場の上面からの重心の高さ (m)
KV = 0.8                 # 速度のずれへの効き
KEEP = 0.01              # 飛んでいる間のバルブのいちばん小さい開き具合（火を消さない）


def add(a, b, k=1.0):
    return [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]


def sub(a, b):
    return [a[0] - b[0], a[1] - b[1], a[2] - b[2]]


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def length(a):
    return math.sqrt(dot(a, a))


def unit(a):
    n = length(a) or 1.0
    return [a[0] / n, a[1] / n, a[2] / n]


def engines(api):
    """推進機と、その燃料を通すバルブ（いちばん近いもの）の組。推力はバルブの開き具合で変える。"""
    vs = api.devices("valve")
    out = []
    for t in api.devices("thruster"):
        to = t.offset
        best = min(vs, key=lambda v: sum((a - b) ** 2 for a, b in zip(v.offset, to)), default=None)
        out.append((t, best))
    return out


def set_power(t, v, x, keep=KEEP):
    """推進機 t の出力を x（0〜1）にする：バルブ v を開き、火が消えていれば点火し直す（点火は電気を少し使う）。
    keep：飛んでいる間は少しだけ開けておき、火が消えないようにする。"""
    x = min(keep + max(x, 0.0), 1.0)          # 少し足しておく（差をつける分は消さない）
    if v is not None:
        v.open = x
    if x > 0:
        t.ignite()


def stop_all(api):
    """すべてのバルブを閉じる（推進機の火が消える）。"""
    for v in api.devices("valve"):
        v.open = 0.0


def gravity(p):
    g = [0.0, 0.0, 0.0]
    for c, mu in BODIES:
        d = sub(c, p)
        r = max(length(d), 1.0)
        g = add(g, d, mu / r ** 3)
    return g


def steer(api, d, base):
    """船の上方向（推力の向き）を d に合わせるよう、推進機の出力に差をつける。"""
    X, Y, Z = api.orientation()
    dx, dy, dz = dot(X, d), dot(Y, d), dot(Z, d)
    if dy < 0.3:                          # 大きくずれている時は、向きを変えるのを先に
        n = math.hypot(dx, dz) or 1.0
        dx, dz = dx / n, dz / n
        base = min(base, 0.25)
    wx, wy, wz = api.angular_velocity_local
    tx = dz * 2.0 - wx * 2.4
    tz = -dx * 2.0 - wz * 2.4
    for t, v in engines(api):
        ox, oy, oz = t.offset
        set_power(t, v, base + (-oz * tx + ox * tz) * 0.5)


def limit_tilt(d, up, deg):
    """向き d を、up から deg 度以内にする（降りるときに大きく傾かないように）。"""
    c = dot(d, up)
    cmax = math.cos(math.radians(deg))
    if c >= cmax:
        return d
    h = unit(add(d, up, -c))
    return unit(add([x * cmax for x in up], h, math.sin(math.radians(deg))))


def tick(api):
    m = api.memory
    if api.time - m.get("last_tick", -99.0) > 0.5:   # 起動し直したら最初から
        m.clear()
        m["t0"] = api.time
    m["last_tick"] = api.time
    phase = m.get("phase", "wait")
    if phase in ("wait", "countdown") and not api.seated:
        phase = "wait"                    # 誰も座っていなければ待つ
        m["t0"] = api.time
    pos, vel = api.position, api.velocity
    n_thr = len(api.devices("thruster"))
    if n_thr < 4:
        api.display("推進機が足りません\n（4つ必要）", width=1.4, color=(1, 0.5, 0.4))
        return
    a_max = THRUST * n_thr / api.mass
    up = api.up
    to_pad = sub(PAD, pos)
    h_pad = dot(sub(pos, PAD), PAD_UP)                # 発着場からの高さ
    over = add(PAD, PAD_UP, HOVER_ALT)                # 降下を始める点
    v_des = [0.0, 0.0, 0.0]
    msg = ""
    if phase == "wait":
        msg = "操縦席に座ると発進します"
        phase = "countdown" if api.seated else "wait"
    if phase == "countdown":
        left = 3 - (api.time - m["t0"])
        msg = f"発進まで {max(left, 0):.0f}"
        if left <= 0:
            phase = "ascent"
            print("発進！")
    if phase == "ascent":
        v_des = [x * 18.0 for x in up]
        msg = "上昇"
        if api.altitude > 35:
            phase = "cruise"
            print("巡航：月へ")
    if phase == "cruise":
        d = sub(over, pos)
        dist = length(d)
        speed = min(CRUISE, math.sqrt(2 * BRAKE * dist))
        v_des = [x * speed for x in unit(d)]
        msg = f"巡航  月まで {dist:.0f} m"
        if dist < 6 and length(vel) < 4:
            phase = "descent"
            print("降下")
    if phase == "descent":
        side = sub(to_pad, [x * dot(to_pad, PAD_UP) for x in PAD_UP])   # 横のずれ
        rate = min(max((h_pad - LAND_H) * 0.2, 0.6), 4.0)
        if length(side) > max(1.0, h_pad * 0.15):
            rate = 0.0                    # ずれているときは高さを保って、真上に来てから降りる
        k = min(0.35, 3.0 / max(length(side), 1e-6))     # 横は 3 m/s まで
        v_des = add([-x * rate for x in PAD_UP], side, k)
        msg = f"降下  高さ {h_pad:.1f} m"
        if h_pad < LAND_H and length(vel) < 1.2:
            phase = "landed"
            print("着陸しました！ ようこそルナ・ポートへ")
    if phase in ("landed", "wait"):
        stop_all(api)
        if phase == "landed":
            msg = "着陸"
    else:
        g = gravity(pos)
        a_cmd = sub([x * KV for x in sub(v_des, vel)], g)
        if phase == "countdown":
            a_cmd = [0.0, 0.0, 0.0]
        base = min(length(a_cmd) / a_max, 1.0)
        d = unit(a_cmd) if base > 0.001 else up
        if phase in ("ascent", "descent"):
            up = unit([-x for x in g])     # 重力の反対向き（テラにも引かれるので、月の真上から少しずれる）
            a_up = dot(a_cmd, up)
            if a_up <= 0.05:              # 下へ速めたいときはほとんど噴かさない（重力で落ちる）。横のずれだけ直す
                a_h = add(a_cmd, up, -a_up)
                d = limit_tilt(unit(add(a_h, up, 1.0)), up, 25.0)
                base = min(length(a_h) / a_max, 0.02)
            else:
                d = limit_tilt(d, up, 30.0)
                base = min(a_up / dot(d, up) / a_max, 1.0)
        steer(api, d, base)
    m["phase"] = phase
    api.display(f"ルミナス号 航法\n{msg}\n速さ {length(vel):.1f} m/s", width=1.3, color=(0.6, 0.95, 1.0))
'''


def moon_script(pad, pad_up, bodies):
    r = lambda v: "[" + ", ".join(f"{float(x):.3f}" for x in v) + "]"  # noqa: E731
    return (MOON_SCRIPT.replace("{pad}", r(pad)).replace("{pad_up}", r(pad_up))
            .replace("{bodies}", "[" + ", ".join(f"({r(c)}, {float(mu):.1f})" for c, mu in bodies) + "]"))


BIKE_SCRIPT = r'''"""ホバーバイクの制御（座っている人のキーで飛ぶ）

操縦席に座ると、このコンピューターが動き出します（座っていない間は止まって着地します）。
  W/S：前・後　A/D：左・右（バイクの向きが前）　Space：上へ　Left Shift：下へ
地面から HOVER m の高さに浮き、四隅の推進機の出力（それぞれの真上のバルブの開き具合）に差をつけて、進みたい方へ少し傾けます。
ドローン（drone.py）と同じ考え方です。
"""
import math

THRUST = 150000.0      # 推進機1つの最大推力 (N)
HOVER = 1.2            # 浮く高さ（地面に置いたときより上）(m)
MAX_SPEED = 11.0       # 水平の最高速度 (m/s)
MAX_LEAN = 0.4
KEEP = 0.02            # 浮いている間のバルブのいちばん小さい開き具合（火を消さない）


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def flat(v, up):
    d = dot(v, up)
    return [v[0] - up[0] * d, v[1] - up[1] * d, v[2] - up[2] * d]


def unit(v):
    n = math.sqrt(dot(v, v)) or 1.0
    return [v[0] / n, v[1] / n, v[2] / n]


def engines(api):
    """推進機と、その燃料を通すバルブ（いちばん近いもの）の組。推力はバルブの開き具合で変える。"""
    vs = api.devices("valve")
    out = []
    for t in api.devices("thruster"):
        to = t.offset
        best = min(vs, key=lambda v: sum((a - b) ** 2 for a, b in zip(v.offset, to)), default=None)
        out.append((t, best))
    return out


def set_power(t, v, x, keep=KEEP):
    """推進機 t の出力を x（0〜1）にする：バルブ v を開き、火が消えていれば点火し直す（点火は電気を少し使う）。
    keep：飛んでいる間は少しだけ開けておき、火が消えないようにする。"""
    x = min(keep + max(x, 0.0), 1.0)          # 少し足しておく（差をつける分は消さない）
    if v is not None:
        v.open = x
    if x > 0:
        t.ignite()


def stop_all(api):
    """すべてのバルブを閉じる（推進機の火が消える）。"""
    for v in api.devices("valve"):
        v.open = 0.0


def tick(api):
    m = api.memory
    up = api.up
    alt = api.altitude
    keys = api.input
    if "ground" not in m:
        m["ground"] = alt
        m["lift"] = HOVER
    seated = api.seated
    lift = m["lift"]
    if "space" in keys:
        lift = min(lift + 0.15, 12.0)
    if "left shift" in keys:
        lift = max(lift - 0.15, 0.4)
    m["lift"] = lift
    if seated:
        m["flying"] = True
        target = m["ground"] + lift
    elif m.get("flying") and alt > m["ground"] + 0.25:
        target = alt - 1.0                # 降りたら、ゆっくり着地する
        keys = set()
    else:
        m["flying"] = False
        m["lift"] = HOVER
        stop_all(api)
        api.display("ホバーバイク\\n座ると動きます", width=1.0, color=(1.0, 0.85, 0.5))
        return
    X, Y, Z = api.orientation()
    fwd, right = unit(flat(Z, up)), unit(flat(X, up))
    sx = (1 if "d" in keys else 0) - (1 if "a" in keys else 0)
    sz = (1 if "w" in keys else 0) - (1 if "s" in keys else 0)
    want = [(right[i] * sx + fwd[i] * sz) * MAX_SPEED for i in range(3)]
    vh = flat(api.velocity, up)
    lean = [(want[i] - vh[i]) * 0.1 for i in range(3)]
    n = math.sqrt(dot(lean, lean))
    if n > MAX_LEAN:
        lean = [x * MAX_LEAN / n for x in lean]
    d = unit([up[i] + lean[i] for i in range(3)])
    ex, ez = dot(d, X), dot(d, Z)
    wx, wy, wz = api.angular_velocity_local
    tx = ez * 3.0 - wx * 1.5
    tz = -ex * 3.0 - wz * 1.5
    vs = api.vertical_speed
    acc = max(-4.0, min(4.0, (target - alt) * 1.8 - vs * 2.2))
    pl = api.planet()
    r2 = dot([api.position[i] - pl["center"][i] for i in range(3)], [api.position[i] - pl["center"][i] for i in range(3)])
    g = pl["mu"] / max(r2, 1.0)
    thrusters = api.devices("thruster")
    base = api.mass * (g + acc) / (len(thrusters) * THRUST * max(dot(Y, up), 0.5)) if thrusters else 0.0
    for t, v in engines(api):
        ox, oy, oz = t.offset
        set_power(t, v, base + (-oz * tx + ox * tz) * 0.4)
    api.display(f"ホバーバイク\\n速さ {math.sqrt(dot(vh, vh)):.1f} m/s\\n高さ {alt - m['ground']:.1f} m", width=1.0,
                color=(0.6, 1.0, 0.8))
'''

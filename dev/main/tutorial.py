"""チュートリアルワールドの作成（ひとつにまとめた版）。

看板（コンピューターの表示）に沿って進みながら、基本操作 → 建築 → ブロックの形 → 物理 → 機器 →
コンピューター → アクチュエーター → 棒の姿勢制御（ブロックプログラミング） → ロケットの組み立て → 軌道投入
までを学べる。
"""
import json

import numpy as np

from . import paths, log
from .construct import Construct
from .planet import make_planet
from .registry import Registry
from .world import default_bodies
from .content import SCRIPT_README, HOVER_SCRIPT, rot_for

L = log.get("tutorial")

ORBIT_SCRIPT = r'''"""軌道投入スクリプト（チュートリアル用）

コンピューターを E で起動すると、カウントダウンのあと自動で打ち上げ、
重力ターンで機体を傾けて加速し、遠点で円軌道化します。
数値を変えて、いろいろな軌道を試してみましょう。
"""
import math

TARGET_AP = 160.0    # 目標の遠点高度 (m)
TARGET_PE = 110.0    # 円軌道化で目指す近点高度 (m)
TURN_START = 25.0    # この高度から機体を傾け始める (m)
TURN_END = 120.0     # この高度で最大まで傾ける (m)
MAX_TILT = 75.0      # 最大の傾き（垂直からの角度・度）


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def add(a, b, k=1.0):
    return [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]


def unit(a):
    n = math.sqrt(dot(a, a)) or 1.0
    return [a[0] / n, a[1] / n, a[2] / n]


def steer(api, d, base):
    """推力の向きを d（ワールド座標の方向）に合わせるよう、4つの推進機の出力に差をつける。"""
    X, Y, Z = api.orientation()          # 機体のローカル軸（ワールド座標）
    dx, dy, dz = dot(X, d), dot(Y, d), dot(Z, d)
    if dy < 0.2:                          # 大きくずれている時は全力で回す
        n = math.hypot(dx, dz) or 1.0
        dx, dz = dx / n, dz / n
    wx, wy, wz = api.angular_velocity_local
    tx = dz * 2.0 - wx * 1.5
    tz = -dx * 2.0 - wz * 1.5
    for t in api.devices("thruster"):
        ox, oy, oz = t.offset
        t.throttle = min(max(base + (-oz * tx + ox * tz) * 0.5, 0.0), 1.0)


def tick(api):
    m = api.memory
    if api.time - m.get("last_tick", -99.0) > 0.5:   # 起動し直したら最初から
        m.clear()
    m["last_tick"] = api.time
    o = api.orbit()
    up = api.up
    phase = m.get("phase", "countdown")
    if "heading" not in m:                # 打ち上げ方向 = 機体の +Z 方向（水平成分）
        Z = api.orientation()[2]
        h = add(Z, up, -dot(Z, up))
        m["heading"] = unit(h)
        m["t0"] = api.time
    heading = m["heading"]
    vel = api.velocity
    vh = add(vel, up, -dot(vel, up))      # 水平方向の速度
    prograde = unit(vh) if dot(vh, vh) > 1.0 else heading

    if phase == "countdown":
        left = 3 - (api.time - m["t0"])
        steer(api, up, 0.0)
        msg = f"打ち上げまで {max(left, 0):.0f}"
        if left <= 0:
            phase = "ascent"
    elif phase == "ascent":                # まっすぐ上昇
        steer(api, up, 1.0)
        msg = "垂直上昇"
        if o["altitude"] > TURN_START:
            phase = "turn"
    elif phase == "turn":                  # 重力ターン：高度とともに傾ける
        k = min(max((o["altitude"] - TURN_START) / (TURN_END - TURN_START), 0.0), 1.0)
        tilt = math.radians(MAX_TILT * k)
        d = add([x * math.cos(tilt) for x in up], heading, math.sin(tilt))
        steer(api, unit(d), 1.0)
        msg = f"重力ターン 傾き {math.degrees(tilt):.0f}°"
        if o["apoapsis"] >= TARGET_AP:
            phase = "coast"
    elif phase == "coast":                 # 遠点まで慣性飛行（向きだけ合わせておく）
        steer(api, prograde, 0.0)
        msg = f"慣性飛行 遠点まで {o['time_to_apoapsis']:.0f} 秒"
        if o["time_to_apoapsis"] < 2.0 or o["vertical_speed"] < 0:
            phase = "circularize"
    elif phase == "circularize":           # 遠点で水平に加速して円軌道に
        # 落ちてきたら少し上向きに押す
        d = add(prograde, up, max(-o["vertical_speed"] * 0.1, 0.0))
        # 目標に近づいたら出力を絞る
        base = min(1.0, max(0.15, (TARGET_PE - o["periapsis"]) / 150.0))
        steer(api, unit(d), base)
        msg = "円軌道化"
        if o["periapsis"] >= TARGET_PE:
            phase = "orbit"
    else:
        steer(api, prograde, 0.0)
        for t in api.devices("thruster"):
            t.throttle = 0.0
        msg = "軌道に乗りました！"
    m["phase"] = phase

    ap = "∞" if o["apoapsis"] == float("inf") else f"{o['apoapsis']:.0f}"
    text = (f"{msg}\n"
            f"高度 {o['altitude']:.0f} m\n"
            f"水平速度 {o['horizontal_speed']:.1f} / {o['orbital_speed']:.1f} m/s\n"
            f"遠点 {ap} m  近点 {o['periapsis']:.0f} m")
    api.display(text, face="+Y", width=1.0)
    if phase != m.get("last"):
        print(msg)
        m["last"] = phase
'''

CLOCK_SCRIPT = """\"\"\"時計（看板のサンプル）

api.display() でブロックの表面に文字を出せます。
P →「エディタで開く」で、下の文字を書き換えて保存してみましょう（自動で読み込み直します）。
\"\"\"


def tick(api):
    t = int(api.time)
    api.display(f"ワールド時刻\\n{t // 60:02d}:{t % 60:02d}\\n\\nP で開いて\\nこの文字を変えてみよう",
                width=1.6, color=(0.6, 1.0, 0.6))
"""

STATUS_SCRIPT = """\"\"\"機器の見本の状態表示\"\"\"


def tick(api):
    p = api.power
    valves = api.devices("valve")
    tanks = api.devices("tank")
    fuel = sum(t.contents.get("fuel", 0) for t in tanks)
    state = "開" if valves and valves[0].open else "閉"
    api.display(f"発電 {p['production'] / 1000:.1f} kW\\n"
                f"蓄電 {p['stored'] / 1e6:.2f} MJ\\n"
                f"燃料 {fuel:.0f} kg\\n"
                f"バルブ {state}",
                width=1.3, color=(1.0, 0.9, 0.5))
"""

SIGN_SCRIPT = """\"\"\"看板スクリプトの例

文字を表示して停止します（停止しても表示は残るので看板になります）。
width で表示板の幅（m）、face で向き（"+X" "-X" "+Y" "-Y" "+Z" "-Z" "all"）を指定できます。
\"\"\"


def tick(api):
    api.display("ここに文字を書く\\n2行目", width=2.0, color=(1, 1, 1), background=(0.1, 0.2, 0.4))
    api.stop()
"""

SIGNS = [
    (5, """ようこそ Alpedia へ！

このチュートリアルでは看板に沿って奥へ進みながら
移動 → 建築 → ブロックの形 → 物理 → 機器 → コンピューター →
アクチュエーター → 姿勢制御 → ロケット → 軌道投入 を学びます。

■ 移動
WASD：移動　マウス：視点
Space：ジャンプ
F：ジェットパック ON/OFF（Space 上昇 / Ctrl 下降）
F1：操作ヘルプの表示切替　Esc：一時停止"""),
    (13, """■ ブロックを置く・壊す

右クリック：選んだ素材・機器を置く
左クリック：ブロックを壊す／地面を掘る
ホイール・数字キー：画面下のアイテムバーから置くものを選ぶ
I：インベントリ（置きたいブロックをアイテムバーへ移す）
R：機器の向きを変える
N：目の前の空中に新しい構築物を作る

地面に置いた最初のブロックが新しい「構築物」になり、
それにくっつけて置いたブロックは同じ構築物になります。
ためしに何か組み立ててみましょう。"""),
    (21, """■ ブロックの形

ブロックは丸い「球」です。くっついたブロックの間は埋まります。
・2つ並べる → 両端の丸い円柱
・階段状に並べる → なめらかなスロープ
・平らに並べる → 角の丸い板
見た目と当たり判定は同じ形なので、坂を転がったり滑ったりします。
（狙ったときの黄色い枠は四角のままです）

右の見本：1つ／2つ／階段（スロープ）／板
階段の一番上に置いたゴムの玉は、スロープを転がり落ちます。
自分でも坂の上にブロックを置いて転がしてみよう。"""),
    (29, """■ 物理演算

ブロックは8つの頂点でできていて、
隣のブロックとのつながりが一定以上離れるとちぎれます。
ちぎれると間の埋まりも消え、別の構築物に分かれます。
素材ごとに硬さ・ちぎれやすさが違います。
（鋼鉄は丈夫、ガラスは脆い、粘土や水は崩れてまたくっつく）

右の木の塔の根元を壊して倒してみましょう。"""),
    (37, """■ 機器

推進機・タンク・配管・バルブ・発電機・バッテリー・
コンピューター・操縦席などがあります。
E：機器を操作（バルブの開閉、発電機の ON/OFF など）
P：機器の名前を付ける（同じ名前の機器はまとめて動きます）

・電力は配線不要。同じ構築物の中で共有されます
・燃料はつながった配管を通ってタンクから届きます
　（面でつながった連続したブロックが必要）
・クリエイティブでは、タンクを E で燃料満タンにできます

右の見本：タンク→バルブ→配管→発電機→バッテリー
バルブを E で閉じると発電が止まります。"""),
    (45, """■ コンピューターと看板

コンピューターは Python で構築物を制御できます。
P：見ているコンピューターの画面（実行・停止・エディタで開く）
E：コンピューターの起動／停止
スクリプトはメモ帳などのエディタで編集し、保存すると
ゲームが自動で読み込み直します

api.display("文字") でブロックの表面に文字を表示できます。
この看板も右の時計も display() で表示しています。
右の時計を P →「エディタで開く」で書き換えてみましょう。
（画面の「ブロックで編集」では、スクラッチのように
　ブロックを並べてプログラムを作れます）"""),
]


ROCKET_SIGNS = [
    """■ ロケットを組み立てよう（右の空き地で）

1. 地面に「推進機」を置く（向きは +Y＝上）
2. それにつなげて 3×3 の1段目を作る
　　四隅＝推進機、残り＝配管
3. 2段目：3×3 すべて「タンク」
　　→ 各タンクを E で燃料満タンに
4. 3段目：中央に「操縦席」、角に「バッテリー」と
　　「コンピューター」、残りはアルミなど
5. 四隅の推進機を見て P で名前を付ける（機体の前＝最初の向き）
　　推進機前左・推進機前右・推進機後左・推進機後右
6. コンピューターを P →「ブロックで編集」→「別のファイルを開く」で
　　orbit を表示して保存（このコンピューターに設定）

左の「見本ロケット」が完成形です（V で三人称にすると見やすい）
ブロックが丸いので、1段目の下は少し丸くなります。平らな所に置こう""",
    """■ 軌道に乗ろう

軌道に乗る＝横向きに十分速く飛び、落ち続けながらも
地面に届かずに惑星を回り続けること。
この惑星（半径 600 m）では高度 150 m で約 70 m/s 必要です。

1. ロケットの操縦席に E で座る
2. コンピューターを P で開いて Ctrl+R（保存して実行）
3. 自動で 上昇 → 重力ターン → 遠点で円軌道化
　（機体の前の方向へ飛びます）
誘導はブロックプログラム（orbit）です。「前への傾き」が
「目標」に近づくように、前後・左右の推進機の出力に差をつけます
（棒の姿勢制御と同じ「ずれ × 係数」の考え方）。
「段階」の変数で カウントダウン → 上昇 → … と進みます

画面左上の「近点」が 0 m より高くなれば軌道に乗っています！
E で降りる / Space で全開噴射
遠点 160・近点 110 の数値を変えて、もっと高い軌道にも挑戦しよう。
（Python 版の orbit_python.py もあります）

おつかれさまでした！ 新規作成で「地球型の惑星」を選ぶと
山・洞窟・海・池のある世界で遊べます。""",
]


def shape_samples():
    """ブロックの形の見本：1つ・2つ・階段（スロープ）・板。"""
    return [
        ("形の見本 1つ（球）", [((0, 0, 0), "steel", None)], 3.0, 18.5),
        ("形の見本 2つ（円柱）", [((0, 0, 0), "steel", None), ((1, 0, 0), "steel", None)], 3.0, 20.5),
        ("形の見本 板", [((x, 0, z), "aluminum", None) for x in range(4) for z in range(3)], 3.0, 23.0),
        ("形の見本 階段（スロープ）",
         [((x, y, 0), "rock", None) for x in range(6) for y in range(6 - x)]
         + [((x, y, 1), "rock", None) for x in range(6) for y in range(6 - x)], 7.0, 19.5),
        ("ゴムの玉", [((0, 0, 0), "rubber", None)], None, None),
    ]


def _surface_point(planet, x, z, lift=0.05):
    p = np.array([x, planet.radius, z], dtype=np.float64)
    up = p / np.linalg.norm(p)
    r = float(planet.surface_radius(up[None])[0])
    return up * (r + lift), up


def sign_voxels(text, width=3.4, height=4):
    """看板：倒れないように、5×3 の岩の台座＋十字の支えの上に木の柱を立てる。"""
    v = [((x, 0, z), "rock", None) for x in range(-2, 3) for z in range(-1, 2)]
    v += [((x, 1, z), "rock", None) for x, z in ((-1, 0), (1, 0), (0, -1), (0, 1))]
    v += [((0, y, 0), "wood", None) for y in range(1, height + 1)]
    v.append(((0, height + 1, 0), "steel", {
        "def": "computer", "dir": 5, "name": "看板",
        "state": {"display": {"text": text, "face": 5, "width": width, "color": [1, 1, 1],
                              "background": [0.05, 0.07, 0.12, 0.93]}}}))
    return v


# 推進機の名前（機体の前 = +Z、右 = +X）。名前を分けると、ブロックで1つずつ出力を決められる
THRUSTER_NAMES = {(0, 0): "推進機後左", (2, 0): "推進機後右", (0, 2): "推進機前左", (2, 2): "推進機前右"}


def orbit_program():
    """軌道投入のブロックプログラム。
    姿勢制御：前後・左右の傾きのずれに比例して、4つの推進機の出力に差をつける（棒の姿勢制御と同じ考え方）。
    誘導：段階 0 カウントダウン → 1 垂直上昇 → 2 重力ターン → 3 慣性飛行 → 4 円軌道化 → 5 軌道。"""
    from . import blocks as B

    def N(v):
        return {"type": "num", "v": v}

    def T(v):
        return {"type": "text", "v": v}

    def S(name):
        return {"type": "sensor", "name": name}

    def V(name):
        return {"type": "var", "name": name}

    def op(o, a, b):
        return {"type": "op", "op": o, "a": a, "b": b}

    def cmp(o, a, b):
        return {"type": "cmp", "op": o, "a": a, "b": b}

    def rnd(a):
        return {"type": "func", "fn": "四捨五入", "a": a}

    def join(*parts):
        node = parts[-1]
        for p in reversed(parts[:-1]):
            node = {"type": "join", "a": p, "b": node}
        return node

    def setv(name, value):
        st = B.new_stmt("set_var")
        st["var"], st["value"] = name, value
        return st

    def iff(cond, body):
        st = B.new_stmt("if")
        st["cond"], st["body"] = cond, body
        return st

    def show(msg):
        st = B.new_stmt("display")
        st["text"] = join(T(msg + "\n高度 "), rnd(S("altitude")), T(" m\n水平速度 "), rnd(S("hspeed")),
                          T(" / "), rnd(S("orbital_speed")), T(" m/s\n遠点 "), rnd(S("apoapsis")),
                          T(" m  近点 "), rnd(S("periapsis")), T(" m"))
        st["width"] = N(1.0)
        return st

    def say(msg):
        st = B.new_stmt("print")
        st["text"] = T(msg)
        return st

    def stage(k):
        return cmp("=", V("段階"), N(k))

    def thr(name, sign_pitch, sign_roll):
        st = B.new_stmt("throttle")
        st["dev"] = name
        a = op("+" if sign_pitch > 0 else "-", V("出力"), V("前後"))
        st["value"] = op("+" if sign_roll > 0 else "-", a, V("左右"))
        return st

    p = B.new_program()
    start = B.new_stmt("on_start")
    start["body"] = [setv("段階", N(0)), setv("目標", N(0)), B.new_stmt("reset_timer")]
    tick = B.new_stmt("on_tick")
    tick["body"] = [
        # 姿勢制御：ずれ × 0.02 − 回る速さ × 0.012（速さの項でゆれを抑える）
        setv("前後", op("-", op("×", op("-", V("目標"), S("pitch")), N(0.02)), op("×", S("pitch_rate"), N(0.012)))),
        setv("左右", op("-", op("×", S("roll"), N(-0.02)), op("×", S("roll_rate"), N(0.012)))),
        iff(stage(0), [
            setv("出力", N(0)), setv("目標", N(0)),
            show("打ち上げまで"),
            iff(cmp(">", S("timer"), N(3)), [setv("段階", N(1)), say("打ち上げ！")]),
        ]),
        iff(stage(1), [
            setv("出力", N(0.6)), setv("目標", N(0)), show("垂直上昇"),
            iff(cmp(">", S("altitude"), N(15)), [setv("段階", N(2)), say("重力ターン開始")]),
        ]),
        iff(stage(2), [
            setv("出力", N(0.6)),
            setv("目標", op("×", op("-", S("altitude"), N(15)), N(1.0))),
            iff(cmp(">", V("目標"), N(75)), [setv("目標", N(75))]),
            show("重力ターン"),
            iff(cmp(">", S("apoapsis"), N(160)), [setv("段階", N(3)), say("慣性飛行")]),
        ]),
        iff(stage(3), [
            setv("出力", N(0)), setv("目標", N(85)), show("慣性飛行"),
            iff({"type": "logic", "op": "or", "a": cmp("<", S("time_to_ap"), N(2)), "b": cmp("<", S("vspeed"), N(0))},
                [setv("段階", N(4)), say("円軌道化")]),
        ]),
        iff(stage(4), [
            setv("出力", op("×", op("-", N(110), S("periapsis")), N(0.007))),
            iff(cmp("<", V("出力"), N(0.15)), [setv("出力", N(0.15))]),
            iff(cmp(">", V("出力"), N(0.9)), [setv("出力", N(0.9))]),
            setv("目標", op("+", N(88), op("×", S("vspeed"), N(3)))),
            iff(cmp(">", V("目標"), N(90)), [setv("目標", N(90))]),
            show("円軌道化"),
            iff(cmp(">", S("periapsis"), N(110)), [setv("段階", N(5)), say("軌道に乗りました！")]),
        ]),
        iff(stage(5), [
            setv("出力", N(0)), setv("前後", N(0)), setv("左右", N(0)), show("軌道に乗りました！"),
        ]),
        # 後ろの推進機を強くすると前へ傾く、左を強くすると右へ傾く
        thr("推進機後左", +1, +1), thr("推進機後右", +1, -1),
        thr("推進機前左", -1, +1), thr("推進機前右", -1, -1),
    ]
    p["scripts"] += [start, tick]
    return p


def rocket_voxels(script="orbit.py", blocks="orbit.blocks.json"):
    V = []
    for x in range(3):
        for z in range(3):
            if (x, z) in THRUSTER_NAMES:
                V.append(((x, 0, z), "steel", {"def": "thruster", "dir": 2, "name": THRUSTER_NAMES[(x, z)]}))
            else:
                V.append(((x, 0, z), "steel", {"def": "pipe", "dir": 2}))
    for x in range(3):
        for z in range(3):
            V.append(((x, 1, z), "steel", {"def": "tank", "dir": 2, "state": {"contents": {"fuel": 800.0}}}))
    top = {(1, 1): {"def": "seat", "dir": 2, "name": "操縦席"},
           (2, 0): {"def": "computer", "dir": 2, "name": "誘導コンピューター",
                    "state": {"script": script, **({"blocks": blocks} if blocks else {})}},
           (0, 0): {"def": "battery", "dir": 2}}
    for x in range(3):
        for z in range(3):
            d = top.get((x, z))
            V.append(((x, 2, z), "steel" if d else "aluminum", d))
    return V


def device_sample():
    return [
        ((0, 0, 0), "steel", {"def": "tank", "dir": 2, "name": "燃料タンク", "state": {"contents": {"fuel": 800.0}}}),
        ((1, 0, 0), "steel", {"def": "valve", "dir": 2, "name": "バルブ"}),
        ((2, 0, 0), "steel", {"def": "pipe", "dir": 2}),
        ((3, 0, 0), "steel", {"def": "generator", "dir": 2, "name": "発電機"}),
        ((4, 0, 0), "steel", {"def": "battery", "dir": 2, "state": {"charge": 2.0e6}}),
        ((5, 0, 0), "steel", {"def": "computer", "dir": 5, "name": "状態表示",
                              "state": {"script": "status.py", "running": True}}),
    ]


def clock_sample():
    v = [((0, y, 0), "wood", None) for y in range(3)]
    v.append(((0, 3, 0), "steel", {"def": "battery", "dir": 2}))
    v.append(((0, 4, 0), "steel", {"def": "computer", "dir": 5, "name": "時計",
                                   "state": {"script": "clock.py", "running": True}}))
    return v


ACTUATOR_SIGN = """■ アクチュエーター

回転・押し出し・引き込み（ブロック1つ分）・
分離・結合ができる機器です。
向き（置く時に R キー）の面が「出力面」になります。

P：動作の種類・量・速度を設定　E：実行
（動かすには電力が必要。後ろのアームで試そう）

コンピューターからも動かせます：
a = api.device("アーム")
a.rotate(90)　a.extend(1.0)　a.retract(1.0)
a.detach()　a.attach()"""


def actuator_sample():
    v = [((x, 0, z), "steel", None) for x in range(3) for z in range(3)]
    v.append(((1, 1, 1), "steel", {"def": "battery", "dir": 2}))
    v.append(((1, 2, 1), "steel", {"def": "actuator", "dir": 2, "name": "アーム",
                                   "state": {"preset": {"action": "rotate", "amount": 90.0, "speed": 45.0}}}))
    v += [((x, 3, 1), "aluminum", None) for x in range(1, 4)]
    return v


def tower():
    return [((x, y, z), "wood", None) for y in range(8) for x in range(2) for z in range(2)]


def marker(text):
    return sign_voxels(text, width=1.6, height=2)


def create_tutorial(name="tutorial"):
    from . import blocks as B
    from . import tutorial_rod as rod
    from . import tutorial_drone as drone
    wdir = paths.world_dir(name)
    if wdir.exists():
        raise FileExistsError(f"ワールド「{name}」は既にあります")
    L.info("チュートリアルワールド '%s' を作成します", name)
    for sub in ("scripts", "exports", "imports"):
        (wdir / sub).mkdir(parents=True, exist_ok=True)
    reg = Registry().load()
    override = {"noise_amp": 1.5}
    terra = make_planet(reg.get("planets", "terra"), [0, 0, 0], override)
    size = 0.5
    settings = {
        "name": name, "format": 2, "voxel_size": size, "gravity_scale": 1.0, "time": 0.0,
        "tutorial": True,
        "cleanup": {"enabled": False, "distance": 5000.0, "seconds": 120.0, "max_vertices": 400, "check_interval": 5.0},
        "player": "default",
        "planets": [{"def": "terra", "position": [0, 0, 0], "craters": [], "override": override},
                    {"def": "luna", "position": [0, 0, 4000], "craters": []}] + default_bodies("terra", False)[1:],
    }
    items = []
    for z, text in SIGNS:
        items.append((f"看板{len(items) + 1}", sign_voxels(text), -2.5, z))
    # ブロックの形
    shapes = shape_samples()
    items += [(t, v, x, z) for t, v, x, z in shapes if x is not None]
    items += [
        ("木の塔", tower(), 3.0, 30.0),
        ("粘土の塊", [((x, y, zz), "clay", None) for x in range(2) for y in range(2) for zz in range(2)], 6.0, 28.0),
        ("機器の見本", device_sample(), 2.0, 39.0),
        ("時計", clock_sample(), 3.0, 48.0),
        ("看板 アクチュエーター", sign_voxels(ACTUATOR_SIGN, width=3.0), -2.5, 53.0),
        ("アクチュエーターの見本", actuator_sample(), 3.0, 53.0),
    ]
    # 棒の姿勢制御
    for k, (z, text) in enumerate(zip((61, 69, 77, 85), rod.SIGNS)):
        items.append((f"看板 姿勢制御{k + 1}", sign_voxels(text), -2.5, z))
    items += [
        ("お手本の装置", rod.apparatus("お手本", "otehon.blocks.json", "otehon.py"), 2.5, 61.0),
        ("課題の装置", rod.apparatus("課題", "kadai.blocks.json", "kadai.py"), 2.5, 70.0),
    ]
    # ロケット
    zr = 95.0
    items += [
        ("看板 ロケット", sign_voxels(ROCKET_SIGNS[0]), -2.5, zr),
        ("看板 軌道", sign_voxels(ROCKET_SIGNS[1]), -2.5, zr + 9),
        ("見本ロケット", rocket_voxels(), -10.0, zr - 2),
        ("目印 見本ロケット", marker("見本ロケット\n（完成形）"), -6.0, zr - 3),
        ("目印 組み立て場所", marker("ロケット\n組み立て場所"), 6.0, zr - 3),
    ]
    for x, z in ((7.0, zr - 1), (12.0, zr - 1), (7.0, zr + 5), (12.0, zr + 5)):
        items.append(("目印", [((0, 0, 0), "rock", None), ((0, 1, 0), "rock", None)], x, z))
    # 無線とドローン
    zd = zr + 22
    items += [
        ("看板 無線", sign_voxels(drone.SIGNS[0]), -2.5, zd),
        ("看板 ドローン", sign_voxels(drone.SIGNS[1]), -2.5, zd + 8),
        ("ドローンの操縦台", drone.remote_voxels(), 1.5, zd + 6),
        ("ドローン", drone.drone_voxels(), 2.0, zd + 13),
    ]
    constructs, spawns = [], {}
    stair = None
    for title, vox, x, z in items:
        base, up = _surface_point(terra, x, z)
        R = rot_for(up)
        c = Construct.from_voxels(reg, vox, base, R, size=size, name=title)
        if title.startswith("形の見本 階段"):
            stair = (base, R)
        constructs.append(c.to_dict())
        spawns[c.id] = c.spawn_dict()
    # ゴムの玉は階段の一番上に乗せる
    if stair is not None:
        base, R = stair
        top = base + R @ np.array([0.1, 6 * size + 0.05, 0.25])
        c = Construct.from_voxels(reg, shapes[-1][1], top, R, size=size, name="ゴムの玉")
        constructs.append(c.to_dict())
        spawns[c.id] = c.spawn_dict()
    sp, up = _surface_point(terra, 0.0, -2.0, 1.2)
    (wdir / "world.json").write_text(json.dumps(settings, ensure_ascii=False, indent=1), encoding="utf-8")
    (wdir / "constructs.json").write_text(json.dumps(constructs, ensure_ascii=False, indent=1), encoding="utf-8")
    (wdir / "spawn.json").write_text(json.dumps(spawns, ensure_ascii=False, indent=1), encoding="utf-8")
    (wdir / "mods.json").write_text(json.dumps({"enabled": []}, ensure_ascii=False, indent=1), encoding="utf-8")
    (wdir / "player.json").write_text(json.dumps({"position": sp.tolist(), "velocity": [0, 0, 0],
                                                  "inventory": {}, "seated": None, "jetpack": False}),
                                      encoding="utf-8")
    sd = wdir / "scripts"
    for fname, text in (("orbit_python.py", ORBIT_SCRIPT), ("clock.py", CLOCK_SCRIPT), ("status.py", STATUS_SCRIPT),
                        ("sign.py", SIGN_SCRIPT), ("hover.py", HOVER_SCRIPT), ("README.txt", SCRIPT_README),
                        ("sway.py", rod.SWAY_SCRIPT), ("drone.py", drone.DRONE_SCRIPT),
                        ("drone_remote.py", drone.REMOTE_SCRIPT)):
        (sd / fname).write_text(text, encoding="utf-8")
    for base, prog in (("kadai", rod.starter_program()), ("otehon", rod.solution_program()),
                       ("orbit", orbit_program())):
        B.save(sd / f"{base}.blocks.json", prog)
        (sd / f"{base}.py").write_text(B.to_python(prog, f"{base}.blocks.json"), encoding="utf-8")
    return wdir

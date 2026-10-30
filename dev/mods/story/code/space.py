"""宇宙の舞台（第11章〜終章）：軌道都市アマツ（場所の MOD「amatsu」）のまわりの艦隊と、乗り物・作業台。

場所は amatsu の甲板の座標（deck.point(x, z, y)：甲板の真ん中が原点、+Z が円筒の軸、+X が右、y は甲板の上の高さ）
と、そこからの向き（deck.R の列が 右・上・前）で決める。
艦隊の位置：迎撃艦は甲板の後ろの上空、帝国艦隊はその先の遠く。
"""
import numpy as np

from main.scenery import CYAN, OFF, REDL, WHITE, Scenery

NAVY = (0.14, 0.2, 0.42)
BLACK = (0.12, 0.12, 0.14)
RED = (0.78, 0.12, 0.1)

FLEET_BACK = -330.0          # 迎撃艦：甲板の真ん中から軸の後ろへ
FLEET_UP = 70.0              # 　　　　甲板から上へ
EMPIRE_BACK = -1100.0        # 帝国艦隊：さらに後ろ
EMPIRE_SHIPS = [(-260.0, 60.0), (-120.0, 110.0), (0.0, 40.0), (140.0, 130.0), (280.0, 70.0)]   # (右, 上)
UNION_SHIPS = [(-90.0, -20.0, 40.0), (100.0, 10.0, -30.0), (-160.0, 30.0, -120.0), (180.0, -10.0, -110.0)]

ROBOT_START = (0.0, -55.0)   # トワのロボット（甲板の x, z）
KROBOT_START = (0.0, 45.0)   # キンギのロボット
UNIT_AT = (18.0, -10.0)      # 再加速の作業台


def fleet_origin(deck):
    return deck.point(0.0, FLEET_BACK, FLEET_UP)


def empire_points(deck):
    """帝国艦隊の艦の位置（ミサイルが出てくる所）。"""
    o = fleet_origin(deck)
    R = deck.R
    return [o + R[:, 2] * (EMPIRE_BACK + 40 * k) + R[:, 0] * x + R[:, 1] * y for k, (x, y) in enumerate(EMPIRE_SHIPS)]


def _ship(s, c, R, length, width, colors, glow):
    """箱の船（張りぼて）：胴・艦橋・砲塔・推進の光。"""
    body, band = colors
    rx, up, fw = R[:, 0], R[:, 1], R[:, 2]
    s.raw(c, R, (width / 2, width * 0.18, length / 2), color=body, collide=False)
    s.raw(c + up * width * 0.22, R, (width * 0.3, width * 0.1, length * 0.42), color=band, collide=False)
    s.raw(c + up * width * 0.4 - fw * length * 0.15, R, (width * 0.12, width * 0.16, length * 0.08), color=body,
          collide=False)
    s.raw(c + up * width * 0.45 - fw * length * 0.15, R, (width * 0.13, width * 0.03, length * 0.02), color=OFF,
          glow=glow, collide=False)
    for k in (-1, 1):
        s.raw(c - fw * (length / 2 + 0.6) + rx * k * width * 0.25, R, (width * 0.1, width * 0.1, 0.4), color=OFF,
              glow=glow, collide=False)


def build_fleet(deck):
    """艦隊の張りぼて（当たり判定なし）。"""
    s = Scenery("fleet", None)
    R = deck.R
    o = fleet_origin(deck)
    rx, up, fw = R[:, 0], R[:, 1], R[:, 2]
    for k, p in enumerate(empire_points(deck)):
        L_ = 110.0 if k == 2 else 70.0
        Rk = np.stack([-rx, up, -fw], axis=1)                 # 帝国艦はこちらを向く
        _ship(s, p, Rk, L_, L_ * 0.22, (BLACK, RED), REDL)
    for x, y, z in UNION_SHIPS:
        _ship(s, o + rx * x + up * y + fw * z, R, 60.0, 13.0, (WHITE, NAVY), CYAN)
    s.raw(o + rx * 0.0 + up * 25.0 + fw * 160.0, R, (14.0, 4.0, 60.0), color=WHITE, collide=False)     # 旗艦アルテミス
    s.raw(o + up * 31.0 + fw * 150.0, R, (6.0, 4.0, 10.0), color=NAVY, collide=False)
    s.raw(o + up * 25.0 + fw * 99.0, R, (8.0, 2.0, 0.6), color=OFF, glow=CYAN, collide=False)
    s.point_raw("fleet", o)
    return s.finish()


# ---------------------------------------------------------------------------
# 迎撃艦（第11章）：甲板・艦橋・迎撃砲塔4つ・レーダー・迎撃の制御（ブロックプログラム）
# ---------------------------------------------------------------------------
INTERCEPTOR_PC = (5, 2, 17)


def interceptor_voxels():
    """前 = +Z（帝国艦隊の方）。甲板は y=0 の上。"""
    V = []
    for x in range(12):
        for z in range(24):
            edge = x in (0, 11) or z in (0, 23)
            V.append(((x, 0, z), "hull_blue" if edge else "hull_white", None))
    for x in range(3, 9):                                   # 艦橋
        for z in range(18, 23):
            for y in range(1, 5):
                shell = x in (3, 8) or z in (18, 22) or y == 4
                if not shell:
                    continue
                if (x, y, z) == INTERCEPTOR_PC:
                    continue
                mat = "glass" if (y == 3 and z == 18 and x in (4, 5, 6, 7)) else ("hull_blue" if y == 1 else "hull_white")
                V.append(((x, y, z), mat, None))
    V.append((INTERCEPTOR_PC, "steel", {"def": "computer", "dir": 5, "name": "迎撃の制御",
                                       "state": {"script": "intercept.py", "blocks": "intercept.blocks.json"}}))
    for k, x in enumerate((4, 5, 6, 7)):
        V.append(((x, 1, 21), "steel", {"def": "battery", "dir": 2, "name": f"バッテリー{k + 1}"}))
    V += [((x, 5, z), "glass", {"def": "solar_panel", "dir": 2, "name": "太陽光パネル"}) for x in (4, 7) for z in (19, 21)]
    V.append(((5, 5, 20), "aluminum", {"def": "radar", "dir": 2, "name": "レーダー"}))
    V.append(((6, 5, 20), "aluminum", {"def": "antenna", "dir": 2, "name": "アンテナ"}))
    for k, (x, z) in enumerate(((1, 4), (10, 4), (1, 13), (10, 13))):
        V.append(((x, 1, z), "steel", {"def": "turret", "dir": 4, "name": f"砲塔{k + 1}"}))
    for x in (2, 9):                                        # 後ろの推進機（飾り）
        V.append(((x, 1, 0), "steel", {"def": "thruster", "dir": 5, "name": "主推進機"}))
    return V


# ---------------------------------------------------------------------------
# ロボット（第12章）：ホバーバイクと同じ浮き方の、ブロックの人型。腕はアクチュエーターで振る
# ---------------------------------------------------------------------------
ROBOT_VALVES = ("バルブ 後左", "バルブ 後右", "バルブ 前左", "バルブ 前右")


def robot_voxels(body="hull_white", trim="hull_blue", pilot=True):
    """前 = +Z、右 = +X。下の段の四隅が姿勢用推進機（上向き）、その上にバルブ、まん中にタンク。"""
    V = []
    corners = {(0, 0): "後左", (2, 0): "後右", (0, 4): "前左", (2, 4): "前右"}
    for x in range(3):
        for z in range(5):
            if (x, z) in corners:
                V.append(((x, 0, z), "steel", {"def": "maneuver_thruster", "dir": 2, "name": "推進機"}))
            else:
                V.append(((x, 0, z), trim, None))
    for (x, z), nm in corners.items():
        V.append(((x, 1, z), "steel", {"def": "valve", "dir": 2, "name": "バルブ " + nm, "state": {"open": 0.0}}))
    V += [((1, 1, 0), "steel", {"def": "pipe", "dir": 2}), ((1, 1, 4), "steel", {"def": "pipe", "dir": 2}),
          ((1, 1, 1), "steel", {"def": "tank", "dir": 2, "name": "燃料タンク", "state": {"contents": {"fuel": 800.0}}}),
          ((1, 1, 3), "steel", {"def": "tank", "dir": 2, "name": "燃料タンク", "state": {"contents": {"fuel": 800.0}}}),
          ((1, 1, 2), body, None)]
    for x in (0, 2):                                        # 脚
        V += [((x, 1, z), body, None) for z in (1, 2, 3)]
        V += [((x, y, 2), trim, None) for y in (2, 3)]
    V.append(((1, 3, 2), body, None))
    for x in range(3):                                      # 胴（中に操縦席）
        for y in range(4, 7):
            for z in range(1, 4):
                if (x, y, z) == (1, 4, 2):
                    continue
                mat = body
                if (x, y, z) == (1, 5, 3):
                    mat = "glass"
                elif (x, y, z) == (1, 5, 1):
                    V.append(((x, y, z), "steel", {"def": "battery", "dir": 2, "name": "バッテリー"}))
                    continue
                elif (x, y, z) == (1, 6, 1):
                    V.append(((x, y, z), "steel", {"def": "computer", "dir": 5, "name": "ロボットの制御",
                                                   "state": {"script": "robot.py", "blocks": "robot.blocks.json",
                                                             "running": True}}))
                    continue
                elif y == 4:
                    mat = trim
                V.append(((x, y, z), mat, None))
    if pilot:
        V.append(((1, 4, 2), "steel", {"def": "seat", "dir": 4, "name": "ロボットの操縦席"}))
    else:
        V.append(((1, 4, 2), body, None))
    V += [((1, 7, 2), "glass", None), ((1, 7, 3), "hull_red" if pilot else "hull_orange", None)]     # 頭と目
    # 腕：肩のアクチュエーター（出力は外向き）→ 3 ブロック下へ
    for side, x0, d in (("左", -1, 1), ("右", 3, 0)):
        V.append(((x0, 5, 2), "steel", {"def": "actuator", "dir": d, "name": f"{side}腕", "state": {"rot_speed": 90.0}}))
        xo = x0 - 1 if side == "左" else x0 + 1
        V += [((xo, y, 2), body if y != 3 else trim, None) for y in (5, 4, 3)]
    return V


def krobot_voxels():
    """キンギのロボット（黒と赤。肩にミサイル発射機）。シナリオが動かす。"""
    V = robot_voxels(body="steel", trim="hull_red", pilot=False)
    V = [v for v in V if not (v[2] and v[2]["def"] in ("computer",))]
    V += [((x, 7, z), "steel", {"def": "missile_launcher", "dir": 2, "name": "ミサイル発射機"})
          for x in (-1, 3) for z in (1, 2)]
    return V


# ---------------------------------------------------------------------------
# 再加速の作業台（終章）：四隅の推進モジュールの枠・バルブ・タンク・航法のコンピューター・看板
# ---------------------------------------------------------------------------
UNIT_SLOTS = [(1, 2, 1), (6, 2, 1), (1, 2, 6), (6, 2, 6)]
UNIT_VALVE_OPEN = [0.9, 0.35, 0.7, 0.5]          # 最初はばらばら（そろえるのが課題）
UNIT_PC = (3, 1, 0)
UNIT_SIGN = (5, 1, 0)


def unit_voxels():
    V = [((x, 0, z), "steel" if (x + z) % 2 else "hull_white", None) for x in range(8) for z in range(8)]
    names = ["左後", "右後", "左前", "右前"]
    for k, (x, _, z) in enumerate(UNIT_SLOTS):
        V.append(((x, 1, z), "steel", {"def": "valve", "dir": 2, "name": f"バルブ {names[k]}",
                                       "state": {"open": UNIT_VALVE_OPEN[k]}}))
        tx = x + 1 if x < 4 else x - 1
        V.append(((tx, 1, z), "steel", {"def": "tank", "dir": 2, "name": f"燃料タンク {names[k]}",
                                        "state": {"contents": {"fuel": 800.0}}}))
    V.append((UNIT_PC, "steel", {"def": "computer", "dir": 5, "name": "再加速の航法",
                                "state": {"script": "reaccel.py", "blocks": "reaccel.blocks.json"}}))
    V.append(((4, 1, 0), "steel", {"def": "battery", "dir": 2, "name": "バッテリー"}))
    V.append(((4, 1, 1), "steel", {"def": "battery", "dir": 2, "name": "バッテリー"}))
    V.append((UNIT_SIGN, "steel", {"def": "computer", "dir": 5, "name": "看板"}))
    V.append(((3, 1, 3), "aluminum", {"def": "antenna", "dir": 2, "name": "アンテナ"}))
    return V

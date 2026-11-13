"""アクチュエーター：回転・押し出し・引き込み・分離・結合（計算は Rust：alpedia_core の game/actuator.rs）。

アクチュエーターは向き（出力面）を持つブロック。
- 回転   : 出力面の先につながったブロック（とその先の部分）を、向きを軸に回す
           ヒンジ（別の機器）は、ヒンジのブロックの中心を軸に -90〜+90° 振る（扉・腕の関節など。曲がる側は置き方で決まる）
- 押し出し: 自分が伸びて、出力面の先を押し出す（最大でブロック1つ分）
- 引き込み: 伸びた分を引き戻す
- 分離   : 出力面の結合を切り離す（先の部分は別の構築物になる）
- 結合   : 出力面の先にあるブロック（別の構築物でも可）とつなぐ

操作方法は2通り:
- コンピューターから: スクリプトで actuator.rotate(90) / .extend(1.0) / .detach() など
- 手動: P で動作設定メニューを開き、動作の種類・量・速度を決めて閉じ、E で実行
"""
from . import native

ACTIONS = ["rotate", "extend", "retract", "detach", "attach", "none"]
ACTION_NAMES = {"rotate": "回転", "extend": "押し出し", "retract": "引き込み", "detach": "分離", "attach": "結合",
                "none": "なし"}
# "none"（なし）：E・操縦席のキー（X）では動かさない。コンピューターのプログラムだけで動かすアクチュエーターに
HINGE_ACTIONS = ["rotate", "detach", "attach", "none"]       # ヒンジは押し出さない（rotate は「曲げる」）
# アクチュエーターの種類（定義の params.motion）と、できる動作。分離・結合はどれでもできる
#   rotor（ローター。機器 "actuator"）：回す / piston（ピストン）：押し出す・引き込む / hinge（ヒンジ）：曲げる
#   motion の無い MOD の機器は、昔のアクチュエーターと同じく全部できる（"both"）
MOTION_ACTIONS = {"rotor": ["rotate", "detach", "attach", "none"], "piston": ["extend", "retract", "detach", "attach", "none"],
                  "hinge": HINGE_ACTIONS, "both": ACTIONS}
MOTION_NAMES = {"rotor": "ローター", "piston": "ピストン", "hinge": "ヒンジ", "both": "アクチュエーター"}
DEFAULT_PRESET = {"action": "rotate", "amount": 90.0, "speed": 90.0}
PISTON_PRESET = {"action": "extend", "amount": 1.0, "speed": 0.5}


_C = native.require()
MotionError = _C.MotionError

HINGE_MAX = 90.0            # ヒンジで曲げられる角度（-90〜+90°。0 がまっすぐ）
HINGE_AMOUNTS = [-90.0, -60.0, -45.0, -30.0, -15.0, 0.0, 15.0, 30.0, 45.0, 60.0, 90.0]   # P メニューで選べる角度

motion = _C.act_motion                      # アクチュエーターの種類："rotor" / "piston" / "hinge" / "both"
actions_for = _C.act_actions_for
default_preset = _C.act_default_preset
migrate_device = _C.act_migrate_device      # 古いセーブの「アクチュエーター」で押し出していた物はピストンに
check_action = _C.act_check_action          # その動作ができなければ MotionError
hinge_sides = _C.act_hinge_sides
default_hinge_side = _C.act_default_hinge_side
hinge_pair = _C.act_hinge_pair
is_hinge_dev = _C.act_is_hinge_dev
hinge_side = _C.act_hinge_side
hinge_of = _C.act_hinge_of                  # ヒンジなら軸になる辺（出力面の4隅のうち2つ）。回転なら None
is_actuator = _C.act_is_actuator
stroke = _C.act_stroke
output_cell = _C.act_output_cell
is_attached = _C.act_is_attached
busy = _C.act_busy
cmd_detach = _C.act_cmd_detach              # 出力面の結合を外す（外れたら True）
face_info = _C.act_face_info                # 出力面の中心・法線・4頂点の位置
measure_twist = _C.act_measure_twist
sync_angle = _C.act_sync_angle
output_angle = _C.act_output_angle          # 出力側ブロックの向き（度）
execute_preset = _C.act_execute_preset      # P メニューで設定した動作を実行（E キー・操縦席のキー）
preset_text = _C.act_preset_text
step = _C.act_step                          # 毎ティック

side_toward = _C.act_side_toward_any         # 置くとき：出力の軸に垂直な4つの向きのうち、見ている向きにいちばん近いもの
mode_name = _C.act_mode_name
init_state = _C.act_init_state              # 足りない項目を既定値で埋める（形は devstate の表）
cmd_rotate = _C.act_cmd_rotate
cmd_extend = _C.act_cmd_extend
cmd_retract = _C.act_cmd_retract
cmd_attach = _C.act_cmd_attach              # 出力面の先にあるブロックと結合する。メッセージを返す
_merge_map = _C.act_merge_map

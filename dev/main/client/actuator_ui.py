"""アクチュエーターの動作設定（P キー）で選べる値。画面は main/client_rs/screens_dev.py の ActuatorPanel。"""
from .. import native

ROT_AMOUNTS = [-360.0, -180.0, -90.0, -45.0, -30.0, -15.0, -5.0, 5.0, 15.0, 30.0, 45.0, 90.0, 180.0, 360.0]
ROT_SPEEDS = [5.0, 15.0, 30.0, 45.0, 90.0, 180.0, 360.0]
EXT_AMOUNTS = [0.1, 0.25, 0.5, 0.75, 1.0]
EXT_SPEEDS = [0.05, 0.1, 0.25, 0.5, 1.0, 2.0]


_nearest = native.require().aui_nearest    # 値の一覧から v にいちばん近い物（v が数でなければ default）

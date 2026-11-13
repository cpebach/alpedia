"""手の操作（狙っている物の説明・置く場所の枠・三人称のカメラ・近くの触れる物）。処理は Rust（alpedia_core.play_*）。"""
from .. import native

_c = native.require()
QUIET_DEVICES = {"pipe", "battery", "anchor"}         # 近くの物の一覧に出さない（E で触るほどの物ではない）

aim = _c.play_aim                     # 狙っている物の枠と説明 {"highlight", "ghost", "text"}
item_text = _c.play_item_text         # アイテムバーの選んでいる枠の説明
camera_clip = _c.play_camera_clip     # 三人称のカメラが壁の向こうに入らないように手前で止める
seat_follow = _c.play_seat_follow     # 遠隔操作の席なら (重心, 距離)
nearby_targets = _c.play_nearby_targets   # 近くの触れるもの（使う順に4つまで）

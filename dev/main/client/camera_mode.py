"""カメラモード（自撮り・建造物の撮影用）。「写真を保存」で、パネルを隠した画面を photos/ に PNG で保存する。

- 自分の体を三人称で映し、カメラは自分のまわりを回る（ドラッグ・なぞる）、ホイール／ピンチで距離、
  WASD・スティックで注目する所を動かす、Space・Ctrl で上下
- パネル：画面の表示（UI）・ブロックの枠の表示、モーション（MOD のかわいいモーションなど）、体の向き、視野角、傾き、距離、
  パネルを隠す（画面を1回タップ・クリックか H で戻る）、元に戻す、終わる
- 遊びは止めない（ほかの人・機械は動き続ける）。自分は動かない
"""
from .. import log, native

L = log.get("camera")

_c = native.require()
_rot = _c.camera_rot                  # 軸 axis のまわりに ang 回す行列（3×3）
CameraMode = _c.CameraMode            # カメラモード：CameraMode(game)・on_key・update・camera()・destroy()

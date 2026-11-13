"""読み込みメニュー（F8）：設計図（最初から入っている乗り物・艦・ロボット）・OBJ・構築物データを選んで、大きさや必要な資源を確かめてから置く。

左：読み込めるファイルの一覧（このワールドの imports・exports、ほかのワールドの exports）
右：選んだものの情報。外部の OBJ は大きさ（いちばん長い辺）・細かさ（ボクセルの大きさ）・素材を指定できる。
    サバイバルでは必要な資源と手持ちを並べて表示する。
OK で目の前に置き、閉じるボタン（または Esc）で何もせず閉じる。

中身は Rust（native/alpedia_core/src/game/import_ui.rs）。ここは名前を残すだけ。
"""
from .. import native

_c = native.require()

ROWS = 13
VOXEL_SIZES = (0.25, 0.5, 1.0)
WARN_VOXELS = 20000

_fmt_len = _c.iu_fmt_len          # 長さの表示（10 未満は小数 2 桁）
ImportPanel = _c.ImportPanel      # 読み込みメニュー：ImportPanel(game)

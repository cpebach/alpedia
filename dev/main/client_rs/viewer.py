"""遊ぶ画面（Rust の alpedia_view で描く）：ワールドの描画・操作・HUD・MOD の画面側。本体は Rust（alpedia_core.Viewer）。

  python run.py --world 名前

描く物：構築物（ブロック・遠景の箱・ライト・機器の記号・看板）、地表と掘った跡、惑星、景色の箱、空の色、
人物（VRM・色の箱）。画面の上の部品（プログラムの画面・MOD の画面・メニューなど）は ursina_shim（ursina の Entity・
Text・Button などをまねる部品）で動き、一時停止メニュー・インベントリ・機器の設定は ui.py・screens*.py。
"""
from .. import log, native

L = log.get("client_rs")
_c = native.require()

SKY_DAY = (0.45, 0.65, 0.9)
SKY_SPACE = (0.012, 0.016, 0.04)

Viewer = _c.Viewer
GameAdapter = _c.GameAdapter
run = _c.viewer_run
_Held = _c._Held
_Pointers = _c._Pointers
_TouchStub = _c._TouchStub

"""コンピューターのブロック（ノード）プログラミング画面。スクラッチのようにブロックを並べて作る。

- 左のパレットでブロックを選ぶと、選択中の行の下（帽子ブロックや「ここに追加」を選んでいればその中）に入る
- 白い入力欄をクリックすると、数値・センサー・計算・比較・変数などを選べる
- 保存すると scripts/<名前>.blocks.json と、変換した <名前>.py ができ、コンピューターは .py を実行する
- 「Python をコピー」で変換したコードをクリップボードへ

中身は Rust（native/alpedia_core/src/game/block_editor.rs）。ここは名前と値を残すだけ。
"""
from .. import native

_c = native.require()

ROW_H = 0.037
VISIBLE = 22
HINT_AFTER = 60.0          # 見本のあるプログラムを、この秒数（開いている時間の合計）作っても完成しなければヒントを出す
HINT_RGB = (0.35, 1.0, 0.55)
AREA_X0, AREA_X1 = -0.50, 0.44
AREA_TOP = 0.42
NAME_CHARS = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_"
TEXT_CHARS = NAME_CHARS + " .,:;!?+-*/=()[]%#@&'\"<>"

_dim = _c.be_dim                  # 色を暗くする
_shade = _c.be_shade              # 同じ色の入れ子を少し明るく（暗く）する
_round_quad = _c.be_round_quad    # 角の丸い四角
_arrow_mesh = _c.be_arrow_mesh    # 矢印の形
_shape = _c.be_shape              # _shape(aspect, radius)：radius="arrow" なら矢印
_expr_rgb = _c.be_expr_rgb        # 式ブロックの色
_expr_parts = _c.be_expr_parts    # 入れ物の式ブロックの並び
_safe = _c.be_safe                # < > を全角に
_to_float = _c.be_to_float
_wrap = _c.be_wrap
Row = _c.Row                      # 画面の1行：Row(kind, depth, parent=None, index=None, node=None, text="")
BlockEditor = _c.BlockEditor      # 画面：BlockEditor(game, construct, index)

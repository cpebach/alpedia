"""構築物を外から読み書きする窓口（スクリプト・ブロックプログラム用）。中身は Rust（game/control.rs）。

スクリプトの api（scripting.py）は構築物の中の配列（頂点の位置・機器のリストなど）を直接触らず、
必ずこの Port を通す。

- 読み取りは値のコピー（リスト・辞書・数値）を返す
- 書き込みは決まった項目だけ（WRITABLE）。値は検査してから入れる
- 機器は番号（構築物の中のブロックの番号）で指す。分離などで番号が変わることがあるので、
  機器の番号は毎ティック devices() で取り直す
"""
from . import native

WRITABLE = ("throttle", "ignite", "open", "enabled", "running")   # スクリプトから書き換えてよい機器の状態
PortError = native.require().PortError
Port = native.require().Port

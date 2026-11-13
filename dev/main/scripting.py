"""コンピューター機器から実行するゲーム内 Python スクリプト（実行の仕組みは Rust：game/scripting.rs）。

スクリプトは saves/<ワールド>/scripts/*.py に置き、`tick(api)` 関数を定義する。
コンピューターが起動している間、一定間隔で tick(api) が呼ばれる。
import できるのは math / random / statistics のみ（簡易サンドボックス）。
スクリプトそのものは Python のまま。api（ScriptAPI）・機器（DeviceHandle）・呼び出し（ScriptRunner）が Rust。
"""
from . import native

_C = native.require()
FACE_NAMES = {"+X": 0, "-X": 1, "+Y": 2, "-Y": 3, "+Z": 4, "-Z": 5}

script_path = _C.script_path              # scripts/ 内のファイルパス（フォルダ外を指す名前は拒否）
_safe_import = _C.safe_import
display_payload = _C.display_payload      # 表面に出す文字の辞書（text が空なら None＝消す）
DeviceHandle = _C.DeviceHandle
ScriptAPI = _C.ScriptAPI
ScriptRunner = _C.ScriptRunner

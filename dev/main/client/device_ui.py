"""機器の設定メニュー（P キー）：名前の変更など。

同じ名前の機器はコンピューターからまとめて動くので、名前を分けると個別に制御できる。
日本語は直接入力できないため、よく使う語（左・右・前・後…）や番号をボタンで付け足せる。
"""
from .. import log, native

L = log.get("device_ui")

TYPE_NAMES = {
    "thruster": "推進機", "tank": "タンク", "pipe": "配管", "valve": "バルブ", "battery": "バッテリー",
    "generator": "発電機", "crank": "手回し発電機", "computer": "コンピューター", "actuator": "アクチュエーター",
    "processor": "精錬機・加工機", "seat": "操縦席", "air_vent": "通気口", "antenna": "アンテナ",
}
NAME_LIMIT = 24


_c = native.require()
same_named = _c.dui_same_named                    # 同じ名前の機器の番号
number_same_named = _c.dui_number_same_named      # 同じ名前の機器に 1, 2, 3… と番号を付ける（位置の順）。付けた数を返す
update_programs = _c.dui_update_programs          # ブロックプログラムに出てくる機器名を置き換えて保存し直す


CODE_FONT_CANDIDATES = [
    "C:/Windows/Fonts/BIZ-UDGothicR.ttc", "C:/Windows/Fonts/msgothic.ttc",
    "/System/Library/Fonts/Osaka.ttf",
]


find_code_font = _c.dui_find_code_font          # 日本語が表示できる等幅フォント（入力欄用）
DevicePanel = _c.UDevicePanel                     # 機器の名前などを設定するメニュー（owner がいれば、その上に重ねて開く）

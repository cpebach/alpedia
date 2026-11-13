"""MOD が足す機器のふるまい（"type": "custom" の機器）と、スクリプト・ブロックプログラムの口。

コアの機器の種類（推進機・タンク・…）は決まっている。MOD は "type": "custom", "behavior": "<名前>" の機器を定義し、
そのふるまい（毎ティックの動き）は MOD のプログラム（world_addon の before_physics など）で書く。
コアは custom の機器について、電力（"power": "electric" なら power_use を使い、state["active"] を付ける）と、
保存（state["enabled"] と state["data"]：MOD が自由に使う辞書）だけを受け持つ。

スクリプトからは次の口で触れる（MOD のプログラムを読み込むときに register_* で登録する。
ワールドを始め直すと全部消えて、そのワールドの MOD がもう一度登録する）：

    register_device(behavior, methods={名前: fn(handle, *args)}, props={名前: fn(handle)})
        api.devices("<behavior>") で取れる機器に d.名前(...) / d.名前 を足す。
        handle.port（control.Port）・handle.index（機器の番号）・handle.data（state["data"]）が使える
    register_api(名前, fn(api, *args))
        api.名前(...) を足す（api._p が Port）
"""
from . import native

_c = native.require()
DEVICE_METHODS = _c.ext_device_methods()     # behavior → {名前: fn}
DEVICE_PROPS = _c.ext_device_props()         # behavior → {名前: fn}
API_FUNCS = _c.ext_api_funcs()               # 名前 → fn
reset = _c.ext_reset
register_device = _c.ext_register_device
register_api = _c.ext_register_api
behavior_of = _c.ext_behavior_of             # 機器の定義 → スクリプトから見た種類（custom なら behavior）

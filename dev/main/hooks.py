"""コアに機能を足す口（MOD のプログラム）。中身は Rust（alpedia_core の game/hooks.rs）。

コア（ワールド・構築物・物理）は、街・経済・資源のルールなどの具体的な中身を知らない。
MOD のプログラム（mods/<MOD名>/code/、modcode.py を参照）は Addon を継承し、必要な口だけを上書きする。
ワールドを読み込むと、そのワールドで選ばれた MOD の Addon が作られる（テストでは World(name, cfg, addons=[...])）。
口の一覧と説明は game/hooks.rs（どれも何もしない既定の実装を持つ）。

当たり判定に使う「動くかもしれない物体」（owner）は、次の2つを持つ:
    owner.is_moving() -> bool
    owner.point_velocity(P) -> (N,3)  その点での速度
"""
from . import native

Addon = native.require().Addon

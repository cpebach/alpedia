"""プレイヤー（描画とは独立した物理・所持品）。中身は Rust（alpedia_core の game/player.rs）。

属性（pos・vel・up・seated・inventory・hotbar・jetpack など）は Python から自由に読み書きできる。
update(world, dt, move, jump, thrust_dir) が毎ティックの動き（重力・水・ジェットパック・歩く・地面や建物・ブロックとの当たり）。
"""
from . import native

Player = native.require().Player

"""画面側の拡張の口。ワールド側の拡張（hooks.Addon）と組で使う。本体は Rust（alpedia_core.ClientAddon）。

ゲーム画面は拡張の中身を知らず、次の口だけを呼ぶ（くわしくは help(ClientAddon)）：
on_key・interact・aim・emotes・bgm・on_photo・on_camera・nearby・player_body・nearby_priority・seat_camera・update・destroy。
拡張はワールドの中身を直接触らず、自分のワールド側（world.addon(名前)）の問い合わせ関数を使う。
"""
from .. import native

ClientAddon = native.require().ClientAddon

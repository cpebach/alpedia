"""音：BGM（mods/bgm の曲）と効果音（sauce/sounds/*.wav）。

音量はゲーム設定の audio.bgm・audio.sfx（0〜100 %）。BGM は音楽の系統、効果音とムービーの音は
効果音の系統で鳴らすので（Rust の alpedia_view.Audio）、それぞれの音量がそのまま効く。

BGM の曲は mods/bgm/bgm.json の "tracks"：{曲の ID: {"file": ファイル名, "gain_db": 音量の補正, "title": 題名}}。
どの場面でどの曲を流すかは、画面側の拡張の bgm()（シナリオなら章ごと）とムービー（set_override）が決める。
曲のファイルが無ければ鳴らさない（BGM の MOD は入れなくても遊べる）。
"""
from .. import log, native

L = log.get("sound")

FADE = 1.6            # 曲を替えるときのフェード（秒）

_c = native.require()
get = _c.sound_get                    # ゲーム全体で1つの音の係（使えなければ鳴らさない係）
volume_of = _c.sound_volume_of        # 設定の音量（0〜1）
library = _c.sound_library            # {曲の ID: (ファイルのパス, 音量の倍率, 題名)}
sources = _c.sound_sources            # 近くの推進機・アクチュエーター・ジェットパックの音の大きさ
SoundSystem = _c.SoundSystem          # BGM・効果音
_Silent = _c._Silent
_RsSound = _c._RsSound
_RsVolume = _c._RsVolume
_RsLoader = _c._RsLoader
_RsBase = _c._RsBase

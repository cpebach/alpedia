"""Windows で速く動かす設定（起動時に1回）。

ノート PC（とくに Intel Core Ultra などの、速いコアと省電力のコアが混ざった CPU）では、Windows が
「省電力のために速さを抑えてよいプロセス」と判断すると、ゲームが省電力のコア・低いクロックで動き、
CPU もグラフィックもほとんど使っていないのに重い、ということが起きる。それを止める：

- 電力の調整（EcoQoS：実行速度を抑える・タイマーの精度を落とす）をこのプロセスでは切る
- 優先度を「通常以上」にする
- タイマーの精度を 1 ms にする（待ち時間が 15.6 ms 単位に丸められないように）

Windows 以外・古い Windows では何もしない（失敗しても続ける）。
"""
from . import log, native

L = log.get("winperf")
boost = native.require().winperf_boost

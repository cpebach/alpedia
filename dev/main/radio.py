"""無線通信：アンテナのある構築物どうしで、コンピューターがチャンネルを決めて情報をやり取りする（Rust：game/radio.rs）。

- 送る側・受ける側のどちらにも、電力が来ていて ON のアンテナが必要（ない構築物のコンピューターは通信の関数を使えない）。
- 届く距離は、送る側と受ける側のアンテナの届く距離（params.range）の短いほう。
  アンテナが複数あるときはいちばん遠くまで届くものを使う。
- 送った情報は少しの間（KEEP 秒）だけ空中に残り、その間に受信した各コンピューターが1回ずつ受け取れる。
- 送れる情報は数値・文字・True/False/None と、それらのリスト・辞書（JSON にできるもの）。大きさの上限あり。
- 同じ構築物から送った情報は、その構築物では受け取らない（自分の声は聞こえない）。
"""
from . import native

KEEP = 2.0              # 送った情報が空中に残る時間（秒）
MAX_PER_CHANNEL = 64    # チャンネルごとに残しておく数
MAX_BYTES = 4096        # 1回に送れる大きさ（JSON にしたときの文字数）
MAX_CHANNEL = 40

RadioError = native.require().RadioError
RadioBus = native.require().RadioBus


def antenna_info(c):
    """構築物の、使えるアンテナの (届く距離, 位置) か None。"""
    return native.require().radio_antenna_info(c)

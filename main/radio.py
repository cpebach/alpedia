"""無線通信：アンテナのある構築物どうしで、コンピューターがチャンネルを決めて情報をやり取りする。

- 送る側・受ける側のどちらにも、電力が来ていて ON のアンテナが必要（ない構築物のコンピューターは通信の関数を使えない）。
- 届く距離は、送る側と受ける側のアンテナの届く距離（params.range）の短いほう。
  アンテナが複数あるときはいちばん遠くまで届くものを使う。
- 送った情報は少しの間（KEEP 秒）だけ空中に残り、その間に受信した各コンピューターが1回ずつ受け取れる。
- 送れる情報は数値・文字・True/False/None と、それらのリスト・辞書（JSON にできるもの）。大きさの上限あり。
- 同じ構築物から送った情報は、その構築物では受け取らない（自分の声は聞こえない）。
"""
import json
from collections import deque

import numpy as np

KEEP = 2.0              # 送った情報が空中に残る時間（秒）
MAX_PER_CHANNEL = 64    # チャンネルごとに残しておく数
MAX_BYTES = 4096        # 1回に送れる大きさ（JSON にしたときの文字数）
MAX_CHANNEL = 40


class RadioError(RuntimeError):
    pass


def antenna_info(c):
    """構築物の、使えるアンテナの (届く距離, 位置) か None。"""
    best = None
    for i, d in enumerate(c.devs):
        if not d:
            continue
        dd = c.reg.device(d["def"]) or {}
        if dd.get("type") != "antenna":
            continue
        st = d["state"]
        if not st.get("enabled", True) or not st.get("active", False):
            continue
        rng = float(dd.get("params", {}).get("range", 3000.0))
        if best is None or rng > best[0]:
            best = (rng, c.pos[8 * i:8 * i + 8].mean(0))
    return best


def channel_name(ch):
    s = str(ch)
    if not s or len(s) > MAX_CHANNEL:
        raise RadioError(f"チャンネル名は 1〜{MAX_CHANNEL} 文字です")
    return s


def _clean(data):
    try:
        txt = json.dumps(data, ensure_ascii=False, allow_nan=False)
    except (TypeError, ValueError) as e:
        raise RadioError(f"送れるのは数値・文字・True/False/None と、それらのリスト・辞書です（{e}）") from None
    if len(txt) > MAX_BYTES:
        raise RadioError(f"一度に送れるのは {MAX_BYTES} 文字までです（{len(txt)} 文字）")
    return json.loads(txt)        # 送ったあとで書き換えても、受け取った側には影響しないように複製する


class RadioBus:
    def __init__(self, world):
        self.world = world
        self.channels = {}        # 名前 → deque[(番号, 時刻, 構築物ID, 構築物名, 位置, 届く距離, 情報)]
        self.seq = 0

    def _expire(self, q):
        t = self.world.time
        while q and (t - q[0][1] > KEEP or q[0][1] > t):   # 古いもの（ワールドを読み直して時刻が戻った場合も）
            q.popleft()

    def send(self, c, channel, data):
        ant = antenna_info(c)
        if ant is None:
            raise RadioError("アンテナがない（または電力が来ていない・OFF）ので送信できません")
        ch = channel_name(channel)
        self.seq += 1
        q = self.channels.setdefault(ch, deque(maxlen=MAX_PER_CHANNEL))
        self._expire(q)
        q.append((self.seq, self.world.time, c.id, c.name, ant[1].copy(), ant[0], _clean(data)))
        return self.seq

    def _visible(self, c, ch):
        ant = antenna_info(c)
        if ant is None:
            raise RadioError("アンテナがない（または電力が来ていない・OFF）ので受信できません")
        q = self.channels.get(channel_name(ch))
        if not q:
            return []
        self._expire(q)
        rng, pos = ant
        out = []
        for m in q:
            if m[2] == c.id:
                continue
            dist = float(np.linalg.norm(m[4] - pos))
            if dist <= min(rng, m[5]):
                out.append((m, dist))
        return out

    def receive(self, c, comp_state, channel):
        """前回の受信より後に届いた情報のリスト（古い順）。"""
        ch = channel_name(channel)
        seen = comp_state.setdefault("_radio_seen", {})
        last = seen.get(ch, 0)
        out = []
        for m, dist in self._visible(c, ch):
            if m[0] > last:
                out.append({"data": json.loads(json.dumps(m[6])), "from": m[3], "distance": dist,
                            "time": m[1], "age": self.world.time - m[1]})
                seen[ch] = max(seen.get(ch, 0), m[0])
        return out

    def latest(self, c, channel, default=None, max_age=1.0):
        """届いている中でいちばん新しい情報（なければ default）。受け取っても消えない。"""
        best = None
        for m, _ in self._visible(c, channel):
            if self.world.time - m[1] <= max_age and (best is None or m[0] > best[0]):
                best = m
        return default if best is None else json.loads(json.dumps(best[6]))

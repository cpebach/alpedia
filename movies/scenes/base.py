"""本編のムービーの共通の土台。"""
from cinema import anim
from cinema.film import Film
from cinema.post import Grade
from scenes.lib import NAME


class Movie(Film):
    """セリフは say() で足す（字幕と口の動きを一緒に）。"""
    chapter = ""

    def __init__(self):
        super().__init__()
        self.lines = []
        self.captions = []
        self.titles = []
        self._talk = []

    def say(self, who, t0, t1, text, actor=None, mouth=True):
        """who：配役のキー（NAME にない時はそのまま名前に）。actor があれば口を動かす。"""
        self.lines.append((t0, t1, NAME.get(who, who), text))
        if actor is not None and mouth:
            # 句読点で少し止まりながら話す
            n = max(1, len(text))
            pauses = [i for i, ch in enumerate(text) if ch in "、。…！？"]
            segs, last = [], 0
            for i in pauses + [n]:
                a = t0 + 0.05 + (t1 - t0 - 0.2) * last / n
                b = t0 + 0.05 + (t1 - t0 - 0.2) * i / n
                if b - a > 0.12:
                    segs.append((a, b))
                last = i + 1
            actor.talk += segs

    def base_grade(self, t, fade_in=0.6, fade_out=0.8):
        g = Grade()
        g.fade = min(1.0, t / fade_in) * (1 - anim.remap(t, self.duration - fade_out, self.duration))
        g.bloom_threshold = 0.86
        g.bloom = 0.35
        return g

"""音：BGM（mods/bgm の曲）と効果音（sauce/sounds/*.wav）。

音量はゲーム設定の audio.bgm・audio.sfx（0〜100 %）。BGM は Panda3D の音楽の系統、効果音とムービーの音は
効果音の系統で鳴らすので、それぞれの音量がそのまま効く。

BGM の曲は mods/bgm/bgm.json の "tracks"：{曲の ID: {"file": ファイル名, "gain_db": 音量の補正, "title": 題名}}。
どの場面でどの曲を流すかは、画面側の拡張の bgm()（シナリオなら章ごと）とムービー（set_override）が決める。
曲のファイルが無ければ鳴らさない（BGM の MOD は入れなくても遊べる）。
"""
import json

from .. import log, paths

L = log.get("sound")

FADE = 1.6            # 曲を替えるときのフェード（秒）
_SYS = None


def get(cfg=None):
    """ゲーム全体で1つの音の係。画面（ursina）を作ったあとで呼ぶ。"""
    global _SYS
    if _SYS is None:
        _SYS = SoundSystem()
    if cfg is not None:
        _SYS.apply(cfg)
    return _SYS


def volume_of(cfg, key, default):
    try:
        return max(0.0, min(100.0, float((cfg or {}).get("audio", {}).get(key, default)))) / 100.0
    except (TypeError, ValueError):
        return default / 100.0


def library():
    """{曲の ID: (ファイルのパス, 音量の倍率, 題名)}。mods/bgm/bgm.json が無い・読めなければ空。"""
    d = paths.MODS / "bgm"
    try:
        meta = json.loads((d / "bgm.json").read_text(encoding="utf-8"))
    except Exception:  # noqa: BLE001
        return {}
    out = {}
    for tid, t in (meta.get("tracks") or {}).items():
        try:
            f = d / t["file"]
            if f.exists():
                out[tid] = (f, 10 ** (float(t.get("gain_db", 0.0)) / 20.0), t.get("title", tid))
        except Exception:  # noqa: BLE001
            continue
    return out


class SoundSystem:
    def __init__(self):
        from ursina import application
        self.base = application.base
        self.bgm_vol = 0.6
        self.sfx_vol = 0.8
        self.lib = None
        self.cache = {}            # 効果音の名前 → [AudioSound]（重ねて鳴らせるように数個）
        self.loops = {}            # 繰り返す効果音の名前 → AudioSound
        self.music = None          # 今の曲 {"key", "snd", "gain", "vol", "target"}
        self.fading = []           # 消えていく曲
        self.want = None           # 場面が流したい曲 (曲の ID, 始める秒)
        self.override = None       # ムービーの曲（場面の曲より優先）
        try:
            self.base.taskMgr.add(self._task, "alpedia-sound")
        except Exception:  # noqa: BLE001
            L.exception("音の係を動かせません")

    # --- 音量 -------------------------------------------------------------
    def apply(self, cfg):
        self.bgm_vol = volume_of(cfg, "bgm", 60)
        self.sfx_vol = volume_of(cfg, "sfx", 80)
        try:
            self.base.musicManager.setVolume(self.bgm_vol)
            for m in self.base.sfxManagerList:
                m.setVolume(self.sfx_vol)
        except Exception:  # noqa: BLE001
            pass

    # --- 効果音 -----------------------------------------------------------
    def _load(self, name, music=False):
        from panda3d.core import Filename
        p = paths.SAUCE / "sounds" / f"{name}.wav"
        if not p.exists():
            return None
        try:
            ld = self.base.loader
            return (ld.loadMusic if music else ld.loadSfx)(Filename.fromOsSpecific(str(p)))
        except Exception:  # noqa: BLE001
            return None

    def sfx(self, name, volume=1.0):
        """効果音を1回鳴らす（同じ音は 3 つまで重ねて鳴る）。"""
        if self.sfx_vol <= 0 or volume <= 0.01:
            return
        pool = self.cache.get(name)
        if pool is None:
            pool = self.cache[name] = [s for s in (self._load(name) for _ in range(3)) if s is not None]
        if not pool:
            return
        snd = next((s for s in pool if s.status() != s.PLAYING), pool[0])
        snd.setVolume(max(0.0, min(1.0, volume)))
        snd.play()

    def loop(self, name, volume):
        """繰り返す効果音の大きさ（0 で止める）。毎フレーム呼ぶ。"""
        snd = self.loops.get(name)
        if volume <= 0.01:
            if snd is not None and snd.status() == snd.PLAYING:
                snd.stop()
            return
        if snd is None:
            snd = self._load(name)
            if snd is None:
                self.loops[name] = None
                return
            snd.setLoop(True)
            self.loops[name] = snd
        snd.setVolume(max(0.0, min(1.0, volume)))
        if snd.status() != snd.PLAYING:
            snd.play()

    def stop_loops(self):
        for snd in self.loops.values():
            if snd is not None:
                snd.stop()

    # --- BGM --------------------------------------------------------------
    def set_bgm(self, want):
        """場面の曲：(曲の ID, 始める秒) か None（止める）。毎フレーム呼んでよい（変わったときだけ替える）。"""
        self.want = want

    def set_override(self, want):
        """ムービーの曲（None で場面の曲に戻す）。"""
        self.override = want

    def _start(self, key):
        if self.lib is None:
            self.lib = library()
        tid, start = key
        t = self.lib.get(tid)
        if t is None:
            return None
        from panda3d.core import Filename
        try:
            snd = self.base.loader.loadMusic(Filename.fromOsSpecific(str(t[0])))
        except Exception:  # noqa: BLE001
            L.warning("BGM を読めません: %s", t[0])
            return None
        if snd is None:
            return None
        snd.setLoop(True)
        snd.setVolume(0.0)
        try:
            if start:
                snd.setTime(float(start))
        except Exception:  # noqa: BLE001
            pass
        snd.play()
        L.info("BGM: %s（%s 秒から）", t[2], start)
        return {"key": key, "snd": snd, "gain": min(1.0, t[1]), "vol": 0.0}

    def _task(self, task):
        try:
            from panda3d.core import ClockObject
            self.update(min(0.1, float(ClockObject.getGlobalClock().getDt())))
        except Exception:  # noqa: BLE001
            L.exception("音の更新でエラー")
        return task.cont

    def update(self, dt):
        key = self.override or self.want
        key = tuple(key) if key else None
        cur = self.music
        if (cur["key"] if cur else None) != key:
            if cur is not None:
                self.fading.append(cur)
            self.music = self._start(key) if key else None
        step = dt / FADE
        m = self.music
        if m is not None:
            m["vol"] = min(1.0, m["vol"] + step)
            m["snd"].setVolume(m["vol"] * m["gain"])
        for f in list(self.fading):
            f["vol"] -= step
            if f["vol"] <= 0:
                f["snd"].stop()
                self.fading.remove(f)
            else:
                f["snd"].setVolume(f["vol"] * f["gain"])

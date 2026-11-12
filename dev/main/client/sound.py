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
        from . import gfx
        if gfx.active():                     # Rust の画面：音も Rust（alpedia_view.Audio）で鳴らす
            try:
                _SYS = SoundSystem(_RsBase())
            except Exception:  # noqa: BLE001
                L.exception("音を鳴らせません")
                _SYS = _Silent()
        else:
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


class _Silent:
    """音を鳴らさない係（Rust の画面。呼び出し口だけ同じ）。"""

    def apply(self, cfg):
        pass

    def sfx(self, name, volume=1.0):
        pass

    def loop(self, name, volume):
        pass

    def stop_loops(self):
        pass

    def set_bgm(self, want):
        pass

    def set_override(self, want):
        pass

    def update(self, dt):
        pass


# ---------------------------------------------------------------------------
# Rust の画面の音（Panda3D の AudioSound・loader のまね。SoundSystem はそのまま使う）
# ---------------------------------------------------------------------------
class _RsSound:
    PLAYING = 2
    READY = 1

    def __init__(self, base, sid, music):
        self.base, self.id, self.music = base, sid, music
        self.vol = 1.0
        self.start = 0.0

    def _apply(self):
        k = self.base.music_vol if self.music else self.base.sfx_vol
        self.base.au.set_volume(self.id, self.vol * k)

    def setVolume(self, v):
        self.vol = float(v)
        self._apply()

    def setLoop(self, on=True):
        self.base.au.set_loop(self.id, bool(on))

    def setTime(self, t):
        self.start = float(t)

    def play(self):
        self._apply()
        self.base.au.play(self.id, self.start)
        self.start = 0.0

    def stop(self):
        self.base.au.stop(self.id)
        if self.music:                       # 曲は止めたら捨てる（大きいので）
            self.base.au.unload(self.id)

    def status(self):
        return self.PLAYING if self.base.au.playing(self.id) else self.READY

    def length(self):
        return self.base.au.length(self.id)


class _RsVolume:
    def __init__(self, base, key):
        self.base, self.key = base, key

    def setVolume(self, v):
        setattr(self.base, self.key, float(v))
        for snd in self.base.made:
            if snd.music == (self.key == "music_vol"):
                snd._apply()


class _RsLoader:
    def __init__(self, base):
        self.base = base

    def _load(self, f, music):
        path = f.toOsSpecific() if hasattr(f, "toOsSpecific") else str(f)
        sid = self.base.au.load(path)
        if sid < 0:
            return None
        snd = _RsSound(self.base, sid, music)
        self.base.made.append(snd)
        if music:
            self.base.made = [x for x in self.base.made if not x.music or x is snd or x.base.au.playing(x.id)]
        return snd

    def loadSfx(self, f):
        return self._load(f, False)

    def loadMusic(self, f):
        return self._load(f, True)

    def loadPcm(self, samples, channels, rate):
        """読み解いた音（float32、チャンネルを交互に）から効果音として（動画の音など）。"""
        import numpy as np
        sid = self.base.au.load_pcm(np.ascontiguousarray(samples, np.float32).reshape(-1), int(channels), int(rate))
        if sid < 0:
            return None
        snd = _RsSound(self.base, sid, False)
        self.base.made.append(snd)
        return snd


class _RsTasks:
    def add(self, *a, **k):
        pass                                 # Rust の画面は毎フレーム update(dt) を呼ぶ


class _RsBase:
    """application.base のうち、SoundSystem が使う所のまね。"""

    def __init__(self):
        import alpedia_view
        self.au = alpedia_view.Audio()
        if not self.au.ok():
            L.warning("音の出口が見つかりません（音は鳴りません）")
        self.music_vol = 1.0
        self.sfx_vol = 1.0
        self.made = []
        self.loader = _RsLoader(self)
        self.musicManager = _RsVolume(self, "music_vol")
        self.sfxManagerList = [_RsVolume(self, "sfx_vol")]
        self.taskMgr = _RsTasks()


class SoundSystem:
    def __init__(self, base=None):
        if base is None:
            from ursina import application
            base = application.base
        self.base = base
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

    def _fname(self, p):
        if isinstance(self.base, _RsBase):
            return str(p)
        from panda3d.core import Filename
        return Filename.fromOsSpecific(str(p))

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
        p = paths.SAUCE / "sounds" / f"{name}.wav"
        if not p.exists():
            return None
        try:
            ld = self.base.loader
            return (ld.loadMusic if music else ld.loadSfx)(self._fname(p))
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
        try:
            snd = self.base.loader.loadMusic(self._fname(t[0]))
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


def sources(w, prev_lit, sfx):
    """近くで燃えている推進機・動いているアクチュエーター・ジェットパックの音の大きさ（距離で小さく）。
    点火した推進機は sfx("ignite") で「ボッ」。戻り値 ({ループの名前: 大きさ}, 燃えている推進機の組)。
    今までの画面（game.py）と Rust の画面の両方で使う。"""
    import numpy as np

    from .. import actuator as act
    eye = w.player.eye()
    out = {"thruster_loop": 0.0, "motor_loop": 0.0, "jetpack_loop": 0.0}
    lit = set()
    for c in w.constructs_near(eye, 150.0):
        if not c.n_vox or not c.has_devices:
            continue
        for i, d in enumerate(c.devs):
            if not d:
                continue
            st = d["state"]
            if "firing" in st or "lit" in st:
                f = float(st.get("firing", 0.0) or 0.0)
                if f > 0.01 or st.get("lit"):
                    dist = float(np.linalg.norm(c.voxel_center(i) - eye))
                    att = 1.0 / (1.0 + (dist / 12.0) ** 2)
                    out["thruster_loop"] += max(f, 0.15) * att * 0.5
                    lit.add((c.id, i))
                    if (c.id, i) not in prev_lit:
                        sfx("ignite", min(1.0, att * 1.5))
            elif "angle_target" in st and act.busy(st):
                dist = float(np.linalg.norm(c.voxel_center(i) - eye))
                out["motor_loop"] += 0.7 / (1.0 + (dist / 8.0) ** 2)
    p = w.player
    if getattr(p, "flying", False) and float(np.linalg.norm(getattr(p, "thrust", np.zeros(3)))) > 0.05:
        out["jetpack_loop"] = 0.6
    return {k: min(1.0, v) for k, v in out.items()}, lit

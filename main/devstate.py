"""機器の状態の決まった形（スキーマ）。

機器（ブロック）ごとの状態 d["state"] に入れてよい項目は、機器の種類ごとにここで決める。
- 保存する項目（saved=True）と、計算中だけ使う項目（saved=False：保存しない）を分ける
- 読み込み・設置のときに normalize() で、足りない項目を既定値で埋め、型をそろえ、知らない項目を捨てる
- "_" で始まる項目は、実行中だけの内部用（スクリプトの実行環境など）。保存も検査もしない

Rust など別の言語で書き直すときは、この表がそのまま構造体の定義になる。
"""
from . import log

L = log.get("devstate")


class F:
    """項目の定義。kind: float / int / bool / str / dict / list / any。default は値か、params を受け取る関数。"""
    __slots__ = ("kind", "default", "saved", "optional")

    def __init__(self, kind, default=None, saved=True, optional=False):
        self.kind = kind
        self.default = default
        self.saved = saved
        self.optional = optional       # True：無ければ作らない（既定値で埋めない）

    def make_default(self, params, st):
        d = self.default
        if callable(d):
            return d(params, st)
        if isinstance(d, (dict, list)):
            return type(d)(d)
        return d


def _p(key, default):
    return lambda params, st: params.get(key, default)


# どの機器にもある項目
COMMON = {
    "error": F("str", optional=True),              # 直前のエラー（スクリプト・電力不足など）
    "error_line": F("int", optional=True),
    "display": F("dict", optional=True),           # 表面に出す文字（コンピューターの api.display）
    "active": F("bool", optional=True, saved=False),   # 電力が来て動いている（毎ティック計算）
}

SCHEMA = {
    "thruster": {"throttle": F("float", 0.0), "firing": F("float", optional=True, saved=False)},
    "tank": {"contents": F("dict", lambda p, st: dict(p.get("contents", {})))},
    "pipe": {},
    "valve": {"open": F("bool", _p("open", True))},
    "battery": {"charge": F("float", _p("charge", 0.0))},
    "generator": {"enabled": F("bool", True)},
    "crank": {"pending": F("float", optional=True)},           # 回したけれどまだ配っていない電気 (J)
    "computer": {"script": F("str", _p("script", "")), "running": F("bool", False), "memory": F("dict", {}),
                 "blocks": F("str", optional=True)},         # ブロックプログラムのファイル名
    "actuator": {"attached": F("bool", optional=True, saved=False),   # 出力面が結合しているか（毎ティック計算）
                 "ext": F("float", 0.0), "ext_target": F("float", lambda p, st: st.get("ext", 0.0)),
                 "ext_speed": F("float", _p("speed", 0.5)),
                 "angle": F("float", 0.0), "angle_target": F("float", lambda p, st: st.get("angle", 0.0)),
                 "rot_speed": F("float", _p("rot_speed", 90.0)),
                 "preset": F("dict", lambda p, st: _default_preset())},
    "processor": {"progress": F("float", 0.0), "enabled": F("bool", True), "next": F("int", optional=True),
                  "working": F("bool", optional=True, saved=False)},
    "seat": {},
    "air_vent": {"mode": F("str", _p("mode", "fill")), "enabled": F("bool", True),
                 "cavity": F("bool", optional=True, saved=False)},
    "antenna": {"enabled": F("bool", True)},
}


def _default_preset():
    from .actuator import DEFAULT_PRESET
    return dict(DEFAULT_PRESET)


def fields(dtype):
    out = dict(COMMON)
    out.update(SCHEMA.get(dtype, {}))
    return out


def _coerce(kind, v):
    if v is None:
        return None
    if kind == "float":
        return float(v)
    if kind == "int":
        return int(v)
    if kind == "bool":
        return bool(v)
    if kind == "str":
        return str(v)
    if kind == "dict":
        if not isinstance(v, dict):
            raise TypeError("dict ではありません")
        return v
    if kind == "list":
        return list(v)
    return v


def _migrate(dtype, st):
    """古い保存形式からの読み替え（形が決まる前のセーブ用）。"""
    if dtype == "actuator" and "target" in st and "ext_target" not in st:
        st["ext_target"] = st.pop("target")


def normalize(dtype, params, state, where=""):
    """状態をこの種類の決まった形にした新しい辞書を返す（元の辞書は変えない）。"""
    st = dict(state or {})
    _migrate(dtype, st)
    fs = fields(dtype)
    out = {}
    for k, v in st.items():
        if k.startswith("_"):
            out[k] = v                   # 実行中だけの内部用
            continue
        f = fs.get(k)
        if f is None:
            L.warning("機器 %s%s の状態の知らない項目 '%s' を捨てました", dtype, where, k)
            continue
        try:
            out[k] = _coerce(f.kind, v)
        except (TypeError, ValueError):
            L.warning("機器 %s%s の状態 '%s' の値 %r が %s でないので既定値にしました", dtype, where, k, v, f.kind)
    for k, f in fs.items():
        if k not in out and not f.optional:
            out[k] = _coerce(f.kind, f.make_default(params or {}, out))
    return out


def fill(dtype, params, st):
    """足りない項目を既定値で埋める（その辞書をそのまま書き換える。動いている機器用）。"""
    for k, f in fields(dtype).items():
        if k not in st and not f.optional:
            st[k] = _coerce(f.kind, f.make_default(params or {}, st))
    return st


def to_saved(dtype, state):
    """保存する分だけ（計算中だけの項目と "_" で始まる項目を除く）。"""
    fs = fields(dtype)
    return {k: v for k, v in state.items() if not k.startswith("_") and (k not in fs or fs[k].saved)}


def check(dtype, state):
    """決まった形になっているか（テスト用）：問題のリスト。"""
    fs = fields(dtype)
    bad = []
    for k, v in state.items():
        if k.startswith("_"):
            continue
        f = fs.get(k)
        if f is None:
            bad.append(f"知らない項目 {k}")
            continue
        try:
            if v is not None and _coerce(f.kind, v) != v:
                bad.append(f"{k} の型が {f.kind} でない: {v!r}")
        except (TypeError, ValueError):
            bad.append(f"{k} の型が {f.kind} でない: {v!r}")
    for k, f in fs.items():
        if not f.optional and k not in state:
            bad.append(f"{k} がない")
    return bad

"""コンピューター機器から実行するゲーム内 Python スクリプト。

スクリプトは saves/<ワールド>/scripts/*.py に置き、`tick(api)` 関数を定義する。
コンピューターが起動している間、一定間隔で tick(api) が呼ばれる。
import できるのは math / random / statistics のみ（簡易サンドボックス）。
"""
import builtins
import re
import traceback
import math
import random
import statistics

import numpy as np

from . import log

L = log.get("script")

_ALLOWED_MODULES = {"math": math, "random": random, "statistics": statistics}
_SAFE_BUILTINS = {k: getattr(builtins, k) for k in (
    "abs", "all", "any", "bool", "dict", "enumerate", "filter", "float", "int", "len", "list", "map",
    "max", "min", "pow", "range", "reversed", "round", "set", "sorted", "str", "sum", "tuple", "zip",
    "isinstance", "Exception", "ValueError", "KeyError", "IndexError", "ZeroDivisionError", "True", "False", "None",
    "getattr", "hasattr", "TypeError", "AttributeError", "RuntimeError",
) if hasattr(builtins, k)}


_NAME_RE = re.compile(r"^[A-Za-z0-9_\-.]+\.py$")


def script_path(world_dir, name):
    """scripts/ 内のファイルパス（フォルダ外を指す名前は拒否）。"""
    if not name or not _NAME_RE.match(name) or name.startswith("."):
        raise ValueError(f"ファイル名が不正です: '{name}'（半角英数字・- _ . で、.py で終わる名前）")
    return world_dir / "scripts" / name


def _safe_import(name, *a, **k):
    if name in _ALLOWED_MODULES:
        return _ALLOWED_MODULES[name]
    raise ImportError(f"ゲーム内スクリプトでは '{name}' は使えません（math, random, statistics のみ）")


FACE_NAMES = {"+X": 0, "-X": 1, "+Y": 2, "-Y": 3, "+Z": 4, "-Z": 5}


def _color(v, default, n=3):
    if v is None:
        return list(default)
    v = [float(x) for x in v][:4]
    if any(x > 1.0 for x in v):
        v = [x / 255.0 for x in v]
    while len(v) < n:
        v.append(1.0)
    return [min(max(x, 0.0), 1.0) for x in v]


def display_payload(direction, text, face=None, width=None, color=None, background=None):
    """表面に出す文字の辞書（text が空なら None＝消す）。direction は機器の向きの番号（face 省略時に使う）。"""
    if text is None or text == "":
        return None
    if face is None:
        f = direction
    elif isinstance(face, str) and face.lower() == "all":
        f = "all"
    elif isinstance(face, str):
        if face.upper() not in FACE_NAMES:
            raise ValueError(f"face は {list(FACE_NAMES)} か 'all' です")
        f = FACE_NAMES[face.upper()]
    else:
        f = int(face) % 6
    w = None if width is None else float(min(max(width, 0.05), 50.0))
    return {"text": str(text)[:2000], "face": f, "width": w,
            "color": _color(color, (1, 1, 1)), "background": _color(background, (0.04, 0.05, 0.08, 0.92), 4)}


class DeviceHandle:
    """スクリプトから見た機器。中身は Port（control.py）を通して読み書きする。"""

    def __init__(self, port, info):
        self._p = port
        self._i, self._kind, self._type, self._name, self._dir = info

    name = property(lambda s: s._name)
    type = property(lambda s: s._type)
    kind = property(lambda s: s._kind)
    direction = property(lambda s: s._dir)

    @property
    def state(self):
        return self._p.device_state(self._i)

    def display(self, text, face=None, width=None, color=None, background=None):
        """このブロックの表面に文字を表示（コンピューターのみ）。"""
        if self.type != "computer":
            raise ValueError("display() はコンピューターでのみ使えます")
        self._p.set_display(self._i, display_payload(self._p.device_dir(self._i), text, face, width, color, background))

    def clear_display(self):
        self._p.set_display(self._i, None)

    @property
    def cell(self):
        return self._p.device_cell(self._i)

    @property
    def offset(self):
        """構築物の重心から見たローカル座標での位置 [x,y,z] (m)。"""
        return self._p.device_offset(self._i)

    # 推進機
    @property
    def throttle(self):
        return self._p.device_get(self._i, "throttle", 0.0)

    @throttle.setter
    def throttle(self, v):
        self._p.device_set(self._i, "throttle", v)

    # バルブ
    @property
    def open(self):
        return self._p.device_get(self._i, "open", True)

    @open.setter
    def open(self, v):
        self._p.device_set(self._i, "open", v)

    # アクチュエーター -------------------------------------------------
    def rotate(self, angle, speed=None):
        """出力側を angle 度だけ回す（相対）。speed: 度/秒"""
        self._p.act_rotate(self._i, angle, speed)

    def rotate_to(self, angle, speed=None):
        """角度 angle 度まで回す（絶対）。"""
        self._p.act_rotate(self._i, angle, speed, absolute=True)

    def extend(self, amount=1.0, speed=None):
        """amount ブロック分押し出す（相対・最大でブロック1つ分）。speed: ブロック/秒"""
        self._p.act_extend(self._i, amount, speed)

    def retract(self, amount=1.0, speed=None):
        """amount ブロック分引き込む。"""
        self._p.act_retract(self._i, amount, speed)

    def extend_to(self, position, speed=None):
        """伸び量を position ブロック（0〜1）にする（絶対）。"""
        self._p.act_extend(self._i, position, speed, absolute=True)

    def detach(self):
        """出力面の結合を切り離す。切り離したら True。"""
        return self._p.act_detach(self._i)

    def attach(self):
        """出力面の先のブロックと結合する。結果のメッセージを返す。"""
        return self._p.act_attach(self._i)

    def execute(self):
        """P メニューで設定した動作を実行する。"""
        return self._p.act_execute(self._i)

    @property
    def angle(self):
        return self._p.device_get(self._i, "angle", 0.0)

    @property
    def extension(self):
        return self._p.device_get(self._i, "ext", 0.0)

    @extension.setter
    def extension(self, v):
        self.extend_to(v)

    @property
    def attached(self):
        return self._p.act_attached(self._i)

    @property
    def output_angle(self):
        """出力側ブロックの向き（度）。横向きの軸なら真上から測った角度。
        台が傾いても「世界から見た」向きが分かる（姿勢制御に使う）。"""
        return self._p.act_output_angle(self._i)

    @property
    def busy(self):
        return self._p.act_busy(self._i)

    # 汎用 ON/OFF
    @property
    def enabled(self):
        return self._p.device_get(self._i, "enabled", True)

    @enabled.setter
    def enabled(self, v):
        self._p.device_set(self._i, "enabled", v)

    # 読み取り
    @property
    def contents(self):
        return dict(self._p.device_get(self._i, "contents", {}) or {})

    @property
    def charge(self):
        return self._p.device_get(self._i, "charge", 0.0)

    def __repr__(self):
        return f"<{self.type} '{self.name}'>"


def _v(a):
    return [float(x) for x in a]


class ScriptAPI:
    """スクリプトの tick(api) に渡す api。構築物の中身は Port（control.py）を通してだけ読み書きする。"""

    def __init__(self, world, c, comp_idx, port=None):
        from .control import Port
        self._p = port or Port(world, c)
        self._ci = comp_idx
        self.memory = self._p.comp_state(comp_idx).setdefault("memory", {})
        self.logs = []

    # 状態
    time = property(lambda s: s._p.time)
    position = property(lambda s: _v(s._p.pose()[0]))
    velocity = property(lambda s: _v(s._p.pose()[2]))
    speed = property(lambda s: float(np.linalg.norm(s._p.pose()[2])))
    mass = property(lambda s: s._p.mass())

    @property
    def up(self):
        return _v(self._p.up())

    @property
    def up_local(self):
        """惑星の上方向を構築物のローカル座標で表したもの（直立なら [0,1,0]）。"""
        R = self._p.pose()[1]
        return _v(R.T @ self._p.up())

    @property
    def velocity_local(self):
        _, R, v, _ = self._p.pose()
        return _v(R.T @ v)

    @property
    def angular_velocity_local(self):
        _, R, _, w = self._p.pose()
        return _v(R.T @ w)

    @property
    def altitude(self):
        return self._p.altitude()

    @property
    def vertical_speed(self):
        return float(np.dot(self._p.pose()[2], self._p.up()))

    @property
    def power(self):
        return self._p.power()

    @property
    def input(self):
        """操縦席に座っている間に押されているキーの集合。"""
        return self._p.input_keys()

    @property
    def storage(self):
        return self._p.storage()

    def orientation(self):
        """構築物の回転行列（行が ローカルX,Y,Z 軸のワールド方向）。"""
        return self._p.pose()[1].T.tolist()

    def devices(self, type=None, name=None):  # noqa: A002
        out = []
        for info in self._p.devices():
            h = DeviceHandle(self._p, info)
            if type and h.type != type and h.kind != type:
                continue
            if name and h.name != name:
                continue
            out.append(h)
        return out

    def device(self, name):
        r = self.devices(name=name)
        return r[0] if r else None

    # --- 表示（看板） ---------------------------------------------------
    def display(self, text, face=None, width=None, color=None, background=None):
        """このコンピューターの表面に文字を表示する。
        face: "+X" "-X" "+Y" "-Y" "+Z" "-Z" か "all"（省略時はブロックの向き）
        width: 表示板の幅 m（省略時はブロックの大きさ。大きくすると看板になる）
        color / background: 文字色・背景色 (r,g,b[,a]) 0〜1 または 0〜255"""
        self._p.set_display(self._ci, display_payload(self._p.device_dir(self._ci), text, face, width, color, background))

    def clear_display(self):
        self._p.set_display(self._ci, None)

    def stop(self):
        """このコンピューターを停止する（表示は残る）。"""
        self._p.device_set(self._ci, "running", False)

    # --- 惑星・軌道 -----------------------------------------------------
    def planet(self):
        """最寄りの惑星: {"name", "center", "radius", "mu"(重力定数×質量), "surface_gravity"}"""
        return self._p.planet()

    def orbit(self):
        """現在の軌道: {"apoapsis", "periapsis"（地表からの高度 m）, "eccentricity", "period"(秒),
        "orbital_speed"（円軌道に必要な速さ）, "horizontal_speed", "time_to_apoapsis"}"""
        return self._p.orbit()

    def cavities(self):
        return self._p.cavities()

    # --- 無線通信（アンテナが必要） --------------------------------------
    @property
    def antenna(self):
        """この構築物に使えるアンテナ（電力が来ていて ON）があるか。"""
        return self._p.antenna() is not None

    @property
    def antenna_range(self):
        """アンテナの届く距離 m（なければ 0）。"""
        a = self._p.antenna()
        return a[0] if a else 0.0

    def send(self, channel, data):
        """チャンネル channel に data を送る（届く範囲のアンテナを持つ構築物のコンピューターが受け取れる）。
        data: 数値・文字・True/False/None と、それらのリスト・辞書。"""
        return self._p.radio_send(channel, data)

    def receive(self, channel):
        """前回呼んだ後にチャンネル channel に届いた情報のリスト（古い順）。
        各要素は {"data": 情報, "from": 送った構築物の名前, "distance": 距離 m, "age": 送ってからの秒数}"""
        return self._p.radio_receive(self._ci, channel)

    def latest(self, channel, default=None, max_age=1.0):
        """チャンネル channel に届いている一番新しい情報（max_age 秒より古いものや、無いときは default）。
        何度呼んでも同じものが返る（受け取っても消えない）。"""
        return self._p.radio_latest(channel, default, max_age)

    def log(self, *msg):
        text = " ".join(str(m) for m in msg)
        self.logs.append(text)
        st = self._p.comp_state(self._ci)
        buf = st.setdefault("_log", [])
        buf.append(f"[{self._p.time:.1f}] {text}")
        del buf[:-50]
        name = self._p.devices_name(self._ci)
        L.info("[%s] %s", name, text)
        self._p.message(f"[{name}] {text}")


class ScriptRunner:
    def __init__(self, world):
        self.world = world
        self.cache = {}  # script name -> (mtime, code)

    def invalidate(self, name):
        self.cache.pop(name, None)

    def _load(self, name):
        path = script_path(self.world.dir, name)
        if not path.exists():
            raise FileNotFoundError(f"スクリプト {name} が scripts/ にありません")
        mt = (path.stat().st_mtime_ns, path.stat().st_size)
        ent = self.cache.get(name)
        if ent and ent[0] == mt:
            return ent[1]
        code = compile(path.read_text(encoding="utf-8"), str(path), "exec")
        self.cache[name] = (mt, code)
        L.info("スクリプト読み込み: %s", name)
        return code

    def tick(self, c, dt):
        for i, d in enumerate(c.devs):
            if not d:
                continue
            dd = c.reg.device(d["def"])
            if not dd or dd["type"] != "computer":
                continue
            st = d["state"]
            if not st.get("running"):
                continue
            if c.status.get("ratio", 1.0) <= 0 or c.status.get("stored", 0) <= 0 and c.status.get("production", 0) <= 0:
                st["running"] = False
                st["error"] = "電力不足"
                continue
            st["_acc"] = st.get("_acc", 0.0) + dt
            interval = float(dd["params"].get("interval", 0.05))
            if st["_acc"] < interval:
                continue
            st["_acc"] = 0.0
            path = None
            try:
                path = script_path(self.world.dir, st.get("script", ""))
                code = self._load(st.get("script", ""))
                env = st.get("_env")
                if env is None or env.get("__code__") is not code:
                    env = {"__builtins__": dict(_SAFE_BUILTINS, __import__=_safe_import, print=None),
                           "math": math, "random": random, "__code__": code}
                    exec(code, env)  # noqa: S102
                    st["_env"] = env
                if i >= len(c.devs) or c.devs[i] is not d:  # 分離・結合で番号が変わった
                    i = next((k for k, x in enumerate(c.devs) if x is d), None)
                    if i is None:
                        continue
                api = ScriptAPI(self.world, c, i)
                env["__builtins__"]["print"] = api.log
                fn = env.get("tick")
                if fn is None:
                    raise RuntimeError("tick(api) 関数がありません")
                fn(api)
            except Exception as e:  # noqa: BLE001
                st["running"] = False
                st["error"] = f"{type(e).__name__}: {e}"
                line = getattr(e, "lineno", None) if isinstance(e, SyntaxError) else None
                if line is None and path is not None:
                    for fr in traceback.extract_tb(e.__traceback__):
                        if fr.filename == str(path):
                            line = fr.lineno
                if line:
                    st["error_line"] = line
                    st["error"] += f"（{line} 行目）"
                else:
                    st.pop("error_line", None)
                buf = st.setdefault("_log", [])
                buf.append(f"[{self.world.time:.1f}] エラー: {st['error']}")
                st.pop("_env", None)
                L.error("スクリプトエラー [%s/%s]: %s", c.name, d["name"], st["error"])
                self.world.message(f"スクリプトエラー: {st['error']}")

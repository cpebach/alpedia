"""素材・機器・惑星・プレイヤー・資源の定義を読み込むレジストリ。

本体の基本定義は sauce/base/<カテゴリ>/*.json、
MOD は mods/<MOD名>/<カテゴリ>/*.json に同じ形式で置く。
定義に "extends": "<既存ID>" を書くと、その定義を引き継いで差分だけ上書きできる。
"""
import copy
import json
from pathlib import Path

from . import paths, log

L = log.get("registry")

CATEGORIES = ("resources", "materials", "devices", "planets", "players")

# カテゴリごとの既定値（足りない項目はここから補う）
DEFAULTS = {
    "resources": {"name": "", "density": 1000.0, "fluid": False, "color": [0.8, 0.8, 0.8]},
    "materials": {
        "name": "", "color": [0.7, 0.7, 0.7], "texture": None,
        "density": 1000.0,        # kg/m^3
        "stiffness": 0.8,         # 0-1 ボクセルの形状保持（硬度）
        "joint_stiffness": 0.8,   # 0-1 頂点同士の結合の強さ（動きにくさ）
        "break_distance": 0.05,   # 結合が離れる距離（ボクセルサイズ比）
        "yield_strain": 0.1,      # これを超える変形で塑性変形が始まる
        "plasticity": 0.0,        # 0-1 塑性（靭性）。大きいほど曲がったまま戻らない
        "damping": 0.02,          # 速度減衰 (1/s)
        "friction": 0.6,
        "rebond": False,          # 離れた頂点が近づくと再結合する（液体・粘土など）
        "rebond_distance": 0.2,   # 再結合する距離（ボクセルサイズ比）
        "cost": {},               # サバイバル時の設置コスト {資源ID: kg}
    },
    "devices": {
        "name": "", "type": "pipe", "material": "steel", "color": [0.5, 0.5, 0.55],
        "texture": None,
        "power": "none",          # none / manual / electric
        "power_use": 0.0,         # W（electric のとき）
        "pipe_port": False,       # 配管ネットワークに繋がるか
        "directional": False,     # 向きを持つか
        "params": {}, "cost": {},
    },
    "planets": {
        "name": "", "radius": 500.0, "surface_gravity": 9.81, "color": [0.45, 0.55, 0.35],
        "texture": None, "noise_amp": 8.0, "noise_freq": 3.0, "seed": 0,
        "dig_resource": "ore", "atmosphere_pressure": 101.3, "terrain": "sphere",
    },
    "players": {
        "name": "", "color": [0.9, 0.75, 0.6], "model": None, "texture": None,
        "height": 1.8, "radius": 0.35, "walk_speed": 5.0, "jump_speed": 5.0,
        "jetpack_accel": 15.0, "jetpack_fuel_rate": 1.0, "reach": 8.0,
    },
}

REQUIRED_TYPES = {
    "thruster", "tank", "pipe", "valve", "battery", "generator", "crank", "computer",
    "actuator", "processor", "seat", "air_vent", "antenna",
}


class Registry:
    def __init__(self):
        self.defs = {c: {} for c in CATEGORIES}
        self.loaded_mods = []
        self.errors = []

    # --- 取得 ---------------------------------------------------------
    def get(self, cat, id_):
        return self.defs[cat].get(id_)

    def material(self, id_):
        return self.defs["materials"].get(id_) or self.defs["materials"]["rock"]

    def device(self, id_):
        return self.defs["devices"].get(id_)

    def res_name(self, id_):
        """資源 ID → 表示名（硫黄鉱石 など）。"""
        return (self.defs["resources"].get(id_) or {}).get("name", id_)

    def res_text(self, amounts, sep="、"):
        """{資源ID: 量} → 「鉄 100、機械部品 5」"""
        return sep.join(f"{self.res_name(k)} {v:.0f}" for k, v in amounts.items() if v)

    # --- 読み込み -------------------------------------------------------
    def load(self, enabled_mods=()):
        self._load_tree(paths.BASE_DEFS, source="base", allow_override=True)
        for mod in enabled_mods:
            mdir = paths.MODS / mod
            if not mdir.is_dir():
                self._err(f"MOD '{mod}' が mods/ に見つかりません")
                continue
            meta = {}
            mj = mdir / "mod.json"
            if mj.exists():
                try:
                    meta = json.loads(mj.read_text(encoding="utf-8"))
                except Exception as e:  # noqa: BLE001
                    self._err(f"MOD '{mod}' の mod.json が読めません: {e}")
                    continue
            L.info("MOD 読み込み開始: %s %s", meta.get("name", mod), meta.get("version", ""))
            n = self._load_tree(mdir, source=mod, allow_override=False)
            self.loaded_mods.append(mod)
            L.info("MOD 読み込み完了: %s（%d 件）", mod, n)
        L.info("定義数: %s", {c: len(v) for c, v in self.defs.items()})
        return self

    def _err(self, msg):
        self.errors.append(msg)
        L.error(msg)

    def _load_tree(self, root: Path, source: str, allow_override: bool) -> int:
        count = 0
        for cat in CATEGORIES:
            d = root / cat
            if not d.is_dir():
                continue
            for f in sorted(d.glob("*.json")):
                try:
                    data = json.loads(f.read_text(encoding="utf-8"))
                except Exception as e:  # noqa: BLE001
                    self._err(f"[{source}] {f.name} の JSON 解析に失敗: {e}")
                    continue
                items = data if isinstance(data, list) else [data]
                for item in items:
                    if self._add(cat, item, root, source, allow_override, f.name):
                        count += 1
        return count

    def _add(self, cat, item, root, source, allow_override, fname) -> bool:
        if not isinstance(item, dict) or "id" not in item:
            self._err(f"[{source}] {cat}/{fname}: 'id' のない定義をスキップ")
            return False
        id_ = item["id"]
        if "extends" in item:
            parent = self.defs[cat].get(item["extends"])
            if parent is None:
                self._err(f"[{source}] {cat}/{id_}: 継承元 '{item['extends']}' が存在しません")
                return False
            merged = _deep_merge(parent, item)
        else:
            merged = _deep_merge(DEFAULTS[cat], item)
        if id_ in self.defs[cat] and not (allow_override or item.get("override")):
            self._err(f"[{source}] {cat}/{id_}: 既存IDと重複（上書きするには \"override\": true）")
            return False
        if cat == "devices" and merged.get("type") not in REQUIRED_TYPES:
            self._err(f"[{source}] devices/{id_}: 未知の type '{merged.get('type')}'")
            return False
        # テクスチャ・モデルのパスを解決
        for key in ("texture", "model"):
            v = item.get(key)
            if v:
                p = (root / v) if not Path(v).is_absolute() else Path(v)
                if not p.exists():
                    p2 = paths.SAUCE / v
                    if p2.exists():
                        p = p2
                    else:
                        L.warning("[%s] %s/%s: %s '%s' が見つかりません", source, cat, id_, key, v)
                        p = None
                merged[key] = str(p) if p else None
        merged["name"] = merged.get("name") or id_
        merged["_source"] = source
        self.defs[cat][id_] = merged
        L.debug("[%s] %s/%s を読み込み", source, cat, id_)
        return True

    def palette(self):
        """建築用の選択肢（素材 → 機器の順）。"""
        out = [("material", k) for k in self.defs["materials"]]
        out += [("device", k) for k in self.defs["devices"]]
        return out


def _deep_merge(base, over):
    out = copy.deepcopy(base)
    for k, v in over.items():
        if k in ("extends", "override"):
            continue
        if isinstance(v, dict) and isinstance(out.get(k), dict):
            out[k] = _deep_merge(out[k], v)
        else:
            out[k] = copy.deepcopy(v)
    return out

"""アイテムバー（画面下の9枠）とインベントリの中身（アイテムの並べ方・説明）。
画面は main/client_rs/screens.py の Inventory。
"""
from .. import log

L = log.get("inventory")

SLOTS = 9
DEFAULT_HOTBAR = [("material", "steel"), ("material", "aluminum"), ("material", "wood"), ("material", "glass"),
                  ("material", "rock"), ("device", "thruster"), ("device", "tank"), ("device", "pipe"),
                  ("device", "computer")]
TYPE_NAMES = {
    "thruster": "推進機", "tank": "タンク", "pipe": "配管", "valve": "バルブ", "battery": "バッテリー",
    "generator": "発電機", "solar": "太陽光パネル", "crank": "手回し発電機", "computer": "コンピューター", "actuator": "アクチュエーター",
    "processor": "精錬機・加工機", "seat": "操縦席", "air_vent": "通気口", "antenna": "アンテナ",
    "anchor": "アンカー（構築物を惑星に固定）", "light": "ライト（電力で光る）",
}


def valid_item(reg, item):
    if not item:
        return False
    kind, id_ = item
    return bool(reg.material(id_) if kind == "material" else reg.device(id_))


def ensure_hotbar(player, reg):
    """アイテムバーを9枠にそろえ、無くなったアイテム（MOD を外した等）を空にする。"""
    hb = list(player.hotbar) if player.hotbar else None
    if hb is None:
        pal = reg.palette()
        hb = [it for it in DEFAULT_HOTBAR if valid_item(reg, it)]
        for it in pal:
            if len(hb) >= SLOTS:
                break
            if it not in hb:
                hb.append(it)
    hb = [tuple(x) if x and valid_item(reg, tuple(x)) else None for x in hb][:SLOTS]
    hb += [None] * (SLOTS - len(hb))
    player.hotbar = hb
    return hb


def item_details(reg, item):
    kind, id_ = item
    if kind == "material":
        m = reg.material(id_)
        lines = [f"素材: {m['name']}",
                 f"密度 {m['density']:.0f} kg/m³　硬さ {m['stiffness']:.2f}　ちぎれにくさ {m['break_distance']:.2f}"]
        extra = []
        if m.get("plasticity", 0) >= 0.5:
            extra.append("曲がったまま戻りにくい")
        if m.get("rebond"):
            extra.append("離れても近づくと再びくっつく")
        if m.get("density", 0) < 1000:
            extra.append("水に浮く")
        if extra:
            lines.append("・" + "・".join(extra))
    else:
        d = reg.device(id_)
        lines = [f"機器: {d['name']}（{TYPE_NAMES.get(d.get('mark') or d['type'], d['type'])}）"]
        pw = {"none": "電力なし", "manual": "手動", "electric": f"電力 {d.get('power_use', 0):.0f} W"}.get(
            d.get("power"), "")
        mat = (reg.material(d.get("material")) or {}).get("name", d.get("material"))
        info = [pw, f"素材 {mat}"]
        if d.get("directional"):
            info.append("向きあり（R で変更）")
        if d.get("pipe_port"):
            info.append("配管につながる")
        lines.append("　".join(x for x in info if x))
        p = d.get("params", {})
        if d["type"] == "generator":
            fr = float(p.get("fuel_rate", 0))
            lines.append(f"発電 {float(p.get('output', 0)) / 1000:.1f} kW" +
                         (f"（{reg.res_name(p.get('fuel', 'fuel'))} {fr:g} kg/秒）" if fr > 0 else "（燃料いらず）"))
        if d["type"] == "processor":
            for r in p.get("recipes", []):
                pw = float(r.get("power", d.get("power_use", 0))) / 1000
                lines.append(f"・{reg.res_text(r.get('in', {}))} → {reg.res_text(r.get('out', {}))}"
                             f"（{float(r.get('time', 1)):g}秒・{pw:g} kW）")
        if d.get("_source") and d.get("_source") != "base":
            lines.append(f"MOD: {d['_source']}")
    return lines

"""MOD の情報（mods/<MOD名>/mod.json）と、MOD どうしの依存関係。

mod.json の項目：
    id / name / version / description
    builtin     本体に入っている MOD（一覧で先に並べる）
    requires    一緒に読み込む必要がある MOD の ID のリスト（無ければ読み込みのときに足す）
    world       この MOD を選んで「新しいワールド」を作ると、ワールドをこの形で作る（シナリオなど）：
                {"planets": [...] または "bodies": "default", "settings": {world.json に足す項目}, "note": "説明"}
                すでにあるワールドに後から入れても、ワールドの形は変わらない
    category    一覧の分け方（"シナリオ"・"ブロック"・"登場人物"・"場所"・"見た目" など。省略可）
"""
import json

from . import log, paths

L = log.get("modmeta")


def read(mod_id, mods_dir=None):
    """mod.json の中身（無い・読めない MOD は最低限の項目だけ）。"""
    d = (mods_dir or paths.MODS) / mod_id
    meta = {"id": mod_id, "name": mod_id, "description": "", "requires": [], "builtin": False}
    mj = d / "mod.json"
    if mj.exists():
        try:
            raw = json.loads(mj.read_text(encoding="utf-8"))
            if isinstance(raw, dict):
                meta.update(raw)
        except Exception as e:  # noqa: BLE001
            meta["error"] = f"mod.json が読めません: {e}"
    meta["id"] = mod_id                       # フォルダの名前が ID
    req = meta.get("requires") or []
    meta["requires"] = [r for r in req if isinstance(r, str) and r != mod_id] if isinstance(req, list) else []
    meta["exists"] = d.is_dir()
    return meta


def list_all(mods_dir=None):
    """mods/ にある MOD の情報のリスト。本体に入っている MOD を先に、その中ではサバイバルを先頭に。"""
    root = mods_dir or paths.MODS
    out = []
    if not root.exists():
        return out
    for d in sorted(root.iterdir()):
        if d.is_dir() and not d.name.startswith(".") and not d.name.startswith("_"):
            out.append(read(d.name, root))
    out.sort(key=lambda m: (not m.get("builtin"), m["id"] != "survival", str(m.get("category", "")), m["id"]))
    return out


def resolve(enabled, mods_dir=None):
    """選んだ MOD に、依存する MOD を足して読み込む順に並べる（依存する方が先）。
    戻り値：(並べた MOD のリスト, 足した MOD のリスト, 見つからない MOD の [(MOD, 必要とした MOD)] )"""
    order, added, missing = [], [], []
    visiting = set()
    chosen = list(dict.fromkeys(enabled))

    def visit(mid, parent):
        if mid in order:
            return
        meta = read(mid, mods_dir)
        if not meta["exists"]:
            if parent is not None:
                missing.append((mid, parent))
            elif mid not in order:
                order.append(mid)            # 選ばれているが無い：読み込みのときにエラーにする
            return
        if mid in visiting:                  # 循環：そのまま並べる
            return
        visiting.add(mid)
        for r in meta["requires"]:
            visit(r, mid)
        visiting.discard(mid)
        if mid not in order:
            order.append(mid)
            if mid not in chosen:
                added.append(mid)
    for m in chosen:
        visit(m, None)
    return order, added, missing


def dependents(mod_id, selected, mods_dir=None):
    """selected の中で mod_id を（間接的にでも）必要とする MOD。"""
    out = []
    for m in selected:
        if m == mod_id:
            continue
        order, _, _ = resolve([m], mods_dir)
        if mod_id in order:
            out.append(m)
    return out


def name_of(mod_id, mods_dir=None):
    return read(mod_id, mods_dir).get("name", mod_id)


def world_preset(mods, mods_dir=None):
    """選んだ MOD のうち、ワールドの形を決めるもの（最初の1つ）：(MOD の ID, world の設定) か (None, None)。"""
    for m in mods:
        w = read(m, mods_dir).get("world")
        if isinstance(w, dict):
            return m, w
    return None, None

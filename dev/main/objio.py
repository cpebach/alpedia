"""構築物の OBJ 出力・読み込み。

出力すると次の3ファイルができる:
  <名前>.obj       形状（素材ごとに usemtl でグループ分け）
  <名前>.mtl       他ソフトで見るための色
  <名前>.alp.json  素材割り当て（usemtl名 → ゲーム内素材ID）と、ボクセル・機器の完全なデータ

読み込み時:
  - .alp.json に "voxels" があればそれをそのまま使う（ゲーム内で作ったものの共有）
  - 無ければ OBJ をボクセル化し、"materials" の対応表で素材を割り当てる
    （Blender 等で作った OBJ に .alp.json を自分で書けば取り込める）
"""
import json
from pathlib import Path

import numpy as np

from . import log
from .construct import _clean_dev

L = log.get("obj")


def _script_files(devs):
    """機器が使うプログラムのファイル名（コンピューターの .py とブロックの .blocks.json）。"""
    out = set()
    for d in devs:
        if not d:
            continue
        st = d.get("state", {})
        for k in ("script", "blocks"):
            f = st.get(k)
            if isinstance(f, str) and f and "/" not in f and "\\" not in f and ".." not in f:
                out.add(f)
    return sorted(out)


def export_construct(c, out_dir: Path, name: str, scripts_dir: Path = None):
    out_dir.mkdir(parents=True, exist_ok=True)
    R, t = c.rigid_fit()
    from . import shape
    mesh = shape.build_mesh(c)
    lines = [f"# Alpedia construct: {c.name}", f"mtllib {name}.mtl", f"o {name}"]
    if mesh is not None:
        # 見た目どおりの丸い形（球＋結合部を埋めた形）で出力
        mv = (shape.skin(c, mesh) - t) @ R
        face_mat = [c.mats[o] for o in mesh["owner"][mesh["faces"][:, 0]]]
        used = sorted(set(face_mat))
        for v in mv:
            lines.append(f"v {v[0]:.5f} {v[1]:.5f} {v[2]:.5f}")
        for m in used:
            lines.append(f"usemtl {m}")
            for f, fm in zip(mesh["faces"], face_mat):
                if fm == m:
                    lines.append("f " + " ".join(str(int(k) + 1) for k in f))
    else:
        used = []                                 # ブロックが無い
    obj_path = out_dir / f"{name}.obj"
    obj_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    mtl = []
    for m in used:
        col = c.reg.material(m)["color"]
        mtl += [f"newmtl {m}", f"Kd {col[0]:.3f} {col[1]:.3f} {col[2]:.3f}", ""]
    (out_dir / f"{name}.mtl").write_text("\n".join(mtl), encoding="utf-8")
    meta = {
        "format": "alpedia-construct", "version": 1, "name": c.name, "obj": f"{name}.obj",
        "voxel_size": c.size, "scale": 1.0,
        "materials": {m: m for m in used}, "default_material": used[0] if used else "steel",
        "voxels": [[cl[0], cl[1], cl[2], m, _clean_dev(d, c.reg)] for cl, m, d in zip(c.cells, c.mats, c.devs)],
        "links": [[list(a), list(b)] for a, b in sorted(c.links)],
        "storage": c.storage,
    }
    # 使っているプログラムも一緒に入れておく（別のワールドや他の人に渡しても動くように）
    if scripts_dir is not None:
        scripts = {}
        for f in _script_files(c.devs):
            fp = scripts_dir / f
            if fp.exists():
                scripts[f] = fp.read_text(encoding="utf-8")
                if f.endswith(".blocks.json"):   # ブロックから変換した .py も
                    py = scripts_dir / (f[:-len(".blocks.json")] + ".py")
                    if py.exists():
                        scripts[py.name] = py.read_text(encoding="utf-8")
        if scripts:
            meta["scripts"] = scripts
    (out_dir / f"{name}.alp.json").write_text(json.dumps(meta, ensure_ascii=False, indent=1), encoding="utf-8")
    L.info("OBJ 出力: %s（ボクセル %d、面 %d）", obj_path, c.n_vox, len(c.face_idx))
    return obj_path


def parse_obj(path: Path):
    verts, tris, tri_mat = [], [], []
    cur = "default"
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        parts = line.strip().split()
        if not parts:
            continue
        if parts[0] == "v" and len(parts) >= 4:
            verts.append([float(x) for x in parts[1:4]])
        elif parts[0] == "usemtl":
            cur = parts[1] if len(parts) > 1 else "default"
        elif parts[0] == "f":
            idx = []
            for p in parts[1:]:
                k = int(p.split("/")[0])
                idx.append(k - 1 if k > 0 else len(verts) + k)
            for j in range(1, len(idx) - 1):
                tris.append([idx[0], idx[j], idx[j + 1]])
                tri_mat.append(cur)
    return np.asarray(verts, dtype=np.float64), np.asarray(tris, dtype=np.int64), tri_mat


def voxelize(verts, tris, tri_mat, size, mapping, default_material, max_cells=200000):
    """閉じたメッシュの内部を判定してボクセル化（x方向レイの交差回数）。"""
    if not len(tris):
        return []
    lo = verts.min(0)
    hi = verts.max(0)
    n = np.maximum(np.ceil((hi - lo) / size).astype(int), 1)
    if np.prod(n) > max_cells:
        raise ValueError(f"大きすぎます（{n.tolist()} セル）。voxel_size か scale を調整してください")
    T = verts[tris]  # (t,3,3)
    out = []
    eps = 1e-4
    for j in range(n[1]):
        y = lo[1] + (j + 0.5) * size + eps
        for k in range(n[2]):
            z = lo[2] + (k + 0.5) * size + eps * 1.7
            a, b, c = T[:, 0], T[:, 1], T[:, 2]
            # yz 平面での重心座標
            v0 = b[:, 1:] - a[:, 1:]
            v1 = c[:, 1:] - a[:, 1:]
            v2 = np.array([y, z]) - a[:, 1:]
            den = v0[:, 0] * v1[:, 1] - v1[:, 0] * v0[:, 1]
            ok = np.abs(den) > 1e-12
            den = np.where(ok, den, 1)
            u = (v2[:, 0] * v1[:, 1] - v1[:, 0] * v2[:, 1]) / den
            w = (v0[:, 0] * v2[:, 1] - v2[:, 0] * v0[:, 1]) / den
            inside = ok & (u >= 0) & (w >= 0) & (u + w <= 1)
            if not inside.any():
                continue
            ids = np.nonzero(inside)[0]
            xs = a[ids, 0] + u[ids] * (b[ids, 0] - a[ids, 0]) + w[ids] * (c[ids, 0] - a[ids, 0])
            order = np.argsort(xs)
            xs, ids = xs[order], ids[order]
            for e in range(0, len(xs) - 1, 2):
                x0, x1 = xs[e], xs[e + 1]
                mat = mapping.get(tri_mat[ids[e]], default_material)
                i0 = max(int(np.ceil((x0 - lo[0]) / size - 0.5)), 0)
                i1 = min(int(np.floor((x1 - lo[0]) / size - 0.5)), n[0] - 1)
                for i in range(i0, i1 + 1):
                    out.append(((i, j, k), mat, None))
    return out


def _resolve(p: Path):
    """(.obj / .alp.json のどちらか) → (meta, obj_path, meta_path)"""
    p = Path(p)
    if p.suffix.lower() == ".obj":
        meta_path = p.parent / (p.stem + ".alp.json")
        obj_path = p
    else:
        meta_path = p
        obj_path = None
    meta = {}
    if meta_path.exists():
        meta = json.loads(meta_path.read_text(encoding="utf-8"))
        if obj_path is None and meta.get("obj"):
            obj_path = meta_path.parent / meta["obj"]
    return meta, obj_path, meta_path


_obj_cache = {}


def _parse_cached(obj_path: Path):
    st = obj_path.stat()
    key = (str(obj_path), st.st_mtime, st.st_size)
    hit = _obj_cache.get(key)
    if hit is None:
        hit = parse_obj(obj_path)
        _obj_cache.clear()
        _obj_cache[key] = hit
    return hit


def inspect(registry, path: Path):
    """読み込みメニュー用の下調べ。
    kind: "voxels"（ゲーム内で作ったボクセルデータ）/ "obj"（外部の OBJ をボクセル化）"""
    meta, obj_path, meta_path = _resolve(path)
    name = meta.get("name", (obj_path or meta_path).stem)
    info = {"name": name, "meta": meta, "obj_path": obj_path, "meta_path": meta_path}
    if meta.get("voxels"):
        info["kind"] = "voxels"
        info["voxel_size"] = float(meta.get("voxel_size", 0.5))
        return info
    if obj_path is None or not obj_path.exists():
        raise FileNotFoundError(f"OBJ が見つかりません: {obj_path}")
    verts, tris, tri_mat = _parse_cached(obj_path)
    if not len(tris):
        raise ValueError("面（f）がありません。閉じた立体の OBJ が必要です")
    ext = verts.max(0) - verts.min(0)
    info.update(kind="obj", extent=ext, n_tris=len(tris), groups=sorted(set(tri_mat)),
                scale=float(meta.get("scale", 1.0)), voxel_size=meta.get("voxel_size"),
                default_material=meta.get("default_material"),
                mapping={k: v for k, v in meta.get("materials", {}).items() if registry.get("materials", v)})
    return info


def import_construct(registry, obj_or_json: Path, voxel_size=None, scale=None, default_material=None,
                     max_cells=200000):
    """戻り値: dict(name, size, voxels, links, storage)
    外部 OBJ のときは voxel_size（ボクセルの大きさ m）・scale（拡大率）・default_material を上書きできる。"""
    meta, obj_path, meta_path = _resolve(obj_or_json)
    size = float(meta.get("voxel_size", 1.0))
    name = meta.get("name", (obj_path or meta_path).stem)
    if meta.get("voxels"):
        vox = []
        for v in meta["voxels"]:
            mat = v[3] if registry.get("materials", v[3]) else meta.get("default_material", "steel")
            dev = v[4] if len(v) > 4 else None
            if dev and not registry.device(dev.get("def")):
                L.warning("未知の機器 %s を素材ボクセルに置き換えます", dev.get("def"))
                dev = None
            vox.append(((v[0], v[1], v[2]), mat, dev))
        links = None
        if meta.get("links"):
            from .construct import link_key
            links = {link_key(tuple(a), tuple(b)) for a, b in meta["links"]}
        L.info("読み込み（ボクセルデータ）: %s %d ボクセル", name, len(vox))
        scripts = dict(meta.get("scripts") or {})
        # 古い出力などでプログラムが入っていなければ、出力元のワールドの scripts から探す
        src = meta_path.parent.parent / "scripts"
        for f in _script_files([v[2] for v in vox]):
            if f not in scripts and (src / f).exists():
                scripts[f] = (src / f).read_text(encoding="utf-8")
                if f.endswith(".blocks.json") and (src / (f[:-12] + ".py")).exists():
                    scripts[f[:-12] + ".py"] = (src / (f[:-12] + ".py")).read_text(encoding="utf-8")
        return {"name": name, "size": size, "voxels": vox, "links": links, "storage": meta.get("storage", {}),
                "scripts": scripts}
    if obj_path is None or not obj_path.exists():
        raise FileNotFoundError(f"OBJ が見つかりません: {obj_path}")
    verts, tris, tri_mat = _parse_cached(obj_path)
    if voxel_size is not None:
        size = float(voxel_size)
    verts = verts * float(scale if scale is not None else meta.get("scale", 1.0))
    mapping = {k: v for k, v in meta.get("materials", {}).items() if registry.get("materials", v)}
    default = default_material or meta.get("default_material", "steel")
    if not registry.get("materials", default):
        default = "steel"
    vox = voxelize(verts, tris, tri_mat, size, mapping, default, max_cells=max_cells)
    L.info("読み込み（OBJボクセル化）: %s 三角形 %d → %d ボクセル", obj_path.name, len(tris), len(vox))
    return {"name": name, "size": size, "voxels": vox, "links": None, "storage": {}}


def list_sources(world_dir: Path, saves_dir: Path):
    """読み込めるファイルの一覧: [(グループ名, Path)]
    このワールドの imports・exports と、マルチプレイで出力したもの、ほかのワールドの exports。"""
    def scan(d):
        if not d.exists():
            return []
        objs = list(d.glob("*.obj"))
        js = [p for p in d.glob("*.alp.json") if not (d / (p.name[:-9] + ".obj")).exists()]
        return sorted(objs + js, key=lambda p: p.name.lower())
    out = [("imports", p) for p in scan(world_dir / "imports")]
    out += [("exports", p) for p in scan(world_dir / "exports")]
    mp = saves_dir / ".net" / "exports"         # マルチプレイで参加中に F9 で出力したもの（自分の PC）
    out += [("マルチプレイで出力", p) for p in scan(mp)]
    if saves_dir.exists():
        for wd in sorted(saves_dir.iterdir()):
            if wd.is_dir() and not wd.name.startswith(".") and wd.resolve() != world_dir.resolve():
                out += [(f"{wd.name} の exports", p) for p in scan(wd / "exports")]
    return out

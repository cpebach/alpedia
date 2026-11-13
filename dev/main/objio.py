"""構築物の OBJ 出力・読み込み。本体は Rust（alpedia_core の objio_*）。

出力すると次の3ファイルができる:
  <名前>.obj       形状（素材ごとに usemtl でグループ分け）
  <名前>.mtl       他ソフトで見るための色
  <名前>.alp.json  素材割り当て（usemtl名 → ゲーム内素材ID）と、ボクセル・機器の完全なデータ

読み込み時:
  - .alp.json に "voxels" があればそれをそのまま使う（ゲーム内で作ったものの共有）
  - 無ければ OBJ をボクセル化し、"materials" の対応表で素材を割り当てる
    （Blender 等で作った OBJ に .alp.json を自分で書けば取り込める）
"""
from . import log, native

_c = native.require()
L = log.get("obj")

_script_files = _c.objio_script_files
export_construct = _c.objio_export_construct
parse_obj = _c.objio_parse_obj
voxelize = _c.objio_voxelize
_resolve = _c.objio_resolve
_parse_cached = _c.objio_parse_cached
inspect = _c.objio_inspect
import_construct = _c.objio_import_construct
blueprint_dirs = _c.objio_blueprint_dirs
list_sources = _c.objio_list_sources

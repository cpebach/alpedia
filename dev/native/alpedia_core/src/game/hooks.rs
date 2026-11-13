#![allow(unused_variables)]
//! コアに機能を足す口（MOD のプログラム）。main/hooks.py の中身。
//!
//! コア（ワールド・構築物・物理）は、街・経済・資源のルールなどの具体的な中身を知らない。
//! MOD のプログラムは Addon を継承し、必要な口だけを上書きする（どれも何もしない既定の実装を持つ）。
//! 当たり判定に使う「動くかもしれない物体」（owner）は owner.is_moving() と owner.point_velocity(P) を持つ。

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyTuple};

/// 拡張の口
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Addon {}

#[pymethods]
impl Addon {
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(_args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        Addon {}
    }

    #[classattr]
    fn name() -> &'static str {
        "addon"
    }

    /// world.json に書く自分の項目の名前（保存形式の検査で使う）
    #[classattr]
    fn settings_keys(py: Python<'_>) -> Py<PyTuple> {
        PyTuple::empty_bound(py).unbind()
    }

    /// True の MOD が入っていると「クリエイティブ」ではなくなる（world.creative が False）
    #[classattr]
    fn resource_rules() -> bool {
        false
    }

    // --- 読み込み・保存 ---------------------------------------------------
    /// ワールドを読み込んだ直後（構築物・プレイヤーは読み込み済み）
    fn on_load(&self, world: PyObject) {}
    /// 保存する直前。world.settings に自分の状態を書く
    fn on_save(&self, world: PyObject) {}

    // --- 1ティックごと ----------------------------------------------------
    /// 物理計算の前。物理で動かさない（この拡張が位置を決める）構築物の ID の集合を返す
    fn before_physics<'py>(&self, py: Python<'py>, world: PyObject, dt: PyObject) -> PyResult<Bound<'py, PySet>> {
        PySet::empty_bound(py)
    }
    /// 範囲 (lo, hi)±margin にある、当たり判定用の箱：[(owner, C, A, E, 部品番号)]
    fn colliders<'py>(&self, py: Python<'py>, world: PyObject, lo: PyObject, hi: PyObject, margin: PyObject) -> Bound<'py, PyList> {
        PyList::empty_bound(py)
    }
    /// 物理計算の後。cons の各構築物の c.ext_events を処理する
    fn after_physics(&self, world: PyObject, cons: PyObject) {}

    // --- 問い合わせ・操作 -------------------------------------------------
    /// 視線などの交差：{"kind", "t", "point", "normal", ...} か None
    fn raycast(&self, world: PyObject, o: PyObject, dvec: PyObject, max_t: PyObject) -> Option<PyObject> {
        None
    }
    /// 球（プレイヤー）を押し出す：[(押し出し, 法線, owner)]
    fn push_sphere<'py>(&self, py: Python<'py>, world: PyObject, center: PyObject, r: PyObject) -> Bound<'py, PyList> {
        PyList::empty_bound(py)
    }
    /// E で構築物のブロック（機器以外）を触った。メッセージを返したら、そこで処理済み
    fn interact(&self, world: PyObject, c: PyObject, i: PyObject) -> Option<PyObject> {
        None
    }
    /// hit（この拡張の raycast の結果）の上に新しい構築物 c を置いた
    fn on_placed(&self, world: PyObject, hit: PyObject, c: PyObject) {}
    /// 景色の箱の触れる物を E で触った。メッセージを返したら、そこで処理済み
    fn on_scenery_talk(&self, world: PyObject, place: PyObject, tag: PyObject) -> Option<PyObject> {
        None
    }
    /// リスポーンで戻る場所（世界座標）。None なら決めない
    fn respawn_point(&self, world: PyObject, player: PyObject) -> Option<PyObject> {
        None
    }
    /// リスポーンした。メッセージを返すと画面に出す
    fn on_respawn(&self, world: PyObject, player: PyObject) -> Option<PyObject> {
        None
    }
    /// item を今使ってよいか（False で使えない、None なら決めない）
    fn item_allowed(&self, world: PyObject, item: PyObject) -> Option<PyObject> {
        None
    }
    /// F8 の「設計図」を出してよいか（False で出さない、None なら決めない）
    fn blueprints_allowed(&self, world: PyObject) -> Option<PyObject> {
        None
    }
    /// ブロックプログラムのパレットで出してよい種類の ID の集合（None なら全部）
    fn block_categories(&self, world: PyObject) -> Option<PyObject> {
        None
    }
    /// ブロックプログラムのパレットにその文ブロックを出してよいか（False で出さない、None なら決めない）
    fn block_allowed(&self, world: PyObject, kind: PyObject) -> Option<PyObject> {
        None
    }
    /// ブロックプログラムの見本（完成形）。None なら出さない
    fn block_hint(&self, world: PyObject, blocks_file: PyObject) -> Option<PyObject> {
        None
    }
    /// 人物（NPC）に E で話しかけた。メッセージを返したら、そこで処理済み
    fn on_npc_talk(&self, world: PyObject, key: PyObject) -> Option<PyObject> {
        None
    }
    /// E で機器を触った（コアの操作より先に呼ばれる）。メッセージを返したら、そこで処理済み
    fn interact_device(&self, world: PyObject, c: PyObject, i: PyObject, d: PyObject, dd: PyObject) -> Option<PyObject> {
        None
    }

    // --- 資源のルール（サバイバルなど） ------------------------------------
    /// ブロックを置く・読み込む費用。払えなければ False（置かない）
    fn pay(&self, world: PyObject, cost: PyObject) -> bool {
        true
    }
    /// ブロックを外したときの返却
    fn refund(&self, world: PyObject, cost: PyObject) {}
    /// プレイヤーが機器 i を置いた
    fn on_device_placed(&self, world: PyObject, c: PyObject, i: PyObject) {}
    /// 地面を掘った（resource はその場所の資源）
    fn on_dig(&self, world: PyObject, hit: PyObject, resource: PyObject) {}
    /// 水面をクリックした。使ったら True
    fn on_water(&self, world: PyObject, hit: PyObject) -> bool {
        false
    }
    /// ジェットパックを使えるか：(使えるか, 噴射の向き)
    fn jetpack(&self, py: Python<'_>, world: PyObject, player: &Bound<'_, PyAny>, dt: PyObject, thrust_dir: PyObject) -> PyResult<(PyObject, PyObject)> {
        let _ = py;
        Ok((player.getattr("jetpack")?.unbind(), thrust_dir))
    }

    // --- マルチプレイ（LAN） -------------------------------------------------
    /// ホスト：参加者が初めて（または戻って）入った（world.player はその人）
    fn on_player_join(&self, world: PyObject, player: PyObject) {}
    /// ホスト：参加者へ送るこの拡張の状態（JSON で書けるもの）。None なら送らない
    fn net_state(&self, world: PyObject) -> Option<PyObject> {
        None
    }
    /// 参加者：net_state で送られた状態を受け取った
    fn net_apply(&self, world: PyObject, state: PyObject) {}
    /// 参加者：毎フレーム。次の状態が届くまで、動いている物を進めておく
    fn net_client_tick(&self, world: PyObject, dt: PyObject) {}
    /// world.addon_call の引数を相手に送れる形にする（この拡張の物なら (種類, ID)、そうでなければ None）
    fn net_ref_of(&self, obj: PyObject) -> Option<PyObject> {
        None
    }
    /// net_ref_of の逆（ホストで、種類と ID から物を探す）
    fn net_ref(&self, world: PyObject, kind: PyObject, id: PyObject) -> Option<PyObject> {
        None
    }
}

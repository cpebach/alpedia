//! ゲームの決まりごと（main/ の Python から Rust に移したもの）。Python 側の同じ名前のモジュールは、ここを呼ぶだけの薄い入り口。

pub mod devstate;
pub mod savefmt;
pub mod airtight;
pub mod radio;
pub mod registry;
pub mod blocks;
pub mod actuator;
pub mod buildkit;
pub mod player;
pub mod scenery;
pub mod hooks;
pub mod survival;
pub mod control;
pub mod scripting;
pub mod modmeta;
pub mod devices;
pub mod construct;

use pyo3::prelude::*;

/// モジュールを読み込む（builtins.__import__ を通さない。ゲーム内スクリプトの実行中は __import__ が制限されているため）
pub fn imp<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyModule>> {
    let n = std::ffi::CString::new(name).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    unsafe {
        let top = pyo3::ffi::PyImport_ImportModuleLevel(n.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0);
        Bound::from_owned_ptr_or_err(py, top)?;          // パッケージの先頭が返る。読み込んだ本体は sys.modules から
        let mods = Bound::from_borrowed_ptr(py, pyo3::ffi::PyImport_GetModuleDict());
        Ok(mods.get_item(name)?.downcast_into::<PyModule>()?)
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<construct::Construct>()?;
    m.add_function(wrap_pyfunction!(construct::construct_link_key, m)?)?;
    m.add_function(wrap_pyfunction!(construct::construct_dir_index, m)?)?;
    m.add_function(wrap_pyfunction!(construct::construct_clean_dev, m)?)?;
    m.add_function(wrap_pyfunction!(construct::construct_native_planets, m)?)?;
    m.add_function(wrap_pyfunction!(devstate::devstate_normalize, m)?)?;
    m.add_function(wrap_pyfunction!(devstate::devstate_fill, m)?)?;
    m.add_function(wrap_pyfunction!(devstate::devstate_to_saved, m)?)?;
    m.add_function(wrap_pyfunction!(devstate::devstate_check, m)?)?;
    m.add_function(wrap_pyfunction!(devstate::devstate_fields, m)?)?;
    m.add_function(wrap_pyfunction!(savefmt::savefmt_upgrade_world, m)?)?;
    m.add_function(wrap_pyfunction!(savefmt::savefmt_check_world, m)?)?;
    m.add_function(wrap_pyfunction!(savefmt::savefmt_check_construct, m)?)?;
    m.add_function(wrap_pyfunction!(savefmt::savefmt_check_spawn, m)?)?;
    m.add_function(wrap_pyfunction!(savefmt::savefmt_check_player, m)?)?;
    m.add_function(wrap_pyfunction!(airtight::airtight_find_cavities, m)?)?;
    m.add_function(wrap_pyfunction!(airtight::airtight_pressure_kpa, m)?)?;
    m.add_function(wrap_pyfunction!(airtight::airtight_air_for_pressure, m)?)?;
    m.add_class::<radio::RadioBus>()?;
    m.add_function(wrap_pyfunction!(radio::radio_antenna_info, m)?)?;
    m.add("RadioError", m.py().get_type_bound::<radio::RadioError>())?;
    m.add_class::<registry::Registry>()?;
    m.add_function(wrap_pyfunction!(registry::registry_defaults, m)?)?;
    m.add_function(wrap_pyfunction!(registry::registry_deep_merge, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_value_kind, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_fits, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_label_parts, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_fmt_num, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_summary, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_expr_children, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_slot_kind, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_new_stmt, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_find_hint, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_hint_text, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_gen_expr, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_gen_stmt, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_gen_body, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_has_wait, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_gen_sequence, m)?)?;
    m.add_function(wrap_pyfunction!(blocks::blocks_to_python, m)?)?;
    m.add("BLOCKS_HEADER", blocks::HEADER)?;
    m.add_function(wrap_pyfunction!(actuator::act_motion, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_actions_for, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_default_preset, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_migrate_device, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_check_action, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_hinge_sides, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_default_hinge_side, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_hinge_pair, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_is_hinge_dev, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_hinge_side, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_hinge_of, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_side_toward, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_mode_name, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_is_actuator, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_stroke, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_output_cell, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_is_attached, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_busy, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_cmd_rotate, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_cmd_extend, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_cmd_detach, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_face_info, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_measure_twist, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_sync_angle, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_output_angle, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_execute_preset, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_preset_text, m)?)?;
    m.add_function(wrap_pyfunction!(actuator::act_step, m)?)?;
    m.add("MotionError", m.py().get_type_bound::<actuator::MotionError>())?;
    m.add_function(wrap_pyfunction!(buildkit::bk_root_cells, m)?)?;
    m.add_function(wrap_pyfunction!(buildkit::bk_anchor_cells, m)?)?;
    m.add_function(wrap_pyfunction!(buildkit::bk_anchored, m)?)?;
    m.add_function(wrap_pyfunction!(buildkit::bk_anchored_inside, m)?)?;
    m.add_function(wrap_pyfunction!(buildkit::bk_anchor_center, m)?)?;
    m.add_function(wrap_pyfunction!(buildkit::bk_grounded, m)?)?;
    m.add_function(wrap_pyfunction!(buildkit::bk_light_devices, m)?)?;
    m.add_function(wrap_pyfunction!(buildkit::bk_set_light_group, m)?)?;
    m.add_class::<buildkit::Kit>()?;
    m.add_class::<player::Player>()?;
    m.add_class::<scenery::Map>()?;
    m.add_class::<scenery::Scenery>()?;
    m.add_function(wrap_pyfunction!(scenery::scenery_unit, m)?)?;
    m.add_function(wrap_pyfunction!(scenery::scenery_colliders, m)?)?;
    m.add_function(wrap_pyfunction!(scenery::scenery_raycast, m)?)?;
    m.add_function(wrap_pyfunction!(scenery::scenery_push_sphere, m)?)?;
    m.add_class::<hooks::Addon>()?;
    m.add_class::<survival::SurvivalRules>()?;
    m.add_function(wrap_pyfunction!(survival::survival_mode_of, m)?)?;
    m.add_class::<control::Port>()?;
    m.add("PortError", m.py().get_type_bound::<control::PortError>())?;
    m.add_class::<scripting::DeviceHandle>()?;
    m.add_class::<scripting::ScriptAPI>()?;
    m.add_class::<scripting::ScriptRunner>()?;
    m.add_function(wrap_pyfunction!(scripting::script_path, m)?)?;
    m.add_function(wrap_pyfunction!(scripting::safe_import, m)?)?;
    m.add_function(wrap_pyfunction!(scripting::display_payload, m)?)?;
    m.add_function(wrap_pyfunction!(modmeta::modmeta_read, m)?)?;
    m.add_function(wrap_pyfunction!(modmeta::modmeta_list_all, m)?)?;
    m.add_function(wrap_pyfunction!(modmeta::modmeta_is_scenario, m)?)?;
    m.add_function(wrap_pyfunction!(modmeta::modmeta_resolve, m)?)?;
    m.add_function(wrap_pyfunction!(modmeta::modmeta_dependents, m)?)?;
    m.add_function(wrap_pyfunction!(modmeta::modmeta_name_of, m)?)?;
    m.add_function(wrap_pyfunction!(modmeta::modmeta_world_preset, m)?)?;
    m.add_function(wrap_pyfunction!(modmeta::config_merge, m)?)?;
    m.add_function(wrap_pyfunction!(modmeta::config_load, m)?)?;
    m.add_function(wrap_pyfunction!(devices::dev_rebuild_networks, m)?)?;
    m.add_function(wrap_pyfunction!(devices::dev_is_fluid, m)?)?;
    m.add_function(wrap_pyfunction!(devices::dev_valve_opening, m)?)?;
    m.add_function(wrap_pyfunction!(devices::dev_proc_outputs, m)?)?;
    m.add_function(wrap_pyfunction!(devices::dev_proc_inputs, m)?)?;
    m.add_function(wrap_pyfunction!(devices::dev_tanks_in, m)?)?;
    m.add_function(wrap_pyfunction!(devices::dev_fluid_amount, m)?)?;
    m.add_function(wrap_pyfunction!(devices::dev_storage_total, m)?)?;
    m.add_function(wrap_pyfunction!(devices::dev_world_axis, m)?)?;
    m.add_function(wrap_pyfunction!(devices::dev_update, m)?)?;
    m.add_function(wrap_pyfunction!(devices::dev_cavity_report, m)?)?;
    m.add_function(wrap_pyfunction!(devices::dev_interact, m)?)?;
    m.add_class::<devices::Networks>()?;
    m.add("SAVE_FORMAT", savefmt::FORMAT)?;
    Ok(())
}

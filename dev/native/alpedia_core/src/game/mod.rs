//! ゲームの決まりごと（main/ の Python から Rust に移したもの）。Python 側の同じ名前のモジュールは、ここを呼ぶだけの薄い入り口。

pub mod devstate;
pub mod savefmt;
pub mod airtight;
pub mod radio;

use pyo3::prelude::*;

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
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
    m.add("SAVE_FORMAT", savefmt::FORMAT)?;
    Ok(())
}

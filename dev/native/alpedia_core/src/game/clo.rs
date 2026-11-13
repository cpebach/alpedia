//! Rust の関数（クロージャ）を Python から呼べる形にする（ボタンの on_click・画面の値の変化など）。

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;
type F = Box<dyn Fn(&Bound<'_, PyTuple>, Option<&Bound<'_, PyDict>>) -> R<PyObject>>;

#[pyclass(module = "alpedia_core", name = "_Closure", unsendable)]
pub struct Closure {
    f: F,
}

#[pymethods]
impl Closure {
    #[pyo3(signature = (*args, **kw))]
    fn __call__(&self, args: &Bound<'_, PyTuple>, kw: Option<&Bound<'_, PyDict>>) -> R<PyObject> {
        (self.f)(args, kw)
    }
}

/// 引数を受け取って値を返す関数
pub fn clo<'py>(py: Python<'py>, f: impl Fn(&Bound<'_, PyTuple>) -> R<PyObject> + 'static) -> R<B<'py>> {
    Ok(Bound::new(py, Closure { f: Box::new(move |a, _k| f(a)) })?.into_any())
}
/// 引数と名前付きの引数を受け取って値を返す関数
pub fn clokw<'py>(py: Python<'py>, f: impl Fn(&Bound<'_, PyTuple>, Option<&Bound<'_, PyDict>>) -> R<PyObject> + 'static) -> R<B<'py>> {
    Ok(Bound::new(py, Closure { f: Box::new(f) })?.into_any())
}
/// 引数なし・戻り値なし
pub fn clo0<'py>(py: Python<'py>, f: impl Fn(Python<'_>) -> R<()> + 'static) -> R<B<'py>> {
    clo(py, move |a| {
        let py = a.py();
        f(py)?;
        Ok(py.None())
    })
}
/// 引数1つ・戻り値なし
pub fn clo1<'py>(py: Python<'py>, f: impl Fn(&B<'_>) -> R<()> + 'static) -> R<B<'py>> {
    clo(py, move |a| {
        let py = a.py();
        f(&a.get_item(0)?)?;
        Ok(py.None())
    })
}
/// 引数1つ・文字を返す
pub fn clos<'py>(py: Python<'py>, f: impl Fn(&B<'_>) -> R<String> + 'static) -> R<B<'py>> {
    clo(py, move |a| {
        let py = a.py();
        Ok(f(&a.get_item(0)?)?.into_py(py))
    })
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    m.add_class::<Closure>()?;
    Ok(())
}

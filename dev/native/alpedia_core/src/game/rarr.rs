//! Rust が持つ配列（A-23）。構築物・人の位置・速度・力などの数の並びは、Python の numpy の配列ではなく Rust のメモリに置く。
//! Python には、そのメモリをそのまま見る numpy の配列（写しではない）を渡すので、MOD やテストは今まで通り `c.pos[...]` で読み書きできる。
//! Rust の側は Python のロック（GIL）を持たずに `Arc<Store>` からじかに読み書きでき、構築物ごとに並べて計算できる。

use std::cell::UnsafeCell;
use std::sync::Arc;

use numpy::ndarray::{ArrayViewMutD, IxDyn};
use numpy::{Element, PyArrayDyn, PyArrayMethods, PyUntypedArrayMethods};
use pyo3::prelude::*;

/// 数の並びの入れ物（形つき）。中身は Rust のメモリ
pub struct Store<T> {
    data: UnsafeCell<Box<[T]>>,
    shape: Vec<usize>,
}

// 中身を同時に書き換えないのは使う側の約束（今までの numpy の配列をロックの外で読み書きしていたのと同じ）
unsafe impl<T: Send> Send for Store<T> {}
unsafe impl<T: Send> Sync for Store<T> {}

impl<T> Store<T> {
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }
    pub fn len(&self) -> usize {
        self.slice().len()
    }
    /// 読むだけ
    pub fn slice(&self) -> &[T] {
        unsafe { &*self.data.get() }
    }
    /// 書き換える。同じ入れ物を同時に書き換えるスレッドが無いことは使う側が守る
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn slice_mut(&self) -> &mut [T] {
        unsafe { &mut *self.data.get() }
    }
    fn ptr(&self) -> *mut T {
        unsafe { (&mut *self.data.get()).as_mut_ptr() }
    }
}

/// numpy の配列の持ち主（Python から配列が見えている間、Rust のメモリを生かしておく）
#[pyclass(module = "alpedia_core", name = "RustArrayOwner")]
pub struct Owner {
    _keep: Arc<dyn std::any::Any + Send + Sync>,
}

/// Rust が持つ配列と、それを見る numpy の配列（いつも同じ物を返す）
pub struct RArr<T: Element> {
    pub store: Arc<Store<T>>,
    view: Py<PyArrayDyn<T>>,
}

impl<T: Element + Copy + Send + Sync + 'static> RArr<T> {
    pub fn from_vec(py: Python<'_>, v: Vec<T>, shape: &[usize]) -> PyResult<Self> {
        let n: usize = shape.iter().product();
        if n != v.len() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!("配列の形 {:?} と数 {} が合いません", shape, v.len())));
        }
        let store = Arc::new(Store { data: UnsafeCell::new(v.into_boxed_slice()), shape: shape.to_vec() });
        let owner = Bound::new(py, Owner { _keep: store.clone() })?;
        let view = unsafe { ArrayViewMutD::from_shape_ptr(IxDyn(shape), store.ptr()) };
        let arr = unsafe { PyArrayDyn::<T>::borrow_from_array_bound(&view, owner.into_any()) };
        Ok(RArr { store, view: arr.unbind() })
    }
    /// numpy の配列（など）の中身を写して作る
    pub fn copy_of(ob: &Bound<'_, PyAny>) -> PyResult<Self> {
        let a = ob.downcast::<PyArrayDyn<T>>()?;
        let shape = a.shape().to_vec();
        let v: Vec<T> = a.to_owned_array().iter().copied().collect();
        Self::from_vec(ob.py(), v, &shape)
    }
    pub fn zeros(py: Python<'_>, shape: &[usize]) -> PyResult<Self>
    where
        T: Default,
    {
        Self::from_vec(py, vec![T::default(); shape.iter().product()], shape)
    }
    /// Python から見える numpy の配列
    #[inline]
    pub fn bind<'py>(&self, py: Python<'py>) -> &Bound<'py, PyArrayDyn<T>> {
        self.view.bind(py)
    }
    /// numpy の配列（Py。同じメモリを見る）
    #[inline]
    pub fn np(&self, py: Python<'_>) -> Py<PyArrayDyn<T>> {
        self.view.clone_ref(py)
    }
    #[inline]
    pub fn clone_ref(&self, py: Python<'_>) -> Self {
        RArr { store: self.store.clone(), view: self.view.clone_ref(py) }
    }
    #[inline]
    pub fn into_any(self) -> PyObject {
        self.view.into_any()
    }
}

impl<T: Element + Copy + Send + Sync + 'static> Clone for RArr<T> {
    fn clone(&self) -> Self {
        Python::with_gil(|py| self.clone_ref(py))
    }
}

impl<T: Element + Copy + Send + Sync + 'static> IntoPy<PyObject> for RArr<T> {
    fn into_py(self, _py: Python<'_>) -> PyObject {
        self.view.into_any()
    }
}

impl<T: Element + Copy + Send + Sync + 'static> IntoPy<PyObject> for &RArr<T> {
    fn into_py(self, py: Python<'_>) -> PyObject {
        self.view.clone_ref(py).into_any()
    }
}

impl<T: Element + Copy + Send + Sync + 'static> ToPyObject for RArr<T> {
    fn to_object(&self, py: Python<'_>) -> PyObject {
        self.view.clone_ref(py).into_any()
    }
}

/// Python の値から：この型の numpy の配列なら中身を写して Rust の配列にする（同じ配列なら写さない）
impl<'py, T: Element + Copy + Send + Sync + 'static> FromPyObject<'py> for RArr<T> {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        // すでに Rust の配列を見ている numpy の配列（持ち主が Owner）なら、写さずに同じ入れ物を使う
        if let Ok(a) = ob.downcast::<PyArrayDyn<T>>() {
            let base = unsafe { (*a.as_array_ptr()).base };
            if !base.is_null() {
                let b = unsafe { Bound::from_borrowed_ptr(ob.py(), base) };
                if let Ok(o) = b.downcast::<Owner>() {
                    if let Ok(st) = o.borrow()._keep.clone().downcast::<Store<T>>() {
                        let full = unsafe { (*a.as_array_ptr()).data as *mut T } == st.ptr() && a.shape() == st.shape() && a.is_c_contiguous();
                        if full {
                            return Ok(RArr { store: st, view: a.clone().unbind() });
                        }
                    }
                }
            }
        }
        Self::copy_of(ob)
    }
}

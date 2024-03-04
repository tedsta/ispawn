use crate::{IntoLocalSpawner, Result, SpawnCompleter, SpawnCompleterBuilder};
use alloc::{alloc::Layout, boxed::Box};
use core::future::Future;

#[derive(Copy, Clone, Debug)]
pub struct DioxusSpawner;

impl IntoLocalSpawner for DioxusSpawner {
    unsafe fn into_handle(self) -> *const () {
        core::ptr::null()
    }

    unsafe fn spawn_dyn(
        _handle: *const (),
        builder: SpawnCompleterBuilder,
        future_layout: Layout,
    ) -> SpawnCompleter {
        let future_ptr = unsafe { alloc::alloc::alloc(future_layout) } as *mut ();
        let task_ptr = future_ptr;
        builder.build(task_ptr, future_ptr)
    }

    unsafe fn finish_spawn(
        _handle: *const (),
        task_ptr_as_dyn_future: *mut dyn Future<Output = ()>,
    ) -> Result<()> {
        let future_box: Box<dyn Future<Output = ()>> =
            unsafe { Box::from_raw(task_ptr_as_dyn_future) };
        let _ = dioxus::prelude::spawn(Box::into_pin(future_box));
        Ok(())
    }

    unsafe fn on_clone(_handle: *const ()) {}

    unsafe fn on_drop(_handle: *const ()) {}
}

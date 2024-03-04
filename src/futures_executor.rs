use crate::{IntoLocalSpawner, Result, SpawnCompleter, SpawnCompleterBuilder, SpawnError};
use alloc::{alloc::Layout, boxed::Box, rc::Rc};
use core::future::Future;

impl IntoLocalSpawner for Rc<futures_executor::LocalSpawner> {
    unsafe fn into_handle(self) -> *const () {
        Rc::into_raw(self) as *const ()
    }

    unsafe fn spawn_dyn(
        _: *const (),
        builder: SpawnCompleterBuilder,
        future_layout: Layout,
    ) -> SpawnCompleter {
        let future_ptr = unsafe { alloc::alloc::alloc(future_layout) } as *mut ();
        let task_ptr = future_ptr;
        builder.build(task_ptr, future_ptr)
    }

    unsafe fn finish_spawn(
        handle: *const (),
        task_ptr_as_dyn_future: *mut dyn Future<Output = ()>,
    ) -> Result<()> {
        use futures_task::LocalSpawn;

        let future_box: Box<dyn Future<Output = ()>> =
            unsafe { Box::from_raw(task_ptr_as_dyn_future) };
        let future_obj: futures_task::LocalFutureObj<()> = future_box.into();

        let this = unsafe { &*(handle as *const futures_executor::LocalSpawner) };
        this.spawn_local_obj(future_obj).map_err(|e| {
            if e.is_shutdown() {
                SpawnError::Shutdown
            } else {
                SpawnError::Other
            }
        })
    }

    unsafe fn on_clone(handle: *const ()) {
        unsafe {
            Rc::increment_strong_count(handle as *const futures_executor::LocalSpawner);
        }
    }

    unsafe fn on_drop(handle: *const ()) {
        unsafe {
            drop(Rc::from_raw(
                handle as *const futures_executor::LocalSpawner,
            ));
        }
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_futures_executor() {
        let mut ex = futures_executor::LocalPool::new();
        let spawner = ex.spawner();
        let spawner = crate::LocalSpawner::new(alloc::rc::Rc::new(spawner));

        let (result_tx, mut result_rx) = localq::mpsc::channel(1);
        spawner
            .spawn(async move {
                result_tx.try_send(42).unwrap();
            })
            .unwrap();

        let result = ex.run_until(async move { result_rx.recv().await });

        assert_eq!(result.unwrap(), 42);
    }
}

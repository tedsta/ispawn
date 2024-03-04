use crate::{IntoLocalSpawner, Result, SpawnCompleter, SpawnCompleterBuilder};
use alloc::{alloc::Layout, boxed::Box, rc::Rc};
use core::future::Future;

impl IntoLocalSpawner for Rc<tokio::task::LocalSet> {
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
        let future_box: Box<dyn Future<Output = ()>> =
            unsafe { Box::from_raw(task_ptr_as_dyn_future) };

        let this = unsafe { &*(handle as *const tokio::task::LocalSet) };
        let _ = this.spawn_local(Box::into_pin(future_box));

        Ok(())
    }

    unsafe fn on_clone(handle: *const ()) {
        unsafe { Rc::increment_strong_count(handle as *const tokio::task::LocalSet) }
    }

    unsafe fn on_drop(handle: *const ()) {
        unsafe {
            let _ = Rc::from_raw(handle as *const tokio::task::LocalSet);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_tokio_executor() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let ex = Rc::new(tokio::task::LocalSet::new());
        let spawner = crate::LocalSpawner::new(ex.clone());

        let (result_tx, mut result_rx) = localq::mpsc::channel(1);
        spawner
            .spawn(async move {
                result_tx.try_send(42).unwrap();
            })
            .unwrap();

        let result = ex.block_on(&rt, async move { result_rx.recv().await });

        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_tokio_executor_drop_before_spawner() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let ex = Rc::new(tokio::task::LocalSet::new());
        let spawner = crate::LocalSpawner::new(ex.clone());

        drop(ex);
        drop(rt);

        spawner.spawn(async move {}).unwrap();
    }
}

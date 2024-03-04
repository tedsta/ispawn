use crate::{IntoLocalSpawner, Result, SpawnCompleter, SpawnCompleterBuilder};
use alloc::{alloc::Layout, boxed::Box, rc::Rc};
use core::future::Future;

impl IntoLocalSpawner for Rc<async_executor::LocalExecutor<'static>> {
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

        let this = unsafe { &*(handle as *const async_executor::LocalExecutor<'static>) };
        this.spawn(Box::into_pin(future_box)).detach();

        Ok(())
    }

    unsafe fn on_clone(handle: *const ()) {
        unsafe {
            Rc::increment_strong_count(handle as *const async_executor::LocalExecutor<'static>)
        }
    }

    unsafe fn on_drop(handle: *const ()) {
        unsafe {
            let _ = Rc::from_raw(handle as *const async_executor::LocalExecutor<'static>);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_async_executor() {
        let ex = Rc::new(async_executor::LocalExecutor::new());
        let spawner = crate::LocalSpawner::new(ex.clone());

        let (result_tx, mut result_rx) = localq::mpsc::channel(1);
        spawner
            .spawn(async move {
                result_tx.try_send(42).unwrap();
            })
            .unwrap();

        let result = pollster::block_on(ex.run(async move { result_rx.recv().await }));

        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_async_executor_drop_before_spawner() {
        let ex = Rc::new(async_executor::LocalExecutor::new());
        let spawner = crate::LocalSpawner::new(ex.clone());

        drop(ex);

        spawner.spawn(async move {}).unwrap();
    }
}

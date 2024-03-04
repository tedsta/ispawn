//! A low-cost, `no_std` abstraction over `Future` spawners that erases the underlying spawn
//! handle's type. This allows one to write libraries that spawn futures in an executor-agnostic way
//! without using viral type parameters throughout.
//!
//! The main idea of this crate is that if async executors are willing to integrate with it, the
//! type-erased spawners in this crate will be able to spawn without additional allocations (though
//! the underling executor-specific spawner likely does allocate on each spawn). While spawning is
//! typically a relatively infrequent operation, and thus is not particularly performance-sensitive,
//! boxing a future before spawning would incur an extra pointer indirection and virtual dispatch
//! every time the future is polled.
//!
//! Currently the hypothetical integrations described in the previous paragraph do not exist - we
//! directly implement shim wrappers for Rc-wrapped spawners which *do* incur the additional
//! allocation on every spawn and thus the additional pointer indirection and virtual dispatch of
//! every poll of spawned futures.
//!
//! Executors that want to support this optimization will need to expose a way to split spawning
//! into two operations:
//! 1. allocate memory for the task
//! 2. write the task's future and register the task with the executor (usually via some kind of
//!    spawn queue)
//!
//! The first operation allocates without knowing the concrete type of the future - it's only given
//! the layout. Executors tend to wrap spawned futures in a task datastructure, with the future
//! potentially inline as a DST - the first step allocates this task datastructure but leaves the
//! future uninitialized.
//!
//! The second operation writes the future to the task structure and queues the task toward the
//! executor.
//!
//! This split allows the `spawn` call to be inlined such that the future's state can be written
//! directly to its final destination - hopefully avoiding a potentially large memcpy from the stack
//! to the heap.

#![no_std]

#[cfg(any(
    feature = "async-executor",
    feature = "dioxus",
    feature = "futures-executor",
    feature = "tokio",
    feature = "wasm-bindgen",
))]
extern crate alloc;

use core::{alloc::Layout, future::Future};

#[cfg(feature = "dioxus")]
pub use dioxus::DioxusSpawner;
#[cfg(feature = "wasm-bindgen")]
pub use wasm_bindgen::WasmBindgenSpawner;

#[cfg(feature = "async-executor")]
mod async_executor;
#[cfg(feature = "dioxus")]
mod dioxus;
#[cfg(feature = "futures-executor")]
mod futures_executor;
#[cfg(feature = "tokio")]
mod tokio;
#[cfg(feature = "wasm-bindgen")]
mod wasm_bindgen;

#[derive(Debug)]
pub enum SpawnError {
    Shutdown,
    Other,
}

pub type Result<T> = core::result::Result<T, SpawnError>;

// A thread-local spawner that can spawn `Future`s which are `!Send`.
pub struct LocalSpawner {
    handle: *const (),
    vtable: &'static LocalSpawnerVtable,
}

impl LocalSpawner {
    // Create a new `LocalSpawner`.
    pub fn new<T: IntoLocalSpawner>(inner: T) -> Self {
        Self {
            handle: unsafe { T::into_handle(inner) } as *const (),
            vtable: LocalSpawnerVtable::get::<T>(),
        }
    }

    // Spawn a `Future`.
    pub fn spawn<F: Future<Output = ()> + 'static>(&self, f: F) -> Result<()> {
        // Safety: we create copies of the `handle` pointer here, but the underlying memory is only
        // ever referenced immutably.

        let builder = SpawnCompleterBuilder {
            handle: self.handle,
            vtable: self.vtable,
        };
        unsafe {
            let spawn_completer = (self.vtable.spawn_dyn)(self.handle, builder, Layout::new::<F>());
            spawn_completer.spawn(f)
        }
    }
}

impl Clone for LocalSpawner {
    fn clone(&self) -> Self {
        unsafe {
            (self.vtable.on_clone)(self.handle);
        }
        Self {
            handle: self.handle,
            vtable: self.vtable,
        }
    }
}

impl Drop for LocalSpawner {
    fn drop(&mut self) {
        unsafe {
            (self.vtable.on_drop)(self.handle);
        }
    }
}

/// The methods of this trait are meant only for internal use in `ispawn`. Implement it to support
/// creating an `ispawn::LocalSpawner` from an executor's thread-local spawner.
pub trait IntoLocalSpawner {
    /// Safety: the implementer must ensure that the memory behind the returned pointer is 'static.
    unsafe fn into_handle(self) -> *const ();

    unsafe fn spawn_dyn(
        handle: *const (),
        builder: SpawnCompleterBuilder,
        future_layout: Layout,
    ) -> SpawnCompleter;

    unsafe fn finish_spawn(
        handle: *const (),
        task_ptr_as_dyn_future: *mut dyn Future<Output = ()>,
    ) -> Result<()>;

    unsafe fn on_clone(handle: *const ());

    unsafe fn on_drop(handle: *const ());
}

pub struct SpawnCompleter {
    handle: *const (),
    vtable: &'static LocalSpawnerVtable,
    task_ptr: *mut (),
    future_ptr: *mut (),
}

pub struct SpawnCompleterBuilder {
    handle: *const (),
    vtable: &'static LocalSpawnerVtable,
}

impl SpawnCompleterBuilder {
    pub fn build(self, task_ptr: *mut (), future_ptr: *mut ()) -> SpawnCompleter {
        SpawnCompleter {
            handle: self.handle,
            vtable: self.vtable,
            task_ptr,
            future_ptr,
        }
    }
}

impl SpawnCompleter {
    /// Safety: The caller must ensure that `F` has the same layout that was used to create this
    /// `SpawnCompleter`.
    unsafe fn spawn<F: Future<Output = ()> + 'static>(self, f: F) -> Result<()> {
        unsafe {
            core::ptr::write(self.future_ptr as *mut F, f);

            // Learned this trick from here:
            //   https://www.reddit.com/r/rust/comments/hcofkh/comment/fvgpv5e
            // This seems pretty dubious, but it works today. It is dubious because `self.task_ptr` is
            // not an instance of F. But it will have the same `dyn Future` vtable as F. So the
            // intermediate cast to `*mut F` is just used to get the right vtable.
            (self.vtable.finish_spawn)(
                self.handle,
                self.task_ptr as *mut F as *mut dyn Future<Output = ()>,
            )
        }
    }
}

struct LocalSpawnerVtable {
    spawn_dyn: unsafe fn(
        handle: *const (),
        builder: SpawnCompleterBuilder,
        future_layout: Layout,
    ) -> SpawnCompleter,

    finish_spawn: unsafe fn(
        handle: *const (),
        task_ptr_as_dyn_future: *mut dyn Future<Output = ()>,
    ) -> Result<()>,

    on_clone: unsafe fn(handle: *const ()),

    on_drop: unsafe fn(handle: *const ()),
}

impl LocalSpawnerVtable {
    fn get<T: IntoLocalSpawner>() -> &'static Self {
        &LocalSpawnerVtable {
            spawn_dyn: T::spawn_dyn,
            finish_spawn: T::finish_spawn,
            on_clone: T::on_clone,
            on_drop: T::on_drop,
        }
    }
}

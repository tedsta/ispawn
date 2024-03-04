# ispawn

*Experimental, not ready to be used*

A low-cost, `no_std` abstraction over `Future` spawners that erases the underlying spawn handle's type. This allows one to write libraries that spawn futures in an executor-agnostic way without using viral type parameters throughout.

The main idea of this crate is that if async executors are willing to integrate with it, the type-erased spawners in this crate will be able to spawn without additional allocations (though the underling executor-specific spawner likely does allocate on each spawn). While spawning is typically a relatively infrequent operation, and thus is not particularly performance-sensitive, boxing a future before spawning would incur an extra pointer indirection and virtual dispatch every time the future is polled.

Currently the hypothetical integrations described in the previous paragraph do not exist - we directly implement shim wrappers for Rc-wrapped spawners which *do* incur the additional allocation on every spawn and thus the additional pointer indirection and virtual dispatch of every poll of spawned futures.

Executors that want to support this optimization will need to expose a way to split spawning into two operations:
1. allocate memory for the task
2. write the task's future and register the task with the executor (usually via some kind of
   spawn queue)

The first operation allocates without knowing the concrete type of the future - it's only given the layout. Executors tend to wrap spawned futures in a task datastructure, with the future potentially inline as a DST - the first step allocates this task datastructure but leaves the future uninitialized.

The second operation writes the future to the task structure and queues the task toward the
executor.

## License

MIT

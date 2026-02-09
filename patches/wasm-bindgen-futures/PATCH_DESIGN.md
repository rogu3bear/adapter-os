# Re-Entrancy Protection Patch for wasm-bindgen-futures 0.4.58

## Bug Summary

`Task::run()` in `singlethread.rs:132` calls `self.inner.borrow_mut()` and holds the
`RefMut` for the entire duration of the poll. During the poll, web_sys DOM calls cross
the WASM-JS boundary. At these boundaries, V8 processes microtask checkpoints, which
can invoke the `run_all` closure again. The re-entrant `run_all` pops tasks from the
queue and calls `run()`. If it reaches a task that the outer `run()` is still polling,
the second `borrow_mut()` panics with `already borrowed`.

### Triggering Sequence

```
1. Microtask fires -> run_all() -> is_scheduled = false, is_spinning = (not tracked)
2. run_all pops Task A, calls task.run()
3. run() borrows inner (RefMut held), sets is_queued = false, starts poll
4. Poll calls web_sys DOM API -> crosses WASM/JS boundary
5. V8 microtask checkpoint fires during the boundary crossing
6. Meanwhile, poll (or another task) called waker.wake() on Task A
   -> is_queued was false, so wake() sets is_queued = true, calls force_wake()
   -> force_wake() -> schedule_task() -> pushes Task A to queue,
      sees is_scheduled = false, calls queueMicrotask, sets is_scheduled = true
7. V8 microtask checkpoint processes the newly queued microtask
8. run_all() RE-ENTERS -> is_scheduled = false (consumed)
9. Re-entrant run_all pops Task A from queue, calls task.run()
10. run() calls self.inner.borrow_mut() -> PANICS (outer still holds RefMut)
```

## Design: Two-Layer Defense

The fix uses two complementary mechanisms:

### Layer 1: Re-Entrancy Guard in `run_all()` (Primary Fix)

Add an `is_spinning: Cell<bool>` field to `QueueState`. When `run_all` detects it is
being called re-entrantly, it returns immediately **without consuming `is_scheduled`**.
After the outer `run_all` finishes its original batch, it drains any tasks that were
added by re-entrant scheduling.

### Layer 2: `try_borrow_mut()` in `Task::run()` (Safety Belt)

Replace `self.inner.borrow_mut()` with `self.inner.try_borrow_mut()`. If the borrow
fails (task is already being polled), push the task back to the front of the queue
so it will be retried. This requires changing `run()` to accept `Rc<Self>` so it can
re-queue itself.

However, this layer should **never** fire in practice if Layer 1 is correct. It exists
only as a defense-in-depth measure.

### Why Not Layer 2 Alone?

Layer 2 alone requires changing `run(&self)` to `run(this: Rc<Self>)`. This works but
is more invasive than necessary. The re-entrancy guard in `run_all` prevents the
problematic call from ever reaching `Task::run()`, making it the primary fix. Layer 2
provides a safety net that prevents a panic even if the guard has a gap.

### Why Not Layer 1 Alone?

Layer 1 should be sufficient in all cases. However, the cost of Layer 2 is one
`try_borrow_mut` call (a single `Cell` read), while the cost of a missed re-entrancy
case is a panic crash. The safety belt is worth the negligible overhead.

## Exact Code Changes

### File: `queue.rs`

#### Change 1: Add `is_spinning` field to `QueueState`

```rust
struct QueueState {
    // The queue of Tasks which are to be run in order.
    tasks: RefCell<VecDeque<Rc<crate::task::Task>>>,

    // This flag indicates whether we've scheduled `run_all` to run in the future.
    is_scheduled: Cell<bool>,

    // Re-entrancy guard: true while run_all is actively draining the queue.
    // When V8 microtask checkpoints cause run_all to be called re-entrantly,
    // the nested call detects this flag and returns immediately, letting the
    // outer run_all drain any newly-added tasks after its current batch.
    is_spinning: Cell<bool>,
}
```

#### Change 2: Add re-entrancy guard to `run_all()`

```rust
impl QueueState {
    fn run_all(&self) {
        // Re-entrancy guard: if we're already spinning, return immediately.
        // Do NOT consume is_scheduled here -- the outer run_all owns that.
        // Tasks queued during re-entrant scheduling remain in the queue and
        // will be drained by the outer run_all after its current batch.
        if self.is_spinning.get() {
            return;
        }

        // "consume" the schedule
        let _was_scheduled = self.is_scheduled.replace(false);
        debug_assert!(_was_scheduled);

        self.is_spinning.set(true);

        // Drain loop: process the current batch, then check for tasks added
        // by re-entrant wakeups that were suppressed by the guard above.
        loop {
            let mut task_count_left = self.tasks.borrow().len();
            if task_count_left == 0 {
                break;
            }
            while task_count_left > 0 {
                task_count_left -= 1;
                let task = match self.tasks.borrow_mut().pop_front() {
                    Some(task) => task,
                    None => break,
                };
                task.run();
            }
            // After processing this batch, loop back to check if any new tasks
            // were added during polling (from re-entrant schedule_task calls
            // that the guard above suppressed).
        }

        self.is_spinning.set(false);

        // All tasks drained. Future schedule_task calls will schedule a fresh
        // microtask because is_scheduled is false.
    }
}
```

#### Change 3: Initialize `is_spinning` in `Queue::new()`

```rust
let state = Rc::new(QueueState {
    is_scheduled: Cell::new(false),
    tasks: RefCell::new(VecDeque::new()),
    is_spinning: Cell::new(false),
});
```

#### Change 4: Optimize `push_task` for the spinning case

```rust
#[cfg(not(target_feature = "atomics"))]
pub(crate) fn push_task(&self, task: Rc<crate::task::Task>) {
    // If run_all is currently spinning, just append to the queue.
    // The outer run_all loop will pick it up after the current batch.
    // No need to schedule a new microtask.
    if self.state.is_spinning.get() {
        self.state.tasks.borrow_mut().push_back(task);
        return;
    }
    self.schedule_task(task)
}
```

### File: `task/singlethread.rs`

#### Change 5: Use `try_borrow_mut()` in `Task::run()` (safety belt)

```rust
pub(crate) fn run(&self) {
    let mut borrow = match self.inner.try_borrow_mut() {
        Ok(borrow) => borrow,
        Err(_) => {
            // Safety belt: this task is already being polled by an outer
            // run() call. The re-entrancy guard in run_all should prevent
            // this from happening, but if it does, we must not panic.
            // Mark the task as needing another poll by ensuring is_queued
            // stays true (it already is, since we haven't cleared it).
            // The outer run_all will re-encounter this task if it was
            // re-queued, or the future's waker will re-queue it.
            return;
        }
    };

    // Wakeups can come in after a Future has finished and been destroyed,
    // so handle this gracefully by just ignoring the request to run.
    let inner = match borrow.as_mut() {
        Some(inner) => inner,
        None => return,
    };

    // Ensure that if poll calls `waker.wake()` we can get enqueued back on
    // the run queue.
    self.is_queued.set(false);

    #[cfg(debug_assertions)]
    let is_ready = match self.console.as_ref() {
        Some(console) => console.run(&mut move || inner.is_ready()),
        None => inner.is_ready(),
    };

    #[cfg(not(debug_assertions))]
    let is_ready = inner.is_ready();

    if is_ready {
        *borrow = None;
    }
}
```

### File: `task/multithread.rs`

No changes needed. The multithread variant uses `Closure`-based scheduling with
`Atomics.waitAsync` and does not go through `run_all()`. Each task has its own
dedicated closure that calls `this.run()` on resolution. Re-entrancy through
`run_all` is a singlethread-specific issue because only the singlethread path
uses a shared task queue with a global `run_all` drain loop.

The multithread `Task::run()` does use `self.inner.borrow_mut()`, but it can only
be called from its own dedicated `Closure`, which is registered as a `then` handler
on a `waitAsync` promise. These closures don't share a queue, so re-entrancy of the
same task's `run()` is not possible through the same mechanism.

## Correctness Argument

### No Lost Tasks

When the re-entrancy guard fires (step 7-8 in the trigger sequence), the re-entrant
`run_all` returns immediately. The task that was scheduled via `schedule_task` during
the poll is still in the `tasks` queue. The outer `run_all` has a drain loop that
checks for new tasks after each batch, so it will find and execute the task.

With the `push_task` optimization (Change 4), tasks added via `force_wake` during
spinning go directly to the queue without scheduling a new microtask. The outer
`run_all` loop picks them up.

### No Double Execution

The `try_borrow_mut` safety belt (Change 5) ensures that even if the same task
somehow reaches `run()` while it's being polled, it returns without polling. The
task will be re-polled when the waker fires again (since `is_queued` is still true
at that point, it won't be re-woken, but the outer loop will reach it if it's in
the queue).

### No `is_scheduled` State Corruption

The re-entrant `run_all` does not touch `is_scheduled` (the guard returns before
line 32). Only the outer `run_all` manages this flag. After the outer `run_all`
completes and sets `is_spinning = false`, `is_scheduled` is `false`, which means
the next `schedule_task` will correctly schedule a new microtask.

The `push_task` optimization avoids calling `schedule_task` while spinning. This
prevents a redundant `queueMicrotask` call and avoids the scenario where the
scheduled microtask fires after `run_all` has already drained the queue.

### Minimal Surface Area

- `queue.rs`: 3 additions (field, guard, initialization) + 1 optimization
- `singlethread.rs`: 1 change (`borrow_mut` -> `try_borrow_mut` with fallback)
- `multithread.rs`: 0 changes
- No signature changes, no new public API, no changes to `Task::spawn()` or waker logic

## Testing Strategy

1. **Manual reproduction**: Navigate to `/chat` in the Leptos SPA. The panic at
   `singlethread.rs:132` should no longer occur.

2. **Stress test**: Rapidly navigate between SPA routes that trigger DOM-heavy
   web_sys calls during future polling. Verify no panics and no lost task execution
   (chat messages should still load, SSE streams should connect).

3. **Regression**: Run the existing wasm-bindgen-futures test suite to ensure no
   behavioral changes for non-re-entrant cases.

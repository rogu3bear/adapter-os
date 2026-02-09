use alloc::collections::VecDeque;
use alloc::rc::Rc;
use core::cell::{Cell, RefCell};
use js_sys::Promise;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen]
    fn queueMicrotask(closure: &Closure<dyn FnMut(JsValue)>);

    type Global;

    #[wasm_bindgen(method, getter, js_name = queueMicrotask)]
    fn hasQueueMicrotask(this: &Global) -> JsValue;
}

struct QueueState {
    // The queue of Tasks which are to be run in order. In practice this is all the
    // synchronous work of futures, and each `Task` represents calling `poll` on
    // a future "at the right time".
    tasks: RefCell<VecDeque<Rc<crate::task::Task>>>,

    // This flag indicates whether we've scheduled `run_all` to run in the future.
    // This is used to ensure that it's only scheduled once.
    is_scheduled: Cell<bool>,

    // Re-entrancy guard: true while run_all is actively draining the queue.
    // V8 microtask checkpoints at WASM/JS boundaries can cause run_all to be
    // called re-entrantly. The nested call detects this flag and returns
    // immediately, letting the outer run_all drain any newly-added tasks.
    is_spinning: Cell<bool>,
}

impl QueueState {
    fn run_all(&self) {
        // Re-entrancy guard: if we're already draining the queue (e.g. V8
        // processed a microtask checkpoint at a WASM/JS boundary during poll),
        // return immediately. Do NOT consume is_scheduled -- the outer run_all
        // owns that flag. Tasks queued by the re-entrant path remain in the
        // deque and will be picked up by the outer loop.
        if self.is_spinning.get() {
            return;
        }

        // "consume" the schedule
        let _was_scheduled = self.is_scheduled.replace(false);
        debug_assert!(_was_scheduled);

        self.is_spinning.set(true);

        // Drain loop: process the current batch, then check for tasks that
        // were added by wakeups during polling. Re-entrant schedule_task calls
        // were suppressed by the guard above, so their tasks sit in the deque
        // waiting for us to pick them up.
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
        }

        self.is_spinning.set(false);
    }
}

pub(crate) struct Queue {
    state: Rc<QueueState>,
    promise: Promise,
    closure: Closure<dyn FnMut(JsValue)>,
    has_queue_microtask: bool,
}

impl Queue {
    // Schedule a task to run on the next tick
    pub(crate) fn schedule_task(&self, task: Rc<crate::task::Task>) {
        self.state.tasks.borrow_mut().push_back(task);
        // Use queueMicrotask to execute as soon as possible. If it does not exist
        // fall back to the promise resolution
        if !self.state.is_scheduled.replace(true) {
            if self.has_queue_microtask {
                queueMicrotask(&self.closure);
            } else {
                let _ = self.promise.then(&self.closure);
            }
        }
    }
    // Append a task to the currently running queue, or schedule it
    #[cfg(not(target_feature = "atomics"))]
    pub(crate) fn push_task(&self, task: Rc<crate::task::Task>) {
        // If run_all is currently draining, just append to the deque.
        // The outer drain loop will pick it up after the current batch.
        // No need to schedule a new microtask (which would re-enter run_all).
        if self.state.is_spinning.get() {
            self.state.tasks.borrow_mut().push_back(task);
            return;
        }
        self.schedule_task(task)
    }
}

impl Queue {
    fn new() -> Self {
        let state = Rc::new(QueueState {
            is_scheduled: Cell::new(false),
            tasks: RefCell::new(VecDeque::new()),
            is_spinning: Cell::new(false),
        });

        let has_queue_microtask = js_sys::global()
            .unchecked_into::<Global>()
            .hasQueueMicrotask()
            .is_function();

        Self {
            promise: Promise::resolve(&JsValue::undefined()),

            closure: {
                let state = Rc::clone(&state);

                // This closure will only be called on the next microtask event
                // tick
                Closure::new(move |_| state.run_all())
            },

            state,
            has_queue_microtask,
        }
    }

    pub(crate) fn with<R>(f: impl FnOnce(&Self) -> R) -> R {
        use once_cell::unsync::Lazy;

        struct Wrapper<T>(Lazy<T>);

        #[cfg(not(target_feature = "atomics"))]
        unsafe impl<T> Sync for Wrapper<T> {}

        #[cfg(not(target_feature = "atomics"))]
        unsafe impl<T> Send for Wrapper<T> {}

        #[cfg_attr(target_feature = "atomics", thread_local)]
        static QUEUE: Wrapper<Queue> = Wrapper(Lazy::new(Queue::new));

        f(&QUEUE.0)
    }
}

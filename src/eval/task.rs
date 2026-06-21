use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use crate::ast::TypeName;
use crate::error::VerseError;

use super::{
    Env, Flow, Interpreter, Value, runtime_value_matches_type_name, structured_task_result_value,
    value_copy,
};

type SuspensionResume = dyn Fn(&Interpreter, Value) -> Result<Flow, VerseError>;
type SuspensionCancel = dyn Fn(&Interpreter) -> Result<(), VerseError>;
pub struct RuntimeTask {
    state: RefCell<RuntimeTaskState>,
    awaiters: RefCell<Vec<RuntimeTaskAwaiter>>,
    scoped_children: RefCell<Vec<Rc<RuntimeTask>>>,
}

enum RuntimeTaskAwaiter {
    Task(Rc<RuntimeTask>),
    Structured {
        task: Rc<RuntimeTask>,
        branch_index: usize,
    },
}

enum RuntimeTaskState {
    Complete(Result<Value, VerseError>),
    Suspended(RuntimeSuspension),
    Running,
}

#[derive(Clone)]
pub struct RuntimeSuspension {
    wait: RuntimeWait,
    resume: Rc<SuspensionResume>,
    cancel: Rc<SuspensionCancel>,
}

#[derive(Clone)]
enum RuntimeWait {
    None,
    Event(Rc<RefCell<Vec<Rc<RuntimeTask>>>>),
    SleepNextTick(Rc<RuntimeScheduler>),
    SleepUntil(Rc<RuntimeScheduler>, Instant),
    Task(Rc<RuntimeTask>),
    StructuredTasks(Rc<RefCell<StructuredTaskWait>>),
}

pub(super) struct StructuredTaskWait {
    pub(super) tasks: Vec<(usize, Rc<RuntimeTask>)>,
    pub(super) registered: bool,
}

pub(super) struct RuntimeScheduler {
    sleepers: RefCell<Vec<Rc<RuntimeTask>>>,
    timed_sleepers: RefCell<Vec<(Instant, Rc<RuntimeTask>)>>,
    detached_tasks: RefCell<Vec<Rc<RuntimeTask>>>,
}

impl RuntimeScheduler {
    pub(super) fn new() -> Self {
        Self {
            sleepers: RefCell::new(Vec::new()),
            timed_sleepers: RefCell::new(Vec::new()),
            detached_tasks: RefCell::new(Vec::new()),
        }
    }

    pub(super) fn schedule_next_tick(&self, task: Rc<RuntimeTask>) {
        self.sleepers.borrow_mut().push(task);
    }

    pub(super) fn schedule_until(&self, deadline: Instant, task: Rc<RuntimeTask>) {
        self.timed_sleepers.borrow_mut().push((deadline, task));
    }

    pub(super) fn take_next_tick_sleepers(&self) -> Vec<Rc<RuntimeTask>> {
        std::mem::take(&mut *self.sleepers.borrow_mut())
            .into_iter()
            .filter(|task| task.is_suspended())
            .collect()
    }

    pub(super) fn take_ready_timed_sleepers(&self, now: Instant) -> Vec<Rc<RuntimeTask>> {
        let mut ready = Vec::new();
        self.timed_sleepers.borrow_mut().retain(|(deadline, task)| {
            if !task.is_suspended() {
                false
            } else if *deadline <= now {
                ready.push(task.clone());
                false
            } else {
                true
            }
        });
        ready
    }

    pub(super) fn next_timed_deadline(&self) -> Option<Instant> {
        self.timed_sleepers
            .borrow()
            .iter()
            .filter(|(_, task)| task.is_suspended())
            .map(|(deadline, _)| *deadline)
            .min()
    }

    pub(super) fn track_detached_task(&self, task: Rc<RuntimeTask>) {
        if task.is_complete() {
            return;
        }
        let mut tasks = self.detached_tasks.borrow_mut();
        if tasks.iter().any(|existing| Rc::ptr_eq(existing, &task)) {
            return;
        }
        tasks.push(task);
    }

    pub(super) fn cleanup_detached_tasks(&self) {
        self.detached_tasks
            .borrow_mut()
            .retain(|task| !task.is_complete());
    }
}

impl RuntimeSuspension {
    pub(super) fn unresumable() -> Self {
        Self {
            wait: RuntimeWait::None,
            resume: Rc::new(|_, _| Ok(Flow::Pending(RuntimeSuspension::unresumable()))),
            cancel: Rc::new(|_| Ok(())),
        }
    }

    pub(super) fn event(waiters: Rc<RefCell<Vec<Rc<RuntimeTask>>>>) -> Self {
        Self {
            wait: RuntimeWait::Event(waiters),
            resume: Rc::new(|_, value| Ok(Flow::Value(value))),
            cancel: Rc::new(|_| Ok(())),
        }
    }

    pub(super) fn sleep_next_tick(scheduler: Rc<RuntimeScheduler>) -> Self {
        Self {
            wait: RuntimeWait::SleepNextTick(scheduler),
            resume: Rc::new(|_, _| Ok(Flow::Value(Value::None))),
            cancel: Rc::new(|_| Ok(())),
        }
    }

    pub(super) fn sleep_until(scheduler: Rc<RuntimeScheduler>, deadline: Instant) -> Self {
        Self {
            wait: RuntimeWait::SleepUntil(scheduler, deadline),
            resume: Rc::new(|_, _| Ok(Flow::Value(Value::None))),
            cancel: Rc::new(|_| Ok(())),
        }
    }

    pub(super) fn task(task: Rc<RuntimeTask>) -> Self {
        Self {
            wait: RuntimeWait::Task(task),
            resume: Rc::new(|_, value| Ok(Flow::Value(value))),
            cancel: Rc::new(|_| Ok(())),
        }
    }

    pub(super) fn structured_tasks(wait: Rc<RefCell<StructuredTaskWait>>) -> Self {
        Self {
            wait: RuntimeWait::StructuredTasks(wait),
            resume: Rc::new(|_, value| Ok(Flow::Value(value))),
            cancel: Rc::new(|_| Ok(())),
        }
    }

    pub(super) fn map(
        self,
        continuation: impl Fn(&Interpreter, Flow) -> Result<Flow, VerseError> + 'static,
    ) -> Self {
        let wait = self.wait.clone();
        let resume = self.resume.clone();
        let cancel = self.cancel.clone();
        Self {
            wait,
            resume: Rc::new(move |interpreter, value| {
                let flow = resume(interpreter, value)?;
                continuation(interpreter, flow)
            }),
            cancel,
        }
    }

    pub(super) fn on_cancel(
        self,
        cleanup: impl Fn(&Interpreter) -> Result<(), VerseError> + 'static,
    ) -> Self {
        let wait = self.wait.clone();
        let resume = self.resume.clone();
        let cancel = self.cancel.clone();
        Self {
            wait,
            resume,
            cancel: Rc::new(move |interpreter| {
                cancel(interpreter)?;
                cleanup(interpreter)
            }),
        }
    }

    pub(super) fn register_task(&self, task: Rc<RuntimeTask>) {
        match &self.wait {
            RuntimeWait::None => {}
            RuntimeWait::Event(waiters) => waiters.borrow_mut().push(task),
            RuntimeWait::SleepNextTick(scheduler) => scheduler.schedule_next_tick(task),
            RuntimeWait::SleepUntil(scheduler, deadline) => {
                scheduler.schedule_until(*deadline, task)
            }
            RuntimeWait::Task(awaited) => awaited
                .awaiters
                .borrow_mut()
                .push(RuntimeTaskAwaiter::Task(task)),
            RuntimeWait::StructuredTasks(wait) => {
                let mut wait = wait.borrow_mut();
                if wait.registered {
                    return;
                }
                wait.registered = true;
                for (branch_index, awaited) in &wait.tasks {
                    awaited
                        .awaiters
                        .borrow_mut()
                        .push(RuntimeTaskAwaiter::Structured {
                            task: task.clone(),
                            branch_index: *branch_index,
                        });
                }
            }
        }
    }

    pub(super) fn cancel(&self, interpreter: &Interpreter) -> Result<(), VerseError> {
        if let RuntimeWait::StructuredTasks(wait) = &self.wait {
            let tasks = wait
                .borrow()
                .tasks
                .iter()
                .map(|(_, task)| task.clone())
                .collect::<Vec<_>>();
            for task in tasks {
                task.cancel_silently(interpreter)?;
            }
        }
        (self.cancel)(interpreter)
    }
}

impl RuntimeTask {
    pub(super) fn new_running() -> Rc<Self> {
        Rc::new(Self {
            state: RefCell::new(RuntimeTaskState::Running),
            awaiters: RefCell::new(Vec::new()),
            scoped_children: RefCell::new(Vec::new()),
        })
    }

    pub(super) fn set_from_flow(self: &Rc<Self>, flow: Flow, interpreter: Option<&Interpreter>) {
        match flow {
            Flow::Value(value) | Flow::Return(value) => {
                self.complete_with_value(value, interpreter);
            }
            Flow::Break => {
                let error = VerseError::runtime("`break` escaped spawned task");
                self.complete_with_error(error, interpreter);
            }
            Flow::Pending(suspension) => {
                self.cleanup_scoped_children();
                *self.state.borrow_mut() = RuntimeTaskState::Suspended(suspension.clone());
                suspension.register_task(self.clone());
            }
        }
    }

    pub(super) fn complete_with_value(
        self: &Rc<Self>,
        value: Value,
        interpreter: Option<&Interpreter>,
    ) {
        if let Some(interpreter) = interpreter
            && let Err(error) = self.cancel_scoped_children(interpreter)
        {
            self.complete_with_error(error, Some(interpreter));
            return;
        }
        *self.state.borrow_mut() = RuntimeTaskState::Complete(Ok(value.clone()));
        self.resume_awaiters(Ok(value), interpreter);
    }

    pub(super) fn complete_with_error(
        self: &Rc<Self>,
        error: VerseError,
        interpreter: Option<&Interpreter>,
    ) {
        let mut error = error;
        if let Some(interpreter) = interpreter
            && let Err(cancel_error) = self.cancel_scoped_children(interpreter)
        {
            error = cancel_error;
        }
        *self.state.borrow_mut() = RuntimeTaskState::Complete(Err(error.clone()));
        self.resume_awaiters(Err(error), interpreter);
    }

    pub(super) fn cancel_silently(&self, interpreter: &Interpreter) -> Result<(), VerseError> {
        let suspension = {
            let mut state = self.state.borrow_mut();
            match &*state {
                RuntimeTaskState::Complete(_) => None,
                RuntimeTaskState::Suspended(suspension) => {
                    let suspension = suspension.clone();
                    *state =
                        RuntimeTaskState::Complete(Err(VerseError::runtime("task was canceled")));
                    Some(suspension)
                }
                RuntimeTaskState::Running => {
                    *state =
                        RuntimeTaskState::Complete(Err(VerseError::runtime("task was canceled")));
                    None
                }
            }
        };
        self.awaiters.borrow_mut().clear();
        if let Some(suspension) = suspension {
            suspension.cancel(interpreter)?;
        }
        self.cancel_scoped_children(interpreter)?;
        Ok(())
    }

    pub(super) fn track_scoped_child(&self, task: Rc<RuntimeTask>) {
        if task.is_complete() {
            return;
        }
        let mut children = self.scoped_children.borrow_mut();
        if children.iter().any(|existing| Rc::ptr_eq(existing, &task)) {
            return;
        }
        children.push(task);
    }

    pub(super) fn cleanup_scoped_children(&self) {
        self.scoped_children
            .borrow_mut()
            .retain(|task| !task.is_complete());
    }

    pub(super) fn cancel_scoped_children(
        &self,
        interpreter: &Interpreter,
    ) -> Result<(), VerseError> {
        let children = std::mem::take(&mut *self.scoped_children.borrow_mut());
        for child in children {
            if !child.is_complete() {
                child.cancel_silently(interpreter)?;
            }
        }
        Ok(())
    }

    pub(super) fn is_complete(&self) -> bool {
        matches!(&*self.state.borrow(), RuntimeTaskState::Complete(_))
    }

    pub(super) fn is_suspended(&self) -> bool {
        matches!(&*self.state.borrow(), RuntimeTaskState::Suspended(_))
    }

    pub(super) fn resume(self: &Rc<Self>, interpreter: &Interpreter, value: Value) {
        let suspension = {
            let mut state = self.state.borrow_mut();
            match std::mem::replace(&mut *state, RuntimeTaskState::Running) {
                RuntimeTaskState::Suspended(suspension) => suspension,
                other => {
                    *state = other;
                    return;
                }
            }
        };

        match interpreter
            .eval_with_task_context(self.clone(), || (suspension.resume)(interpreter, value))
        {
            Ok(flow) => self.set_from_flow(flow, Some(interpreter)),
            Err(error) => self.complete_with_error(error, Some(interpreter)),
        }
    }

    pub(super) fn resume_awaiters(
        self: &Rc<Self>,
        result: Result<Value, VerseError>,
        interpreter: Option<&Interpreter>,
    ) {
        let awaiters = std::mem::take(&mut *self.awaiters.borrow_mut());
        let Some(interpreter) = interpreter else {
            return;
        };
        for awaiter in awaiters {
            match &result {
                Ok(value) => awaiter.resume(interpreter, value_copy(value)),
                Err(error) => awaiter.complete_with_error(error.clone(), Some(interpreter)),
            }
        }
    }

    pub(super) fn resume_from_task_result(
        self: &Rc<Self>,
        interpreter: &Interpreter,
        value: Value,
    ) {
        let suspension = {
            let mut state = self.state.borrow_mut();
            match std::mem::replace(&mut *state, RuntimeTaskState::Running) {
                RuntimeTaskState::Suspended(suspension) => suspension,
                other => {
                    *state = other;
                    return;
                }
            }
        };

        let flow = interpreter
            .eval_with_task_context(self.clone(), || (suspension.resume)(interpreter, value));
        match flow {
            Ok(flow) => self.set_from_flow(flow, Some(interpreter)),
            Err(error) => self.complete_with_error(error, Some(interpreter)),
        }
    }

    pub(super) fn await_result(&self) -> Result<Option<Value>, VerseError> {
        match &*self.state.borrow() {
            RuntimeTaskState::Complete(Ok(value)) => Ok(Some(value_copy(value))),
            RuntimeTaskState::Complete(Err(error)) => Err(error.clone()),
            RuntimeTaskState::Suspended(_) | RuntimeTaskState::Running => Ok(None),
        }
    }

    pub(super) fn matches_payload_type(&self, payload: &TypeName, env: &Env) -> bool {
        match &*self.state.borrow() {
            RuntimeTaskState::Complete(Ok(value)) => {
                runtime_value_matches_type_name(value, payload, env)
            }
            RuntimeTaskState::Complete(Err(_)) => false,
            RuntimeTaskState::Suspended(_) | RuntimeTaskState::Running => true,
        }
    }
}

impl RuntimeTaskAwaiter {
    pub(super) fn resume(&self, interpreter: &Interpreter, value: Value) {
        match self {
            RuntimeTaskAwaiter::Task(task) => {
                task.resume_from_task_result(interpreter, value);
            }
            RuntimeTaskAwaiter::Structured { task, branch_index } => {
                task.resume_from_task_result(
                    interpreter,
                    structured_task_result_value(*branch_index, value),
                );
            }
        }
    }

    pub(super) fn complete_with_error(&self, error: VerseError, interpreter: Option<&Interpreter>) {
        match self {
            RuntimeTaskAwaiter::Task(task) | RuntimeTaskAwaiter::Structured { task, .. } => {
                task.complete_with_error(error, interpreter);
            }
        }
    }
}

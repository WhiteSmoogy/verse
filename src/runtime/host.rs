use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;
#[cfg(feature = "tokio-host")]
use std::time::Instant;

use crate::eval::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PendingToken(pub(crate) u64);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PredictionKey {
    object: usize,
    field: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PredictionDefaultKey {
    class_name: String,
    field: String,
}

pub(crate) trait Host {
    fn now(&self) -> Duration;
    fn arm_timer(&mut self, delay: Duration, token: PendingToken);
    fn arm_future(&mut self, future: Pin<Box<dyn Future<Output = Value>>>, token: PendingToken);
    fn poll_ready(&mut self) -> Vec<(PendingToken, Value)>;
    fn cancel(&mut self, token: PendingToken);
    fn has_pending(&self) -> bool;
    fn prediction_value(
        &mut self,
        object: usize,
        class_name: &str,
        field: &str,
        default: &Value,
    ) -> Value;
    fn set_prediction_value(&mut self, object: usize, class_name: &str, field: &str, value: Value);
}

#[derive(Debug, Clone)]
struct TimerEntry {
    deadline: Duration,
    sequence: u64,
    token: PendingToken,
}

#[derive(Clone)]
struct ScriptedFutureCompletion {
    polls_until_ready: usize,
    value: Value,
}

struct ScriptedFuture {
    polls_until_ready: usize,
    value: Option<Value>,
}

impl Future for ScriptedFuture {
    type Output = Value;

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        if self.polls_until_ready > 1 {
            self.polls_until_ready -= 1;
            return Poll::Pending;
        }
        Poll::Ready(
            self.value
                .take()
                .expect("scripted future polled after completion"),
        )
    }
}

impl PartialEq for TimerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline
            && self.sequence == other.sequence
            && self.token == other.token
    }
}

impl Eq for TimerEntry {}

impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimerEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .deadline
            .cmp(&self.deadline)
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}

type PendingFuture = (PendingToken, Pin<Box<dyn Future<Output = Value>>>);

pub(crate) struct MockHost {
    now: Duration,
    next_timer_sequence: u64,
    timers: BinaryHeap<TimerEntry>,
    futures: Vec<PendingFuture>,
    scripted_future_completions: VecDeque<ScriptedFutureCompletion>,
    prediction_values: HashMap<PredictionKey, Value>,
    prediction_defaults: HashMap<PredictionDefaultKey, Value>,
    poll_count: usize,
}

impl Default for MockHost {
    fn default() -> Self {
        Self {
            now: Duration::ZERO,
            next_timer_sequence: 0,
            timers: BinaryHeap::new(),
            futures: Vec::new(),
            scripted_future_completions: VecDeque::new(),
            prediction_values: HashMap::new(),
            prediction_defaults: HashMap::new(),
            poll_count: 0,
        }
    }
}

impl MockHost {
    #[cfg(test)]
    pub(crate) fn with_scripted_future_completion(polls_until_ready: usize, value: Value) -> Self {
        let mut host = Self::default();
        host.scripted_future_completions
            .push_back(ScriptedFutureCompletion {
                polls_until_ready,
                value,
            });
        host
    }

    #[cfg(test)]
    pub(crate) fn poll_count(&self) -> usize {
        self.poll_count
    }

    #[cfg(test)]
    pub(crate) fn with_prediction_default(
        class_name: impl Into<String>,
        field: impl Into<String>,
        value: Value,
    ) -> Self {
        let mut host = Self::default();
        host.prediction_defaults.insert(
            PredictionDefaultKey {
                class_name: class_name.into(),
                field: field.into(),
            },
            value,
        );
        host
    }

    fn poll_futures(&mut self, ready: &mut Vec<(PendingToken, Value)>) {
        let waker = noop_waker();
        let mut context = Context::from_waker(&waker);
        let mut pending = Vec::new();
        for (token, mut future) in self.futures.drain(..) {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => ready.push((token, value)),
                Poll::Pending => pending.push((token, future)),
            }
        }
        self.futures = pending;
    }
}

impl Host for MockHost {
    fn now(&self) -> Duration {
        self.now
    }

    fn arm_timer(&mut self, delay: Duration, token: PendingToken) {
        let deadline = self.now + delay;
        let sequence = self.next_timer_sequence;
        self.next_timer_sequence += 1;
        self.timers.push(TimerEntry {
            deadline,
            sequence,
            token,
        });
    }

    fn arm_future(
        &mut self,
        mut future: Pin<Box<dyn Future<Output = Value>>>,
        token: PendingToken,
    ) {
        if let Some(scripted) = self.scripted_future_completions.pop_front() {
            future = Box::pin(ScriptedFuture {
                polls_until_ready: scripted.polls_until_ready,
                value: Some(scripted.value),
            });
        }
        self.futures.push((token, future));
    }

    fn poll_ready(&mut self) -> Vec<(PendingToken, Value)> {
        self.poll_count += 1;
        let mut ready = Vec::new();
        self.poll_futures(&mut ready);

        if let Some(next) = self.timers.peek()
            && next.deadline > self.now
        {
            self.now = next.deadline;
        }
        while self
            .timers
            .peek()
            .is_some_and(|entry| entry.deadline <= self.now)
        {
            let entry = self
                .timers
                .pop()
                .expect("timer heap peeked Some before pop");
            ready.push((entry.token, Value::None));
        }

        ready
    }

    fn cancel(&mut self, token: PendingToken) {
        self.futures
            .retain(|(pending_token, _)| *pending_token != token);
        self.timers = std::mem::take(&mut self.timers)
            .into_iter()
            .filter(|entry| entry.token != token)
            .collect();
    }

    fn has_pending(&self) -> bool {
        !self.timers.is_empty() || !self.futures.is_empty()
    }

    fn prediction_value(
        &mut self,
        object: usize,
        class_name: &str,
        field: &str,
        default: &Value,
    ) -> Value {
        let key = PredictionKey {
            object,
            field: field.to_string(),
        };
        if let Some(value) = self.prediction_values.get(&key) {
            return value.clone();
        }
        let value = self
            .prediction_defaults
            .get(&PredictionDefaultKey {
                class_name: class_name.to_string(),
                field: field.to_string(),
            })
            .cloned()
            .unwrap_or_else(|| default.clone());
        self.prediction_values.insert(key, value.clone());
        value
    }

    fn set_prediction_value(
        &mut self,
        object: usize,
        _class_name: &str,
        field: &str,
        value: Value,
    ) {
        self.prediction_values.insert(
            PredictionKey {
                object,
                field: field.to_string(),
            },
            value,
        );
    }
}

#[cfg(feature = "tokio-host")]
pub(crate) struct TokioHost {
    start: Instant,
    runtime: tokio::runtime::Runtime,
    local: tokio::task::LocalSet,
    ready: RcReadyQueue,
    pending: HashMap<PendingToken, tokio::task::JoinHandle<()>>,
    prediction_values: HashMap<PredictionKey, Value>,
    prediction_defaults: HashMap<PredictionDefaultKey, Value>,
}

#[cfg(feature = "tokio-host")]
type RcReadyQueue = std::rc::Rc<std::cell::RefCell<Vec<(PendingToken, Value)>>>;

#[cfg(feature = "tokio-host")]
impl TokioHost {
    pub(crate) fn new() -> Self {
        Self {
            start: Instant::now(),
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .expect("TokioHost runtime should initialize"),
            local: tokio::task::LocalSet::new(),
            ready: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            pending: HashMap::new(),
            prediction_values: HashMap::new(),
            prediction_defaults: HashMap::new(),
        }
    }

    fn drive_once(&self) {
        self.runtime.block_on(
            self.local
                .run_until(async { tokio::task::yield_now().await }),
        );
    }
}

#[cfg(feature = "tokio-host")]
impl Default for TokioHost {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "tokio-host")]
impl Host for TokioHost {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    fn arm_timer(&mut self, delay: Duration, token: PendingToken) {
        self.arm_future(
            Box::pin(async move {
                tokio::time::sleep(delay).await;
                Value::None
            }),
            token,
        );
    }

    fn arm_future(&mut self, future: Pin<Box<dyn Future<Output = Value>>>, token: PendingToken) {
        let ready = self.ready.clone();
        let handle = self.local.spawn_local(async move {
            let value = future.await;
            ready.borrow_mut().push((token, value));
        });
        self.pending.insert(token, handle);
    }

    fn poll_ready(&mut self) -> Vec<(PendingToken, Value)> {
        self.drive_once();
        let ready = std::mem::take(&mut *self.ready.borrow_mut());
        for (token, _) in &ready {
            self.pending.remove(token);
        }
        ready
    }

    fn cancel(&mut self, token: PendingToken) {
        if let Some(handle) = self.pending.remove(&token) {
            handle.abort();
        }
        self.ready
            .borrow_mut()
            .retain(|(ready_token, _)| *ready_token != token);
    }

    fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    fn prediction_value(
        &mut self,
        object: usize,
        class_name: &str,
        field: &str,
        default: &Value,
    ) -> Value {
        let key = PredictionKey {
            object,
            field: field.to_string(),
        };
        if let Some(value) = self.prediction_values.get(&key) {
            return value.clone();
        }
        let value = self
            .prediction_defaults
            .get(&PredictionDefaultKey {
                class_name: class_name.to_string(),
                field: field.to_string(),
            })
            .cloned()
            .unwrap_or_else(|| default.clone());
        self.prediction_values.insert(key, value.clone());
        value
    }

    fn set_prediction_value(
        &mut self,
        object: usize,
        _class_name: &str,
        field: &str,
        value: Value,
    ) {
        self.prediction_values.insert(
            PredictionKey {
                object,
                field: field.to_string(),
            },
            value,
        );
    }
}

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

fn noop_waker() -> Waker {
    Waker::from(Arc::new(NoopWake))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_host_advances_virtual_time_to_timer_deadlines() {
        let mut host = MockHost::default();
        host.arm_timer(Duration::from_secs(2), PendingToken(2));
        host.arm_timer(Duration::from_secs(1), PendingToken(1));

        assert_eq!(host.poll_ready(), vec![(PendingToken(1), Value::None)]);
        assert_eq!(host.now(), Duration::from_secs(1));
        assert_eq!(host.poll_ready(), vec![(PendingToken(2), Value::None)]);
        assert_eq!(host.now(), Duration::from_secs(2));
        assert!(!host.has_pending());
    }

    #[test]
    fn mock_host_polls_ready_futures() {
        let mut host = MockHost::default();
        host.arm_future(
            Box::pin(std::future::ready(Value::Int(42))),
            PendingToken(7),
        );

        assert_eq!(host.poll_ready(), vec![(PendingToken(7), Value::Int(42))]);
        assert!(!host.has_pending());
    }

    #[test]
    fn mock_host_cancels_pending_timers_and_futures() {
        let mut host = MockHost::default();
        host.arm_timer(Duration::from_secs(1), PendingToken(1));
        host.arm_future(Box::pin(std::future::pending::<Value>()), PendingToken(2));

        host.cancel(PendingToken(1));
        host.cancel(PendingToken(2));

        assert!(host.poll_ready().is_empty());
        assert!(!host.has_pending());
    }
}

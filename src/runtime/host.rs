use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
#[cfg(feature = "tokio-host")]
use std::time::{Duration, Instant};

use crate::eval::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PendingToken(pub(crate) u64);

pub(crate) trait Host {
    fn now(&self) -> f64;
    fn arm_timer(&mut self, seconds: f64, token: PendingToken);
    fn arm_future(&mut self, future: Pin<Box<dyn Future<Output = Value>>>, token: PendingToken);
    fn poll_ready(&mut self) -> Vec<(PendingToken, Value)>;
    fn has_pending(&self) -> bool;
}

#[derive(Debug, Clone)]
struct TimerEntry {
    deadline: f64,
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
        self.deadline.to_bits() == other.deadline.to_bits()
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
            .total_cmp(&self.deadline)
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}

pub(crate) struct MockHost {
    now: f64,
    next_timer_sequence: u64,
    timers: BinaryHeap<TimerEntry>,
    futures: Vec<(PendingToken, Pin<Box<dyn Future<Output = Value>>>)>,
    scripted_future_completions: VecDeque<ScriptedFutureCompletion>,
    poll_count: usize,
}

impl Default for MockHost {
    fn default() -> Self {
        Self {
            now: 0.0,
            next_timer_sequence: 0,
            timers: BinaryHeap::new(),
            futures: Vec::new(),
            scripted_future_completions: VecDeque::new(),
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
    fn now(&self) -> f64 {
        self.now
    }

    fn arm_timer(&mut self, seconds: f64, token: PendingToken) {
        if !seconds.is_finite() {
            return;
        }
        let deadline = self.now + seconds.max(0.0);
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

    fn has_pending(&self) -> bool {
        !self.timers.is_empty() || !self.futures.is_empty()
    }
}

#[cfg(feature = "tokio-host")]
pub(crate) struct TokioHost {
    start: Instant,
    runtime: tokio::runtime::Runtime,
    local: tokio::task::LocalSet,
    ready: RcReadyQueue,
    pending_count: usize,
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
            pending_count: 0,
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
    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn arm_timer(&mut self, seconds: f64, token: PendingToken) {
        if !seconds.is_finite() {
            return;
        }
        let delay = Duration::from_secs_f64(seconds.max(0.0));
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
        self.pending_count += 1;
        self.local.spawn_local(async move {
            let value = future.await;
            ready.borrow_mut().push((token, value));
        });
    }

    fn poll_ready(&mut self) -> Vec<(PendingToken, Value)> {
        self.drive_once();
        let ready = std::mem::take(&mut *self.ready.borrow_mut());
        self.pending_count = self.pending_count.saturating_sub(ready.len());
        ready
    }

    fn has_pending(&self) -> bool {
        self.pending_count > 0
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
        host.arm_timer(2.0, PendingToken(2));
        host.arm_timer(1.0, PendingToken(1));
        host.arm_timer(f64::INFINITY, PendingToken(3));

        assert_eq!(host.poll_ready(), vec![(PendingToken(1), Value::None)]);
        assert_eq!(host.now(), 1.0);
        assert_eq!(host.poll_ready(), vec![(PendingToken(2), Value::None)]);
        assert_eq!(host.now(), 2.0);
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
}

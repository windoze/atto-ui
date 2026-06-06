use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use once_cell::sync::Lazy;
use parking_lot::Mutex;

type TimerId = u64;
type TimerCallback = Box<dyn FnMut() -> bool + Send + 'static>;

const DEFAULT_TIMER_SLOTS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimerHandle {
    id: TimerId,
}

impl TimerHandle {
    pub fn id(self) -> TimerId {
        self.id
    }
}

struct TimerEntry {
    id: TimerId,
    due_tick: u64,
    callback: TimerCallback,
}

/// Tick-based timer wheel used by the global timer utilities.
pub struct TimerWheel {
    tick: u64,
    slots: Vec<Vec<TimerId>>,
    entries: HashMap<TimerId, TimerEntry>,
    in_flight: HashSet<TimerId>,
    canceled_in_flight: HashSet<TimerId>,
    next_id: TimerId,
}

impl TimerWheel {
    pub fn new() -> Self {
        Self::with_slots(DEFAULT_TIMER_SLOTS)
    }

    pub fn with_slots(slot_count: usize) -> Self {
        let slot_count = slot_count.max(1);
        Self {
            tick: 0,
            slots: vec![Vec::new(); slot_count],
            entries: HashMap::new(),
            in_flight: HashSet::new(),
            canceled_in_flight: HashSet::new(),
            next_id: 1,
        }
    }

    /// Registers a timer with an interval in ticks.
    pub fn register<F>(&mut self, interval_ticks: u64, callback: F) -> TimerHandle
    where
        F: FnMut() -> bool + Send + 'static,
    {
        let interval_ticks = interval_ticks.max(1);
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);

        let due_tick = self.tick.saturating_add(interval_ticks);
        let slot = (due_tick % self.slots.len() as u64) as usize;

        self.slots[slot].push(id);
        self.entries.insert(
            id,
            TimerEntry {
                id,
                due_tick,
                callback: Box::new(callback),
            },
        );

        TimerHandle { id }
    }

    pub fn cancel(&mut self, handle: TimerHandle) -> bool {
        if self.entries.remove(&handle.id).is_some() {
            return true;
        }
        if self.in_flight.contains(&handle.id) {
            self.canceled_in_flight.insert(handle.id);
            return true;
        }
        false
    }

    pub fn tick(&mut self) {
        let (tick, due) = self.advance_tick();
        if due.is_empty() {
            return;
        }

        let mut reschedule = Vec::new();
        let mut finished = Vec::new();
        for mut entry in due {
            if (entry.callback)() {
                reschedule.push(entry);
            } else {
                finished.push(entry.id);
            }
        }

        self.finish_tick(tick, reschedule, &finished);
    }

    fn advance_tick(&mut self) -> (u64, Vec<TimerEntry>) {
        self.tick = self.tick.saturating_add(1);
        let slot = (self.tick % self.slots.len() as u64) as usize;
        let bucket = std::mem::take(&mut self.slots[slot]);

        let mut due = Vec::new();
        for id in bucket {
            let Some(entry) = self.entries.get(&id) else {
                continue;
            };
            if entry.due_tick <= self.tick {
                let entry = self.entries.remove(&id).expect("timer entry missing");
                self.in_flight.insert(id);
                due.push(entry);
            } else {
                let target = (entry.due_tick % self.slots.len() as u64) as usize;
                self.slots[target].push(id);
            }
        }

        (self.tick, due)
    }

    fn finish_tick(&mut self, tick: u64, reschedule: Vec<TimerEntry>, finished: &[TimerId]) {
        for id in finished {
            self.in_flight.remove(id);
            self.canceled_in_flight.remove(id);
        }

        for mut entry in reschedule {
            let id = entry.id;
            self.in_flight.remove(&id);
            if self.canceled_in_flight.remove(&id) {
                continue;
            }

            entry.due_tick = tick.saturating_add(1);
            let slot = (entry.due_tick % self.slots.len() as u64) as usize;
            self.slots[slot].push(id);
            self.entries.insert(id, entry);
        }
    }
}

impl Default for TimerWheel {
    fn default() -> Self {
        Self::new()
    }
}

static GLOBAL_TIMER_WHEEL: Lazy<Mutex<TimerWheel>> = Lazy::new(|| Mutex::new(TimerWheel::new()));
static GLOBAL_TICK_RATE_NANOS: AtomicU64 = AtomicU64::new(16_000_000);

/// Register a timer on the global timer wheel with an interval in ticks.
pub fn register_timer<F>(interval_ticks: u64, callback: F) -> TimerHandle
where
    F: FnMut() -> bool + Send + 'static,
{
    let mut wheel = GLOBAL_TIMER_WHEEL.lock();
    wheel.register(interval_ticks, callback)
}

/// Register a timer using a `Duration`, based on the configured global tick rate.
pub fn register_timer_with_duration<F>(interval: Duration, callback: F) -> TimerHandle
where
    F: FnMut() -> bool + Send + 'static,
{
    let ticks = ticks_for_duration(interval, GLOBAL_TICK_RATE_NANOS.load(Ordering::Acquire));
    register_timer(ticks, callback)
}

/// Sets the global tick rate used when registering timers by `Duration`.
pub fn set_global_tick_rate(tick_rate: Duration) {
    let nanos = tick_rate.as_nanos().min(u64::MAX as u128) as u64;
    let nanos = nanos.max(1);
    GLOBAL_TICK_RATE_NANOS.store(nanos, Ordering::Release);
}

/// Cancel a timer by handle. Returns true if the timer was removed or canceled.
pub fn cancel_timer(handle: TimerHandle) -> bool {
    let mut wheel = GLOBAL_TIMER_WHEEL.lock();
    wheel.cancel(handle)
}

/// Advance the global timer wheel by one tick and dispatch due callbacks.
pub fn tick_global_timers() {
    let (tick, due) = {
        let mut wheel = GLOBAL_TIMER_WHEEL.lock();
        wheel.advance_tick()
    };

    if due.is_empty() {
        return;
    }

    let mut reschedule = Vec::new();
    let mut finished = Vec::new();
    for mut entry in due {
        if (entry.callback)() {
            reschedule.push(entry);
        } else {
            finished.push(entry.id);
        }
    }

    let mut wheel = GLOBAL_TIMER_WHEEL.lock();
    wheel.finish_tick(tick, reschedule, &finished);
}

fn ticks_for_duration(duration: Duration, tick_rate_nanos: u64) -> u64 {
    let rate = tick_rate_nanos.max(1) as u128;
    let total = duration.as_nanos();
    let ticks = (total.saturating_add(rate - 1)) / rate;
    ticks.max(1).min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn timer_triggers_after_interval() {
        let mut wheel = TimerWheel::with_slots(8);
        let fired = Arc::new(AtomicUsize::new(0));
        let fired_clone = fired.clone();

        wheel.register(2, move || {
            fired_clone.fetch_add(1, Ordering::SeqCst);
            false
        });

        wheel.tick();
        assert_eq!(fired.load(Ordering::SeqCst), 0);
        wheel.tick();
        assert_eq!(fired.load(Ordering::SeqCst), 1);
        wheel.tick();
        assert_eq!(fired.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn timer_reschedules_on_next_tick() {
        let mut wheel = TimerWheel::with_slots(8);
        let fired = Arc::new(AtomicUsize::new(0));
        let fired_clone = fired.clone();

        wheel.register(1, move || {
            let count = fired_clone.fetch_add(1, Ordering::SeqCst) + 1;
            count < 3
        });

        wheel.tick();
        assert_eq!(fired.load(Ordering::SeqCst), 1);
        wheel.tick();
        assert_eq!(fired.load(Ordering::SeqCst), 2);
        wheel.tick();
        assert_eq!(fired.load(Ordering::SeqCst), 3);
        wheel.tick();
        assert_eq!(fired.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn timer_can_be_canceled() {
        let mut wheel = TimerWheel::with_slots(8);
        let fired = Arc::new(AtomicUsize::new(0));
        let fired_clone = fired.clone();

        let handle = wheel.register(3, move || {
            fired_clone.fetch_add(1, Ordering::SeqCst);
            false
        });

        assert!(wheel.cancel(handle));

        wheel.tick();
        wheel.tick();
        wheel.tick();
        assert_eq!(fired.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn timer_wraps_across_wheel_slots() {
        let mut wheel = TimerWheel::with_slots(2);
        let fired = Arc::new(AtomicUsize::new(0));
        let fired_clone = fired.clone();

        wheel.register(5, move || {
            fired_clone.fetch_add(1, Ordering::SeqCst);
            false
        });

        for _ in 0..4 {
            wheel.tick();
        }
        assert_eq!(fired.load(Ordering::SeqCst), 0);

        wheel.tick();
        assert_eq!(fired.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn timer_zero_interval_fires_on_next_tick() {
        let mut wheel = TimerWheel::with_slots(4);
        let fired = Arc::new(AtomicUsize::new(0));
        let fired_clone = fired.clone();

        wheel.register(0, move || {
            fired_clone.fetch_add(1, Ordering::SeqCst);
            false
        });

        assert_eq!(fired.load(Ordering::SeqCst), 0);
        wheel.tick();
        assert_eq!(fired.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn duration_to_ticks_rounds_up() {
        let tick_rate = 16_000_000;
        assert_eq!(ticks_for_duration(Duration::from_millis(16), tick_rate), 1);
        assert_eq!(ticks_for_duration(Duration::from_millis(17), tick_rate), 2);
        assert_eq!(ticks_for_duration(Duration::from_millis(32), tick_rate), 2);
        assert_eq!(ticks_for_duration(Duration::from_millis(0), tick_rate), 1);
    }
}

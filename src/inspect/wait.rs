//! Wait-condition polling helpers shared by [`DesktopInspector::wait_for`],
//! [`DesktopInspector::wait_for_with_interval`] and
//! [`DesktopInspector::wait_for_predicate`].

use std::thread;
use std::time::{Duration, Instant};

use super::DesktopChangeTracker;

pub(super) fn sleep_until_next_wait_poll(
    deadline: Instant,
    poll_interval: Duration,
    tracker: &mut DesktopChangeTracker,
) {
    if tracker.changed_since_last_poll() {
        return;
    }
    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
        return;
    };
    thread::sleep(remaining.min(poll_interval));
}

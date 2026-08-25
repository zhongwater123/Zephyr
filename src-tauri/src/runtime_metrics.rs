use metrics::{
    Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn, Key, KeyName, Metadata, Recorder,
    SharedString, Unit,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Default)]
struct AtomicCounter(AtomicU64);

impl CounterFn for AtomicCounter {
    fn increment(&self, value: u64) {
        self.0.fetch_add(value, Ordering::Relaxed);
    }

    fn absolute(&self, value: u64) {
        self.0.fetch_max(value, Ordering::Relaxed);
    }
}

#[derive(Default)]
struct AtomicGauge(AtomicU64);

impl AtomicGauge {
    fn update(&self, operation: impl Fn(f64) -> f64) {
        let mut current = self.0.load(Ordering::Relaxed);
        loop {
            let next = operation(f64::from_bits(current)).to_bits();
            match self
                .0
                .compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return,
                Err(actual) => current = actual,
            }
        }
    }
}

impl GaugeFn for AtomicGauge {
    fn increment(&self, value: f64) {
        self.update(|current| current + value);
    }

    fn decrement(&self, value: f64) {
        self.update(|current| current - value);
    }

    fn set(&self, value: f64) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }
}

#[derive(Default)]
struct AtomicHistogram {
    count: AtomicU64,
    latest: AtomicU64,
}

impl HistogramFn for AtomicHistogram {
    fn record(&self, value: f64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.latest.store(value.to_bits(), Ordering::Relaxed);
    }
}

#[derive(Default)]
pub struct AtomicMetricsRecorder {
    sessions_completed: Arc<AtomicCounter>,
    vault_events_dropped: Arc<AtomicCounter>,
    shortcut_backend_transitions: Arc<AtomicCounter>,
    shortcut_registration_errors: Arc<AtomicCounter>,
    shortcut_hook_installed: Arc<AtomicCounter>,
    shortcut_hook_uninstalled: Arc<AtomicCounter>,
    shortcut_hook_reinstalled: Arc<AtomicCounter>,
    shortcut_hook_install_errors: Arc<AtomicCounter>,
    shortcut_hook_events_dropped: Arc<AtomicCounter>,
    vault_queue_depth: Arc<AtomicGauge>,
    recording_duration: Arc<AtomicHistogram>,
}

impl Recorder for AtomicMetricsRecorder {
    fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    fn register_counter(&self, key: &Key, _: &Metadata<'_>) -> Counter {
        match key.name() {
            "voice.sessions.completed" => Counter::from_arc(self.sessions_completed.clone()),
            "incident.vault.events_dropped" => Counter::from_arc(self.vault_events_dropped.clone()),
            "shortcut.backend.transitions" => {
                Counter::from_arc(self.shortcut_backend_transitions.clone())
            }
            "shortcut.registration.errors" => {
                Counter::from_arc(self.shortcut_registration_errors.clone())
            }
            "shortcut.hook.installed" => Counter::from_arc(self.shortcut_hook_installed.clone()),
            "shortcut.hook.uninstalled" => {
                Counter::from_arc(self.shortcut_hook_uninstalled.clone())
            }
            "shortcut.hook.reinstalled" => {
                Counter::from_arc(self.shortcut_hook_reinstalled.clone())
            }
            "shortcut.hook.install_errors" => {
                Counter::from_arc(self.shortcut_hook_install_errors.clone())
            }
            "shortcut.hook.events_dropped" => {
                Counter::from_arc(self.shortcut_hook_events_dropped.clone())
            }
            _ => Counter::noop(),
        }
    }

    fn register_gauge(&self, key: &Key, _: &Metadata<'_>) -> Gauge {
        match key.name() {
            "voice.audio_queue.high_watermark" => Gauge::from_arc(self.vault_queue_depth.clone()),
            _ => Gauge::noop(),
        }
    }

    fn register_histogram(&self, key: &Key, _: &Metadata<'_>) -> Histogram {
        match key.name() {
            "voice.recording.duration_ms" => Histogram::from_arc(self.recording_duration.clone()),
            _ => Histogram::noop(),
        }
    }
}

pub fn install() {
    if metrics::set_global_recorder(AtomicMetricsRecorder::default()).is_err() {
        log::warn!("metrics recorder already installed; keeping the existing recorder");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_handles_only_update_atomic_values() {
        let counter = AtomicCounter::default();
        counter.increment(2);
        counter.absolute(5);
        counter.absolute(3);
        assert_eq!(counter.0.load(Ordering::Relaxed), 5);

        let gauge = AtomicGauge::default();
        gauge.set(2.0);
        gauge.increment(1.5);
        assert_eq!(f64::from_bits(gauge.0.load(Ordering::Relaxed)), 3.5);

        let histogram = AtomicHistogram::default();
        histogram.record(12.0);
        assert_eq!(histogram.count.load(Ordering::Relaxed), 1);
        assert_eq!(
            f64::from_bits(histogram.latest.load(Ordering::Relaxed)),
            12.0
        );
    }
}

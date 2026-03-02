use super::{use_cached_api_resource, use_polling, CacheTtl, LoadingState, Refetch};
use crate::api::{use_sse_json_events, ApiClient, SseState};
use crate::components::{ChartPoint, DataSeries, TimeSeriesData};
use adapteros_api_types::SystemMetricsResponse;
use leptos::prelude::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use wasm_bindgen::JsCast;

const METRICS_HISTORY_SIZE: usize = 60;
const METRICS_SAMPLE_MIN_INTERVAL_MS: u64 = 250;
const METRICS_LERP_FACTOR: f64 = 0.35;
const METRICS_SNAP_EPSILON: f64 = 0.05;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MetricViewMode {
    #[default]
    Throughput,
    Latency,
}

/// Lightweight live metrics snapshot tuned for dashboard/monitoring display.
#[derive(Clone, Copy, Debug, Default)]
pub struct LiveSystemMetricsSnapshot {
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub gpu_utilization: f32,
    pub requests_per_second: f32,
    pub avg_latency_ms: f32,
    pub active_workers: i32,
    pub active_sessions: Option<i32>,
    pub uptime_seconds: u64,
    pub load_1min: f64,
    pub load_5min: f64,
    pub load_15min: f64,
    pub latency_p95_ms: Option<f32>,
    pub tokens_per_second: Option<f32>,
    pub error_rate: Option<f32>,
}

impl LiveSystemMetricsSnapshot {
    fn from_response(metrics: &SystemMetricsResponse) -> Self {
        Self {
            cpu_usage: metrics.cpu_usage_percent.unwrap_or(metrics.cpu_usage),
            memory_usage: metrics.memory_usage_percent.unwrap_or(metrics.memory_usage),
            gpu_utilization: metrics.gpu_utilization,
            requests_per_second: metrics.requests_per_second,
            avg_latency_ms: metrics.avg_latency_ms,
            active_workers: metrics.active_workers,
            active_sessions: metrics.active_sessions,
            uptime_seconds: metrics.uptime_seconds,
            load_1min: metrics.load_average.load_1min,
            load_5min: metrics.load_average.load_5min,
            load_15min: metrics.load_average.load_15min,
            latency_p95_ms: metrics.latency_p95_ms,
            tokens_per_second: metrics.tokens_per_second,
            error_rate: metrics.error_rate,
        }
    }

    fn lerp_toward(self, target: Self, factor: f64) -> Self {
        Self {
            cpu_usage: lerp_f32(self.cpu_usage, target.cpu_usage, factor),
            memory_usage: lerp_f32(self.memory_usage, target.memory_usage, factor),
            gpu_utilization: lerp_f32(self.gpu_utilization, target.gpu_utilization, factor),
            requests_per_second: lerp_f32(
                self.requests_per_second,
                target.requests_per_second,
                factor,
            ),
            avg_latency_ms: lerp_f32(self.avg_latency_ms, target.avg_latency_ms, factor),
            active_workers: lerp_i32(self.active_workers, target.active_workers, factor),
            active_sessions: lerp_option_i32(self.active_sessions, target.active_sessions, factor),
            // Keep uptime monotonic and exact; no interpolation.
            uptime_seconds: target.uptime_seconds,
            load_1min: lerp_f64(self.load_1min, target.load_1min, factor),
            load_5min: lerp_f64(self.load_5min, target.load_5min, factor),
            load_15min: lerp_f64(self.load_15min, target.load_15min, factor),
            latency_p95_ms: lerp_option_f32(self.latency_p95_ms, target.latency_p95_ms, factor),
            tokens_per_second: lerp_option_f32(
                self.tokens_per_second,
                target.tokens_per_second,
                factor,
            ),
            error_rate: lerp_option_f32(self.error_rate, target.error_rate, factor),
        }
    }

    fn is_close_to(&self, other: &Self, epsilon: f64) -> bool {
        is_close_f32(self.cpu_usage, other.cpu_usage, epsilon)
            && is_close_f32(self.memory_usage, other.memory_usage, epsilon)
            && is_close_f32(self.gpu_utilization, other.gpu_utilization, epsilon)
            && is_close_f32(self.requests_per_second, other.requests_per_second, epsilon)
            && is_close_f32(self.avg_latency_ms, other.avg_latency_ms, epsilon)
            && is_close_f64(self.load_1min, other.load_1min, epsilon)
            && is_close_f64(self.load_5min, other.load_5min, epsilon)
            && is_close_f64(self.load_15min, other.load_15min, epsilon)
            && is_close_option_f32(self.latency_p95_ms, other.latency_p95_ms, epsilon)
            && is_close_option_f32(self.tokens_per_second, other.tokens_per_second, epsilon)
            && is_close_option_f32(self.error_rate, other.error_rate, epsilon)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct TimestampedMetrics {
    pub(crate) timestamp: u64,
    pub(crate) requests_per_second: f64,
    pub(crate) avg_latency_ms: f64,
}

impl TimestampedMetrics {
    fn from_snapshot(snapshot: LiveSystemMetricsSnapshot, timestamp: u64) -> Self {
        Self {
            timestamp,
            requests_per_second: snapshot.requests_per_second as f64,
            avg_latency_ms: snapshot.avg_latency_ms as f64,
        }
    }
}

/// History buffer for sparkline + line chart visualizations.
#[derive(Clone, Default)]
pub struct MetricsHistory {
    snapshots: VecDeque<TimestampedMetrics>,
}

impl MetricsHistory {
    pub(crate) fn push(&mut self, snapshot: LiveSystemMetricsSnapshot, timestamp: u64) {
        self.snapshots
            .push_back(TimestampedMetrics::from_snapshot(snapshot, timestamp));
        while self.snapshots.len() > METRICS_HISTORY_SIZE {
            self.snapshots.pop_front();
        }
    }

    fn to_time_series<F>(&self, name: &str, extractor: F) -> TimeSeriesData
    where
        F: Fn(&TimestampedMetrics) -> f64,
    {
        let mut points: Vec<ChartPoint> = self
            .snapshots
            .iter()
            .map(|snapshot| ChartPoint::new(snapshot.timestamp, extractor(snapshot)))
            .collect();

        append_synthetic_second_point_if_needed(&mut points);

        let mut data = TimeSeriesData::new();
        data.series.push(DataSeries {
            name: name.to_string(),
            points,
            color: String::new(),
        });
        data
    }

    pub(crate) fn throughput_series(&self) -> TimeSeriesData {
        self.to_time_series("Requests/sec", |snapshot| snapshot.requests_per_second)
    }

    pub(crate) fn latency_series(&self) -> TimeSeriesData {
        self.to_time_series("Latency (ms)", |snapshot| snapshot.avg_latency_ms)
    }

    pub(crate) fn series_for_mode(&self, mode: MetricViewMode) -> TimeSeriesData {
        match mode {
            MetricViewMode::Throughput => self.throughput_series(),
            MetricViewMode::Latency => self.latency_series(),
        }
    }
}

pub(crate) fn append_synthetic_second_point_if_needed(points: &mut Vec<ChartPoint>) {
    if points.len() != 1 {
        return;
    }

    let first = points[0].clone();
    points.push(ChartPoint::new(
        first.timestamp.saturating_add(1),
        first.value,
    ));
}

#[derive(Clone, Copy)]
pub struct LiveSystemMetricsHandle {
    pub sse_status: RwSignal<SseState>,
    pub target_metrics: RwSignal<Option<LiveSystemMetricsSnapshot>>,
    pub display_metrics: RwSignal<Option<LiveSystemMetricsSnapshot>>,
    pub history: RwSignal<MetricsHistory>,
    pub refetch_fallback: Refetch,
}

pub fn use_live_system_metrics() -> LiveSystemMetricsHandle {
    let target_metrics: RwSignal<Option<LiveSystemMetricsSnapshot>> = RwSignal::new(None);
    let display_metrics: RwSignal<Option<LiveSystemMetricsSnapshot>> = RwSignal::new(None);
    let history: RwSignal<MetricsHistory> = RwSignal::new(MetricsHistory::default());
    let last_metrics_update = StoredValue::new(0u64);

    let (sse_status, _sse_reconnect) = use_sse_json_events::<SystemMetricsResponse, _>(
        "/v1/stream/metrics",
        &["metrics"],
        move |metrics| {
            let now = js_sys::Date::now() as u64;
            apply_metrics_sample(
                &metrics,
                now,
                last_metrics_update,
                target_metrics,
                display_metrics,
                history,
            );
        },
    );

    let (metrics_fallback, refetch_metrics_fallback) = use_cached_api_resource(
        "system_metrics",
        CacheTtl::STATUS,
        |client: Arc<ApiClient>| async move { client.system_metrics().await },
    );

    Effect::new(move || {
        let Some(sse_state) = sse_status.try_get() else {
            return;
        };

        let has_target_metrics = target_metrics.try_get().flatten().is_some();
        if !has_target_metrics
            || matches!(
                sse_state,
                SseState::Error | SseState::CircuitOpen | SseState::Disconnected
            )
        {
            if let Some(LoadingState::Loaded(ref response)) = metrics_fallback.try_get() {
                let now = js_sys::Date::now() as u64;
                apply_metrics_sample(
                    response,
                    now,
                    last_metrics_update,
                    target_metrics,
                    display_metrics,
                    history,
                );
            }
        }
    });

    let refetch_metrics_fallback_stored = StoredValue::new(refetch_metrics_fallback);
    Effect::new(move || {
        let Some(sse_state) = sse_status.try_get() else {
            return;
        };

        if matches!(
            sse_state,
            SseState::Error | SseState::CircuitOpen | SseState::Disconnected
        ) {
            let _ = refetch_metrics_fallback_stored.try_with_value(|refetch| refetch.run(()));
        }
    });

    let refetch_metrics_fallback_poll = refetch_metrics_fallback;
    let _ = use_polling(5_000, move || async move {
        let disconnected = matches!(
            sse_status.get_untracked(),
            SseState::Error | SseState::CircuitOpen | SseState::Disconnected
        );
        if disconnected {
            refetch_metrics_fallback_poll.run(());
        }
    });

    // Use requestAnimationFrame for smooth interpolation instead of 100ms polling.
    // Aligns with browser repaint and reduces timer overhead.
    #[cfg(target_arch = "wasm32")]
    {
        let target_metrics_raf = target_metrics;
        let display_metrics_raf = display_metrics;
        let raf_closure: Rc<RefCell<Option<wasm_bindgen::closure::Closure<dyn FnMut(f64)>>>> =
            Rc::new(RefCell::new(None));
        let raf_closure_for_loop = Rc::clone(&raf_closure);
        let raf_closure_for_closure = Rc::clone(&raf_closure);
        let frame_id_atomic = Arc::new(AtomicI32::new(-1));
        let frame_id_atomic_for_schedule = Arc::clone(&frame_id_atomic);
        let running = Arc::new(AtomicBool::new(true));
        let running_for_closure = Arc::clone(&running);

        *raf_closure_for_loop.borrow_mut() = Some(wasm_bindgen::closure::Closure::new(
            move |_timestamp: f64| {
                if !running_for_closure.load(Ordering::Relaxed) {
                    return;
                }
                let Some(target) = target_metrics_raf.get_untracked() else {
                    schedule_next_frame(&raf_closure_for_closure, &frame_id_atomic_for_schedule);
                    return;
                };
                let next = match display_metrics_raf.get_untracked() {
                    Some(current) => {
                        let interpolated = current.lerp_toward(target, METRICS_LERP_FACTOR);
                        if interpolated.is_close_to(&target, METRICS_SNAP_EPSILON) {
                            target
                        } else {
                            interpolated
                        }
                    }
                    None => target,
                };
                let _ = display_metrics_raf.try_set(Some(next));
                schedule_next_frame(&raf_closure_for_closure, &frame_id_atomic_for_schedule);
            },
        ));

        if let Some(window) = web_sys::window() {
            if let Some(closure) = raf_closure.borrow().as_ref() {
                if let Ok(id) = window.request_animation_frame(closure.as_ref().unchecked_ref()) {
                    frame_id_atomic.store(id, Ordering::Relaxed);
                }
            }
        }

        let frame_id_for_cleanup = Arc::clone(&frame_id_atomic);
        on_cleanup(move || {
            running.store(false, Ordering::Relaxed);
            let id = frame_id_for_cleanup.swap(-1, Ordering::Relaxed);
            if id >= 0 {
                if let Some(window) = web_sys::window() {
                    let _ = window.cancel_animation_frame(id);
                }
            }
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        // On native (e.g. tests), no interpolation loop; display_metrics set by apply_metrics_sample.
    }

    LiveSystemMetricsHandle {
        sse_status,
        target_metrics,
        display_metrics,
        history,
        refetch_fallback: refetch_metrics_fallback,
    }
}

#[cfg(target_arch = "wasm32")]
fn schedule_next_frame(
    raf_closure: &Rc<RefCell<Option<wasm_bindgen::closure::Closure<dyn FnMut(f64)>>>>,
    frame_id: &Arc<AtomicI32>,
) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let guard = raf_closure.borrow();
    let Some(closure) = guard.as_ref() else {
        return;
    };
    if let Ok(id) = window.request_animation_frame(closure.as_ref().unchecked_ref()) {
        frame_id.store(id, Ordering::Relaxed);
    }
}

fn apply_metrics_sample(
    metrics: &SystemMetricsResponse,
    now: u64,
    last_metrics_update: StoredValue<u64>,
    target_metrics: RwSignal<Option<LiveSystemMetricsSnapshot>>,
    display_metrics: RwSignal<Option<LiveSystemMetricsSnapshot>>,
    history: RwSignal<MetricsHistory>,
) {
    let Some(last) = last_metrics_update.try_get_value() else {
        return;
    };

    if now.saturating_sub(last) < METRICS_SAMPLE_MIN_INTERVAL_MS {
        return;
    }

    let _ = last_metrics_update.try_set_value(now);

    let snapshot = LiveSystemMetricsSnapshot::from_response(metrics);
    let _ = target_metrics.try_set(Some(snapshot));
    let _ = history.try_update(|buffer| buffer.push(snapshot, now));

    let _ = display_metrics.try_update(|current| {
        if current.is_none() {
            *current = Some(snapshot);
        }
    });
}

fn lerp_f32(current: f32, target: f32, factor: f64) -> f32 {
    lerp_f64(current as f64, target as f64, factor) as f32
}

fn lerp_f64(current: f64, target: f64, factor: f64) -> f64 {
    let clamped = factor.clamp(0.0, 1.0);
    current + (target - current) * clamped
}

fn lerp_i32(current: i32, target: i32, factor: f64) -> i32 {
    lerp_f64(current as f64, target as f64, factor).round() as i32
}

fn lerp_option_f32(current: Option<f32>, target: Option<f32>, factor: f64) -> Option<f32> {
    match (current, target) {
        (Some(current), Some(target)) => Some(lerp_f32(current, target, factor)),
        _ => target,
    }
}

fn lerp_option_i32(current: Option<i32>, target: Option<i32>, factor: f64) -> Option<i32> {
    match (current, target) {
        (Some(current), Some(target)) => Some(lerp_i32(current, target, factor)),
        _ => target,
    }
}

fn is_close_f32(current: f32, target: f32, epsilon: f64) -> bool {
    is_close_f64(current as f64, target as f64, epsilon)
}

fn is_close_f64(current: f64, target: f64, epsilon: f64) -> bool {
    (current - target).abs() <= epsilon
}

fn is_close_option_f32(current: Option<f32>, target: Option<f32>, epsilon: f64) -> bool {
    match (current, target) {
        (Some(current), Some(target)) => is_close_f32(current, target, epsilon),
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_synthetic_second_point_when_series_has_one_point() {
        let mut points = vec![ChartPoint::new(1700000000, 42.0)];
        append_synthetic_second_point_if_needed(&mut points);

        assert_eq!(points.len(), 2);
        assert_eq!(points[1].timestamp, 1700000001);
        assert_eq!(points[1].value, 42.0);
    }
}

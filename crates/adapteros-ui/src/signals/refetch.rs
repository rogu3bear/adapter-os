//! Global refetch coordination context
//!
//! Provides a way to trigger refetches across components without prop drilling.
//! Components can register for specific "topics" and trigger refetches globally.
//!
//! Lifecycle event dispatchers (`dispatch_adapter_event`, `dispatch_training_event`,
//! `dispatch_health_event`) translate typed SSE events into refetch topic triggers.

use crate::api::types::{
    AdapterLifecycleEvent, AdapterVersionEvent, SystemHealthTransitionEvent, TrainingLifecycleEvent,
};
use crate::hooks::cache_invalidate;
use leptos::prelude::*;
use std::collections::HashMap;

/// Refetch topic identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefetchTopic {
    /// Adapters list
    Adapters,
    /// Adapter stacks
    Stacks,
    /// Training jobs
    TrainingJobs,
    /// Repositories
    Repositories,
    /// API keys
    ApiKeys,
    /// Users
    Users,
    /// System health
    Health,
    /// Models
    Models,
    /// Workers list
    Workers,
    /// All topics
    All,
}

impl RefetchTopic {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Adapters => "adapters",
            Self::Stacks => "stacks",
            Self::TrainingJobs => "training_jobs",
            Self::Repositories => "repositories",
            Self::ApiKeys => "api_keys",
            Self::Users => "users",
            Self::Health => "health",
            Self::Models => "models",
            Self::Workers => "workers",
            Self::All => "all",
        }
    }
}

/// Refetch state tracking
#[derive(Debug, Clone, Default)]
pub struct RefetchState {
    /// Counter for each topic - increment to trigger refetch
    pub counters: HashMap<RefetchTopic, u32>,
}

impl RefetchState {
    pub fn new() -> Self {
        Self {
            counters: HashMap::new(),
        }
    }

    /// Get counter for a topic
    pub fn get_counter(&self, topic: RefetchTopic) -> u32 {
        self.counters.get(&topic).copied().unwrap_or(0)
    }
}

/// Refetch action helper
#[derive(Clone)]
pub struct RefetchAction {
    state: RwSignal<RefetchState>,
}

impl RefetchAction {
    pub fn new(state: RwSignal<RefetchState>) -> Self {
        Self { state }
    }

    /// Trigger a refetch for a specific topic
    pub fn trigger(&self, topic: RefetchTopic) {
        self.state.update(|state| {
            let counter = state.counters.entry(topic).or_insert(0);
            *counter = counter.wrapping_add(1);
            invalidate_cache_for_topic(topic);

            // If All is triggered, also increment all individual topics
            if topic == RefetchTopic::All {
                for t in [
                    RefetchTopic::Adapters,
                    RefetchTopic::Stacks,
                    RefetchTopic::TrainingJobs,
                    RefetchTopic::Repositories,
                    RefetchTopic::ApiKeys,
                    RefetchTopic::Users,
                    RefetchTopic::Health,
                    RefetchTopic::Models,
                    RefetchTopic::Workers,
                ] {
                    let counter = state.counters.entry(t).or_insert(0);
                    *counter = counter.wrapping_add(1);
                    invalidate_cache_for_topic(t);
                }
            }
        });
    }

    /// Trigger refetch for adapters
    pub fn adapters(&self) {
        self.trigger(RefetchTopic::Adapters);
    }

    /// Trigger refetch for stacks
    pub fn stacks(&self) {
        self.trigger(RefetchTopic::Stacks);
    }

    /// Trigger refetch for training jobs
    pub fn training_jobs(&self) {
        self.trigger(RefetchTopic::TrainingJobs);
    }

    /// Trigger refetch for repositories
    pub fn repositories(&self) {
        self.trigger(RefetchTopic::Repositories);
    }

    /// Trigger refetch for models
    pub fn models(&self) {
        self.trigger(RefetchTopic::Models);
    }

    /// Trigger refetch for workers
    pub fn workers(&self) {
        self.trigger(RefetchTopic::Workers);
    }

    /// Trigger refetch for system health
    pub fn health(&self) {
        self.trigger(RefetchTopic::Health);
    }

    /// Trigger refetch for all topics
    pub fn all(&self) {
        self.trigger(RefetchTopic::All);
    }

    /// Dispatch an adapter lifecycle event to the appropriate refetch topics.
    pub fn dispatch_adapter_event(&self, event: &AdapterLifecycleEvent) {
        match event {
            AdapterLifecycleEvent::Promoted { .. }
            | AdapterLifecycleEvent::Loaded { .. }
            | AdapterLifecycleEvent::LoadFailed { .. }
            | AdapterLifecycleEvent::Evicted { .. } => {
                self.adapters();
                self.models();
            }
        }
    }

    /// Dispatch an adapter version event to the appropriate refetch topics.
    pub fn dispatch_adapter_version_event(&self, event: &AdapterVersionEvent) {
        match event {
            AdapterVersionEvent::VersionPromoted { .. }
            | AdapterVersionEvent::VersionRolledBack { .. }
            | AdapterVersionEvent::AutoRollbackApplied { .. } => {
                self.adapters();
                self.repositories();
                self.models();
            }
        }
    }

    /// Dispatch a training lifecycle event to the appropriate refetch topics.
    pub fn dispatch_training_event(&self, event: &TrainingLifecycleEvent) {
        match event {
            TrainingLifecycleEvent::JobStarted { .. }
            | TrainingLifecycleEvent::EpochCompleted { .. }
            | TrainingLifecycleEvent::CheckpointSaved { .. }
            | TrainingLifecycleEvent::JobFailed { .. } => {
                self.training_jobs();
            }
            TrainingLifecycleEvent::JobCompleted { .. } => {
                // Completed training may produce a new adapter
                self.training_jobs();
                self.adapters();
                self.models();
            }
        }
    }

    /// Dispatch a system health transition event to the appropriate refetch topics.
    pub fn dispatch_health_event(&self, event: &SystemHealthTransitionEvent) {
        match event {
            SystemHealthTransitionEvent::WorkerStateChanged { .. }
            | SystemHealthTransitionEvent::DrainStarted { .. } => {
                self.workers();
                self.health();
            }
            SystemHealthTransitionEvent::AdapterEvicted { .. } => {
                self.models();
                self.health();
            }
        }
    }
}

/// Invalidate SWR cache entries associated with a refetch topic so that
/// re-navigation shows fresh data instead of a stale cache hit.
fn invalidate_cache_for_topic(topic: RefetchTopic) {
    match topic {
        RefetchTopic::Adapters => cache_invalidate("adapters_list"),
        RefetchTopic::TrainingJobs => cache_invalidate("training_jobs_list"),
        RefetchTopic::Workers => {
            cache_invalidate("workers_detail");
            cache_invalidate("workers_list");
            cache_invalidate("nodes_list");
        }
        RefetchTopic::Models => {
            cache_invalidate("models_status");
            cache_invalidate("models_list");
        }
        RefetchTopic::Health => {
            cache_invalidate("system_status");
            cache_invalidate("system_metrics");
        }
        // Stacks, Repositories, ApiKeys, Users, All — no cached keys yet
        _ => {}
    }
}

/// Refetch context type
pub type RefetchContext = (ReadSignal<RefetchState>, RefetchAction);

/// Provide refetch context at the app root
pub fn provide_refetch_context() {
    let state = RwSignal::new(RefetchState::new());
    let action = RefetchAction::new(state);
    provide_context((state.read_only(), action));
}

/// Use refetch context - panics if not provided
pub fn use_refetch_context() -> RefetchContext {
    expect_context::<RefetchContext>()
}

/// Get the refetch action for triggering refetches
pub fn use_refetch() -> RefetchAction {
    use_refetch_context().1
}

/// Get read-only refetch state for subscribing to changes
pub fn use_refetch_state() -> ReadSignal<RefetchState> {
    use_refetch_context().0
}

/// Create a signal that updates when a topic is refetched
///
/// Use this to trigger effects when a particular topic is refetched:
/// ```ignore
/// let counter = use_refetch_signal(RefetchTopic::Adapters);
/// Effect::new(move || {
///     let _ = counter.get();
///     // Refetch data here
/// });
/// ```
pub fn use_refetch_signal(topic: RefetchTopic) -> Signal<u32> {
    let state = use_refetch_state();
    Signal::derive(move || state.try_get().map(|s| s.get_counter(topic)).unwrap_or(0))
}

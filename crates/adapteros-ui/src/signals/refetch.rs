//! Global refetch coordination context
//!
//! Provides a way to trigger refetches across components without prop drilling.
//! Components can register for specific "topics" and trigger refetches globally.

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
                ] {
                    let counter = state.counters.entry(t).or_insert(0);
                    *counter = counter.wrapping_add(1);
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

    /// Trigger refetch for all topics
    pub fn all(&self) {
        self.trigger(RefetchTopic::All);
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
    Signal::derive(move || state.get().get_counter(topic))
}

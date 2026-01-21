//! Application pages
//!
//! Route page components for the application.

pub mod adapters;
pub mod admin;
pub mod agents;
pub mod audit;
pub mod chat;
pub mod collections;
pub mod dashboard;
pub mod datasets;
pub mod diff;
pub mod documents;
pub mod errors;
pub mod flight_recorder;
pub mod login;
pub mod models;
pub mod monitoring;
pub mod not_found;
pub mod policies;
pub mod repositories;
pub mod reviews;
pub mod routing;
pub mod safe;
pub mod settings;
pub mod stacks;
pub mod style_audit;
pub mod system;
pub mod training;
pub mod workers;

pub use adapters::{AdapterDetail, Adapters};
pub use admin::Admin;
pub use agents::Agents;
pub use audit::Audit;
pub use chat::{Chat, ChatSession};
pub use collections::{CollectionDetail, Collections};
pub use dashboard::Dashboard;
pub use datasets::{DatasetDetail, Datasets};
pub use diff::Diff;
pub use documents::{DocumentDetail, Documents};
pub use errors::Errors;
pub use flight_recorder::{FlightRecorder, FlightRecorderDetail};
pub use login::Login;
pub use models::Models;
pub use monitoring::Monitoring;
pub use not_found::NotFound;
pub use policies::Policies;
pub use repositories::{Repositories, RepositoryDetail};
pub use reviews::Reviews;
pub use routing::Routing;
pub use safe::Safe;
pub use settings::Settings;
pub use stacks::{StackDetail, Stacks};
pub use style_audit::StyleAudit;
pub use system::System;
pub use training::Training;
pub use workers::{WorkerDetail, Workers};

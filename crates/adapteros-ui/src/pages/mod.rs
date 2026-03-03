//! Application pages
//!
//! Route page components for the application.

pub mod adapters;
pub mod admin;
pub mod audit;
pub mod chat;
pub mod dashboard;
pub mod datasets;
pub mod documents;
pub mod flight_recorder;
pub mod login;
pub mod models;
pub mod not_found;
pub mod policies;
pub mod safe;
pub mod settings;
pub mod system;
pub mod training;
pub mod update_center;
pub mod user;
pub mod welcome;
pub mod workers;

pub use adapters::{AdapterDetail, Adapters};
pub use admin::Admin;
pub use audit::Audit;
pub use chat::{Chat, ChatHistory, ChatSession, ChatSessionEquivalent};
pub use dashboard::Dashboard;
pub use datasets::{DatasetDetail, Datasets};
pub use documents::{DocumentDetail, Documents};
pub use flight_recorder::{FlightRecorder, FlightRecorderDetail};
pub use login::Login;
pub use models::{ModelDetail, Models};
pub use not_found::NotFound;
pub use policies::Policies;
pub use safe::Safe;
pub use settings::Settings;
pub use system::System;
pub use training::Training;
pub use update_center::UpdateCenter;
pub use user::User;
pub use welcome::Welcome;
pub use workers::{WorkerDetail, Workers};

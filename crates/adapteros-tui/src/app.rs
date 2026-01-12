use anyhow::Result;
use std::time::Instant;
use tokio::time::Duration;
use tracing::{debug, info, warn};

/// Menu items displayed on the dashboard
const MENU_ITEMS: &[&str] = &[
    "Boot All Services",
    "Service Control",
    "Debug Service",
    "View Metrics",
    "View Logs",
    "Edit Config",
    "Toggle Mode",
];

/// Configuration fields in the config editor
const CONFIG_FIELDS: &[&str] = &[
    "server_port",
    "max_connections",
    "model_path",
    "k_sparse_value",
    "batch_size",
    "cache_size_mb",
    "jwt_mode",
    "require_pf_deny",
];

pub mod api;
pub mod config_io;
pub mod db;
pub mod service_control;
pub mod sse;
pub mod types;

use api::{AdapterInfo, ApiClient, HealthStatus, LogQuery};
use config_io::TuiConfig;
use db::{DbClient, DbStatsSummary};
use service_control::ServiceControl;
use sse::SseClient;
use types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Services,
    Adapters,
    Training,
    Chat,
    Logs,
    Metrics,
    Config,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    ServiceSelect,
    ConfigEdit,
    ChatInput,
    Filter(LogFilterMode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFilterMode {
    TraceId,
    Tenant,
}

pub struct App {
    // Navigation
    pub current_screen: Screen,
    pub current_mode: Mode,
    pub selected_menu_item: usize,
    pub selected_service: usize,
    pub selected_adapter: usize,
    pub selected_config_field: usize,
    pub config_edit_value: String,

    // Status
    pub model_status: ModelStatus,
    pub system_status: SystemStatus,
    pub services: Vec<ServiceStatus>,

    // Live data
    pub metrics: SystemMetrics,
    pub recent_logs: Vec<LogEntry>,
    pub adapters: Vec<AdapterInfo>,
    pub db_stats: DbStatsSummary,
    pub log_filter_trace: Option<String>,
    pub log_filter_tenant: Option<String>,
    pub log_filter_input: String,
    pub log_filter_mode: Option<LogFilterMode>,

    // Adapter browser state
    #[allow(dead_code)]
    pub adapter_filter: String,

    // Training jobs state
    pub selected_training_job: usize,
    pub training_jobs: Vec<TrainingJobInfo>,

    // Chat state
    pub chat_messages: Vec<ChatMessage>,
    pub chat_input: String,
    pub chat_streaming: bool,

    // Configuration
    pub config: SystemConfig,
    pub production_mode: bool,

    // UI State
    pub show_help: bool,
    pub confirmation_message: Option<String>,
    pub error_message: Option<String>,
    pub last_update: Instant,
    pub last_prereq_check: Instant,
    pub setup_state: SetupState,
    pub health_status: Option<HealthStatus>,
    pub server_connected: bool,

    // API Client
    api_client: ApiClient,

    // Database Client
    db_client: DbClient,

    service_control: ServiceControl,

    // Persistent TUI configuration
    tui_config: TuiConfig,

    // SSE Client for streaming operations
    sse_client: SseClient,
}

impl App {
    pub async fn new() -> Result<Self> {
        // Load persisted config, use its server_url if available
        let tui_config = TuiConfig::load().unwrap_or_default();
        Self::new_with_config(tui_config).await
    }

    #[allow(dead_code)]
    pub async fn new_with_url(server_url: String) -> Result<Self> {
        // Load persisted config but override server_url with provided value
        let mut tui_config = TuiConfig::load().unwrap_or_default();
        tui_config.server_url = server_url;
        Self::new_with_config(tui_config).await
    }

    async fn new_with_config(tui_config: TuiConfig) -> Result<Self> {
        let api_client = ApiClient::new(tui_config.server_url.clone())?;
        let sse_client = SseClient::new(tui_config.server_url.clone());
        let db_client = DbClient::new().await?;
        let service_control = ServiceControl::new()?;
        let setup_state = SetupState::new(service_control.missing_prereqs());

        Ok(Self {
            current_screen: Screen::Dashboard,
            current_mode: Mode::Normal,
            selected_menu_item: 0,
            selected_service: 0,
            selected_adapter: 0,
            selected_config_field: 0,
            config_edit_value: String::new(),

            model_status: ModelStatus {
                name: "llama-7b-lora-q15".to_string(),
                loaded: false,
                memory_usage_mb: 0,
                total_memory_mb: 1024,
            },

            system_status: SystemStatus {
                cpu_percent: 0.0,
                network_rx_mbps: 0.0,
                network_tx_mbps: 0.0,
            },

            services: vec![
                ServiceStatus {
                    name: "Database".to_string(),
                    status: Status::Stopped,
                    message: "Not started".to_string(),
                },
                ServiceStatus {
                    name: "Router".to_string(),
                    status: Status::Stopped,
                    message: "Not started".to_string(),
                },
                ServiceStatus {
                    name: "Metrics System".to_string(),
                    status: Status::Stopped,
                    message: "Not started".to_string(),
                },
                ServiceStatus {
                    name: "Policy Engine".to_string(),
                    status: Status::Stopped,
                    message: "Not started".to_string(),
                },
                ServiceStatus {
                    name: "Training Service".to_string(),
                    status: Status::Stopped,
                    message: "Not started".to_string(),
                },
                ServiceStatus {
                    name: "Telemetry".to_string(),
                    status: Status::Stopped,
                    message: "Not started".to_string(),
                },
            ],

            metrics: SystemMetrics::default(),
            recent_logs: Vec::new(),
            adapters: Vec::new(),
            db_stats: DbStatsSummary {
                total_adapters: 0,
                total_training_jobs: 0,
                active_training_jobs: 0,
                total_tenants: 0,
                database_connected: false,
            },
            log_filter_trace: None,
            log_filter_tenant: None,
            log_filter_input: String::new(),
            log_filter_mode: None,

            // Adapter browser state
            adapter_filter: String::new(),

            // Training jobs state
            selected_training_job: 0,
            training_jobs: Vec::new(),

            // Chat state
            chat_messages: Vec::new(),
            chat_input: String::new(),
            chat_streaming: false,

            config: SystemConfig::default(),
            production_mode: false,

            show_help: false,
            confirmation_message: None,
            error_message: None,
            last_update: Instant::now(),
            last_prereq_check: Instant::now(),
            setup_state,
            health_status: None,
            server_connected: false,

            api_client,
            sse_client,
            db_client,

            service_control,

            tui_config,
        })
    }

    pub async fn update(&mut self) -> Result<()> {
        // Update every second
        if self.last_update.elapsed() < Duration::from_secs(1) {
            return Ok(());
        }

        self.last_update = Instant::now();

        if self.last_prereq_check.elapsed() > Duration::from_secs(10) {
            self.setup_state.missing_prereqs = self.service_control.missing_prereqs();
            self.last_prereq_check = Instant::now();
        }

        if let Ok(health) = self.api_client.get_health().await {
            let status = health.status.to_lowercase();
            self.health_status = Some(health.clone());
            self.setup_state.infrastructure_online =
                matches!(status.as_str(), "healthy" | "ready" | "running" | "online");
            self.server_connected = true;
        } else {
            self.setup_state.infrastructure_online = false;
            self.health_status = None;
            self.server_connected = false;
        }

        // Try to fetch real data from API
        if let Ok(metrics) = self.api_client.get_metrics().await {
            self.metrics = metrics;
        }

        // Fetch real service status from API
        if let Ok(service_statuses) = self.api_client.get_service_status().await {
            for status_response in service_statuses {
                if let Some(service) = self
                    .services
                    .iter_mut()
                    .find(|s| s.name == status_response.name)
                {
                    // Map API status to our Status enum
                    service.status = match status_response.status.as_str() {
                        "running" | "Running" => Status::Running,
                        "starting" | "Starting" => Status::Starting,
                        "stopped" | "Stopped" => Status::Stopped,
                        "failed" | "Failed" => Status::Failed,
                        "warning" | "Warning" => Status::Warning,
                        _ => Status::Stopped,
                    };

                    // Update message from API or use status as fallback
                    service.message = status_response
                        .message
                        .unwrap_or_else(|| service.status.as_str().to_string());

                    debug!(
                        service = %service.name,
                        status = %service.status.as_str(),
                        "Updated service status from API"
                    );
                }
            }
        }

        // Fetch adapter list from API
        if let Ok(adapters) = self.api_client.get_adapters().await {
            self.adapters = adapters;
            debug!(count = self.adapters.len(), "Updated adapter list from API");
        }

        // Fetch training jobs from API
        if let Ok(jobs) = self.api_client.list_training_jobs().await {
            self.training_jobs = jobs
                .into_iter()
                .map(|j| TrainingJobInfo {
                    id: j.id.clone(),
                    status: j.status,
                    progress_pct: j.progress_pct,
                    current_epoch: j.current_epoch,
                    total_epochs: 0,
                    current_batch: 0,
                    total_batches: 0,
                    current_loss: j.current_loss,
                    learning_rate: 0.0,
                    tokens_per_second: j.tokens_per_second,
                    dataset_name: j.dataset_name,
                    dataset_samples: None,
                    backend: j.backend,
                    checkpoints_saved: 0,
                    started_at: None,
                })
                .collect();
            debug!(
                count = self.training_jobs.len(),
                "Updated training jobs from API"
            );
        }

        // Fetch logs from API
        let log_query = LogQuery {
            tenant_id: self.log_filter_tenant.clone(),
            trace_id: self.log_filter_trace.clone(),
        };
        if let Ok(logs) = self.api_client.get_logs(&log_query).await {
            self.recent_logs = logs;
        }

        // Fetch database stats
        if let Ok(db_stats) = self.db_client.get_stats_summary().await {
            debug!(
                adapters = db_stats.total_adapters,
                training_jobs = db_stats.total_training_jobs,
                active_jobs = db_stats.active_training_jobs,
                tenants = db_stats.total_tenants,
                connected = db_stats.database_connected,
                "Updated database stats"
            );
            self.db_stats = db_stats;
        }

        // Update model status based on Router service
        self.model_status.loaded = self
            .services
            .iter()
            .any(|s| s.name == "Router" && s.status == Status::Running);

        // Calculate memory usage from loaded adapters
        if self.model_status.loaded {
            let adapter_memory: u32 = self
                .adapters
                .iter()
                .filter(|a| a.loaded)
                .filter_map(|a| a.memory_mb)
                .sum();

            // Base model memory (estimated) + adapter memory
            self.model_status.memory_usage_mb = 256 + adapter_memory;
        } else {
            self.model_status.memory_usage_mb = 0;
        }

        // Clear old error messages after 3 seconds
        if self.error_message.is_some() && self.last_update.elapsed() > Duration::from_secs(3) {
            self.error_message = None;
        }

        Ok(())
    }

    pub fn filtered_logs(&self) -> Vec<&LogEntry> {
        self.recent_logs
            .iter()
            .filter(|entry| {
                if let Some(trace) = &self.log_filter_trace {
                    if entry.trace_id.as_deref() != Some(trace.as_str()) {
                        return false;
                    }
                }

                if let Some(tenant) = &self.log_filter_tenant {
                    if entry.tenant_id.as_deref() != Some(tenant.as_str()) {
                        return false;
                    }
                }

                true
            })
            .collect()
    }

    fn apply_log_filter(&mut self, mode: LogFilterMode) {
        let value = self.log_filter_input.trim();
        match mode {
            LogFilterMode::TraceId => {
                self.log_filter_trace = if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                };
            }
            LogFilterMode::Tenant => {
                self.log_filter_tenant = if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                };
            }
        }
        self.log_filter_input.clear();
        self.log_filter_mode = None;
    }

    // Navigation handlers
    pub fn on_up(&mut self) {
        // Handle screen-specific navigation first
        if self.current_screen == Screen::Adapters && self.current_mode == Mode::Normal {
            if self.selected_adapter > 0 {
                self.selected_adapter -= 1;
            }
            return;
        }

        match self.current_mode {
            Mode::ServiceSelect => {
                if self.selected_service > 0 {
                    self.selected_service -= 1;
                }
            }
            Mode::ConfigEdit => {
                if self.selected_config_field > 0 {
                    self.selected_config_field -= 1;
                }
            }
            Mode::ChatInput | Mode::Filter(_) => {}
            Mode::Normal => {
                if self.selected_menu_item > 0 {
                    self.selected_menu_item -= 1;
                }
            }
        }
    }

    pub fn on_down(&mut self) {
        // Handle screen-specific navigation first
        if self.current_screen == Screen::Adapters && self.current_mode == Mode::Normal {
            if !self.adapters.is_empty() && self.selected_adapter < self.adapters.len() - 1 {
                self.selected_adapter += 1;
            }
            return;
        }

        match self.current_mode {
            Mode::ServiceSelect => {
                if self.selected_service < self.services.len() - 1 {
                    self.selected_service += 1;
                }
            }
            Mode::ConfigEdit => {
                if self.selected_config_field < CONFIG_FIELDS.len() - 1 {
                    self.selected_config_field += 1;
                }
            }
            Mode::ChatInput | Mode::Filter(_) => {}
            Mode::Normal => {
                if self.selected_menu_item < MENU_ITEMS.len() - 1 {
                    self.selected_menu_item += 1;
                }
            }
        }
    }

    pub fn on_left(&mut self) {
        // Navigate between screens
        self.current_screen = match self.current_screen {
            Screen::Dashboard => Screen::Help,
            Screen::Services => Screen::Dashboard,
            Screen::Adapters => Screen::Services,
            Screen::Training => Screen::Adapters,
            Screen::Chat => Screen::Training,
            Screen::Logs => Screen::Chat,
            Screen::Metrics => Screen::Logs,
            Screen::Config => Screen::Metrics,
            Screen::Help => Screen::Config,
        };
    }

    pub fn on_right(&mut self) {
        // Navigate between screens
        self.current_screen = match self.current_screen {
            Screen::Dashboard => Screen::Services,
            Screen::Services => Screen::Adapters,
            Screen::Adapters => Screen::Training,
            Screen::Training => Screen::Chat,
            Screen::Chat => Screen::Logs,
            Screen::Logs => Screen::Metrics,
            Screen::Metrics => Screen::Config,
            Screen::Config => Screen::Help,
            Screen::Help => Screen::Dashboard,
        };
    }

    pub async fn on_enter(&mut self) -> Result<()> {
        match self.current_mode {
            Mode::Normal => {
                // Handle Chat screen specially - Enter switches to ChatInput mode
                if self.current_screen == Screen::Chat {
                    self.current_mode = Mode::ChatInput;
                    return Ok(());
                }

                // Dashboard menu actions
                match self.selected_menu_item {
                    0 => self.boot_all_services().await?,
                    1 => self.current_mode = Mode::ServiceSelect,
                    2 => self.debug_service().await?,
                    3 => self.current_screen = Screen::Metrics,
                    4 => self.current_screen = Screen::Logs,
                    5 => {
                        self.current_screen = Screen::Config;
                        self.current_mode = Mode::ConfigEdit;
                    }
                    6 => self.toggle_production_mode(),
                    _ => {}
                }
            }
            Mode::ServiceSelect => {
                self.boot_single_service(self.selected_service).await?;
                self.current_mode = Mode::Normal;
            }
            Mode::ConfigEdit => {
                self.save_config_value();
                self.current_mode = Mode::Normal;
            }
            Mode::ChatInput => {
                self.send_chat_message().await?;
            }
            Mode::Filter(mode) => {
                self.apply_log_filter(mode);
                self.current_mode = Mode::Normal;
            }
        }
        Ok(())
    }

    pub fn on_tab(&mut self) {
        // Cycle through screens
        self.on_right();
    }

    pub fn on_backtab(&mut self) {
        // Reverse cycle through screens
        self.on_left();
    }

    pub fn on_backspace(&mut self) {
        match self.current_mode {
            Mode::ConfigEdit => {
                self.config_edit_value.pop();
            }
            Mode::ChatInput => {
                self.chat_input.pop();
            }
            Mode::Filter(_) => {
                self.log_filter_input.pop();
            }
            _ => {}
        };
    }

    pub fn on_escape(&mut self) {
        match self.current_mode {
            Mode::ServiceSelect | Mode::ConfigEdit | Mode::Filter(_) => {
                self.current_mode = Mode::Normal;
                self.log_filter_input.clear();
                self.log_filter_mode = None;
            }
            Mode::ChatInput => {
                // Exit chat input mode but don't clear the input
                self.current_mode = Mode::Normal;
            }
            Mode::Normal => {
                if self.show_help {
                    self.show_help = false;
                } else {
                    self.current_screen = Screen::Dashboard;
                }
            }
        }
    }

    pub async fn on_char(&mut self, c: char) -> Result<()> {
        if self.current_mode == Mode::ConfigEdit {
            if c == '\n' {
                self.save_config_value();
                self.current_mode = Mode::Normal;
            } else {
                self.config_edit_value.push(c);
            }
            return Ok(());
        }

        if self.current_mode == Mode::ChatInput {
            if c == '\n' {
                self.send_chat_message().await?;
            } else {
                self.chat_input.push(c);
            }
            return Ok(());
        }

        if let Mode::Filter(mode) = self.current_mode {
            if c == '\n' {
                self.apply_log_filter(mode);
                self.current_mode = Mode::Normal;
            } else {
                self.log_filter_input.push(c);
            }
            return Ok(());
        }

        if self.current_mode == Mode::ServiceSelect {
            match c {
                's' | 'S' => {
                    self.boot_single_service(self.selected_service).await?;
                    self.current_mode = Mode::Normal;
                }
                'r' | 'R' => {
                    self.restart_service(self.selected_service).await?;
                    self.current_mode = Mode::Normal;
                }
                'x' | 'X' => {
                    self.stop_service(self.selected_service).await?;
                    self.current_mode = Mode::Normal;
                }
                _ => {}
            }
            return Ok(());
        }

        // Screen-specific action handlers (must be before general shortcuts)
        // Adapters screen: L=Load, U=Unload, P=Pin/Unpin
        if self.current_screen == Screen::Adapters {
            if let Some(adapter) = self.adapters.get(self.selected_adapter).cloned() {
                let adapter_id = adapter.id.clone();
                match c {
                    'l' | 'L' => {
                        match self.api_client.load_adapter(&adapter_id).await {
                            Ok(_) => {
                                self.confirmation_message =
                                    Some(format!("Loading adapter: {}", adapter_id))
                            }
                            Err(e) => self.error_message = Some(format!("Failed to load: {}", e)),
                        }
                        return Ok(());
                    }
                    'u' | 'U' => {
                        match self.api_client.unload_adapter(&adapter_id).await {
                            Ok(_) => {
                                self.confirmation_message =
                                    Some(format!("Unloading adapter: {}", adapter_id))
                            }
                            Err(e) => self.error_message = Some(format!("Failed to unload: {}", e)),
                        }
                        return Ok(());
                    }
                    'p' | 'P' => {
                        if adapter.pinned {
                            match self.api_client.unpin_adapter(&adapter_id).await {
                                Ok(_) => {
                                    self.confirmation_message =
                                        Some(format!("Unpinned adapter: {}", adapter_id))
                                }
                                Err(e) => {
                                    self.error_message = Some(format!("Failed to unpin: {}", e))
                                }
                            }
                        } else {
                            match self.api_client.pin_adapter(&adapter_id).await {
                                Ok(_) => {
                                    self.confirmation_message =
                                        Some(format!("Pinned adapter: {}", adapter_id))
                                }
                                Err(e) => {
                                    self.error_message = Some(format!("Failed to pin: {}", e))
                                }
                            }
                        }
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }

        // Training screen: C=Cancel
        if self.current_screen == Screen::Training {
            if let Some(job) = self.training_jobs.get(self.selected_training_job).cloned() {
                let job_id = job.id.clone();
                match c {
                    'c' | 'C' => {
                        match self.api_client.cancel_training_job(&job_id).await {
                            Ok(_) => {
                                self.confirmation_message =
                                    Some(format!("Cancelling job: {}", job_id))
                            }
                            Err(e) => self.error_message = Some(format!("Failed to cancel: {}", e)),
                        }
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }

        match c {
            'h' => self.show_help = !self.show_help,
            'a' => self.current_screen = Screen::Adapters,
            'r' => self.current_screen = Screen::Training,
            'i' => self.current_screen = Screen::Chat,
            's' => self.current_screen = Screen::Services,
            'l' => self.current_screen = Screen::Logs,
            'm' => self.current_screen = Screen::Metrics,
            'c' => self.current_screen = Screen::Config,
            'd' => self.current_screen = Screen::Dashboard,
            'b' => self.boot_all_services().await?,
            'p' => self.toggle_production_mode(),
            't' => {
                self.current_screen = Screen::Logs;
                self.current_mode = Mode::Filter(LogFilterMode::TraceId);
                self.log_filter_mode = Some(LogFilterMode::TraceId);
                self.log_filter_input.clear();
            }
            'n' => {
                self.current_screen = Screen::Logs;
                self.current_mode = Mode::Filter(LogFilterMode::Tenant);
                self.log_filter_mode = Some(LogFilterMode::Tenant);
                self.log_filter_input.clear();
            }
            'x' => {
                self.log_filter_trace = None;
                self.log_filter_tenant = None;
                self.log_filter_input.clear();
                self.log_filter_mode = None;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn should_quit(&self) -> bool {
        // Add any cleanup logic here
        true
    }

    // Service management
    async fn boot_all_services(&mut self) -> Result<()> {
        info!("Booting all services");
        self.confirmation_message = Some("Booting all services...".to_string());

        // Update service statuses optimistically
        for service in &mut self.services {
            service.status = Status::Starting;
            service.message = "Starting...".to_string();
        }

        match self.api_client.start_all_services().await {
            Ok(_) => {
                self.confirmation_message = Some("Services starting...".to_string());
            }
            Err(api_error) => {
                warn!(error = %api_error, "Failed to start services via API");
                self.error_message =
                    Some(format!("Failed to start services via API: {}", api_error));

                if self.setup_state.missing_prereqs.is_empty() {
                    match self.service_control.start_all_services().await {
                        Ok(result) => {
                            self.setup_state.set_last_action(
                                format!("Executed {}", result.command),
                                result.combined_output(),
                            );
                            self.confirmation_message =
                                Some("Launching AdapterOS stack locally...".to_string());
                            self.error_message = None;
                        }
                        Err(launch_error) => {
                            warn!(error = %launch_error, "Local launch failed");
                            self.error_message = Some(format!(
                                "Failed to launch services locally: {}",
                                launch_error
                            ));
                        }
                    }
                } else {
                    self.error_message = Some(format!(
                        "Setup incomplete. Resolve prerequisites before launching: {}",
                        self.setup_state.missing_prereqs.join("; ")
                    ));
                }
            }
        }

        Ok(())
    }

    async fn boot_single_service(&mut self, index: usize) -> Result<()> {
        if let Some(service) = self.services.get_mut(index) {
            let service_name = service.name.clone();
            info!("Booting service: {}", service_name);
            service.status = Status::Starting;
            service.message = "Starting...".to_string();

            self.confirmation_message = Some(format!("Starting {}...", service_name));

            // Actually call API to start the service
            if let Err(e) = self.api_client.start_service(&service_name).await {
                warn!("Failed to start service {}: {}", service_name, e);
                self.error_message = Some(format!("Failed to start {}: {}", service_name, e));
                service.status = Status::Failed;
                service.message = format!("Failed: {}", e);
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn stop_service(&mut self, index: usize) -> Result<()> {
        if let Some(service) = self.services.get_mut(index) {
            let service_name = service.name.clone();
            info!("Stopping service: {}", service_name);

            self.confirmation_message = Some(format!("Stopping {}...", service_name));

            // Call API to stop the service
            if let Err(e) = self.api_client.stop_service(&service_name).await {
                warn!("Failed to stop service {}: {}", service_name, e);
                self.error_message = Some(format!("Failed to stop {}: {}", service_name, e));
            } else {
                service.status = Status::Stopped;
                service.message = "Stopped".to_string();
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn restart_service(&mut self, index: usize) -> Result<()> {
        if let Some(service) = self.services.get_mut(index) {
            let service_name = service.name.clone();
            info!("Restarting service: {}", service_name);
            service.status = Status::Starting;
            service.message = "Restarting...".to_string();

            self.confirmation_message = Some(format!("Restarting {}...", service_name));

            // Call API to restart the service
            if let Err(e) = self.api_client.restart_service(&service_name).await {
                warn!("Failed to restart service {}: {}", service_name, e);
                self.error_message = Some(format!("Failed to restart {}: {}", service_name, e));
                service.status = Status::Failed;
                service.message = format!("Failed: {}", e);
            }
        }
        Ok(())
    }

    async fn debug_service(&mut self) -> Result<()> {
        self.current_mode = Mode::ServiceSelect;
        self.confirmation_message = Some("Select a service to debug".to_string());
        Ok(())
    }

    fn toggle_production_mode(&mut self) {
        self.production_mode = !self.production_mode;
        let mode = if self.production_mode {
            "PRODUCTION"
        } else {
            "DEVELOPMENT"
        };
        self.confirmation_message = Some(format!("Switched to {} mode", mode));

        if self.production_mode {
            warn!("Production mode enabled - enforcing security policies");
        } else {
            info!("Development mode enabled");
        }
    }

    fn save_config_value(&mut self) {
        let val = self.config_edit_value.trim();
        if val.is_empty() {
            return;
        }

        match self.selected_config_field {
            0 => {
                if let Ok(p) = val.parse() {
                    self.config.server_port = p;
                }
            }
            1 => {
                if let Ok(c) = val.parse() {
                    self.config.max_connections = c;
                }
            }
            2 => self.config.model_path = val.to_string(),
            3 => {
                if let Ok(k) = val.parse() {
                    self.config.k_sparse_value = k;
                }
            }
            4 => {
                if let Ok(b) = val.parse() {
                    self.config.batch_size = b;
                }
            }
            5 => {
                if let Ok(s) = val.parse() {
                    self.config.cache_size_mb = s;
                }
            }
            6 => {
                self.config.jwt_mode = if val.to_uppercase() == "EDDSA" {
                    JwtMode::EdDsa
                } else {
                    JwtMode::Hmac
                };
            }
            7 => {
                self.config.require_pf_deny = val.to_lowercase() == "yes"
                    || val.to_lowercase() == "true"
                    || val.to_lowercase() == "1";
            }
            _ => {}
        }

        // Persist TUI-specific config to disk
        if let Err(e) = self.tui_config.save() {
            warn!("Failed to save TUI config to disk: {}", e);
            self.confirmation_message = Some("Config updated (failed to save to disk)".to_string());
        } else {
            self.confirmation_message = Some("Config updated and saved to disk".to_string());
        }
        self.config_edit_value.clear();
    }

    /// Send the current chat message and stream the response via SSE
    async fn send_chat_message(&mut self) -> Result<()> {
        let content = self.chat_input.trim().to_string();
        if content.is_empty() {
            return Ok(());
        }

        // Add user message to chat history
        self.chat_messages.push(ChatMessage {
            role: ChatRole::User,
            content: content.clone(),
        });

        // Clear input but stay in ChatInput mode for continued conversation
        self.chat_input.clear();

        // Mark streaming as active
        self.chat_streaming = true;

        // Add empty assistant message that we'll append tokens to
        self.chat_messages.push(ChatMessage {
            role: ChatRole::Assistant,
            content: String::new(),
        });

        // Get adapter if available
        let adapter_id = self.adapters.first().map(|a| a.id.as_str());

        info!(message = %content, adapter = ?adapter_id, "Sending chat message via SSE");

        // Stream inference via SSE
        match self.sse_client.stream_inference(&content, adapter_id).await {
            Ok(mut rx) => {
                while let Some(token) = rx.recv().await {
                    // Append token to the last message (the assistant's response)
                    if let Some(last_msg) = self.chat_messages.last_mut() {
                        last_msg.content.push_str(&token.token);
                    }
                }
                debug!("SSE stream completed successfully");
            }
            Err(e) => {
                // Update the assistant message with error
                if let Some(last_msg) = self.chat_messages.last_mut() {
                    last_msg.content =
                        format!("Error: Failed to connect to inference server - {}", e);
                }
                warn!("SSE inference error: {}", e);
            }
        }

        // Streaming complete
        self.chat_streaming = false;
        Ok(())
    }
}

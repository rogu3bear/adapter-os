use anyhow::Result;
use std::time::Instant;
use tokio::time::Duration;
use tracing::{debug, info, warn};

pub mod api;
pub mod db;
pub mod types;

use api::{AdapterInfo, ApiClient};
use db::{DbClient, DbStatsSummary};
use types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Services,
    Logs,
    Metrics,
    Config,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    ServiceSelect,
    LogView,
    ConfigEdit,
    Confirmation,
}

pub struct App {
    // Navigation
    pub current_screen: Screen,
    pub current_mode: Mode,
    pub selected_menu_item: usize,
    pub selected_service: usize,

    // Status
    pub model_status: ModelStatus,
    pub system_status: SystemStatus,
    pub services: Vec<ServiceStatus>,

    // Live data
    pub metrics: SystemMetrics,
    pub recent_logs: Vec<LogEntry>,
    pub alerts: Vec<Alert>,
    pub adapters: Vec<AdapterInfo>,
    pub db_stats: DbStatsSummary,

    // Configuration
    pub config: SystemConfig,
    pub production_mode: bool,

    // UI State
    pub show_help: bool,
    pub confirmation_message: Option<String>,
    pub error_message: Option<String>,
    pub last_update: Instant,

    // API Client
    api_client: ApiClient,

    // Database Client
    db_client: DbClient,
}

impl App {
    pub async fn new() -> Result<Self> {
        let api_client = ApiClient::new("http://localhost:3300".to_string())?;
        let db_client = DbClient::new().await?;

        Ok(Self {
            current_screen: Screen::Dashboard,
            current_mode: Mode::Normal,
            selected_menu_item: 0,
            selected_service: 0,

            model_status: ModelStatus {
                name: "llama-7b-lora-q15".to_string(),
                loaded: false,
                memory_usage_mb: 0,
                total_memory_mb: 1024,
            },

            system_status: SystemStatus {
                uptime: Duration::from_secs(0),
                cpu_percent: 0.0,
                memory_percent: 0.0,
                disk_percent: 0.0,
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
            alerts: Vec::new(),
            adapters: Vec::new(),
            db_stats: DbStatsSummary {
                total_adapters: 0,
                total_training_jobs: 0,
                active_training_jobs: 0,
                total_tenants: 0,
                database_connected: false,
            },

            config: SystemConfig::default(),
            production_mode: false,

            show_help: false,
            confirmation_message: None,
            error_message: None,
            last_update: Instant::now(),

            api_client,
            db_client,
        })
    }

    pub async fn update(&mut self) -> Result<()> {
        // Update every second
        if self.last_update.elapsed() < Duration::from_secs(1) {
            return Ok(());
        }

        self.last_update = Instant::now();

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

    // Navigation handlers
    pub fn on_up(&mut self) {
        match self.current_mode {
            Mode::ServiceSelect => {
                if self.selected_service > 0 {
                    self.selected_service -= 1;
                }
            }
            Mode::Normal => {
                if self.selected_menu_item > 0 {
                    self.selected_menu_item -= 1;
                }
            }
            _ => {}
        }
    }

    pub fn on_down(&mut self) {
        match self.current_mode {
            Mode::ServiceSelect => {
                if self.selected_service < self.services.len() - 1 {
                    self.selected_service += 1;
                }
            }
            Mode::Normal => {
                if self.selected_menu_item < 6 {
                    self.selected_menu_item += 1;
                }
            }
            _ => {}
        }
    }

    pub fn on_left(&mut self) {
        // Navigate between screens
        self.current_screen = match self.current_screen {
            Screen::Dashboard => Screen::Help,
            Screen::Services => Screen::Dashboard,
            Screen::Logs => Screen::Services,
            Screen::Metrics => Screen::Logs,
            Screen::Config => Screen::Metrics,
            Screen::Help => Screen::Config,
        };
    }

    pub fn on_right(&mut self) {
        // Navigate between screens
        self.current_screen = match self.current_screen {
            Screen::Dashboard => Screen::Services,
            Screen::Services => Screen::Logs,
            Screen::Logs => Screen::Metrics,
            Screen::Metrics => Screen::Config,
            Screen::Config => Screen::Help,
            Screen::Help => Screen::Dashboard,
        };
    }

    pub async fn on_enter(&mut self) -> Result<()> {
        match self.current_mode {
            Mode::Normal => match self.selected_menu_item {
                0 => self.boot_all_services().await?,
                1 => self.current_mode = Mode::ServiceSelect,
                2 => self.debug_service().await?,
                3 => self.current_screen = Screen::Metrics,
                4 => self.current_screen = Screen::Logs,
                5 => self.current_screen = Screen::Config,
                6 => self.toggle_production_mode(),
                _ => {}
            },
            Mode::ServiceSelect => {
                self.boot_single_service(self.selected_service).await?;
                self.current_mode = Mode::Normal;
            }
            Mode::Confirmation => {
                self.confirmation_message = None;
                self.current_mode = Mode::Normal;
            }
            _ => {}
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

    pub fn on_escape(&mut self) {
        match self.current_mode {
            Mode::ServiceSelect | Mode::LogView | Mode::ConfigEdit | Mode::Confirmation => {
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
        match c {
            'h' => self.show_help = !self.show_help,
            's' => self.current_screen = Screen::Services,
            'l' => self.current_screen = Screen::Logs,
            'm' => self.current_screen = Screen::Metrics,
            'c' => self.current_screen = Screen::Config,
            'd' => self.current_screen = Screen::Dashboard,
            'b' => self.boot_all_services().await?,
            'p' => self.toggle_production_mode(),
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

        // Actually call API to start services
        if let Err(e) = self.api_client.start_all_services().await {
            warn!("Failed to start all services: {}", e);
            self.error_message = Some(format!("Failed to start services: {}", e));
        } else {
            self.confirmation_message = Some("Services starting...".to_string());
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
}

//! Unix Domain Socket metrics exporter
//!
//! Provides Prometheus-compatible metrics export via Unix domain sockets,
//! ensuring zero network egress during serving (Egress Ruleset #1).

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, Mutex};
use tracing::{debug, error, info, warn};

/// Metric value types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricValue {
    /// Counter (monotonically increasing)
    Counter(f64),
    /// Gauge (can go up or down)
    Gauge(f64),
    /// Histogram bucket
    Histogram {
        count: u64,
        sum: f64,
        buckets: Vec<(f64, u64)>,
    },
    /// Summary
    Summary {
        count: u64,
        sum: f64,
        quantiles: Vec<(f64, f64)>,
    },
}

/// Metric metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricMetadata {
    /// Metric name
    pub name: String,
    /// Help text
    pub help: String,
    /// Metric type
    pub metric_type: String,
    /// Labels
    pub labels: HashMap<String, String>,
    /// Value
    pub value: MetricValue,
}

/// Unix Domain Socket metrics exporter
pub struct UdsMetricsExporter {
    /// Socket path
    socket_path: PathBuf,
    /// Listener
    listener: Option<UnixListener>,
    /// Metrics registry
    metrics_registry: Arc<Mutex<HashMap<String, MetricMetadata>>>,
    /// Whether to enable Prometheus compatibility
    prometheus_compat: bool,
}

impl UdsMetricsExporter {
    /// Create a new UDS metrics exporter
    pub fn new(socket_path: PathBuf) -> Result<Self> {
        Ok(Self {
            socket_path,
            listener: None,
            metrics_registry: Arc::new(Mutex::new(HashMap::new())),
            prometheus_compat: true,
        })
    }

    /// Create with Prometheus compatibility disabled
    pub fn with_prometheus_compat(mut self, enabled: bool) -> Self {
        self.prometheus_compat = enabled;
        self
    }

    /// Gracefully shutdown the UDS metrics exporter
    ///
    /// Closes the listener socket and cleans up resources.
    /// Existing connections will complete naturally.
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down UDS metrics exporter");

        if let Some(listener) = self.listener.take() {
            // Drop the listener to stop accepting new connections
            drop(listener);
        }

        // Clean up the socket file
        if self.socket_path.exists() {
            if let Err(e) = std::fs::remove_file(&self.socket_path) {
                warn!("Failed to remove UDS socket file during shutdown: {}", e);
            } else {
                info!("UDS socket file cleaned up");
            }
        }

        info!("UDS metrics exporter shutdown complete");
        Ok(())
    }

    /// Bind to the Unix domain socket
    pub async fn bind(&mut self) -> Result<()> {
        // Remove existing socket if present
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path).map_err(|e| {
                AosError::Telemetry(format!("Failed to remove existing socket: {}", e))
            })?;
        }

        // Create parent directory if needed
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AosError::Telemetry(format!("Failed to create socket directory: {}", e))
            })?;
        }

        // Bind listener
        let listener = UnixListener::bind(&self.socket_path)
            .map_err(|e| AosError::Telemetry(format!("Failed to bind UDS listener: {}", e)))?;

        info!("UDS metrics exporter bound to: {:?}", self.socket_path);

        self.listener = Some(listener);
        Ok(())
    }

    /// Serve metrics over UDS with graceful shutdown support
    ///
    /// The server will run until a shutdown signal is received via the broadcast receiver.
    pub async fn serve(&self, mut shutdown_rx: broadcast::Receiver<()>) -> Result<()> {
        let listener = self
            .listener
            .as_ref()
            .ok_or_else(|| AosError::Telemetry("Listener not bound".to_string()))?;

        info!("Starting UDS metrics exporter...");

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _addr)) => {
                            let registry = self.metrics_registry.clone();
                            let prometheus_compat = self.prometheus_compat;

                            tokio::spawn(async move {
                                if let Err(e) =
                                    Self::handle_connection(stream, registry, prometheus_compat).await
                                {
                                    error!("Failed to handle UDS metrics request: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept UDS connection: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("UDS metrics exporter received shutdown signal");
                    break;
                }
            }
        }

        info!("UDS metrics exporter stopped accepting connections");
        Ok(())
    }

    /// Legacy serve method without shutdown support (for backward compatibility)
    pub async fn serve_legacy(&self) -> Result<()> {
        let listener = self
            .listener
            .as_ref()
            .ok_or_else(|| AosError::Telemetry("Listener not bound".to_string()))?;

        info!("Starting UDS metrics exporter (legacy mode)...");

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let registry = self.metrics_registry.clone();
                    let prometheus_compat = self.prometheus_compat;

                    tokio::spawn(async move {
                        if let Err(e) =
                            Self::handle_connection(stream, registry, prometheus_compat).await
                        {
                            error!("Failed to handle UDS metrics request: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept UDS connection: {}", e);
                }
            }
        }
    }

    /// Handle a single connection
    async fn handle_connection(
        mut stream: UnixStream,
        registry: Arc<Mutex<HashMap<String, MetricMetadata>>>,
        prometheus_compat: bool,
    ) -> Result<()> {
        // Read request (simple protocol: GET or empty)
        let mut buffer = vec![0u8; 1024];
        let n = stream
            .read(&mut buffer)
            .await
            .map_err(|e| AosError::Telemetry(format!("Failed to read request: {}", e)))?;

        debug!("Received metrics request: {} bytes", n);

        // Format metrics
        let metrics_output = {
            let registry = registry.lock().await;
            if prometheus_compat {
                Self::format_prometheus_metrics(&registry)
            } else {
                Self::format_json_metrics(&registry)
            }
        };

        // Write response
        stream
            .write_all(metrics_output.as_bytes())
            .await
            .map_err(|e| AosError::Telemetry(format!("Failed to write response: {}", e)))?;

        stream
            .flush()
            .await
            .map_err(|e| AosError::Telemetry(format!("Failed to flush response: {}", e)))?;

        debug!("Sent metrics response: {} bytes", metrics_output.len());

        Ok(())
    }

    /// Format metrics in Prometheus text format
    fn format_prometheus_metrics(registry: &HashMap<String, MetricMetadata>) -> String {
        let mut output = String::new();

        for (name, metadata) in registry.iter() {
            // Add HELP line
            output.push_str(&format!("# HELP {} {}\n", name, metadata.help));

            // Add TYPE line
            output.push_str(&format!("# TYPE {} {}\n", name, metadata.metric_type));

            // Format labels
            let labels_str = if metadata.labels.is_empty() {
                String::new()
            } else {
                let labels: Vec<String> = metadata
                    .labels
                    .iter()
                    .map(|(k, v)| format!("{}=\"{}\"", k, v))
                    .collect();
                format!("{{{}}}", labels.join(","))
            };

            // Add metric line(s)
            match &metadata.value {
                MetricValue::Counter(value) | MetricValue::Gauge(value) => {
                    output.push_str(&format!("{}{} {}\n", name, labels_str, value));
                }
                MetricValue::Histogram {
                    count,
                    sum,
                    buckets,
                } => {
                    for (le, count) in buckets {
                        output.push_str(&format!(
                            "{}_bucket{{le=\"{}\"{}}} {}\n",
                            name,
                            le,
                            if labels_str.is_empty() {
                                "".to_string()
                            } else {
                                format!(",{}", &labels_str[1..labels_str.len() - 1])
                            },
                            count
                        ));
                    }
                    output.push_str(&format!("{}_sum{} {}\n", name, labels_str, sum));
                    output.push_str(&format!("{}_count{} {}\n", name, labels_str, count));
                }
                MetricValue::Summary {
                    count,
                    sum,
                    quantiles,
                } => {
                    for (quantile, value) in quantiles {
                        output.push_str(&format!(
                            "{}{{quantile=\"{}\"{}}} {}\n",
                            name,
                            quantile,
                            if labels_str.is_empty() {
                                "".to_string()
                            } else {
                                format!(",{}", &labels_str[1..labels_str.len() - 1])
                            },
                            value
                        ));
                    }
                    output.push_str(&format!("{}_sum{} {}\n", name, labels_str, sum));
                    output.push_str(&format!("{}_count{} {}\n", name, labels_str, count));
                }
            }

            output.push('\n');
        }

        output
    }

    /// Format metrics as JSON
    fn format_json_metrics(registry: &HashMap<String, MetricMetadata>) -> String {
        serde_json::to_string_pretty(registry).unwrap_or_else(|_| "{}".to_string())
    }

    /// Register a metric
    pub async fn register_metric(&self, metadata: MetricMetadata) {
        let mut registry = self.metrics_registry.lock().await;
        registry.insert(metadata.name.clone(), metadata);
    }

    /// Update a metric value
    pub async fn update_metric(&self, name: &str, value: MetricValue) -> Result<()> {
        let mut registry = self.metrics_registry.lock().await;
        if let Some(metadata) = registry.get_mut(name) {
            metadata.value = value;
            Ok(())
        } else {
            Err(AosError::Validation(format!(
                "Metric '{}' not registered",
                name
            )))
        }
    }

    /// Increment a counter
    pub async fn increment_counter(&self, name: &str, delta: f64) -> Result<()> {
        let mut registry = self.metrics_registry.lock().await;
        if let Some(metadata) = registry.get_mut(name) {
            match &mut metadata.value {
                MetricValue::Counter(ref mut value) => {
                    *value += delta;
                    Ok(())
                }
                _ => Err(AosError::Validation(format!(
                    "Metric '{}' is not a counter",
                    name
                ))),
            }
        } else {
            Err(AosError::Validation(format!(
                "Metric '{}' not registered",
                name
            )))
        }
    }

    /// Set a gauge value
    pub async fn set_gauge(&self, name: &str, value: f64) -> Result<()> {
        self.update_metric(name, MetricValue::Gauge(value)).await
    }

    /// Get the socket path
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }
}

impl Drop for UdsMetricsExporter {
    fn drop(&mut self) {
        // Clean up socket file
        if self.socket_path.exists() {
            if let Err(e) = std::fs::remove_file(&self.socket_path) {
                warn!("Failed to remove UDS socket on drop: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("tempdir")
    }

    #[tokio::test]
    async fn test_uds_metrics_exporter() {
        let temp_dir = new_test_tempdir();
        let socket_path = temp_dir.path().join("metrics.sock");

        let mut exporter = UdsMetricsExporter::new(socket_path.clone()).unwrap();
        exporter.bind().await.unwrap();

        // Register a test metric
        exporter
            .register_metric(MetricMetadata {
                name: "test_counter".to_string(),
                help: "Test counter".to_string(),
                metric_type: "counter".to_string(),
                labels: HashMap::new(),
                value: MetricValue::Counter(0.0),
            })
            .await;

        // Increment counter
        exporter
            .increment_counter("test_counter", 5.0)
            .await
            .unwrap();

        // Verify metric value
        let registry = exporter.metrics_registry.lock().await;
        let metric = registry.get("test_counter").unwrap();
        match metric.value {
            MetricValue::Counter(value) => assert_eq!(value, 5.0),
            _ => panic!("Unexpected metric type"),
        }
    }

    #[test]
    fn test_prometheus_formatting() {
        let mut registry = HashMap::new();
        registry.insert(
            "test_gauge".to_string(),
            MetricMetadata {
                name: "test_gauge".to_string(),
                help: "Test gauge metric".to_string(),
                metric_type: "gauge".to_string(),
                labels: HashMap::new(),
                value: MetricValue::Gauge(42.5),
            },
        );

        let output = UdsMetricsExporter::format_prometheus_metrics(&registry);

        assert!(output.contains("# HELP test_gauge Test gauge metric"));
        assert!(output.contains("# TYPE test_gauge gauge"));
        assert!(output.contains("test_gauge 42.5"));
    }
}

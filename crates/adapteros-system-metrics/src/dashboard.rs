#![allow(unused_variables)]

//! Dashboard configuration support
//!
//! Provides dashboard configuration with widget types and data queries.
//! Supports time-series, gauge, alert list, anomaly heatmap, and other widget types.

use crate::monitoring_types::*;
use adapteros_core::Result;
use adapteros_db::Db;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

/// Dashboard configuration service
pub struct DashboardService {
    db: Arc<Db>,
}

/// Row structure for dashboard config query
#[derive(Debug)]
struct DashboardConfigRow {
    dashboard_config: Option<String>,
}

/// Dashboard configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashboardConfig {
    pub widgets: Vec<DashboardWidget>,
    pub refresh_interval: u64,
    pub time_range: String,
    pub layout: DashboardLayout,
    pub theme: DashboardTheme,
}

/// Dashboard layout configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashboardLayout {
    pub columns: usize,
    pub rows: usize,
    pub grid_size: usize,
    pub auto_arrange: bool,
}

/// Dashboard theme configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashboardTheme {
    pub name: String,
    pub colors: ThemeColors,
    pub fonts: ThemeFonts,
}

/// Theme colors
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ThemeColors {
    pub primary: String,
    pub secondary: String,
    pub background: String,
    pub surface: String,
    pub text: String,
    pub text_secondary: String,
    pub success: String,
    pub warning: String,
    pub error: String,
    pub critical: String,
}

/// Theme fonts
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ThemeFonts {
    pub primary: String,
    pub secondary: String,
    pub size_small: u32,
    pub size_medium: u32,
    pub size_large: u32,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            widgets: vec![],
            refresh_interval: 30,
            time_range: "24h".to_string(),
            layout: DashboardLayout {
                columns: 4,
                rows: 3,
                grid_size: 12,
                auto_arrange: true,
            },
            theme: DashboardTheme {
                name: "default".to_string(),
                colors: ThemeColors {
                    primary: "#1976d2".to_string(),
                    secondary: "#424242".to_string(),
                    background: "#fafafa".to_string(),
                    surface: "#ffffff".to_string(),
                    text: "#212121".to_string(),
                    text_secondary: "#757575".to_string(),
                    success: "#4caf50".to_string(),
                    warning: "#ff9800".to_string(),
                    error: "#f44336".to_string(),
                    critical: "#d32f2f".to_string(),
                },
                fonts: ThemeFonts {
                    primary: "Roboto".to_string(),
                    secondary: "Roboto Mono".to_string(),
                    size_small: 12,
                    size_medium: 14,
                    size_large: 16,
                },
            },
        }
    }
}

impl DashboardService {
    /// Create a new dashboard service
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// Get dashboard configuration
    pub async fn get_dashboard_config(&self, dashboard_id: &str) -> Result<DashboardConfig> {
        let rows = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT dashboard_config FROM process_monitoring_dashboards WHERE id = ?",
        )
        .bind(dashboard_id)
        .fetch_all(self.db.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to get dashboard config: {}", e))
        })?;

        if let Some((config_json,)) = rows.first() {
            if let Some(config_json) = config_json {
                let config: DashboardConfig = serde_json::from_str(config_json).map_err(|e| {
                    adapteros_core::AosError::Config(format!("Invalid dashboard config: {}", e))
                })?;
                Ok(config)
            } else {
                Ok(DashboardConfig::default())
            }
        } else {
            Ok(DashboardConfig::default())
        }
    }

    /// Get data for a specific widget
    pub async fn get_widget_data(
        &self,
        widget: &DashboardWidget,
        time_range: &str,
    ) -> Result<WidgetData> {
        match widget.widget_type {
            WidgetType::TimeSeries => self.get_time_series_data(widget, time_range).await,
            WidgetType::Gauge => self.get_gauge_data(widget, time_range).await,
            WidgetType::AlertList => self.get_alert_list_data(widget, time_range).await,
            WidgetType::AnomalyHeatmap => self.get_anomaly_heatmap_data(widget, time_range).await,
            WidgetType::MetricCard => self.get_metric_card_data(widget, time_range).await,
            WidgetType::StatusIndicator => self.get_status_indicator_data(widget, time_range).await,
        }
    }

    /// Get time series data for a widget
    async fn get_time_series_data(
        &self,
        widget: &DashboardWidget,
        time_range: &str,
    ) -> Result<WidgetData> {
        let config: TimeSeriesWidgetConfig = serde_json::from_value(widget.config.clone())
            .map_err(|e| {
                adapteros_core::AosError::Config(format!("Invalid time series config: {}", e))
            })?;

        let duration = self.parse_time_range(time_range)?;
        let start_time = chrono::Utc::now() - duration;

        let filters = MetricFilters {
            worker_id: None,
            tenant_id: None,
            metric_name: Some(config.metric.clone()),
            start_time: Some(start_time),
            end_time: None,
            limit: Some(1000),
        };

        let metrics = ProcessHealthMetric::query(self.db.pool(), filters).await?;

        let points: Vec<TimeSeriesPoint> = metrics
            .into_iter()
            .map(|m| TimeSeriesPoint {
                timestamp: m.collected_at.to_rfc3339(),
                value: m.metric_value,
                worker_id: Some(m.worker_id),
            })
            .collect();

        let data = TimeSeriesData {
            metric: config.metric,
            points,
            aggregation: config.aggregation,
            window: config.window,
        };

        Ok(WidgetData {
            widget_id: widget.id.clone(),
            widget_type: widget.widget_type.to_string(),
            data: serde_json::to_value(data)?,
            error: None,
        })
    }

    /// Get gauge data for a widget
    async fn get_gauge_data(
        &self,
        widget: &DashboardWidget,
        _time_range: &str,
    ) -> Result<WidgetData> {
        let config: GaugeWidgetConfig =
            serde_json::from_value(widget.config.clone()).map_err(|e| {
                adapteros_core::AosError::Config(format!("Invalid gauge config: {}", e))
            })?;

        // Get the most recent metric value
        let filters = MetricFilters {
            worker_id: None,
            tenant_id: None,
            metric_name: Some(config.metric.clone()),
            start_time: None,
            end_time: None,
            limit: Some(1),
        };

        let metrics = ProcessHealthMetric::query(self.db.pool(), filters).await?;
        let current_value = metrics.first().map(|m| m.metric_value).unwrap_or(0.0);

        let status = if current_value >= config.threshold_critical {
            "critical"
        } else if current_value >= config.threshold_warning {
            "warning"
        } else {
            "healthy"
        };

        let data = GaugeData {
            metric: config.metric,
            current_value,
            threshold_warning: config.threshold_warning,
            threshold_critical: config.threshold_critical,
            status: status.to_string(),
            unit: config.unit,
        };

        Ok(WidgetData {
            widget_id: widget.id.clone(),
            widget_type: widget.widget_type.to_string(),
            data: serde_json::to_value(data)?,
            error: None,
        })
    }

    /// Get alert list data for a widget
    async fn get_alert_list_data(
        &self,
        widget: &DashboardWidget,
        _time_range: &str,
    ) -> Result<WidgetData> {
        let config: AlertListWidgetConfig =
            serde_json::from_value(widget.config.clone()).map_err(|e| {
                adapteros_core::AosError::Config(format!("Invalid alert list config: {}", e))
            })?;

        let filters = AlertFilters {
            tenant_id: None,
            worker_id: None,
            status: if config.show_acknowledged {
                None
            } else {
                Some(AlertStatus::Active)
            },
            severity: None,
            start_time: None,
            end_time: None,
            limit: Some(config.limit),
            offset: None,
        };

        let alerts = ProcessAlert::list(self.db.pool(), filters).await?;

        let alert_summaries: Vec<AlertSummary> = alerts
            .into_iter()
            .map(|a| AlertSummary {
                id: a.id,
                title: a.title,
                severity: a.severity.to_string(),
                status: a.status.to_string(),
                worker_id: a.worker_id,
                created_at: a.created_at.to_rfc3339(),
                acknowledged_by: a.acknowledged_by,
            })
            .collect();

        let unacknowledged_count = alert_summaries
            .iter()
            .filter(|a| a.status == "active")
            .count();

        let data = AlertListData {
            alerts: alert_summaries.clone(),
            total_count: alert_summaries.len() as i64,
            unacknowledged_count: unacknowledged_count as i64,
        };

        Ok(WidgetData {
            widget_id: widget.id.clone(),
            widget_type: widget.widget_type.to_string(),
            data: serde_json::to_value(data)?,
            error: None,
        })
    }

    /// Get anomaly heatmap data for a widget
    async fn get_anomaly_heatmap_data(
        &self,
        widget: &DashboardWidget,
        time_range: &str,
    ) -> Result<WidgetData> {
        let config: AnomalyHeatmapWidgetConfig = serde_json::from_value(widget.config.clone())
            .map_err(|e| {
                adapteros_core::AosError::Config(format!("Invalid anomaly heatmap config: {}", e))
            })?;

        let duration = self.parse_time_range(time_range)?;
        let start_time = chrono::Utc::now() - duration;

        let mut worker_data = Vec::new();

        for worker_id in &config.workers {
            let filters = AnomalyFilters {
                tenant_id: None,
                worker_id: Some(worker_id.clone()),
                status: None,
                anomaly_type: None,
                start_time: Some(start_time),
                end_time: None,
                limit: Some(100),
                offset: None,
            };

            let anomalies = ProcessAnomaly::list(self.db.pool(), filters).await?;

            let latest_anomaly = anomalies.first().map(|a| AnomalySummary {
                id: a.id.clone(),
                anomaly_type: a.anomaly_type.clone(),
                confidence_score: a.confidence_score,
                severity: a.severity.to_string(),
                detected_at: a.created_at.to_rfc3339(),
            });

            let confidence_scores: Vec<f64> =
                anomalies.iter().map(|a| a.confidence_score).collect();

            worker_data.push(WorkerAnomalyData {
                worker_id: worker_id.clone(),
                anomaly_count: anomalies.len() as i64,
                latest_anomaly,
                confidence_scores,
            });
        }

        let data = AnomalyHeatmapData {
            workers: worker_data,
            metric: config.metric,
            time_window: config.time_window,
        };

        Ok(WidgetData {
            widget_id: widget.id.clone(),
            widget_type: widget.widget_type.to_string(),
            data: serde_json::to_value(data)?,
            error: None,
        })
    }

    /// Get metric card data for a widget
    async fn get_metric_card_data(
        &self,
        widget: &DashboardWidget,
        time_range: &str,
    ) -> Result<WidgetData> {
        let config: MetricCardWidgetConfig = serde_json::from_value(widget.config.clone())
            .map_err(|e| {
                adapteros_core::AosError::Config(format!("Invalid metric card config: {}", e))
            })?;

        let duration = self.parse_time_range(time_range)?;
        let start_time = chrono::Utc::now() - duration;

        let filters = MetricFilters {
            worker_id: None,
            tenant_id: None,
            metric_name: Some(config.metric.clone()),
            start_time: Some(start_time),
            end_time: None,
            limit: Some(100),
        };

        let metrics = ProcessHealthMetric::query(self.db.pool(), filters).await?;

        if metrics.is_empty() {
            return Ok(WidgetData {
                widget_id: widget.id.clone(),
                widget_type: widget.widget_type.to_string(),
                data: serde_json::json!({
                    "metric": config.metric,
                    "value": 0.0,
                    "aggregation": config.aggregation,
                    "window": config.window,
                    "trend": null,
                    "unit": config.unit
                }),
                error: None,
            });
        }

        let values: Vec<f64> = metrics.iter().map(|m| m.metric_value).collect();
        let current_value = match config.aggregation.as_str() {
            "avg" => values.iter().sum::<f64>() / values.len() as f64,
            "max" => values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            "min" => values.iter().cloned().fold(f64::INFINITY, f64::min),
            "sum" => values.iter().sum(),
            _ => values.last().copied().unwrap_or(0.0),
        };

        // Calculate trend (simple comparison of first half vs second half)
        let trend = if values.len() >= 4 {
            let mid = values.len() / 2;
            let first_half_avg = values[..mid].iter().sum::<f64>() / mid as f64;
            let second_half_avg = values[mid..].iter().sum::<f64>() / (values.len() - mid) as f64;

            let percentage_change = if first_half_avg != 0.0 {
                ((second_half_avg - first_half_avg) / first_half_avg) * 100.0
            } else {
                0.0
            };

            let direction = if percentage_change > 1.0 {
                "up"
            } else if percentage_change < -1.0 {
                "down"
            } else {
                "stable"
            };

            Some(TrendData {
                direction: direction.to_string(),
                percentage_change,
                period: time_range.to_string(),
            })
        } else {
            None
        };

        let data = MetricCardData {
            metric: config.metric,
            value: current_value,
            aggregation: config.aggregation,
            window: config.window,
            trend,
            unit: config.unit,
        };

        Ok(WidgetData {
            widget_id: widget.id.clone(),
            widget_type: widget.widget_type.to_string(),
            data: serde_json::to_value(data)?,
            error: None,
        })
    }

    /// Get status indicator data for a widget
    async fn get_status_indicator_data(
        &self,
        widget: &DashboardWidget,
        _time_range: &str,
    ) -> Result<WidgetData> {
        let config: StatusIndicatorWidgetConfig = serde_json::from_value(widget.config.clone())
            .map_err(|e| {
                adapteros_core::AosError::Config(format!("Invalid status indicator config: {}", e))
            })?;

        // Get the most recent metric value
        let filters = MetricFilters {
            worker_id: None,
            tenant_id: None,
            metric_name: Some(config.metric.clone()),
            start_time: None,
            end_time: None,
            limit: Some(1),
        };

        let metrics = ProcessHealthMetric::query(self.db.pool(), filters).await?;
        let current_value = metrics.first().map(|m| m.metric_value).unwrap_or(0.0);

        let status = match config.operator.as_str() {
            "gt" => {
                if current_value > config.critical_threshold {
                    "critical"
                } else if current_value > config.warning_threshold {
                    "warning"
                } else {
                    "healthy"
                }
            }
            "lt" => {
                if current_value < config.critical_threshold {
                    "critical"
                } else if current_value < config.warning_threshold {
                    "warning"
                } else {
                    "healthy"
                }
            }
            "gte" => {
                if current_value >= config.critical_threshold {
                    "critical"
                } else if current_value >= config.warning_threshold {
                    "warning"
                } else {
                    "healthy"
                }
            }
            "lte" => {
                if current_value <= config.critical_threshold {
                    "critical"
                } else if current_value <= config.warning_threshold {
                    "warning"
                } else {
                    "healthy"
                }
            }
            _ => "unknown",
        };

        let data = StatusIndicatorData {
            metric: config.metric,
            status: status.to_string(),
            current_value,
            thresholds: ThresholdData {
                healthy: config.healthy_threshold,
                warning: config.warning_threshold,
                critical: config.critical_threshold,
            },
        };

        Ok(WidgetData {
            widget_id: widget.id.clone(),
            widget_type: widget.widget_type.to_string(),
            data: serde_json::to_value(data)?,
            error: None,
        })
    }

    /// Parse time range string to duration
    fn parse_time_range(&self, time_range: &str) -> Result<chrono::Duration> {
        let duration = match time_range {
            "1h" => chrono::Duration::hours(1),
            "6h" => chrono::Duration::hours(6),
            "24h" => chrono::Duration::hours(24),
            "7d" => chrono::Duration::days(7),
            "30d" => chrono::Duration::days(30),
            _ => chrono::Duration::hours(24), // Default to 24 hours
        };
        Ok(duration)
    }

    /// Get dashboard data for all widgets
    pub async fn get_dashboard_data(
        &self,
        dashboard_id: &str,
        time_range: Option<&str>,
    ) -> Result<DashboardData> {
        let config = self.get_dashboard_config(dashboard_id).await?;
        let time_range = time_range.unwrap_or(&config.time_range);

        let mut widgets = Vec::new();

        for widget in &config.widgets {
            match self.get_widget_data(widget, time_range).await {
                Ok(widget_data) => widgets.push(widget_data),
                Err(e) => {
                    widgets.push(WidgetData {
                        widget_id: widget.id.clone(),
                        widget_type: widget.widget_type.to_string(),
                        data: serde_json::json!({}),
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        Ok(DashboardData {
            dashboard_id: dashboard_id.to_string(),
            widgets,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Export dashboard data
    pub async fn export_dashboard_data(
        &self,
        dashboard_id: &str,
        format: &str,
        time_range: Option<&str>,
    ) -> Result<String> {
        let dashboard_data = self.get_dashboard_data(dashboard_id, time_range).await?;

        match format {
            "json" => Ok(serde_json::to_string_pretty(&dashboard_data)?),
            "csv" => self.export_csv(&dashboard_data).await,
            _ => Err(adapteros_core::AosError::Config(format!(
                "Unsupported export format: {}",
                format
            ))),
        }
    }

    /// Export dashboard data as CSV
    async fn export_csv(&self, dashboard_data: &DashboardData) -> Result<String> {
        let mut csv = String::new();
        csv.push_str("widget_id,widget_type,timestamp,data\n");

        for widget in &dashboard_data.widgets {
            csv.push_str(&format!(
                "{},{},{},{}\n",
                widget.widget_id,
                widget.widget_type,
                dashboard_data.timestamp,
                serde_json::to_string(&widget.data).unwrap_or_else(|_| "{}".to_string())
            ));
        }

        Ok(csv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dashboard_config_defaults() {
        let config = DashboardConfig::default();

        assert_eq!(config.refresh_interval, 30);
        assert_eq!(config.time_range, "24h");
        assert_eq!(config.layout.columns, 4);
        assert_eq!(config.layout.rows, 3);
        assert_eq!(config.layout.grid_size, 12);
        assert!(config.layout.auto_arrange);
        assert_eq!(config.theme.name, "default");
        assert_eq!(config.theme.colors.primary, "#1976d2");
        assert_eq!(config.theme.fonts.primary, "Roboto");
    }

    #[tokio::test]
    async fn test_dashboard_service_creation() {
        let db = Arc::new(
            adapteros_db::Db::connect(":memory:")
                .await
                .expect("Failed to create test database"),
        );

        let service = DashboardService::new(db.clone());
        // Verify service is properly initialized
        assert_eq!(Arc::strong_count(&service.db), 2); // service + db variables
    }

    #[tokio::test]
    async fn test_time_range_parsing() {
        let db = Arc::new(
            adapteros_db::Db::connect(":memory:")
                .await
                .expect("Failed to create test database"),
        );

        let service = DashboardService::new(db);

        assert_eq!(
            service.parse_time_range("1h").unwrap(),
            chrono::Duration::hours(1)
        );
        assert_eq!(
            service.parse_time_range("6h").unwrap(),
            chrono::Duration::hours(6)
        );
        assert_eq!(
            service.parse_time_range("24h").unwrap(),
            chrono::Duration::hours(24)
        );
        assert_eq!(
            service.parse_time_range("7d").unwrap(),
            chrono::Duration::days(7)
        );
        assert_eq!(
            service.parse_time_range("30d").unwrap(),
            chrono::Duration::days(30)
        );
        assert_eq!(
            service.parse_time_range("unknown").unwrap(),
            chrono::Duration::hours(24)
        ); // Default
    }

    #[tokio::test]
    async fn test_widget_type_display() {
        assert_eq!(WidgetType::TimeSeries.to_string(), "time_series");
        assert_eq!(WidgetType::Gauge.to_string(), "gauge");
        assert_eq!(WidgetType::AlertList.to_string(), "alert_list");
        assert_eq!(WidgetType::AnomalyHeatmap.to_string(), "anomaly_heatmap");
        assert_eq!(WidgetType::MetricCard.to_string(), "metric_card");
        assert_eq!(WidgetType::StatusIndicator.to_string(), "status_indicator");
    }
}

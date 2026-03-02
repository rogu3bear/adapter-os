//! OpenTelemetry tracing initialization and lifecycle management.
//!
//! Provides graceful integration with the existing tracing infrastructure,
//! adding OTLP export capabilities without breaking existing logging.

use adapteros_server_api::config::OtelConfig;
use anyhow::Result;
use opentelemetry::trace::TracerProvider;
use opentelemetry::KeyValue;
use opentelemetry_sdk::{
    trace::{BatchConfigBuilder, BatchSpanProcessor, Sampler, SdkTracerProvider},
    Resource,
};
use std::time::Duration;
use tracing::info;

/// Guard that ensures proper OpenTelemetry shutdown on drop
pub struct OtelGuard {
    provider: Option<SdkTracerProvider>,
    #[allow(dead_code)]
    shutdown_timeout: Duration,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            info!("Shutting down OpenTelemetry tracer provider");
            // Force flush any pending spans
            if let Err(e) = provider.force_flush() {
                tracing::warn!(error = %e, "Failed to flush OpenTelemetry spans");
            }
            if let Err(e) = provider.shutdown() {
                tracing::warn!(error = %e, "OpenTelemetry shutdown error");
            }
        }
    }
}

/// Initialize OpenTelemetry with OTLP exporter.
///
/// Returns the tracer that can be used to create a tracing layer, plus a guard
/// for graceful shutdown. The caller creates the layer inline to avoid type
/// composition issues with boxed layers.
///
/// Returns None if otel is disabled or initialization fails (graceful degradation).
pub fn init_otel(
    config: &OtelConfig,
) -> Result<Option<(opentelemetry_sdk::trace::Tracer, OtelGuard)>> {
    if !config.enabled {
        info!("OpenTelemetry tracing disabled");
        return Ok(None);
    }

    let endpoint = &config.endpoint;
    info!(endpoint = %endpoint, protocol = %config.protocol, "Initializing OpenTelemetry");

    // Build resource with service metadata
    let resource = Resource::builder()
        .with_attributes([
            KeyValue::new("service.name", config.service_name.clone()),
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new(
                "deployment.environment",
                std::env::var("AOS_ENVIRONMENT").unwrap_or_else(|_| "development".to_string()),
            ),
        ])
        .build();

    // Configure sampler
    let sampler = if config.sampling_ratio >= 1.0 {
        Sampler::AlwaysOn
    } else if config.sampling_ratio <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.sampling_ratio)
    };

    // Build batch processor config
    let batch_config = BatchConfigBuilder::default()
        .with_max_queue_size(config.max_queue_size)
        .with_scheduled_delay(Duration::from_secs(5))
        .build();

    // Configure exporter based on protocol
    let export_timeout = Duration::from_secs(config.export_timeout_secs);

    let provider = match config.protocol.as_str() {
        "http" | "http/protobuf" => {
            use opentelemetry_otlp::WithExportConfig;
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .with_endpoint(endpoint.clone())
                .with_timeout(export_timeout)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build HTTP OTLP exporter: {}", e))?;

            let batch_processor = BatchSpanProcessor::builder(exporter)
                .with_batch_config(batch_config)
                .build();

            SdkTracerProvider::builder()
                .with_resource(resource)
                .with_sampler(sampler)
                .with_span_processor(batch_processor)
                .build()
        }
        _ => {
            // Default to gRPC
            use opentelemetry_otlp::WithExportConfig;
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint.clone())
                .with_timeout(export_timeout)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build gRPC OTLP exporter: {}", e))?;

            let batch_processor = BatchSpanProcessor::builder(exporter)
                .with_batch_config(batch_config)
                .build();

            SdkTracerProvider::builder()
                .with_resource(resource)
                .with_sampler(sampler)
                .with_span_processor(batch_processor)
                .build()
        }
    };

    // Get tracer (caller creates the layer inline to avoid type composition issues)
    let tracer = provider.tracer("adapteros");

    let guard = OtelGuard {
        provider: Some(provider),
        shutdown_timeout: Duration::from_secs(config.shutdown_timeout_secs),
    };

    info!(
        service_name = %config.service_name,
        endpoint = %endpoint,
        sampling_ratio = config.sampling_ratio,
        "OpenTelemetry initialized successfully"
    );

    Ok(Some((tracer, guard)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_server_api::config::OtelConfig;

    #[test]
    fn test_init_otel_disabled() {
        let config = OtelConfig {
            enabled: false,
            ..Default::default()
        };

        let result = init_otel(&config).expect("init_otel should not fail when disabled");
        assert!(result.is_none(), "Should return None when disabled");
    }

    #[tokio::test]
    async fn test_init_otel_enabled_creates_tracer() {
        // Note: This test verifies the tracer is created but won't actually export
        // spans since there's no OTLP collector running. The tracer will batch
        // spans and fail silently on export timeout.
        let config = OtelConfig {
            enabled: true,
            endpoint: "http://localhost:4317".to_string(),
            protocol: "grpc".to_string(),
            service_name: "test-service".to_string(),
            sampling_ratio: 1.0,
            export_timeout_secs: 1,
            max_queue_size: 100,
            shutdown_timeout_secs: 1,
        };

        let result = init_otel(&config).expect("init_otel should not fail");
        assert!(result.is_some(), "Should return tracer when enabled");

        let (_tracer, guard) = result.unwrap();
        // Guard will flush/shutdown on drop - this tests the cleanup path
        drop(guard);
    }
}

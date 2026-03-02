use adapteros_db::diagnostics::{
    get_all_diag_events_for_run, get_diag_run_by_id, insert_diag_run, SqliteDiagPersister,
};
use adapteros_telemetry::diagnostics::{
    spawn_diagnostics_writer, DiagEnvelope, DiagEvent, DiagLevel, DiagRunId, DiagSeverity,
    DiagStage, DiagnosticsConfig, DiagnosticsService, WriterConfig, DIAG_SCHEMA_VERSION,
};
use chrono::Utc;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

#[tokio::test]
async fn diagnostics_end_to_end_flow_persists_events() -> adapteros_core::Result<()> {
    let db = adapteros_db::Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("Diagnostics Tenant", false).await?;

    let trace_id = Uuid::new_v4().simple().to_string();
    let run_id = DiagRunId::from_trace_id(&trace_id);
    insert_diag_run(
        db.pool(),
        run_id.as_str(),
        &tenant_id,
        &trace_id,
        Utc::now().timestamp_millis(),
        "req-hash",
        None,
    )
    .await?;

    let config = DiagnosticsConfig {
        enabled: true,
        level: DiagLevel::Tokens,
        channel_capacity: 10,
        max_events_per_run: 100,
        batch_size: 1,
        batch_timeout_ms: 10,
    };
    let (service, receiver) = DiagnosticsService::new(config.clone());
    let service = Arc::new(service);

    let persister = SqliteDiagPersister::new_arc(db.pool().clone());
    let writer_config = WriterConfig {
        batch_size: config.batch_size,
        batch_timeout: Duration::from_millis(config.batch_timeout_ms),
        max_events_per_run: config.max_events_per_run,
    };
    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
    let writer_handle = spawn_diagnostics_writer(
        persister,
        receiver,
        service.run_tracker(),
        writer_config,
        shutdown_rx,
    );

    service.start_run(&run_id);

    let span_id = Uuid::new_v4().simple().to_string();
    let mut mono_us = 1000u64;
    let events = [
        DiagEvent::RunStarted {
            request_id: "req-123".to_string(),
            is_replay: false,
        },
        DiagEvent::StageEnter {
            stage: DiagStage::RequestValidation,
        },
        DiagEvent::StageExit {
            stage: DiagStage::RequestValidation,
            duration_us: 123,
            ok: true,
            error_code: None,
        },
    ];

    for event in events {
        let envelope = DiagEnvelope {
            schema_version: DIAG_SCHEMA_VERSION,
            emitted_at_mono_us: mono_us,
            trace_id: trace_id.clone(),
            span_id: span_id.clone(),
            tenant_id: tenant_id.clone(),
            run_id: run_id.clone(),
            severity: DiagSeverity::Info,
            payload: event,
        };
        mono_us += 1000;
        service.emit(envelope).expect("emit diagnostics event");
    }

    sleep(Duration::from_millis(50)).await;
    let _ = shutdown_tx.send(());
    writer_handle.await.expect("diagnostics writer");

    let records = get_all_diag_events_for_run(db.pool(), &tenant_id, run_id.as_str(), 100).await?;
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].event_type, "run_started");
    assert_eq!(records[1].event_type, "stage_enter");
    assert_eq!(records[2].event_type, "stage_exit");

    let run = get_diag_run_by_id(db.pool(), &tenant_id, run_id.as_str())
        .await?
        .expect("diagnostic run");
    assert_eq!(run.total_events_count, 3);

    Ok(())
}

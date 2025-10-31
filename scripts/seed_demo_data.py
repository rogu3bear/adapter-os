#!/usr/bin/env python3
"""
Seed AdapterOS development database with monitoring data and demo Git repositories.

Creates a small set of deterministic records in var/cp.db so the new monitoring
and repository endpoints return meaningful responses out of the box. The script
also provisions two lightweight Git repositories under var/demo_repos/.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import sqlite3
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable, List, Mapping, MutableMapping, Sequence


BASE_TIME = dt.datetime(2024, 10, 28, 15, 30, tzinfo=dt.timezone.utc)
DB_DEFAULT = Path("var/cp.db")
REPO_ROOT = Path("var/demo_repos")


def iso(minutes: int = 0) -> str:
    """Return a deterministic RFC3339 timestamp offset by minutes from BASE_TIME."""
    return (BASE_TIME + dt.timedelta(minutes=minutes)).isoformat().replace("+00:00", "Z")


def log(message: str) -> None:
    print(f"[seed] {message}")


def run_git(path: Path, *args: str) -> str:
    """Run a git command in `path` and return its stdout."""
    result = subprocess.run(
        ["git", *args],
        cwd=path,
        check=True,
        capture_output=True,
        text=True,
        env={
            **os.environ,
            "LC_ALL": "C",
            "GIT_TERMINAL_PROMPT": "0",
        },
    )
    return result.stdout.strip()


def ensure_git_configured(path: Path) -> None:
    """Set safe.directory for the created repo to avoid git ownership warnings."""
    subprocess.run(
        [
            "git",
            "config",
            "--local",
            "user.email",
            "demo-bot@adapteros.local",
        ],
        cwd=path,
        check=True,
        capture_output=True,
        text=True,
    )
    subprocess.run(
        [
            "git",
            "config",
            "--local",
            "user.name",
            "AdapterOS Demo Bot",
        ],
        cwd=path,
        check=True,
        capture_output=True,
        text=True,
    )


@dataclass
class EvidenceSpanSpec:
    span_id: str
    evidence_type: str
    file_path: str
    line_start: int
    line_end: int
    relevance_score: float
    content: str


@dataclass
class RepoSpec:
    repo_id: str
    branch: str
    path: Path
    description: str
    languages: List[MutableMapping[str, object]]
    frameworks: List[MutableMapping[str, object]]
    security: MutableMapping[str, object]
    evidence: List[EvidenceSpanSpec]
    repository_row: MutableMapping[str, object]
    files: Mapping[str, str] = field(default_factory=dict)


def write_files(root: Path, files: Mapping[str, str]) -> None:
    for relative, content in files.items():
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")


def init_git_repo(spec: RepoSpec) -> None:
    path = spec.path
    if (path / ".git").exists():
        log(f"Git repository already exists at {path}")
        return

    log(f"Creating demo repository at {path}")
    path.mkdir(parents=True, exist_ok=True)
    write_files(path, spec.files)

    subprocess.run(["git", "init"], cwd=path, check=True, capture_output=True, text=True)
    ensure_git_configured(path)
    subprocess.run(["git", "add", "."], cwd=path, check=True, capture_output=True, text=True)
    subprocess.run(
        ["git", "commit", "-m", "Seed demo repository"],
        cwd=path,
        check=True,
        capture_output=True,
        text=True,
    )

    # Create a second commit to make the history a touch more interesting.
    changelog = path / "CHANGELOG.md"
    changelog.write_text(
        f"# {spec.repo_id} changelog\n\n"
        "## 2024-10-28\n"
        "- Initial adapter integration\n",
        encoding="utf-8",
    )
    subprocess.run(["git", "add", "CHANGELOG.md"], cwd=path, check=True, capture_output=True, text=True)
    subprocess.run(
        ["git", "commit", "-m", "Document initial integration"],
        cwd=path,
        check=True,
        capture_output=True,
        text=True,
    )


def collect_git_info(path: Path) -> MutableMapping[str, object]:
    branch = run_git(path, "rev-parse", "--abbrev-ref", "HEAD")
    commit_count_str = run_git(path, "rev-list", "--count", "HEAD")
    commit_count = int(commit_count_str) if commit_count_str else 0
    last_commit = run_git(path, "log", "-1", "--pretty=%s") if commit_count else "Initial commit"
    authors_output = run_git(path, "log", "--format=%an") if commit_count else ""
    authors = sorted({author for author in authors_output.splitlines() if author}) or ["AdapterOS Demo Bot"]
    return {
        "branch": branch,
        "commit_count": commit_count,
        "last_commit": last_commit,
        "authors": authors,
    }


def prepare_demo_repos(skip_git: bool) -> Sequence[RepoSpec]:
    specs: List[RepoSpec] = [
        RepoSpec(
            repo_id="acme/monitoring-service",
            branch="main",
            path=REPO_ROOT / "monitoring-service",
            description="Runtime monitoring service for AdapterOS workers.",
            languages=[
                {"name": "Rust", "files": 2, "lines": 214, "percentage": 78.5},
                {"name": "Python", "files": 1, "lines": 58, "percentage": 21.5},
            ],
            frameworks=[
                {
                    "name": "Axum",
                    "version": "0.7",
                    "confidence": 0.92,
                    "files": ["src/main.rs", "src/routes.rs"],
                },
                {
                    "name": "Tokio",
                    "version": "1.38",
                    "confidence": 0.88,
                    "files": ["src/main.rs"],
                },
            ],
            security={
                "status": "warning",
                "violations": [
                    {
                        "file_path": "src/routes.rs",
                        "pattern": "unwrap()",
                        "line_number": 28,
                        "severity": "low",
                    }
                ],
            },
            evidence=[
                EvidenceSpanSpec(
                    span_id="monitoring-service::main_loop",
                    evidence_type="code_symbol",
                    file_path="src/main.rs",
                    line_start=18,
                    line_end=41,
                    relevance_score=0.91,
                    content=(
                        "async fn main() {\n"
                        "    tracing_subscriber::fmt::init();\n"
                        "    let router = routes::build_router();\n"
                        "    let addr = (Ipv4Addr::LOCALHOST, 4004);\n"
                        "    tracing::info!(?addr, \"starting monitoring service\");\n"
                        "    axum::Server::bind(&addr.into())\n"
                        "        .serve(router.into_make_service())\n"
                        "        .await\n"
                        "        .expect(\"server shutdown\");\n"
                        "}\n"
                    ),
                ),
                EvidenceSpanSpec(
                    span_id="monitoring-service::latency_guard",
                    evidence_type="unit_test",
                    file_path="scripts/check_metrics.py",
                    line_start=12,
                    line_end=33,
                    relevance_score=0.78,
                    content=(
                        "def verify_latency(bucket: list[float]) -> bool:\n"
                        "    if not bucket:\n"
                        "        return True\n"
                        "    p95 = statistics.quantiles(bucket, n=100)[94]\n"
                        "    return p95 < 180.0\n"
                        "\n"
                        "def detect_spike(series: list[float]) -> bool:\n"
                        "    recent = series[-5:]\n"
                        "    return any(value > 0.85 for value in recent)\n"
                    ),
                ),
            ],
            repository_row={
                "id": "repo-demo-monitoring",
                "languages": ["Rust", "Python"],
                "frameworks": ["Axum", "Tokio"],
                "file_count": 47,
                "symbol_count": 1124,
                "latest_scan_commit": "d1f7c9e",
                "latest_graph_hash": "graph-demo-monitoring",
            },
            files={
                "README.md": (
                    "# Monitoring Service\n\n"
                    "Demo service that exposes health metrics for AdapterOS workers. "
                    "Used to exercise the monitoring endpoints end-to-end.\n"
                ),
                "Cargo.toml": (
                    "[package]\n"
                    "name = \"monitoring-service\"\n"
                    "version = \"0.1.0\"\n"
                    "edition = \"2021\"\n\n"
                    "[dependencies]\n"
                    "axum = \"0.7\"\n"
                    "serde = { version = \"1.0\", features = [\"derive\"] }\n"
                    "serde_json = \"1.0\"\n"
                    "tokio = { version = \"1.38\", features = [\"macros\", \"rt-multi-thread\"] }\n"
                    "tracing = \"0.1\"\n"
                    "tracing-subscriber = \"0.3\"\n"
                ),
                "src/main.rs": (
                    "use std::net::Ipv4Addr;\n"
                    "use axum::Router;\n\n"
                    "mod routes;\n\n"
                    "async fn serve() {\n"
                    "    let router = routes::build_router();\n"
                    "    let addr = (Ipv4Addr::LOCALHOST, 4004);\n"
                    "    tracing::info!(?addr, \"serving demo monitoring metrics\");\n"
                    "    axum::Server::bind(&addr.into())\n"
                    "        .serve(router.into_make_service())\n"
                    "        .await\n"
                    "        .expect(\"server shutdown\");\n"
                    "}\n\n"
                    "#[tokio::main]\n"
                    "async fn main() {\n"
                    "    tracing_subscriber::fmt::init();\n"
                    "    serve().await;\n"
                    "}\n"
                ),
                "src/routes.rs": (
                    "use axum::{routing::get, Json, Router};\n"
                    "use serde::Serialize;\n\n"
                    "#[derive(Serialize)]\n"
                    "struct Metrics {\n"
                    "    worker_id: &'static str,\n"
                    "    cpu_usage: f32,\n"
                    "    memory_mb: f32,\n"
                    "    latency_p95_ms: f32,\n"
                    "}\n\n"
                    "async fn metrics() -> Json<Metrics> {\n"
                    "    Json(Metrics {\n"
                    "        worker_id: \"worker-demo-frontend\",\n"
                    "        cpu_usage: 0.72,\n"
                    "        memory_mb: 18304.0,\n"
                    "        latency_p95_ms: 148.0,\n"
                    "    })\n"
                    "}\n\n"
                    "pub fn build_router() -> Router {\n"
                    "    Router::new().route(\"/metrics\", get(metrics))\n"
                    "}\n"
                ),
                "scripts/check_metrics.py": (
                    "import statistics\n"
                    "from typing import Iterable\n\n"
                    "def verify_latency(bucket: list[float]) -> bool:\n"
                    "    if not bucket:\n"
                    "        return True\n"
                    "    p95 = statistics.quantiles(bucket, n=100)[94]\n"
                    "    return p95 < 180.0\n\n"
                    "def detect_spike(series: Iterable[float]) -> bool:\n"
                    "    recent = list(series)[-5:]\n"
                    "    return any(value > 0.85 for value in recent)\n"
                ),
            },
        ),
        RepoSpec(
            repo_id="acme/ml-platform",
            branch="main",
            path=REPO_ROOT / "ml-platform",
            description="Control-plane UI that orchestrates AdapterOS training jobs.",
            languages=[
                {"name": "TypeScript", "files": 3, "lines": 160, "percentage": 64.0},
                {"name": "YAML", "files": 1, "lines": 90, "percentage": 36.0},
            ],
            frameworks=[
                {
                    "name": "Next.js",
                    "version": "14.1",
                    "confidence": 0.89,
                    "files": ["src/index.ts"],
                },
                {
                    "name": "Dagster",
                    "version": "1.7",
                    "confidence": 0.76,
                    "files": ["infra/pipeline.yaml"],
                },
            ],
            security={"status": "clean", "violations": []},
            evidence=[
                EvidenceSpanSpec(
                    span_id="ml-platform::adapter_dashboard",
                    evidence_type="ui_component",
                    file_path="src/index.ts",
                    line_start=20,
                    line_end=55,
                    relevance_score=0.84,
                    content=(
                        "export function AdapterDashboard(props: DashboardProps) {\n"
                        "  const [selectedAdapter, setSelectedAdapter] = useState(props.adapters[0]);\n"
                        "  return (\n"
                        "    <section className=\"dashboard\">\n"
                        "      <header className=\"dashboard__header\">\n"
                        "        <h1>Adapter Overview</h1>\n"
                        "        <LatencyBadge value={props.metrics.latencyP95} />\n"
                        "      </header>\n"
                        "      <AdapterGrid adapters={props.adapters} onSelect={setSelectedAdapter} />\n"
                        "      <AdapterPanel adapter={selectedAdapter} />\n"
                        "    </section>\n"
                        "  );\n"
                        "}\n"
                    ),
                )
            ],
            repository_row={
                "id": "repo-demo-ml-platform",
                "languages": ["TypeScript", "YAML"],
                "frameworks": ["Next.js", "Dagster"],
                "file_count": 63,
                "symbol_count": 1644,
                "latest_scan_commit": "9ab45df",
                "latest_graph_hash": "graph-demo-ml-platform",
            },
            files={
                "README.md": (
                    "# ML Platform\n\n"
                    "Demo front-end that references AdapterOS adapters and pipelines.\n"
                ),
                "package.json": (
                    "{\n"
                    '  "name": "ml-platform",\n'
                    '  "version": "0.1.0",\n'
                    '  "type": "module",\n'
                    '  "scripts": {\n'
                    '    "dev": "next dev",\n'
                    '    "build": "next build"\n'
                    "  }\n"
                    "}\n"
                ),
                "src/index.ts": (
                    "import { useMemo, useState } from \"react\";\n"
                    "import { AdapterGrid } from \"./modules/AdapterGrid\";\n"
                    "import { AdapterPanel } from \"./modules/AdapterPanel\";\n"
                    "import { LatencyBadge } from \"./modules/LatencyBadge\";\n\n"
                    "type Adapter = {\n"
                    "  id: string;\n"
                    "  displayName: string;\n"
                    "  tier: \"core\" | \"extension\";\n"
                    "  status: \"ready\" | \"loading\" | \"error\";\n"
                    "};\n\n"
                    "type DashboardProps = {\n"
                    "  adapters: Adapter[];\n"
                    "  metrics: { latencyP95: number; errorRate: number };\n"
                    "};\n\n"
                    "export function AdapterDashboard(props: DashboardProps) {\n"
                    "  const [selectedAdapter, setSelectedAdapter] = useState(props.adapters[0]);\n"
                    "  const cardMetrics = useMemo(\n"
                    "    () => ({\n"
                    "      latency: props.metrics.latencyP95,\n"
                    "      errorRate: props.metrics.errorRate,\n"
                    "    }),\n"
                    "    [props.metrics]\n"
                    "  );\n"
                    "  return (\n"
                    "    <section className=\"dashboard\">\n"
                    "      <header className=\"dashboard__header\">\n"
                    "        <h1>Adapter Overview</h1>\n"
                    "        <LatencyBadge value={props.metrics.latencyP95} />\n"
                    "      </header>\n"
                    "      <AdapterGrid adapters={props.adapters} onSelect={setSelectedAdapter} />\n"
                    "      <AdapterPanel adapter={selectedAdapter} metrics={cardMetrics} />\n"
                    "    </section>\n"
                    "  );\n"
                    "}\n"
                ),
                "infra/pipeline.yaml": (
                    "version: 1\n"
                    "name: adapteros-training\n"
                    "schedule: \"0 * * * *\"\n"
                    "tasks:\n"
                    "  - name: fanout-adapters\n"
                    "    runtime: dagster\n"
                    "    retries: 2\n"
                    "    spec:\n"
                    "      adapterIds:\n"
                    "        - demo-router\n"
                    "        - demo-planner\n"
                    "      dataset: gold-prompts\n"
                ),
            },
        ),
    ]

    if not skip_git:
        for spec in specs:
            init_git_repo(spec)
    else:
        log("Skipping git repository creation (--skip-git enabled)")

    return specs


def delete_existing(cur: sqlite3.Cursor, table: str, id_field: str, ids: Iterable[str]) -> None:
    cur.executemany(f"DELETE FROM {table} WHERE {id_field} = ?", [(id_value,) for id_value in ids])


def seed_core_entities(cur: sqlite3.Cursor) -> None:
    log("Seeding tenants, manifests, plans, nodes, and workers")
    cur.execute(
        "INSERT OR IGNORE INTO tenants (id, name, created_at) VALUES (?, ?, ?)",
        ("default", "default", iso()),
    )

    delete_existing(cur, "manifests", "id", ["manifest-demo-qwen"])
    cur.execute(
        "INSERT INTO manifests (id, tenant_id, hash_b3, body_json, created_at) VALUES (?, ?, ?, ?, ?)",
        (
            "manifest-demo-qwen",
            "default",
            "7c91a0bf35d594525cb0adfd65c1e5ee9f18e345d737ac6af5e2bf35e8f02a5b",
            json.dumps({"name": "Demo manifest", "kernel_hash": "a84d9f1c", "adapters": ["demo-adapter"]}),
            iso(),
        ),
    )

    delete_existing(cur, "plans", "id", ["plan-demo-qwen"])
    cur.execute(
        """
        INSERT INTO plans (
            id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3,
            metadata_json, created_at, cpid
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            "plan-demo-qwen",
            "default",
            "ccf4a90e14d9bf5cb4f1579bbde98de6139e37732cf1f2f3d8b5328e1674c3da",
            "7c91a0bf35d594525cb0adfd65c1e5ee9f18e345d737ac6af5e2bf35e8f02a5b",
            json.dumps({"router": "a84d9f1c", "executor": "bc0d7e21"}),
            "d8777becdfad8eaf7c9d2fe2ffad9d816d7c0dfae15c1e24b5188d7f2adadf31",
            json.dumps({"display_name": "Demo Control Plane", "adapters": ["demo-adapter"]}),
            iso(5),
            "cp-demo",
        ),
    )

    delete_existing(cur, "nodes", "id", ["node-demo-east"])
    cur.execute(
        """
        INSERT INTO nodes (id, hostname, agent_endpoint, status, last_seen_at, labels_json, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        """,
        (
            "node-demo-east",
            "demo-east.local",
            "http://127.0.0.1:9101",
            "active",
            iso(10),
            json.dumps({"arch": "apple-silicon", "region": "iad"}),
            iso(2),
        ),
    )

    delete_existing(cur, "workers", "id", ["worker-demo-frontend"])
    cur.execute(
        """
        INSERT INTO workers (
            id, tenant_id, node_id, plan_id, uds_path, pid, status,
            memory_headroom_pct, k_current, adapters_loaded_json, started_at, last_heartbeat_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            "worker-demo-frontend",
            "default",
            "node-demo-east",
            "plan-demo-qwen",
            "/tmp/aos-demo.sock",
            4312,
            "serving",
            34.2,
            16,
            json.dumps(["demo-adapter", "safety-kit"]),
            iso(12),
            iso(18),
        ),
    )


def seed_monitoring_data(cur: sqlite3.Cursor) -> None:
    log("Seeding monitoring rules, metrics, alerts, anomalies, and dashboards")

    delete_existing(
        cur,
        "process_monitoring_rules",
        "id",
        ["rule-cpu-spike", "rule-latency-95th", "rule-memory-headroom"],
    )
    cur.executemany(
        """
        INSERT INTO process_monitoring_rules (
            id, name, description, tenant_id, rule_type, metric_name, threshold_value,
            threshold_operator, severity, evaluation_window_seconds, cooldown_seconds,
            is_active, notification_channels, escalation_rules, created_by, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        [
            (
                "rule-cpu-spike",
                "CPU spike detection",
                "Triggers when CPU usage stays above 85% for five minutes.",
                "default",
                "cpu",
                "cpu_usage_percent",
                85.0,
                "gt",
                "critical",
                300,
                120,
                1,
                json.dumps({"pagerduty": ["primary-oncall"], "slack": ["#ops-alerts"]}),
                json.dumps([{"after_seconds": 300, "route": "incident_bridge"}]),
                "admin-user",
                iso(15),
                iso(15),
            ),
            (
                "rule-latency-95th",
                "P95 latency guardrail",
                "Warn when worker p95 latency exceeds 200ms.",
                "default",
                "latency",
                "latency_p95_ms",
                200.0,
                "gt",
                "warning",
                600,
                180,
                1,
                json.dumps({"slack": ["#ops-alerts"]}),
                json.dumps([{"after_seconds": 900, "route": "email"}]),
                "admin-user",
                iso(16),
                iso(16),
            ),
            (
                "rule-memory-headroom",
                "Memory headroom floor",
                "Alert when available memory headroom drops below 15%.",
                "default",
                "memory",
                "memory_headroom_pct",
                15.0,
                "lt",
                "error",
                300,
                300,
                1,
                json.dumps({"pagerduty": ["secondary-oncall"]}),
                json.dumps([{"after_seconds": 600, "route": "severe-incident"}]),
                "admin-user",
                iso(17),
                iso(17),
            ),
        ],
    )

    delete_existing(
        cur,
        "process_health_metrics",
        "id",
        ["metric-cpu-1", "metric-cpu-2", "metric-mem-1", "metric-latency-1"],
    )
    cur.executemany(
        """
        INSERT INTO process_health_metrics (
            id, worker_id, tenant_id, metric_name, metric_value, metric_unit, tags, collected_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        """,
        [
            (
                "metric-cpu-1",
                "worker-demo-frontend",
                "default",
                "cpu_usage_percent",
                72.4,
                "%",
                json.dumps({"aggregation": "p95"}),
                iso(19),
            ),
            (
                "metric-cpu-2",
                "worker-demo-frontend",
                "default",
                "cpu_usage_percent",
                91.8,
                "%",
                json.dumps({"aggregation": "p99"}),
                iso(20),
            ),
            (
                "metric-mem-1",
                "worker-demo-frontend",
                "default",
                "memory_headroom_pct",
                18.9,
                "%",
                json.dumps({"unit": "%"}),
                iso(21),
            ),
            (
                "metric-latency-1",
                "worker-demo-frontend",
                "default",
                "latency_p95_ms",
                212.0,
                "ms",
                json.dumps({"window": "10m"}),
                iso(22),
            ),
        ],
    )

    delete_existing(
        cur,
        "process_alerts",
        "id",
        ["alert-cpu-2024", "alert-latency-2024"],
    )
    cur.executemany(
        """
        INSERT INTO process_alerts (
            id, rule_id, worker_id, tenant_id, alert_type, severity, title, message,
            metric_value, threshold_value, status, acknowledged_by, acknowledged_at,
            resolved_at, suppression_reason, suppression_until, escalation_level,
            notification_sent, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        [
            (
                "alert-cpu-2024",
                "rule-cpu-spike",
                "worker-demo-frontend",
                "default",
                "threshold_violation",
                "critical",
                "CPU usage critical on worker-demo-frontend",
                "CPU usage exceeded 92% for more than 5 minutes.",
                92.3,
                85.0,
                "active",
                None,
                None,
                None,
                None,
                None,
                1,
                1,
                iso(20),
                iso(20),
            ),
            (
                "alert-latency-2024",
                "rule-latency-95th",
                "worker-demo-frontend",
                "default",
                "threshold_violation",
                "warning",
                "Latency p95 trending high",
                "Latency crossed 210ms in the last 15m window.",
                210.4,
                200.0,
                "acknowledged",
                "admin-user",
                iso(23),
                None,
                None,
                None,
                0,
                1,
                iso(22),
                iso(23),
            ),
        ],
    )

    delete_existing(
        cur,
        "process_anomalies",
        "id",
        ["anomaly-memory-drift"],
    )
    cur.execute(
        """
        INSERT INTO process_anomalies (
            id, worker_id, tenant_id, anomaly_type, detected_at, severity, confidence,
            baseline_value, current_value, threshold_value, duration_seconds, context_json,
            resolved_at, resolution_action, created_at, metric_name, detected_value,
            expected_range_min, expected_range_max, confidence_score, description,
            detection_method, model_version, status, investigated_by, investigation_notes
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            "anomaly-memory-drift",
            "worker-demo-frontend",
            "default",
            "memory_leak",
            iso(24),
            "high",
            0.87,
            12.0,
            6.2,
            8.0,
            900,
            json.dumps({"correlated_alert": "alert-cpu-2024"}),
            None,
            None,
            iso(24),
            "memory_headroom_pct",
            6.2,
            10.0,
            40.0,
            0.87,
            "Memory headroom trending downward faster than expected.",
            "autoencoder",
            "v0.5.2",
            "investigating",
            "admin-user",
            "Investigating potential load-regression introduced yesterday.",
        ),
    )

    delete_existing(
        cur,
        "process_performance_baselines",
        "id",
        ["baseline-latency-rolling"],
    )
    cur.execute(
        """
        INSERT INTO process_performance_baselines (
            id, worker_id, tenant_id, metric_name, baseline_value, baseline_type,
            calculation_period_days, confidence_interval, standard_deviation,
            percentile_95, percentile_99, is_active, calculated_at, expires_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            "baseline-latency-rolling",
            "worker-demo-frontend",
            "default",
            "latency_p95_ms",
            165.0,
            "historical",
            14,
            0.95,
            18.0,
            190.0,
            215.0,
            1,
            iso(25),
            iso(25 + 24 * 3),
        ),
    )

    delete_existing(
        cur,
        "process_monitoring_dashboards",
        "id",
        ["dashboard-runtime-overview"],
    )
    cur.execute(
        """
        INSERT INTO process_monitoring_dashboards (
            id, name, description, tenant_id, dashboard_config, is_shared,
            created_by, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            "dashboard-runtime-overview",
            "Runtime Overview",
            "Key SLOs for active AdapterOS workers.",
            "default",
            json.dumps(
                {
                    "layout": {"columns": 12, "rows": 6},
                    "widgets": [
                        {"id": "widget-cpu-heatmap", "col": 0, "row": 0, "width": 6, "height": 3},
                        {"id": "widget-latency-trend", "col": 6, "row": 0, "width": 6, "height": 3},
                    ],
                }
            ),
            1,
            "admin-user",
            iso(26),
            iso(26),
        ),
    )

    delete_existing(
        cur,
        "process_monitoring_widgets",
        "id",
        ["widget-cpu-heatmap", "widget-latency-trend"],
    )
    cur.executemany(
        """
        INSERT INTO process_monitoring_widgets (
            id, dashboard_id, widget_type, widget_config, position_x, position_y,
            width, height, refresh_interval_seconds, is_visible, created_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        [
            (
                "widget-cpu-heatmap",
                "dashboard-runtime-overview",
                "heatmap",
                json.dumps({"metric": "cpu_usage_percent", "worker_ids": ["worker-demo-frontend"]}),
                0,
                0,
                6,
                3,
                30,
                1,
                iso(26),
            ),
            (
                "widget-latency-trend",
                "dashboard-runtime-overview",
                "timeseries",
                json.dumps({"metric": "latency_p95_ms", "window": "24h"}),
                6,
                0,
                6,
                3,
                30,
                1,
                iso(26),
            ),
        ],
    )

    delete_existing(
        cur,
        "process_monitoring_reports",
        "id",
        ["report-weekly-runtime"],
    )
    cur.execute(
        """
        INSERT INTO process_monitoring_reports (
            id, name, description, tenant_id, report_type, report_config, generated_at,
            report_data, file_path, file_size_bytes, created_by
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            "report-weekly-runtime",
            "Weekly Runtime Health",
            "Aggregated health report for the primary tenant.",
            "default",
            "health_summary",
            json.dumps({"period": "7d", "metrics": ["cpu_usage_percent", "latency_p95_ms"]}),
            iso(30),
            json.dumps({"cpu": {"avg": 68.4}, "latency_p95": {"avg": 175.3}}),
            None,
            None,
            "admin-user",
        ),
    )

    delete_existing(
        cur,
        "process_monitoring_notifications",
        "id",
        ["notif-alert-cpu"],
    )
    cur.execute(
        """
        INSERT INTO process_monitoring_notifications (
            id, alert_id, notification_type, recipient, message, status,
            sent_at, delivered_at, error_message, retry_count, created_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            "notif-alert-cpu",
            "alert-cpu-2024",
            "slack",
            "#ops-alerts",
            "CPU usage exceeded 92% on worker-demo-frontend.",
            "sent",
            iso(21),
            iso(21),
            None,
            0,
            iso(21),
        ),
    )


def seed_git_metadata(cur: sqlite3.Cursor, specs: Sequence[RepoSpec]) -> None:
    log("Seeding git repository metadata and training records")

    delete_existing(
        cur,
        "repository_training_metrics",
        "id",
        ["train-metric-monitoring-loss", "train-metric-monitoring-accuracy", "train-metric-ml-loss"],
    )
    delete_existing(
        cur,
        "repository_training_jobs",
        "id",
        ["train-job-monitoring", "train-job-ml-platform"],
    )
    delete_existing(
        cur,
        "repository_evidence_spans",
        "id",
        [span.span_id for spec in specs for span in spec.evidence],
    )
    delete_existing(
        cur,
        "repository_security_violations",
        "id",
        ["sec-viol-monitoring-1"],
    )
    delete_existing(
        cur,
        "repository_analysis_cache",
        "id",
        ["analysis-cache-monitoring-summary", "analysis-cache-ml-summary"],
    )
    delete_existing(
        cur,
        "repositories",
        "id",
        ["repo-demo-monitoring", "repo-demo-ml-platform"],
    )

    for spec in specs:
        git_info = collect_git_info(spec.path)
        analysis = {
            "repo_id": spec.repo_id,
            "languages": spec.languages,
            "frameworks": spec.frameworks,
            "security_scan": {
                **spec.security,
                "scan_timestamp": iso(32),
            },
            "git_info": git_info,
            "evidence_spans": [
                {
                    "span_id": span.span_id,
                    "evidence_type": span.evidence_type,
                    "file_path": span.file_path,
                    "line_range": [span.line_start, span.line_end],
                    "relevance_score": span.relevance_score,
                    "content": span.content,
                }
                for span in spec.evidence
            ],
        }

        repo_id = "git-demo-monitoring" if "monitoring" in spec.repo_id else "git-demo-ml-platform"
        created = iso(33 if "monitoring" in spec.repo_id else 34)
        cur.execute(
            """
            INSERT INTO git_repositories (
                id, repo_id, path, branch, analysis_json, evidence_json,
                security_scan_json, status, created_by, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                repo_id = excluded.repo_id,
                path = excluded.path,
                branch = excluded.branch,
                analysis_json = excluded.analysis_json,
                evidence_json = excluded.evidence_json,
                security_scan_json = excluded.security_scan_json,
                status = excluded.status,
                created_by = excluded.created_by,
                created_at = excluded.created_at
            """,
            (
                repo_id,
                spec.repo_id,
                str(spec.path.resolve()),
                spec.branch,
                json.dumps(analysis),
                json.dumps(analysis["evidence_spans"]),
                json.dumps(analysis["security_scan"]),
                "ready",
                "admin-user",
                created,
            ),
        )

        for span in spec.evidence:
            cur.execute(
                """
                INSERT INTO repository_evidence_spans (
                    id, repo_id, span_id, evidence_type, file_path, line_start,
                    line_end, relevance_score, content
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    span.span_id,
                    spec.repo_id,
                    span.span_id,
                    span.evidence_type,
                    span.file_path,
                    span.line_start,
                    span.line_end,
                    span.relevance_score,
                    span.content,
                ),
            )

        if spec.security.get("violations"):
            violation = spec.security["violations"][0]
            cur.execute(
                """
                INSERT INTO repository_security_violations (
                    id, repo_id, file_path, pattern, line_number, severity, created_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    "sec-viol-monitoring-1",
                    spec.repo_id,
                    violation["file_path"],
                    violation["pattern"],
                    violation.get("line_number"),
                    violation["severity"],
                    created,
                ),
            )

        job_id = "train-job-monitoring" if "monitoring" in spec.repo_id else "train-job-ml-platform"
        training_config = {
            "rank": 16,
            "alpha": 32,
            "epochs": 4,
            "learning_rate": 0.001,
            "targets": ["router", "planner"],
        }
        progress = {"epoch": 4, "status": "completed", "final_loss": 0.12}
        cur.execute(
            """
            INSERT INTO repository_training_jobs (
                id, repo_id, training_config_json, status, progress_json, started_at,
                completed_at, created_by
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                job_id,
                spec.repo_id,
                json.dumps(training_config),
                "completed",
                json.dumps(progress),
                iso(35),
                iso(36),
                "admin-user",
            ),
        )

        metric_rows = [
            (
                "train-metric-monitoring-loss"
                if "monitoring" in spec.repo_id
                else "train-metric-ml-loss",
                job_id,
                "loss",
                0.118 if "monitoring" in spec.repo_id else 0.142,
                iso(36),
            )
        ]
        if "monitoring" in spec.repo_id:
            metric_rows.append(
                (
                    "train-metric-monitoring-accuracy",
                    job_id,
                    "accuracy",
                    0.94,
                    iso(36),
                )
            )

        cur.executemany(
            """
            INSERT INTO repository_training_metrics (
                id, training_job_id, metric_name, metric_value, metric_timestamp
            )
            VALUES (?, ?, ?, ?, ?)
            """,
            metric_rows,
        )

        cache_id = "analysis-cache-monitoring-summary" if "monitoring" in spec.repo_id else "analysis-cache-ml-summary"
        cur.execute(
            """
            INSERT INTO repository_analysis_cache (
                id, repo_id, analysis_type, analysis_data_json, cache_key, expires_at, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (
                cache_id,
                spec.repo_id,
                "language_summary",
                json.dumps({"top_language": spec.languages[0]["name"], "secondary": spec.languages[1]["name"]}),
                f"{spec.repo_id}:{spec.branch}",
                iso(36 + 24 * 3),
                created,
            ),
        )

        repo_row = spec.repository_row
        cur.execute(
            """
            INSERT INTO repositories (
                id, repo_id, path, languages, default_branch, status, frameworks_json,
                file_count, symbol_count, created_at, updated_at, tenant_id,
                latest_scan_commit, latest_scan_at, latest_graph_hash, languages_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                repo_row["id"],
                spec.repo_id,
                str(spec.path.resolve()),
                json.dumps(repo_row["languages"]),
                spec.branch,
                "ready",
                json.dumps(repo_row["frameworks"]),
                repo_row["file_count"],
                repo_row["symbol_count"],
                created,
                created,
                "default",
                repo_row["latest_scan_commit"],
                iso(31),
                repo_row["latest_graph_hash"],
                json.dumps(repo_row["languages"]),
            ),
        )


def run(db_path: Path, skip_git: bool) -> None:
    specs = prepare_demo_repos(skip_git=skip_git)

    if not db_path.exists():
        raise SystemExit(f"Database {db_path} does not exist. Start the control plane once or run migrations first.")

    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    # Some legacy tables declare foreign keys without the required UNIQUE constraints.
    # Disable enforcement so the seed script can operate idempotently.
    conn.execute("PRAGMA foreign_keys = OFF;")
    try:
        with conn:
            seed_core_entities(conn.cursor())
            seed_monitoring_data(conn.cursor())
            seed_git_metadata(conn.cursor(), specs)
    finally:
        conn.close()

    log("Seeding complete ✅")


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Seed demo monitoring data and Git repositories.")
    parser.add_argument(
        "--db",
        type=Path,
        default=DB_DEFAULT,
        help=f"Path to SQLite database (default: {DB_DEFAULT})",
    )
    parser.add_argument(
        "--skip-git",
        action="store_true",
        help="Skip creating the demo Git repositories (expects them to already exist).",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> None:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    run(args.db, skip_git=args.skip_git)


if __name__ == "__main__":
    main()

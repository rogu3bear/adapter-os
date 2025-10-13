//! HTML report generation from telemetry bundles

use crate::replay::{load_replay_bundle, ReplayBundle};
use adapteros_core::Result;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Generate an HTML report from a telemetry bundle
pub fn generate_html_report<P: AsRef<Path>, Q: AsRef<Path>>(
    bundle_path: P,
    output_path: Q,
) -> Result<()> {
    let bundle = load_replay_bundle(bundle_path)?;
    let html = create_report_html(&bundle)?;

    let mut file = File::create(output_path.as_ref()).map_err(|e| {
        adapteros_core::AosError::Telemetry(format!("Failed to create report: {}", e))
    })?;

    file.write_all(html.as_bytes()).map_err(|e| {
        adapteros_core::AosError::Telemetry(format!("Failed to write report: {}", e))
    })?;

    Ok(())
}

fn create_report_html(bundle: &ReplayBundle) -> Result<String> {
    // Analyze events
    let stats = analyze_bundle(bundle);

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>AdapterOS Telemetry Report - {cpid}</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{ 
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: #f5f5f7;
            padding: 20px;
            line-height: 1.6;
        }}
        .container {{ 
            max-width: 1200px;
            margin: 0 auto;
            background: white;
            border-radius: 12px;
            box-shadow: 0 2px 8px rgba(0,0,0,0.1);
            overflow: hidden;
        }}
        header {{
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 30px;
        }}
        h1 {{ font-size: 28px; margin-bottom: 10px; }}
        .subtitle {{ opacity: 0.9; font-size: 14px; }}
        .content {{ padding: 30px; }}
        .stat-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
            gap: 20px;
            margin: 20px 0;
        }}
        .stat-card {{
            background: #f8f9fa;
            border-radius: 8px;
            padding: 20px;
            border-left: 4px solid #667eea;
        }}
        .stat-value {{
            font-size: 32px;
            font-weight: bold;
            color: #667eea;
            margin: 10px 0;
        }}
        .stat-label {{ color: #666; font-size: 14px; }}
        .chart {{
            background: #f8f9fa;
            border-radius: 8px;
            padding: 20px;
            margin: 20px 0;
        }}
        .bar {{
            display: flex;
            align-items: center;
            margin: 10px 0;
        }}
        .bar-label {{ width: 150px; font-size: 14px; }}
        .bar-fill {{
            flex: 1;
            height: 30px;
            background: linear-gradient(90deg, #667eea 0%, #764ba2 100%);
            border-radius: 4px;
            display: flex;
            align-items: center;
            padding: 0 10px;
            color: white;
            font-weight: bold;
        }}
        table {{
            width: 100%;
            border-collapse: collapse;
            margin: 20px 0;
        }}
        th, td {{
            padding: 12px;
            text-align: left;
            border-bottom: 1px solid #e0e0e0;
        }}
        th {{ background: #f8f9fa; font-weight: 600; }}
        .timestamp {{ color: #666; font-size: 12px; }}
    </style>
</head>
<body>
    <div class="container">
        <header>
            <h1>AdapterOS Telemetry Report</h1>
            <div class="subtitle">CPID: {cpid} | Plan: {plan_id}</div>
        </header>
        
        <div class="content">
            <h2>Summary Statistics</h2>
            <div class="stat-grid">
                <div class="stat-card">
                    <div class="stat-label">Total Events</div>
                    <div class="stat-value">{total_events}</div>
                </div>
                <div class="stat-card">
                    <div class="stat-label">Event Types</div>
                    <div class="stat-value">{event_types}</div>
                </div>
                <div class="stat-card">
                    <div class="stat-label">Duration</div>
                    <div class="stat-value">{duration_ms}ms</div>
                </div>
                <div class="stat-card">
                    <div class="stat-label">Bundle Hash</div>
                    <div class="stat-value" style="font-size: 16px;">{seed_hash}</div>
                </div>
            </div>

            <h2>Event Distribution</h2>
            <div class="chart">
                {event_bars}
            </div>

            <h2>Event Timeline (Recent)</h2>
            <table>
                <thead>
                    <tr>
                        <th>Timestamp</th>
                        <th>Type</th>
                        <th>Hash</th>
                    </tr>
                </thead>
                <tbody>
                    {event_rows}
                </tbody>
            </table>
        </div>
    </div>
</body>
</html>"#,
        cpid = bundle.cpid,
        plan_id = bundle.plan_id,
        total_events = stats.total_events,
        event_types = stats.event_type_counts.len(),
        duration_ms = stats.duration_ms,
        seed_hash = bundle.seed_global,
        event_bars = generate_event_bars(&stats.event_type_counts),
        event_rows = generate_event_rows(&bundle.events, 20),
    );

    Ok(html)
}

struct BundleStats {
    total_events: usize,
    event_type_counts: HashMap<String, usize>,
    duration_ms: u128,
}

fn analyze_bundle(bundle: &ReplayBundle) -> BundleStats {
    let mut event_type_counts: HashMap<String, usize> = HashMap::new();

    for event in &bundle.events {
        *event_type_counts
            .entry(event.event_type.clone())
            .or_insert(0) += 1;
    }

    let duration_ms = if bundle.events.len() >= 2 {
        let first = bundle
            .events
            .first()
            .expect("Bundle should have at least 2 events")
            .timestamp;
        let last = bundle
            .events
            .last()
            .expect("Bundle should have at least 2 events")
            .timestamp;
        (last - first) / 1_000_000 // Convert ns to ms
    } else {
        0
    };

    BundleStats {
        total_events: bundle.events.len(),
        event_type_counts,
        duration_ms,
    }
}

fn generate_event_bars(counts: &HashMap<String, usize>) -> String {
    let max_count = counts.values().max().copied().unwrap_or(1);
    let mut bars = Vec::new();

    let mut sorted: Vec<_> = counts.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));

    for (event_type, count) in sorted {
        let width_pct = (*count as f64 / max_count as f64) * 100.0;
        bars.push(format!(
            r#"<div class="bar">
                <div class="bar-label">{}</div>
                <div class="bar-fill" style="width: {}%">{}</div>
            </div>"#,
            event_type, width_pct, count
        ));
    }

    bars.join("\n")
}

fn generate_event_rows(events: &[crate::replay::ReplayEvent], limit: usize) -> String {
    let mut rows = Vec::new();

    for event in events.iter().rev().take(limit) {
        rows.push(format!(
            r#"<tr>
                <td class="timestamp">{}</td>
                <td>{}</td>
                <td style="font-family: monospace; font-size: 12px;">{}</td>
            </tr>"#,
            format_timestamp(event.timestamp),
            event.event_type,
            format!("{}", event.event_hash)
                .chars()
                .take(16)
                .collect::<String>(),
        ));
    }

    rows.join("\n")
}

fn format_timestamp(ts: u128) -> String {
    // Convert nanoseconds to milliseconds
    let ms = ts / 1_000_000;
    format!("{}ms", ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_timestamp() {
        assert_eq!(format_timestamp(1_500_000_000), "1500ms");
    }
}

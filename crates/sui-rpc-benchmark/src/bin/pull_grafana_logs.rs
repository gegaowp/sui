// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//! This script pulls "Sampled read request" logs from Grafana, extracts JSON bodies,
//! and groups them by RPC "method" for potential replay or analysis.

use reqwest::header::{ACCEPT, AUTHORIZATION};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::process;
use tracing::{error, info, warn};

#[derive(Debug, Deserialize)]
struct GrafanaLog {
    message: String,
}

fn extract_body_from_message(message: &str) -> Option<String> {
    if let Some(body_start) = message.find("body=") {
        if let Some(peer_type_start) = message.find(" peer_type=") {
            let raw_body = &message[(body_start + 5)..peer_type_start].trim();
            if raw_body.starts_with('b') {
                let trimmed = raw_body.trim_start_matches('b').trim_matches('"');
                let unescaped = trimmed.replace("\\\"", "\"");
                return Some(unescaped);
            }
        }
    }
    None
}

#[tokio::main]
async fn main() {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();
    if let Err(e) = run().await {
        error!("Error: {}", e);
        process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error>> {
    let grafana_url = env::var("GRAFANA_LOGS_URL")
        .unwrap_or_else(|_| "https://your-grafana-endpoint/api/logs".to_string());
    let grafana_token =
        env::var("GRAFANA_API_TOKEN").unwrap_or_else(|_| "<YOUR_TOKEN>".to_string());
    let query = "Sampled read request";

    let client = reqwest::Client::new();
    let resp = client
        .get(&grafana_url)
        .header(ACCEPT, "application/json")
        .header(AUTHORIZATION, format!("Bearer {}", grafana_token))
        .query(&[("query", query)])
        .send()
        .await?;

    if !resp.status().is_success() {
        warn!("Request failed with status: {}", resp.status());
        return Ok(());
    }

    let logs: Vec<GrafanaLog> = resp.json().await?;
    info!("Found {} logs containing '{}'.", logs.len(), query);

    let mut method_map: HashMap<String, Vec<String>> = HashMap::new();

    for log_entry in logs {
        if let Some(body_content) = extract_body_from_message(&log_entry.message) {
            if let Ok(parsed) = serde_json::from_str::<Value>(&body_content) {
                let method = parsed
                    .get("method")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown_method")
                    .to_string();
                method_map.entry(method).or_default().push(body_content);
            }
        }
    }

    let file = File::create("sampled_read_requests.jsonl")?;
    let mut writer = BufWriter::new(file);

    for (method, bodies) in method_map {
        info!("Writing {} logs for method: {}", bodies.len(), method);
        for body in bodies {
            let line = format!(r#"{{"method":"{}", "body":{}}}"#, method, body);
            writer.write_all(line.as_bytes())?;
            writer.write_all(b"\n")?;
        }
    }

    writer.flush()?;
    info!("Done! Wrote grouped logs to sampled_read_requests.jsonl");
    Ok(())
}

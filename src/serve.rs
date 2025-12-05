//! HTTP server for interactive analysis mode
//!
//! `losselot serve ./folder` → starts server, opens browser, shows results

use crate::report::Summary;
use crate::{AnalysisResult, Analyzer};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use tiny_http::{Header, Method, Request, Response, Server};
use walkdir::WalkDir;

// Embed the UI directly in the binary
const UI_HTML: &str = include_str!("ui.html");

#[derive(Serialize)]
struct ApiResponse<T> {
    ok: bool,
    data: Option<T>,
    error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    fn success(data: T) -> Self {
        Self { ok: true, data: Some(data), error: None }
    }
}

#[derive(Deserialize, Debug)]
pub struct AnalyzeParams {
    pub path: String,
    #[serde(default = "default_threshold")]
    pub threshold: u32,
    #[serde(default = "default_suspect")]
    pub suspect_threshold: u32,
    #[serde(default)]
    pub skip_spectral: bool,
}

fn default_threshold() -> u32 { 65 }
fn default_suspect() -> u32 { 35 }

#[derive(Serialize)]
pub struct AnalysisReport {
    pub generated: String,
    pub summary: Summary,
    pub files: Vec<AnalysisResult>,
    pub params: AnalyzeParams,
}

// Make AnalyzeParams serializable for the response
impl Serialize for AnalyzeParams {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AnalyzeParams", 4)?;
        s.serialize_field("path", &self.path)?;
        s.serialize_field("threshold", &self.threshold)?;
        s.serialize_field("suspect_threshold", &self.suspect_threshold)?;
        s.serialize_field("skip_spectral", &self.skip_spectral)?;
        s.end()
    }
}

/// Start server, open browser, serve UI
pub fn start(port: u16, path: PathBuf) -> std::io::Result<()> {
    let addr = format!("127.0.0.1:{}", port);
    let server = Server::http(&addr).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
    })?;

    let url = format!("http://localhost:{}", port);
    let path_str = path.canonicalize().unwrap_or(path.clone()).display().to_string();

    eprintln!("\n\x1b[1;32m🎳 Losselot\x1b[0m");
    eprintln!("   {}", url);
    eprintln!("   Analyzing: {}\n", path_str);

    // Open browser
    let _ = open::that(&url);

    // Handle requests
    for request in server.incoming_requests() {
        if let Err(e) = handle_request(request, &path_str) {
            eprintln!("Error: {}", e);
        }
    }

    Ok(())
}

fn handle_request(mut request: Request, default_path: &str) -> std::io::Result<()> {
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("/");
    let method = request.method().clone();

    match (&method, path) {
        // Serve embedded UI
        (&Method::Get, "/") => {
            // Inject the default path into the HTML
            let html = UI_HTML.replace("{{DEFAULT_PATH}}", default_path);
            let response = Response::from_string(html)
                .with_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).unwrap());
            request.respond(response)
        }

        // API: Analyze
        (&Method::Get, "/api/analyze") | (&Method::Post, "/api/analyze") => {
            let params = parse_params(&mut request, default_path)?;
            eprintln!("→ {}", params.path);

            let report = run_analysis(&params);
            let json = serde_json::to_string(&ApiResponse::success(report))?;

            let response = Response::from_string(json)
                .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
            request.respond(response)
        }

        // 404
        _ => {
            let response = Response::from_string("Not found").with_status_code(404);
            request.respond(response)
        }
    }
}

fn parse_params(request: &mut Request, default_path: &str) -> std::io::Result<AnalyzeParams> {
    let url = request.url().to_string();

    // Try query string
    if let Some(query) = url.split('?').nth(1) {
        if let Ok(params) = serde_urlencoded::from_str::<AnalyzeParams>(query) {
            return Ok(params);
        }
    }

    // Try JSON body
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    if !body.is_empty() {
        if let Ok(params) = serde_json::from_str::<AnalyzeParams>(&body) {
            return Ok(params);
        }
    }

    // Fall back to default path
    Ok(AnalyzeParams {
        path: default_path.to_string(),
        threshold: default_threshold(),
        suspect_threshold: default_suspect(),
        skip_spectral: false,
    })
}

fn run_analysis(params: &AnalyzeParams) -> AnalysisReport {
    let path = PathBuf::from(&params.path);

    let supported: HashSet<&str> = [
        "flac", "wav", "wave", "aiff", "aif", "mp3", "m4a", "aac", "ogg", "opus", "wma", "alac",
    ].iter().cloned().collect();

    let files: Vec<PathBuf> = if path.is_dir() {
        WalkDir::new(&path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| supported.contains(ext.to_ascii_lowercase().as_str()))
                    .unwrap_or(false)
            })
            .map(|e| e.path().to_path_buf())
            .collect()
    } else if path.exists() {
        vec![path]
    } else {
        vec![]
    };

    let analyzer = Analyzer::new()
        .with_skip_spectral(params.skip_spectral)
        .with_thresholds(params.suspect_threshold, params.threshold);

    let results: Vec<AnalysisResult> = files.par_iter().map(|p| analyzer.analyze(p)).collect();
    let summary = Summary::from_results(&results);

    AnalysisReport {
        generated: chrono::Local::now().to_rfc3339(),
        summary,
        files: results,
        params: AnalyzeParams {
            path: params.path.clone(),
            threshold: params.threshold,
            suspect_threshold: params.suspect_threshold,
            skip_spectral: params.skip_spectral,
        },
    }
}

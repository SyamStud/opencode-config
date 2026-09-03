use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

#[derive(Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    provider: BTreeMap<String, serde_json::Value>,
}

#[derive(Serialize, Clone)]
struct ConfigInfo {
    path: String,
    wsl_path: String,
    config: serde_json::Value,
}

#[derive(Serialize, Clone)]
struct CandidateInfo {
    path: String,
    size: u64,
}

fn wsl_path(p: &str) -> String {
    format!("\\\\wsl.localhost\\Ubuntu{p}")
}

fn find_config() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let xdg = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".config"));
    let candidates = [
        xdg.join("opencode/opencode.jsonc"),
        xdg.join("opencode/opencode.json"),
        home.join(".config/opencode/opencode.jsonc"),
        home.join(".config/opencode/opencode.json"),
        PathBuf::from("/home/lenovo/.config/opencode/opencode.jsonc"),
        PathBuf::from("/home/lenovo/.config/opencode/opencode.json"),
    ];
    for p in candidates {
        if p.exists() {
            return p;
        }
    }
    xdg.join("opencode/opencode.jsonc")
}

fn strip_jsonc(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut in_str = false;
    let mut esc = false;
    let mut in_line = false;
    let mut in_block = false;
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if in_line {
            if c == '\n' {
                in_line = false;
                out.push(c);
            }
            continue;
        }
        if in_block {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block = false;
            }
            continue;
        }
        if in_str {
            out.push(c);
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                chars.next();
                in_line = true;
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                in_block = true;
            }
            _ => out.push(c),
        }
    }
    if in_str || in_block {
        None
    } else {
        Some(out)
    }
}

fn read_config(path: &PathBuf) -> Result<serde_json::Value, String> {
    if !path.exists() {
        return Ok(serde_json::json!({ "$schema": "https://opencode.ai/config.json", "provider": {} }));
    }
    let raw = std::fs::read_to_string(path).map_err(|e| format!("Read: {e}"))?;
    let clean = strip_jsonc(&raw).ok_or("JSONC invalid")?;
    serde_json::from_str(&clean).map_err(|e| format!("Parse: {e}"))
}

fn write_config(path: &PathBuf, config: &serde_json::Value) -> Result<(), String> {
    if config.get("provider").is_none() {
        return Err("provider required".into());
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| format!("Ser: {e}"))?;
    std::fs::write(path, format!("{json}\n")).map_err(|e| format!("Write: {e}"))
}

fn to_info(path: &PathBuf) -> Result<ConfigInfo, String> {
    let config = read_config(path)?;
    Ok(ConfigInfo {
        path: path.display().to_string(),
        wsl_path: wsl_path(&path.display().to_string()),
        config,
    })
}

fn search_candidates() -> Vec<CandidateInfo> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let xdg = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".config"));
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for p in [
        xdg.join("opencode/opencode.jsonc"),
        xdg.join("opencode/opencode.json"),
        home.join(".config/opencode/opencode.jsonc"),
        home.join(".config/opencode/opencode.json"),
        PathBuf::from("/home/lenovo/.config/opencode/opencode.jsonc"),
        PathBuf::from("/home/lenovo/.config/opencode/opencode.json"),
    ] {
        let s = p.display().to_string();
        if seen.contains(&s) {
            continue;
        }
        seen.push(s.clone());
        if p.exists() {
            let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            out.push(CandidateInfo { path: s, size });
        }
    }
    out
}

#[tauri::command(rename_all = "snake_case")]
fn cmd_load_config(state: tauri::State<Mutex<PathBuf>>) -> Result<ConfigInfo, String> {
    let path = state.lock().unwrap().clone();
    to_info(&path)
}

#[tauri::command(rename_all = "snake_case")]
fn cmd_save_config(state: tauri::State<Mutex<PathBuf>>, config: serde_json::Value) -> Result<(), String> {
    let path = state.lock().unwrap().clone();
    write_config(&path, &config)
}

#[tauri::command(rename_all = "snake_case")]
fn cmd_browse_config(state: tauri::State<Mutex<PathBuf>>) -> Result<Option<ConfigInfo>, String> {
    let picked = rfd::FileDialog::new()
        .add_filter("opencode config", &["jsonc", "json"])
        .pick_file();
    let Some(p) = picked else {
        return Ok(None);
    };
    let mut guard = state.lock().unwrap();
    *guard = p.clone();
    drop(guard);
    Ok(Some(to_info(&p)?))
}

#[tauri::command(rename_all = "snake_case")]
fn cmd_use_config(state: tauri::State<Mutex<PathBuf>>, path: String) -> Result<ConfigInfo, String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("File tidak ada".into());
    }
    let mut guard = state.lock().unwrap();
    *guard = p.clone();
    drop(guard);
    to_info(&p)
}

#[tauri::command(rename_all = "snake_case")]
fn cmd_search_configs() -> Vec<CandidateInfo> {
    search_candidates()
}

#[tauri::command(rename_all = "snake_case")]
async fn cmd_fetch_models(base_url: String, api_key: Option<String>) -> Result<Vec<String>, String> {
    if base_url.trim().is_empty() {
        return Err("baseURL kosong".into());
    }
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    tauri::async_runtime::spawn_blocking(move || {
        let mut req = ureq::Agent::new().get(&url);
        if let Some(k) = api_key {
            if !k.is_empty() {
                req = req.set("Authorization", &format!("Bearer {k}"));
            }
        }
        let body = req
            .call()
            .map_err(|e| format!("{e}"))?
            .into_string()
            .map_err(|e| format!("{e}"))?;
        let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("{e}"))?;
        let arr = if let Some(a) = v.as_array() {
            a.clone()
        } else if let Some(d) = v.get("data").and_then(|x| x.as_array()) {
            d.clone()
        } else if let Some(m) = v.get("models").and_then(|x| x.as_array()) {
            m.clone()
        } else {
            return Err("No models array".into());
        };
        Ok(arr
            .into_iter()
            .filter_map(|m| {
                if let Some(s) = m.as_str() {
                    Some(s.to_string())
                } else if let Some(id) = m.get("id").and_then(|x| x.as_str()) {
                    Some(id.to_string())
                } else {
                    None
                }
            })
            .collect())
    })
    .await
    .map_err(|e| format!("{e}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config_path = find_config();
    tauri::Builder::default()
        .manage(Mutex::new(config_path))
        .setup(|app| {
            if let Some(win) = app.get_webview_window("main") {
                let bytes = include_bytes!("../icons/128x128.png");
                if let Ok(img) = image::load_from_memory(bytes) {
                    let rgba = img.to_rgba8();
                    let (w, h) = (rgba.width(), rgba.height());
                    let icon = tauri::image::Image::new_owned(rgba.into_raw(), w, h);
                    let _ = win.set_icon(icon);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cmd_load_config,
            cmd_save_config,
            cmd_browse_config,
            cmd_use_config,
            cmd_search_configs,
            cmd_fetch_models
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
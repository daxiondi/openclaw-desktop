#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{Emitter, Manager};

const OFFICIAL_WEB_URL: &str = "http://127.0.0.1:18789/";
const BOOTSTRAP_LOG_EVENT: &str = "bootstrap-log";
const CLAUDE_KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const DEFAULT_OPENCLAW_AGENT_ID: &str = "main";
const OPENAI_CODEX_DEFAULT_MODEL: &str = "openai-codex/gpt-5.3-codex";

const FALLBACK_OAUTH_PROVIDERS: &[&str] = &[
    "openai-codex",
    "anthropic",
    "github-copilot",
    "chutes",
    "google-antigravity",
    "google-gemini-cli",
    "minimax-portal",
    "qwen-portal",
    "copilot-proxy",
];

const OPENCLAW_AUTH_CHOICE_NON_PROVIDER: &[&str] = &[
    "skip",
    "token",
    "apiKey",
    "setup-token",
    "oauth",
    "claude-cli",
    "codex-cli",
    "minimax-cloud",
    "minimax",
];

const OPENCLAW_BIN_CANDIDATES: &[&str] = &[
    "openclaw",
    "/opt/homebrew/bin/openclaw",
    "/usr/local/bin/openclaw",
    "/usr/bin/openclaw",
    "C:\\Program Files\\OpenClaw\\openclaw.exe",
];

const OPENCLAW_INSTALL_SH: &str =
    "curl -fsSL --proto '=https' --tlsv1.2 https://openclaw.ai/install.sh | \
     bash -s -- --install-method npm --no-prompt --no-onboard";
const OPENCLAW_INSTALL_PS1: &str =
    "& ([scriptblock]::Create((iwr -useb https://openclaw.ai/install.ps1))) -NoOnboard";

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LoginResult {
    provider_id: String,
    launched: bool,
    command_hint: String,
    details: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OllamaStatus {
    endpoint: String,
    reachable: bool,
    models: Vec<String>,
    error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CodexAuthStatus {
    detected: bool,
    source: String,
    last_refresh: Option<String>,
    token_fields: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CodexConnectivityStatus {
    ok: bool,
    expected: String,
    response: Option<String>,
    error: Option<String>,
    command: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OfficialWebStatus {
    ready: bool,
    installed: bool,
    running: bool,
    started: bool,
    url: String,
    command_hint: String,
    message: String,
    error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OpenOfficialWebResult {
    opened: bool,
    url: String,
    detail: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BootstrapStatus {
    ready: bool,
    installed: bool,
    initialized: bool,
    web: OfficialWebStatus,
    message: String,
    logs: Vec<String>,
    error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LocalOAuthToolStatus {
    id: String,
    label: String,
    provider_id: String,
    cli_found: bool,
    auth_detected: bool,
    source: String,
    detail: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LocalCodexReuseResult {
    reused: bool,
    profile_id: Option<String>,
    model: Option<String>,
    message: String,
    error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BrowserDetectedExecutable {
    kind: String,
    path: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BrowserModeStatus {
    mode: String,
    default_profile: String,
    executable_path: Option<String>,
    detected_browsers: Vec<BrowserDetectedExecutable>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BrowserRelayStatus {
    installed: bool,
    path: Option<String>,
    command_hint: String,
    message: String,
    error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BrowserRelayDiagnostic {
    relay_url: String,
    relay_reachable: bool,
    extension_connected: Option<bool>,
    tabs_count: usize,
    likely_cause: String,
    detail: String,
    command_hint: String,
}

#[derive(Deserialize)]
struct LocalCodexAuthFile {
    tokens: Option<LocalCodexAuthTokens>,
}

#[derive(Deserialize)]
struct LocalCodexAuthTokens {
    access_token: Option<String>,
    refresh_token: Option<String>,
    account_id: Option<String>,
    id_token: Option<String>,
}

#[derive(Deserialize)]
struct ModelsStatusJson {
    auth: Option<ModelsStatusAuth>,
}

#[derive(Deserialize)]
struct ModelsStatusAuth {
    #[serde(rename = "providersWithOAuth")]
    providers_with_oauth: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Option<Vec<OllamaModel>>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: Option<String>,
}

fn resolve_codex_auth_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".codex").join("auth.json");
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        return PathBuf::from(profile).join(".codex").join("auth.json");
    }
    PathBuf::from(".codex/auth.json")
}

fn resolve_user_home() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return Some(PathBuf::from(home));
        }
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        if !profile.trim().is_empty() {
            return Some(PathBuf::from(profile));
        }
    }
    None
}

fn read_env_path(name: &str) -> Option<PathBuf> {
    let value = std::env::var(name).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

fn resolve_openclaw_state_dir() -> PathBuf {
    if let Some(state_dir) = read_env_path("OPENCLAW_STATE_DIR") {
        return state_dir;
    }
    if let Some(home) = resolve_user_home() {
        return home.join(".openclaw");
    }
    PathBuf::from(".openclaw")
}

fn resolve_openclaw_config_path() -> PathBuf {
    if let Some(config_path) = read_env_path("OPENCLAW_CONFIG_PATH") {
        return config_path;
    }
    resolve_openclaw_state_dir().join("openclaw.json")
}

fn load_openclaw_config_value() -> serde_json::Value {
    let config_path = resolve_openclaw_config_path();
    if !config_path.exists() {
        return serde_json::json!({});
    }

    let content = fs::read_to_string(&config_path).unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(&content)
        .or_else(|_| json5::from_str::<serde_json::Value>(&content))
        .unwrap_or_else(|_| serde_json::json!({}))
}

fn save_openclaw_config_value(value: &serde_json::Value) -> Result<(), String> {
    let config_path = resolve_openclaw_config_path();
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "Failed to create config dir {}: {}",
                parent.to_string_lossy(),
                err
            )
        })?;
    }

    fs::write(
        &config_path,
        serde_json::to_string_pretty(value)
            .map_err(|err| format!("Failed to serialize OpenClaw config: {}", err))?,
    )
    .map_err(|err| format!("Failed to write {}: {}", config_path.to_string_lossy(), err))
}

fn resolve_openclaw_agent_dir() -> PathBuf {
    if let Some(agent_dir) = read_env_path("OPENCLAW_AGENT_DIR") {
        return agent_dir;
    }
    if let Some(agent_dir) = read_env_path("PI_CODING_AGENT_DIR") {
        return agent_dir;
    }
    resolve_openclaw_state_dir()
        .join("agents")
        .join(DEFAULT_OPENCLAW_AGENT_ID)
        .join("agent")
}

fn resolve_openclaw_auth_profiles_path() -> PathBuf {
    resolve_openclaw_agent_dir().join("auth-profiles.json")
}

fn decode_jwt_payload(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?.trim();
    if payload.is_empty() {
        return None;
    }

    let decoded = URL_SAFE_NO_PAD
        .decode(payload.as_bytes())
        .or_else(|_| URL_SAFE.decode(payload.as_bytes()))
        .ok()?;
    serde_json::from_slice::<serde_json::Value>(&decoded).ok()
}

fn jwt_exp_millis(token: &str) -> Option<i64> {
    let payload = decode_jwt_payload(token)?;
    let exp = payload.get("exp").and_then(|v| v.as_i64())?;
    Some(if exp > 10_000_000_000 {
        exp
    } else {
        exp.saturating_mul(1000)
    })
}

fn jwt_email(token: &str) -> Option<String> {
    let payload = decode_jwt_payload(token)?;
    let email = payload
        .pointer("/https://api.openai.com/profile/email")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("email").and_then(|v| v.as_str()))?
        .trim()
        .to_string();
    if email.is_empty() {
        None
    } else {
        Some(email)
    }
}

fn jwt_openai_account_id(token: &str) -> Option<String> {
    let payload = decode_jwt_payload(token)?;
    let account_id = payload
        .pointer("/https://api.openai.com/auth/chatgpt_account_id")
        .and_then(|v| v.as_str())
        .or_else(|| {
            payload
                .pointer("/https://api.openai.com/auth/account_id")
                .and_then(|v| v.as_str())
        })?
        .trim()
        .to_string();
    if account_id.is_empty() {
        None
    } else {
        Some(account_id)
    }
}

fn sync_local_codex_auth_to_openclaw(set_default_model: bool) -> Result<LocalCodexReuseResult, String> {
    let codex_auth_path = resolve_codex_auth_path();
    let raw = fs::read_to_string(&codex_auth_path)
        .map_err(|err| format!("Failed to read {}: {}", codex_auth_path.to_string_lossy(), err))?;
    let parsed = serde_json::from_str::<LocalCodexAuthFile>(&raw)
        .map_err(|err| format!("Invalid Codex auth file format: {}", err))?;
    let tokens = parsed
        .tokens
        .ok_or_else(|| "Codex auth tokens field is missing.".to_string())?;

    let access_token = tokens.access_token.unwrap_or_default().trim().to_string();
    let refresh_token = tokens.refresh_token.unwrap_or_default().trim().to_string();
    if access_token.is_empty() || refresh_token.is_empty() {
        return Err("Codex auth file is missing access_token or refresh_token.".to_string());
    }

    let account_id = tokens
        .account_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| jwt_openai_account_id(&access_token));
    let expires = jwt_exp_millis(&access_token)
        .or_else(|| tokens.id_token.as_deref().and_then(jwt_exp_millis))
        .unwrap_or_else(|| {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            now_ms + 60 * 60 * 1000
        });
    let email = jwt_email(&access_token).or_else(|| tokens.id_token.as_deref().and_then(jwt_email));
    let profile_id = email
        .as_ref()
        .map(|mail| format!("openai-codex:{}", mail))
        .unwrap_or_else(|| "openai-codex:default".to_string());

    let auth_profiles_path = resolve_openclaw_auth_profiles_path();
    if let Some(parent) = auth_profiles_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "Failed to create auth profile dir {}: {}",
                parent.to_string_lossy(),
                err
            )
        })?;
    }

    let mut auth_profiles_value = if auth_profiles_path.exists() {
        let content = fs::read_to_string(&auth_profiles_path).unwrap_or_default();
        serde_json::from_str::<serde_json::Value>(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if !auth_profiles_value.is_object() {
        auth_profiles_value = serde_json::json!({});
    }
    let auth_profiles_obj = auth_profiles_value
        .as_object_mut()
        .ok_or_else(|| "Failed to parse auth-profiles root object.".to_string())?;
    auth_profiles_obj.insert("version".to_string(), serde_json::json!(1));
    let profiles_entry = auth_profiles_obj
        .entry("profiles".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !profiles_entry.is_object() {
        *profiles_entry = serde_json::json!({});
    }
    let profiles_obj = profiles_entry
        .as_object_mut()
        .ok_or_else(|| "Failed to parse auth-profiles profiles object.".to_string())?;

    let mut credential = serde_json::Map::new();
    credential.insert("type".to_string(), serde_json::json!("oauth"));
    credential.insert("provider".to_string(), serde_json::json!("openai-codex"));
    credential.insert("access".to_string(), serde_json::json!(access_token));
    credential.insert("refresh".to_string(), serde_json::json!(refresh_token));
    credential.insert("expires".to_string(), serde_json::json!(expires));
    if let Some(value) = &account_id {
        credential.insert("accountId".to_string(), serde_json::json!(value));
    }
    if let Some(value) = &email {
        credential.insert("email".to_string(), serde_json::json!(value));
    }
    profiles_obj.insert(profile_id.clone(), serde_json::Value::Object(credential));

    fs::write(
        &auth_profiles_path,
        serde_json::to_string_pretty(&auth_profiles_value)
            .map_err(|err| format!("Failed to serialize auth-profiles: {}", err))?,
    )
    .map_err(|err| {
        format!(
            "Failed to write {}: {}",
            auth_profiles_path.to_string_lossy(),
            err
        )
    })?;

    let config_path = resolve_openclaw_config_path();
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "Failed to create config dir {}: {}",
                parent.to_string_lossy(),
                err
            )
        })?;
    }

    let mut config_value = if config_path.exists() {
        let content = fs::read_to_string(&config_path).unwrap_or_default();
        serde_json::from_str::<serde_json::Value>(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    if !config_value.is_object() {
        config_value = serde_json::json!({});
    }

    let config_obj = config_value
        .as_object_mut()
        .ok_or_else(|| "Failed to parse config root object.".to_string())?;
    let auth_entry = config_obj
        .entry("auth".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !auth_entry.is_object() {
        *auth_entry = serde_json::json!({});
    }
    let auth_obj = auth_entry
        .as_object_mut()
        .ok_or_else(|| "Failed to parse config auth object.".to_string())?;

    let cfg_profiles = auth_obj
        .entry("profiles".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !cfg_profiles.is_object() {
        *cfg_profiles = serde_json::json!({});
    }
    let cfg_profiles_obj = cfg_profiles
        .as_object_mut()
        .ok_or_else(|| "Failed to parse config auth.profiles object.".to_string())?;
    let mut profile_meta = serde_json::Map::new();
    profile_meta.insert("provider".to_string(), serde_json::json!("openai-codex"));
    profile_meta.insert("mode".to_string(), serde_json::json!("oauth"));
    if let Some(value) = &email {
        profile_meta.insert("email".to_string(), serde_json::json!(value));
    }
    cfg_profiles_obj.insert(profile_id.clone(), serde_json::Value::Object(profile_meta));

    let order_entry = auth_obj
        .entry("order".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !order_entry.is_object() {
        *order_entry = serde_json::json!({});
    }
    let order_obj = order_entry
        .as_object_mut()
        .ok_or_else(|| "Failed to parse config auth.order object.".to_string())?;
    let mut next_order = vec![profile_id.clone()];
    if let Some(existing) = order_obj.get("openai-codex").and_then(|v| v.as_array()) {
        for item in existing {
            if let Some(id) = item.as_str() {
                let trimmed = id.trim();
                if !trimmed.is_empty() && !next_order.iter().any(|current| current == trimmed) {
                    next_order.push(trimmed.to_string());
                }
            }
        }
    }
    order_obj.insert("openai-codex".to_string(), serde_json::json!(next_order));

    let mut selected_model: Option<String> = None;
    if set_default_model {
        let agents_entry = config_obj
            .entry("agents".to_string())
            .or_insert_with(|| serde_json::json!({}));
        if !agents_entry.is_object() {
            *agents_entry = serde_json::json!({});
        }
        let agents_obj = agents_entry
            .as_object_mut()
            .ok_or_else(|| "Failed to parse config agents object.".to_string())?;
        let defaults_entry = agents_obj
            .entry("defaults".to_string())
            .or_insert_with(|| serde_json::json!({}));
        if !defaults_entry.is_object() {
            *defaults_entry = serde_json::json!({});
        }
        let defaults_obj = defaults_entry
            .as_object_mut()
            .ok_or_else(|| "Failed to parse config agents.defaults object.".to_string())?;

        let current_primary = match defaults_obj.get("model") {
            Some(serde_json::Value::String(model)) => model.trim().to_string(),
            Some(serde_json::Value::Object(model_obj)) => model_obj
                .get("primary")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string(),
            _ => String::new(),
        };
        let should_override = current_primary.is_empty()
            || current_primary.starts_with("anthropic/")
            || current_primary.starts_with("openai/");

        if should_override {
            let model_entry = defaults_obj
                .entry("model".to_string())
                .or_insert_with(|| serde_json::json!({}));
            match model_entry {
                serde_json::Value::Object(model_obj) => {
                    model_obj.insert(
                        "primary".to_string(),
                        serde_json::json!(OPENAI_CODEX_DEFAULT_MODEL),
                    );
                }
                _ => {
                    *model_entry = serde_json::json!({
                        "primary": OPENAI_CODEX_DEFAULT_MODEL
                    });
                }
            }
            selected_model = Some(OPENAI_CODEX_DEFAULT_MODEL.to_string());
        } else if !current_primary.is_empty() {
            selected_model = Some(current_primary);
        }
    }

    fs::write(
        &config_path,
        serde_json::to_string_pretty(&config_value)
            .map_err(|err| format!("Failed to serialize config: {}", err))?,
    )
    .map_err(|err| format!("Failed to write {}: {}", config_path.to_string_lossy(), err))?;

    Ok(LocalCodexReuseResult {
        reused: true,
        profile_id: Some(profile_id),
        model: selected_model,
        message: "Local Codex auth has been synced into OpenClaw.".to_string(),
        error: None,
    })
}

fn read_gateway_auth_token() -> Option<String> {
    if let Ok(token) = std::env::var("OPENCLAW_GATEWAY_TOKEN") {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let config_path = resolve_openclaw_config_path();
    let raw = fs::read_to_string(config_path).ok()?;
    let parsed = serde_json::from_str::<serde_json::Value>(&raw)
        .or_else(|_| json5::from_str::<serde_json::Value>(&raw))
        .ok()?;
    let token = parsed
        .pointer("/gateway/auth/token")
        .or_else(|| parsed.pointer("/gateway/token"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");

    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn percent_encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{:02X}", byte));
        }
    }
    encoded
}

fn resolve_official_dashboard_url() -> String {
    if let Some(token) = read_gateway_auth_token() {
        return format!("{}#token={}", OFFICIAL_WEB_URL, percent_encode_component(&token));
    }
    OFFICIAL_WEB_URL.to_string()
}

fn resolve_claude_credentials_path() -> PathBuf {
    if let Some(home) = resolve_user_home() {
        return home.join(".claude").join(".credentials.json");
    }
    PathBuf::from(".claude/.credentials.json")
}

fn command_exists(binary: &str, args: &[&str]) -> bool {
    match Command::new(binary).args(args).output() {
        Ok(output) => {
            if output.status.success() {
                return true;
            }
            let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
            let stdout = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
            !(stderr.contains("permission denied")
                || stderr.contains("is a directory")
                || stdout.contains("permission denied")
                || stdout.contains("is a directory"))
        }
        Err(_) => false,
    }
}

#[derive(Clone)]
struct BrowserExecutableCandidate {
    kind: &'static str,
    path: PathBuf,
}

fn path_is_file(path: &Path) -> bool {
    fs::metadata(path).map(|meta| meta.is_file()).unwrap_or(false)
}

fn normalize_path_key(path: &Path) -> String {
    let text = path.to_string_lossy().to_string();
    if cfg!(target_os = "windows") {
        text.to_ascii_lowercase()
    } else {
        text
    }
}

fn resolve_binary_in_path(binary: &str) -> Option<PathBuf> {
    let finder = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    let output = Command::new(finder).arg(binary).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .find(|path| path_is_file(path))
}

fn detect_local_browser_candidates() -> Vec<BrowserExecutableCandidate> {
    let mut found = Vec::new();
    let mut seen = BTreeSet::new();

    let mut push_candidate = |kind: &'static str, path: PathBuf| {
        if !path_is_file(&path) {
            return;
        }
        let key = normalize_path_key(&path);
        if seen.insert(key) {
            found.push(BrowserExecutableCandidate { kind, path });
        }
    };

    if cfg!(target_os = "macos") {
        let mut app_roots = vec![PathBuf::from("/Applications")];
        if let Some(home) = resolve_user_home() {
            app_roots.push(home.join("Applications"));
        }
        for root in app_roots {
            push_candidate(
                "chrome",
                root.join("Google Chrome.app")
                    .join("Contents")
                    .join("MacOS")
                    .join("Google Chrome"),
            );
            push_candidate(
                "brave",
                root.join("Brave Browser.app")
                    .join("Contents")
                    .join("MacOS")
                    .join("Brave Browser"),
            );
            push_candidate(
                "edge",
                root.join("Microsoft Edge.app")
                    .join("Contents")
                    .join("MacOS")
                    .join("Microsoft Edge"),
            );
            push_candidate(
                "chromium",
                root.join("Chromium.app")
                    .join("Contents")
                    .join("MacOS")
                    .join("Chromium"),
            );
            push_candidate(
                "canary",
                root.join("Google Chrome Canary.app")
                    .join("Contents")
                    .join("MacOS")
                    .join("Google Chrome Canary"),
            );
        }
    } else if cfg!(target_os = "windows") {
        let mut roots = Vec::new();
        if let Ok(v) = std::env::var("PROGRAMFILES") {
            roots.push(PathBuf::from(v));
        }
        if let Ok(v) = std::env::var("ProgramFiles") {
            roots.push(PathBuf::from(v));
        }
        if let Ok(v) = std::env::var("PROGRAMFILES(X86)") {
            roots.push(PathBuf::from(v));
        }
        if let Ok(v) = std::env::var("LOCALAPPDATA") {
            roots.push(PathBuf::from(v));
        }

        for root in roots {
            push_candidate(
                "chrome",
                root.join("Google")
                    .join("Chrome")
                    .join("Application")
                    .join("chrome.exe"),
            );
            push_candidate(
                "brave",
                root.join("BraveSoftware")
                    .join("Brave-Browser")
                    .join("Application")
                    .join("brave.exe"),
            );
            push_candidate(
                "edge",
                root.join("Microsoft")
                    .join("Edge")
                    .join("Application")
                    .join("msedge.exe"),
            );
            push_candidate(
                "chromium",
                root.join("Chromium")
                    .join("Application")
                    .join("chrome.exe"),
            );
            push_candidate(
                "canary",
                root.join("Google")
                    .join("Chrome SxS")
                    .join("Application")
                    .join("chrome.exe"),
            );
        }
    } else {
        for (kind, cmd) in [
            ("chrome", "google-chrome"),
            ("chrome", "google-chrome-stable"),
            ("brave", "brave-browser"),
            ("edge", "microsoft-edge"),
            ("chromium", "chromium"),
            ("chromium", "chromium-browser"),
        ] {
            if let Some(path) = resolve_binary_in_path(cmd) {
                push_candidate(kind, path);
            }
        }

        for (kind, path) in [
            ("chrome", "/usr/bin/google-chrome"),
            ("chrome", "/usr/bin/google-chrome-stable"),
            ("brave", "/usr/bin/brave-browser"),
            ("edge", "/usr/bin/microsoft-edge"),
            ("chromium", "/usr/bin/chromium"),
            ("chromium", "/usr/bin/chromium-browser"),
        ] {
            push_candidate(kind, PathBuf::from(path));
        }
    }

    for (kind, cmd) in [
        ("chrome", "chrome"),
        ("chrome", "google-chrome"),
        ("brave", "brave"),
        ("brave", "brave-browser"),
        ("edge", "msedge"),
        ("edge", "microsoft-edge"),
        ("chromium", "chromium"),
        ("chromium", "chromium-browser"),
    ] {
        if let Some(path) = resolve_binary_in_path(cmd) {
            push_candidate(kind, path);
        }
    }

    found
}

fn ensure_browser_defaults(
    app: &tauri::AppHandle,
    logs: &mut Vec<String>,
) -> Result<(), String> {
    let config_path = resolve_openclaw_config_path();
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "Failed to create config dir {}: {}",
                parent.to_string_lossy(),
                err
            )
        })?;
    }

    let mut config_value = if config_path.exists() {
        let content = fs::read_to_string(&config_path).unwrap_or_default();
        serde_json::from_str::<serde_json::Value>(&content)
            .or_else(|_| json5::from_str::<serde_json::Value>(&content))
            .unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    if !config_value.is_object() {
        config_value = serde_json::json!({});
    }

    let config_obj = config_value
        .as_object_mut()
        .ok_or_else(|| "Failed to parse OpenClaw config root object.".to_string())?;
    let browser_entry = config_obj
        .entry("browser".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !browser_entry.is_object() {
        *browser_entry = serde_json::json!({});
    }
    let browser_obj = browser_entry
        .as_object_mut()
        .ok_or_else(|| "Failed to parse OpenClaw config browser object.".to_string())?;

    let current_executable = browser_obj
        .get("executablePath")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let current_profile = browser_obj
        .get("defaultProfile")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let mut changed = false;
    let candidates = detect_local_browser_candidates();
    if candidates.is_empty() {
        push_bootstrap_log(
            app,
            logs,
            "Browser detection: no local Chromium-based browser found.",
        );
    } else {
        let summary = candidates
            .iter()
            .take(3)
            .map(|item| format!("{} ({})", item.kind, item.path.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(", ");
        push_bootstrap_log(
            app,
            logs,
            format!("Browser detection: found {}", summary),
        );
    }

    if browser_obj
        .get("enabled")
        .and_then(|value| value.as_bool())
        .is_none()
    {
        browser_obj.insert("enabled".to_string(), serde_json::json!(true));
        changed = true;
    }

    if current_profile.is_none() {
        browser_obj.insert("defaultProfile".to_string(), serde_json::json!("openclaw"));
        push_bootstrap_log(
            app,
            logs,
            "Browser config: set browser.defaultProfile=openclaw",
        );
        changed = true;
    }

    if current_executable.is_none() {
        if let Some(chosen) = candidates.first() {
            browser_obj.insert(
                "executablePath".to_string(),
                serde_json::json!(chosen.path.to_string_lossy().to_string()),
            );
            push_bootstrap_log(
                app,
                logs,
                format!(
                    "Browser config: set browser.executablePath={} ({})",
                    chosen.path.to_string_lossy(),
                    chosen.kind
                ),
            );
            changed = true;
        } else {
            push_bootstrap_log(
                app,
                logs,
                "Browser config: keep browser.executablePath unset (auto detection in OpenClaw runtime).",
            );
        }
    } else if let Some(path) = current_executable {
        push_bootstrap_log(
            app,
            logs,
            format!("Browser config: existing browser.executablePath={}", path),
        );
    }

    if changed {
        fs::write(
            &config_path,
            serde_json::to_string_pretty(&config_value)
                .map_err(|err| format!("Failed to serialize OpenClaw config: {}", err))?,
        )
        .map_err(|err| format!("Failed to write {}: {}", config_path.to_string_lossy(), err))?;
        push_bootstrap_log(app, logs, "Browser config defaults ensured.");
    } else {
        push_bootstrap_log(app, logs, "Browser config already initialized; no changes.");
    }

    Ok(())
}

fn browser_mode_status_from_config(config_value: &serde_json::Value) -> BrowserModeStatus {
    let default_profile = config_value
        .pointer("/browser/defaultProfile")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("openclaw")
        .to_string();

    let mode = if default_profile.eq_ignore_ascii_case("chrome") {
        "chrome".to_string()
    } else {
        "openclaw".to_string()
    };

    let executable_path = config_value
        .pointer("/browser/executablePath")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let detected_browsers = detect_local_browser_candidates()
        .into_iter()
        .map(|candidate| BrowserDetectedExecutable {
            kind: candidate.kind.to_string(),
            path: candidate.path.to_string_lossy().to_string(),
        })
        .collect::<Vec<_>>();

    BrowserModeStatus {
        mode,
        default_profile,
        executable_path,
        detected_browsers,
    }
}

#[tauri::command]
fn get_browser_mode_status() -> Result<BrowserModeStatus, String> {
    let config_value = load_openclaw_config_value();
    Ok(browser_mode_status_from_config(&config_value))
}

#[tauri::command]
fn set_browser_mode(mode: String) -> Result<BrowserModeStatus, String> {
    let normalized_mode = mode.trim().to_ascii_lowercase();
    let target_profile = match normalized_mode.as_str() {
        "openclaw" => "openclaw",
        "chrome" => "chrome",
        _ => return Err("Unsupported browser mode. Use 'openclaw' or 'chrome'.".to_string()),
    };

    let mut config_value = load_openclaw_config_value();
    if !config_value.is_object() {
        config_value = serde_json::json!({});
    }

    let config_obj = config_value
        .as_object_mut()
        .ok_or_else(|| "Failed to parse OpenClaw config root object.".to_string())?;
    let browser_entry = config_obj
        .entry("browser".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !browser_entry.is_object() {
        *browser_entry = serde_json::json!({});
    }

    let browser_obj = browser_entry
        .as_object_mut()
        .ok_or_else(|| "Failed to parse OpenClaw config browser object.".to_string())?;
    browser_obj.insert(
        "defaultProfile".to_string(),
        serde_json::json!(target_profile),
    );
    if browser_obj
        .get("enabled")
        .and_then(|value| value.as_bool())
        .is_none()
    {
        browser_obj.insert("enabled".to_string(), serde_json::json!(true));
    }

    save_openclaw_config_value(&config_value)?;
    Ok(browser_mode_status_from_config(&config_value))
}

fn extract_browser_relay_path(output: &str) -> Option<String> {
    output.lines().map(str::trim).find_map(|line| {
        if line.is_empty() {
            return None;
        }
        if line.starts_with("Docs:") || line.starts_with("Next:") || line.starts_with("- ") {
            return None;
        }
        if line.eq_ignore_ascii_case("Copied to clipboard.") {
            return None;
        }

        let lower = line.to_ascii_lowercase();
        if lower.contains("chrome extension is not installed") {
            return None;
        }
        Some(line.to_string())
    })
}

fn browser_relay_status_with_binary(binary: &str) -> BrowserRelayStatus {
    let command_hint = "openclaw browser extension install".to_string();
    match run_command(binary, &["browser", "extension", "path"]) {
        Ok((true, output)) => {
            let path = extract_browser_relay_path(&output);
            if path.is_some() {
                BrowserRelayStatus {
                    installed: true,
                    path,
                    command_hint,
                    message: "Browser relay extension is ready.".to_string(),
                    error: None,
                }
            } else {
                BrowserRelayStatus {
                    installed: false,
                    path: None,
                    command_hint,
                    message: "Relay path is unavailable.".to_string(),
                    error: if output.trim().is_empty() {
                        None
                    } else {
                        Some(output)
                    },
                }
            }
        }
        Ok((false, output)) => BrowserRelayStatus {
            installed: false,
            path: None,
            command_hint,
            message: "Browser relay extension is not installed.".to_string(),
            error: if output.trim().is_empty() {
                None
            } else {
                Some(output)
            },
        },
        Err(error) => BrowserRelayStatus {
            installed: false,
            path: None,
            command_hint,
            message: "Failed to check browser relay extension.".to_string(),
            error: Some(error),
        },
    }
}

fn ensure_browser_relay_installed(app: &tauri::AppHandle, binary: &str, logs: &mut Vec<String>) {
    push_bootstrap_log(
        app,
        logs,
        "Ensuring browser relay extension assets are prepared...",
    );

    match run_command(binary, &["browser", "extension", "install"]) {
        Ok((true, output)) => {
            let path = extract_browser_relay_path(&output)
                .or_else(|| browser_relay_status_with_binary(binary).path);
            if let Some(path) = path {
                push_bootstrap_log(
                    app,
                    logs,
                    format!("Browser relay extension ready at {}", path),
                );
            } else {
                push_bootstrap_log(
                    app,
                    logs,
                    "Browser relay extension install command completed.",
                );
            }
        }
        Ok((false, output)) => {
            let detail = if output.trim().is_empty() {
                "no output".to_string()
            } else {
                output
            };
            push_bootstrap_log(
                app,
                logs,
                format!("WARN: failed to prepare browser relay extension: {}", detail),
            );
        }
        Err(error) => {
            push_bootstrap_log(
                app,
                logs,
                format!(
                    "WARN: failed to run browser relay extension install command: {}",
                    error
                ),
            );
        }
    }
}

#[tauri::command]
fn get_browser_relay_status() -> BrowserRelayStatus {
    let command_hint = "openclaw browser extension install".to_string();
    let Some(binary) = resolve_openclaw_binary() else {
        return BrowserRelayStatus {
            installed: false,
            path: None,
            command_hint,
            message: "openclaw binary not found.".to_string(),
            error: Some("Install OpenClaw first, then retry.".to_string()),
        };
    };
    browser_relay_status_with_binary(&binary)
}

#[tauri::command]
fn prepare_browser_relay() -> BrowserRelayStatus {
    let command_hint = "openclaw browser extension install".to_string();
    let Some(binary) = resolve_openclaw_binary() else {
        return BrowserRelayStatus {
            installed: false,
            path: None,
            command_hint,
            message: "openclaw binary not found.".to_string(),
            error: Some("Install OpenClaw first, then retry.".to_string()),
        };
    };

    match run_command(&binary, &["browser", "extension", "install"]) {
        Ok((true, output)) => {
            let mut status = browser_relay_status_with_binary(&binary);
            if status.installed {
                status.message = "Browser relay extension prepared.".to_string();
            } else {
                status.message =
                    "Install command finished, but relay extension path is still unavailable."
                        .to_string();
                if status.error.is_none() && !output.trim().is_empty() {
                    status.error = Some(output);
                }
            }
            status
        }
        Ok((false, output)) => BrowserRelayStatus {
            installed: false,
            path: None,
            command_hint,
            message: "Failed to prepare browser relay extension.".to_string(),
            error: if output.trim().is_empty() {
                Some("openclaw browser extension install failed".to_string())
            } else {
                Some(output)
            },
        },
        Err(error) => BrowserRelayStatus {
            installed: false,
            path: None,
            command_hint,
            message: "Failed to prepare browser relay extension.".to_string(),
            error: Some(error),
        },
    }
}

#[derive(Deserialize)]
struct BrowserExtensionStatusResponse {
    connected: bool,
}

fn resolve_browser_relay_url_from_config(config_value: &serde_json::Value) -> String {
    let from_profile = config_value
        .pointer("/browser/profiles/chrome/cdpUrl")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let from_port = config_value
        .pointer("/browser/profiles/chrome/cdpPort")
        .and_then(|value| value.as_i64())
        .filter(|port| *port > 0 && *port <= 65535)
        .map(|port| format!("http://127.0.0.1:{}", port));

    from_profile
        .or(from_port)
        .unwrap_or_else(|| "http://127.0.0.1:18792".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn parse_browser_tabs_count(output: &str) -> Option<usize> {
    let parsed = serde_json::from_str::<serde_json::Value>(output).ok()?;
    let tabs = parsed.get("tabs")?.as_array()?;
    Some(tabs.len())
}

#[tauri::command]
async fn diagnose_browser_relay() -> BrowserRelayDiagnostic {
    let command_hint = "openclaw browser --browser-profile chrome tabs --json".to_string();
    let config_value = load_openclaw_config_value();
    let relay_url = resolve_browser_relay_url_from_config(&config_value);
    let Some(binary) = resolve_openclaw_binary() else {
        return BrowserRelayDiagnostic {
            relay_url,
            relay_reachable: false,
            extension_connected: None,
            tabs_count: 0,
            likely_cause: "openclaw CLI 未安装".to_string(),
            detail: "未检测到 openclaw 可执行文件，无法诊断浏览器中继。".to_string(),
            command_hint,
        };
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build();
    let mut relay_reachable = false;
    let mut extension_connected: Option<bool> = None;
    let mut detail_parts: Vec<String> = Vec::new();

    match client {
        Ok(http) => {
            let probe = http
                .head(format!("{}/", relay_url))
                .send()
                .await
                .map(|response| response.status().is_success())
                .unwrap_or(false);
            relay_reachable = probe;

            if relay_reachable {
                match http
                    .get(format!("{}/extension/status", relay_url))
                    .send()
                    .await
                {
                    Ok(response) => {
                        if response.status().is_success() {
                            match response.json::<BrowserExtensionStatusResponse>().await {
                                Ok(parsed) => {
                                    extension_connected = Some(parsed.connected);
                                }
                                Err(error) => {
                                    detail_parts.push(format!(
                                        "无法解析 extension/status 响应: {}",
                                        error
                                    ));
                                }
                            }
                        } else {
                            detail_parts.push(format!(
                                "extension/status 响应异常: HTTP {}",
                                response.status().as_u16()
                            ));
                        }
                    }
                    Err(error) => {
                        detail_parts.push(format!("请求 extension/status 失败: {}", error));
                    }
                }
            } else {
                detail_parts.push(format!("中继地址不可达: {}/", relay_url));
            }
        }
        Err(error) => {
            detail_parts.push(format!("创建诊断 HTTP 客户端失败: {}", error));
        }
    }

    let mut tabs_count = 0usize;
    match run_command(&binary, &["browser", "--browser-profile", "chrome", "tabs", "--json"]) {
        Ok((true, output)) => {
            tabs_count = parse_browser_tabs_count(&output).unwrap_or(0);
            if tabs_count == 0 {
                detail_parts.push("当前没有已附加的 Chrome 标签页。".to_string());
            }
        }
        Ok((false, output)) => {
            if output.trim().is_empty() {
                detail_parts.push("获取 chrome profile 标签页失败。".to_string());
            } else {
                detail_parts.push(output);
            }
        }
        Err(error) => {
            detail_parts.push(format!("执行 tabs 检查失败: {}", error));
        }
    }

    let likely_cause = if !relay_reachable {
        "本地中继服务不可达".to_string()
    } else if extension_connected == Some(false) {
        "扩展未连接到本地中继".to_string()
    } else if extension_connected == Some(true) && tabs_count == 0 {
        "扩展已连上中继，但标签页附加失败".to_string()
    } else if tabs_count > 0 {
        "中继工作正常".to_string()
    } else {
        "状态不完整，请重试诊断".to_string()
    };

    if extension_connected == Some(true) && tabs_count == 0 {
        detail_parts.push(
            "常见原因：标签页打开了 DevTools、被其他自动化工具占用，或加载了多个 OpenClaw Browser Relay 扩展实例。"
                .to_string(),
        );
    }

    BrowserRelayDiagnostic {
        relay_url,
        relay_reachable,
        extension_connected,
        tabs_count,
        likely_cause,
        detail: detail_parts.join(" | "),
        command_hint,
    }
}

fn parse_onboard_auth_choices(help_text: &str) -> Vec<String> {
    let marker = "Auth:";
    let Some(start) = help_text.find(marker) else {
        return Vec::new();
    };
    let remaining = &help_text[start + marker.len()..];
    let end = remaining.find("\n  --").unwrap_or(remaining.len());
    let raw = remaining[..end].trim();
    raw.split('|')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn normalize_provider_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut normalized = trimmed.to_string();

    // openclaw models status --json may return values like "qwen-portal (1)".
    // Strip usage-count suffix to avoid duplicated provider entries in UI.
    if trimmed.ends_with(')') {
        if let Some(open_idx) = trimmed.rfind(" (") {
            let digits = &trimmed[(open_idx + 2)..(trimmed.len() - 1)];
            if !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()) {
                let stripped = trimmed[..open_idx].trim();
                if stripped.is_empty() {
                    return None;
                }
                normalized = stripped.to_string();
            }
        }
    }

    let lowered = normalized.to_ascii_lowercase();
    let canonical = match lowered.as_str() {
        "codex" | "openai-codex-cli" => "openai-codex",
        "claude" | "claude-code" => "anthropic",
        "gemini" | "google-gemini" => "google-gemini-cli",
        _ => lowered.as_str(),
    };
    Some(canonical.to_string())
}

fn looks_like_oauth_provider(choice: &str) -> bool {
    if OPENCLAW_AUTH_CHOICE_NON_PROVIDER.contains(&choice) {
        return false;
    }
    if choice.contains("api-key")
        || choice.contains("apiKey")
        || choice == "custom-api-key"
        || choice.starts_with("minimax-api")
    {
        return false;
    }
    matches!(
        choice,
        "openai-codex"
            | "anthropic"
            | "chutes"
            | "github-copilot"
            | "copilot-proxy"
            | "google-antigravity"
            | "google-gemini-cli"
            | "minimax-portal"
            | "qwen-portal"
    ) || choice.starts_with("google-")
        || choice.ends_with("-portal")
}

fn resolve_provider_plugin_id(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "google-antigravity" => Some("google-antigravity-auth"),
        "google-gemini-cli" => Some("google-gemini-cli-auth"),
        "qwen-portal" => Some("qwen-portal-auth"),
        "copilot-proxy" => Some("copilot-proxy"),
        "minimax-portal" => Some("minimax-portal-auth"),
        _ => None,
    }
}

fn resolve_provider_default_model(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "qwen-portal" => Some("qwen-portal/coder-model"),
        "minimax-portal" => Some("minimax-portal/MiniMax-M2.5"),
        _ => None,
    }
}

fn resolve_openclaw_binary() -> Option<String> {
    let mut candidates = Vec::new();

    if let Ok(custom_bin) = std::env::var("OPENCLAW_BIN") {
        if !custom_bin.trim().is_empty() {
            candidates.push(custom_bin);
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        candidates.push(format!("{}/.local/bin/openclaw", home));
        candidates.push(format!("{}/.npm-global/bin/openclaw", home));
        candidates.push(format!("{}/.openclaw/bin/openclaw", home));
        candidates.push(format!("{}/.openclaw/node_modules/.bin/openclaw", home));
        candidates.push(format!("{}/.openclaw/node_modules/openclaw/openclaw.mjs", home));
        candidates.push(format!(
            "{}/.openclaw/lib/node_modules/openclaw/openclaw.mjs",
            home
        ));
    }

    if let Ok(profile) = std::env::var("USERPROFILE") {
        candidates.push(format!("{}\\.local\\bin\\openclaw.cmd", profile));
        candidates.push(format!("{}\\.local\\bin\\openclaw.exe", profile));
        candidates.push(format!("{}\\.openclaw\\bin\\openclaw.cmd", profile));
        candidates.push(format!("{}\\.openclaw\\bin\\openclaw.exe", profile));
        candidates.push(format!(
            "{}\\.openclaw\\node_modules\\.bin\\openclaw.cmd",
            profile
        ));
        candidates.push(format!(
            "{}\\.openclaw\\node_modules\\openclaw\\openclaw.mjs",
            profile
        ));
        candidates.push(format!(
            "{}\\.openclaw\\lib\\node_modules\\openclaw\\openclaw.mjs",
            profile
        ));
    }

    candidates.extend(
        OPENCLAW_BIN_CANDIDATES
            .iter()
            .map(std::string::ToString::to_string),
    );

    for candidate in candidates {
        let output = Command::new(&candidate).arg("--version").output();
        if let Ok(output) = output {
            if output.status.success() {
                return Some(candidate);
            }
        }
    }

    None
}

fn summarize_output(stdout: &[u8], stderr: &[u8]) -> String {
    let mut combined = String::new();
    if !stdout.is_empty() {
        combined.push_str(&String::from_utf8_lossy(stdout));
    }
    if !stderr.is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(&String::from_utf8_lossy(stderr));
    }

    let text = combined.trim().to_string();
    if text.len() > 1200 {
        format!("{}...(truncated)", &text[..1200])
    } else {
        text
    }
}

fn strip_ansi_and_controls(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut out = String::with_capacity(text.len());

    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1B {
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() {
                    let c = bytes[i];
                    i += 1;
                    if (b'@'..=b'~').contains(&c) {
                        break;
                    }
                }
            } else {
                while i < bytes.len() {
                    let c = bytes[i];
                    i += 1;
                    if (b'@'..=b'~').contains(&c) {
                        break;
                    }
                }
            }
            continue;
        }

        if b == b'\r' {
            i += 1;
            continue;
        }

        let ch = b as char;
        if ch.is_control() && ch != '\n' && ch != '\t' {
            i += 1;
            continue;
        }

        out.push(ch);
        i += 1;
    }

    out
}

fn normalize_oauth_output(raw: &str) -> String {
    let stripped = strip_ansi_and_controls(raw);
    let mut lines = stripped
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return String::new();
    }

    let single_char_lines = lines.iter().filter(|line| line.chars().count() == 1).count();
    if lines.len() > 40 && single_char_lines * 100 / lines.len() >= 65 {
        let merged = lines.join("");
        lines = merged
            .split('\n')
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(std::string::ToString::to_string)
            .collect();
    }

    let normalized = lines.join("\n");
    if normalized.len() > 1200 {
        format!("{}...(truncated)", &normalized[..1200])
    } else {
        normalized
    }
}

fn oauth_output_looks_failed(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    lower.contains("canceled")
        || lower.contains("cancelled")
        || lower.contains("timed out")
        || lower.contains("oauth failed")
        || lower.contains("error:")
}

fn push_bootstrap_log(app: &tauri::AppHandle, logs: &mut Vec<String>, message: impl Into<String>) {
    let line = message.into();
    logs.push(line.clone());
    let _ = app.emit(BOOTSTRAP_LOG_EVENT, line);
}

fn run_command(binary: &str, args: &[&str]) -> Result<(bool, String), String> {
    let output = Command::new(binary)
        .args(args)
        .output()
        .map_err(|err| err.to_string())?;

    let clipped = summarize_output(&output.stdout, &output.stderr);
    Ok((output.status.success(), clipped))
}

fn run_oauth_login_with_tty(binary: &str, provider_id: &str) -> Result<(bool, String), String> {
    let args = ["models", "auth", "login", "--provider", provider_id];

    #[cfg(not(target_os = "windows"))]
    {
        let output = Command::new("script")
            .arg("-q")
            .arg("/dev/null")
            .arg(binary)
            .args(args)
            .output();

        if let Ok(output) = output {
            let clipped = normalize_oauth_output(&summarize_output(&output.stdout, &output.stderr));
            return Ok((output.status.success(), clipped));
        }
    }

    run_command(binary, &args)
}

fn provider_has_auth_profile(provider_id: &str) -> bool {
    let auth_path = resolve_openclaw_auth_profiles_path();
    let Ok(raw) = fs::read_to_string(auth_path) else {
        return false;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    let Some(profiles) = parsed.get("profiles").and_then(|v| v.as_object()) else {
        return false;
    };

    profiles.values().any(|profile| {
        profile
            .get("provider")
            .and_then(|v| v.as_str())
            .map(|provider| provider == provider_id)
            .unwrap_or(false)
    })
}

fn run_openclaw(
    app: &tauri::AppHandle,
    binary: &str,
    args: &[&str],
    logs: &mut Vec<String>,
) -> Result<(), String> {
    let (ok, output) = run_command(binary, args)?;
    let cmd = format!("openclaw {}", args.join(" "));

    if ok {
        push_bootstrap_log(app, logs, format!("OK: {}", cmd));
        return Ok(());
    }

    let detail = if output.is_empty() {
        "no output".to_string()
    } else {
        output
    };
    Err(format!("{} failed: {}", cmd, detail))
}

fn check_models_auth_ready(app: &tauri::AppHandle, binary: &str, logs: &mut Vec<String>) -> bool {
    match run_command(binary, &["models", "status", "--check"]) {
        Ok((true, _)) => {
            push_bootstrap_log(app, logs, "OK: openclaw models status --check");
            true
        }
        Ok((false, output)) => {
            let detail = if output.trim().is_empty() {
                "no output".to_string()
            } else {
                output
            };
            push_bootstrap_log(
                app,
                logs,
                format!("WARN: openclaw models status --check failed: {}", detail),
            );
            false
        }
        Err(error) => {
            push_bootstrap_log(
                app,
                logs,
                format!("WARN: failed to run openclaw models status --check: {}", error),
            );
            false
        }
    }
}

fn run_installer_script(app: &tauri::AppHandle, logs: &mut Vec<String>) -> Result<(), String> {
    match std::env::consts::OS {
        "windows" => {
            push_bootstrap_log(app, logs, "Installing OpenClaw using install.ps1");
            let (ok, output) = run_command(
                "powershell",
                &["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", OPENCLAW_INSTALL_PS1],
            )?;
            if ok {
                Ok(())
            } else if output.is_empty() {
                Err("install.ps1 failed".to_string())
            } else {
                Err(format!("install.ps1 failed: {}", output))
            }
        }
        _ => {
            push_bootstrap_log(app, logs, "Installing OpenClaw using install.sh");
            let (ok, output) = run_command("bash", &["-lc", OPENCLAW_INSTALL_SH])?;
            if ok {
                Ok(())
            } else if output.is_empty() {
                Err("install.sh failed".to_string())
            } else {
                Err(format!("install.sh failed: {}", output))
            }
        }
    }
}

fn resolve_bundled_openclaw_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let resolver = app.path();
    if let Ok(path) = resolver.resolve("openclaw-bundle", tauri::path::BaseDirectory::Resource) {
        candidates.push(path);
    }

    // tauri dev 下资源不会自动打入 app bundle，这里给本地目录兜底。
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("src-tauri").join("bundle").join("resources").join("openclaw-bundle"));
        candidates.push(cwd.join("bundle").join("resources").join("openclaw-bundle"));
        candidates.push(
            cwd.join("..")
                .join("src-tauri")
                .join("bundle")
                .join("resources")
                .join("openclaw-bundle"),
        );
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(
                exe_dir
                    .join("..")
                    .join("..")
                    .join("Resources")
                    .join("openclaw-bundle"),
            );
            candidates.push(
                exe_dir
                    .join("..")
                    .join("..")
                    .join("bundle")
                    .join("resources")
                    .join("openclaw-bundle"),
            );
        }
    }

    for candidate in candidates {
        if candidate.exists() {
            return Some(candidate.canonicalize().unwrap_or(candidate));
        }
    }
    None
}

fn copy_dir_with_native_tool(src: &PathBuf, dst: &PathBuf) -> Result<(), String> {
    if dst.exists() {
        fs::remove_dir_all(dst).map_err(|err| err.to_string())?;
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    if cfg!(target_os = "windows") {
        let src_escaped = src.to_string_lossy().replace('\'', "''");
        let dst_escaped = dst.to_string_lossy().replace('\'', "''");
        let script = format!(
            "Copy-Item -LiteralPath '{}' -Destination '{}' -Recurse -Force",
            src_escaped, dst_escaped
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
            .output()
            .map_err(|err| err.to_string())?;
        if output.status.success() {
            return Ok(());
        }
        return Err(format!(
            "Copy-Item failed: {}",
            summarize_output(&output.stdout, &output.stderr)
        ));
    }

    let output = Command::new("cp")
        .arg("-R")
        .arg(src)
        .arg(dst)
        .output()
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "cp -R failed: {}",
            summarize_output(&output.stdout, &output.stderr)
        ))
    }
}

fn resolve_prefix_openclaw_entry(prefix: &PathBuf) -> Option<PathBuf> {
    let candidates = vec![
        prefix.join("node_modules").join("openclaw").join("openclaw.mjs"),
        prefix
            .join("lib")
            .join("node_modules")
            .join("openclaw")
            .join("openclaw.mjs"),
    ];
    candidates.into_iter().find(|candidate| candidate.exists())
}

fn resolve_bundled_node_binary(bundle_dir: &PathBuf) -> Option<PathBuf> {
    let candidates = if cfg!(target_os = "windows") {
        vec![
            bundle_dir.join("node").join("bin").join("node.exe"),
            bundle_dir.join("node").join("node.exe"),
        ]
    } else {
        vec![
            bundle_dir.join("node").join("bin").join("node"),
            bundle_dir.join("node").join("node"),
        ]
    };
    candidates.into_iter().find(|candidate| candidate.exists())
}

fn resolve_node_runtime_root(node_binary: &PathBuf) -> Option<PathBuf> {
    let parent = node_binary.parent()?;
    let is_bin_dir = parent
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case("bin"))
        .unwrap_or(false);
    if is_bin_dir {
        parent.parent().map(PathBuf::from)
    } else {
        Some(PathBuf::from(parent))
    }
}

fn resolve_node_binary_in_runtime(runtime_dir: &PathBuf) -> Option<PathBuf> {
    let candidates = if cfg!(target_os = "windows") {
        vec![
            runtime_dir.join("bin").join("node.exe"),
            runtime_dir.join("node.exe"),
        ]
    } else {
        vec![runtime_dir.join("bin").join("node"), runtime_dir.join("node")]
    };
    candidates.into_iter().find(|candidate| candidate.exists())
}

fn ensure_prefix_openclaw_launcher(
    prefix: &PathBuf,
    bundle_dir: &PathBuf,
    logs: &mut Vec<String>,
) -> Result<(), String> {
    let openclaw_entry = resolve_prefix_openclaw_entry(prefix).ok_or_else(|| {
        "openclaw.mjs not found under bundled prefix (node_modules/openclaw)".to_string()
    })?;

    let bin_dir = prefix.join("bin");
    fs::create_dir_all(&bin_dir).map_err(|err| err.to_string())?;
    let node_runtime_dir = prefix.join("node-runtime");
    let mut node_cmd = "node".to_string();

    if let Some(bundled_node) = resolve_bundled_node_binary(bundle_dir) {
        if let Some(runtime_root) = resolve_node_runtime_root(&bundled_node) {
            if node_runtime_dir.exists() {
                fs::remove_dir_all(&node_runtime_dir).map_err(|err| err.to_string())?;
            }
            copy_dir_with_native_tool(&runtime_root, &node_runtime_dir)?;
            if let Some(node_target) = resolve_node_binary_in_runtime(&node_runtime_dir) {
                #[cfg(unix)]
                {
                    fs::set_permissions(&node_target, fs::Permissions::from_mode(0o755))
                        .map_err(|err| err.to_string())?;
                }
                node_cmd = node_target.to_string_lossy().to_string();
            } else {
                logs.push(
                    "Bundled node runtime copied, but node binary was not found; launcher will use system node."
                        .to_string(),
                );
            }
        } else {
            logs.push("Bundled node runtime path is invalid; launcher will use system node.".to_string());
        }
    } else {
        logs.push("Bundled node runtime missing; launcher will use system node.".to_string());
    }

    if cfg!(target_os = "windows") {
        let launcher = bin_dir.join("openclaw.cmd");
        let script = format!(
            "@echo off\r\n\"{}\" \"{}\" %*\r\n",
            node_cmd,
            openclaw_entry.to_string_lossy()
        );
        fs::write(&launcher, script).map_err(|err| err.to_string())?;
    } else {
        let launcher = bin_dir.join("openclaw");
        let script = format!(
            "#!/bin/sh\nexec \"{}\" \"{}\" \"$@\"\n",
            node_cmd,
            openclaw_entry.to_string_lossy()
        );
        fs::write(&launcher, script).map_err(|err| err.to_string())?;
        #[cfg(unix)]
        {
            fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755))
                .map_err(|err| err.to_string())?;
        }
    }

    logs.push("Generated local launcher: ~/.openclaw/bin/openclaw".to_string());
    Ok(())
}

fn prefix_has_openclaw_binary(prefix: &PathBuf) -> bool {
    let candidates = if cfg!(target_os = "windows") {
        vec![
            prefix.join("bin").join("openclaw.cmd"),
            prefix.join("bin").join("openclaw.exe"),
            prefix.join("node_modules").join(".bin").join("openclaw.cmd"),
            prefix
                .join("node_modules")
                .join("openclaw")
                .join("openclaw.mjs"),
            prefix
                .join("lib")
                .join("node_modules")
                .join("openclaw")
                .join("openclaw.mjs"),
        ]
    } else {
        vec![
            prefix.join("bin").join("openclaw"),
            prefix.join("node_modules").join(".bin").join("openclaw"),
            prefix
                .join("node_modules")
                .join("openclaw")
                .join("openclaw.mjs"),
            prefix
                .join("lib")
                .join("node_modules")
                .join("openclaw")
                .join("openclaw.mjs"),
        ]
    };
    candidates.into_iter().any(|candidate| candidate.exists())
}

fn install_openclaw_from_bundle(
    app: &tauri::AppHandle,
    logs: &mut Vec<String>,
) -> Result<bool, String> {
    let Some(bundle_dir) = resolve_bundled_openclaw_dir(app) else {
        push_bootstrap_log(
            app,
            logs,
            "No bundled OpenClaw payload found in installer resources.",
        );
        return Ok(false);
    };

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Cannot resolve user home path for offline install".to_string())?;
    let prefix = PathBuf::from(home).join(".openclaw");
    fs::create_dir_all(&prefix).map_err(|err| err.to_string())?;

    let prepared_prefix = bundle_dir.join("prefix");
    if prepared_prefix.exists() {
        push_bootstrap_log(app, logs, "Installing OpenClaw from bundled prefix snapshot...");
        copy_dir_with_native_tool(&prepared_prefix, &prefix)?;
        if let Err(error) = ensure_prefix_openclaw_launcher(&prefix, &bundle_dir, logs) {
            push_bootstrap_log(app, logs, format!("WARN: {}", error));
        }
        if prefix_has_openclaw_binary(&prefix) {
            push_bootstrap_log(app, logs, "OpenClaw bundled prefix install completed.");
            return Ok(true);
        }
        push_bootstrap_log(
            app,
            logs,
            "Bundled prefix copied but openclaw binary was not found; fallback to npm offline install.",
        );
    }

    let Some(node_bin) = resolve_bundled_node_binary(&bundle_dir) else {
        push_bootstrap_log(app, logs, "Bundled payload is incomplete; skip offline install.");
        return Ok(false);
    };
    let npm_cli = bundle_dir.join("npm").join("bin").join("npm-cli.js");
    let openclaw_tgz = bundle_dir.join("openclaw.tgz");
    let npm_cache = bundle_dir.join("npm-cache");

    if !npm_cli.exists() || !openclaw_tgz.exists() || !npm_cache.exists() {
        push_bootstrap_log(app, logs, "Bundled payload is incomplete; skip offline install.");
        return Ok(false);
    }

    push_bootstrap_log(app, logs, "Installing OpenClaw from bundled offline payload...");
    let output = Command::new(&node_bin)
        .arg(&npm_cli)
        .arg("install")
        .arg("--prefix")
        .arg(&prefix)
        .arg(&openclaw_tgz)
        .arg("--cache")
        .arg(&npm_cache)
        .arg("--offline")
        .arg("--no-audit")
        .arg("--no-fund")
        .arg("--loglevel=error")
        .output()
        .map_err(|err| format!("Failed to run bundled npm installer: {}", err))?;

    let detail = summarize_output(&output.stdout, &output.stderr);
    if output.status.success() {
        if let Err(error) = ensure_prefix_openclaw_launcher(&prefix, &bundle_dir, logs) {
            push_bootstrap_log(app, logs, format!("WARN: {}", error));
        }
        if prefix_has_openclaw_binary(&prefix) {
            push_bootstrap_log(app, logs, "OpenClaw offline bundle install completed.");
            return Ok(true);
        }
        return Err("Bundled npm install succeeded but openclaw binary not found.".to_string());
    }

    if detail.is_empty() {
        Err("Bundled offline install failed with no output.".to_string())
    } else {
        Err(format!("Bundled offline install failed: {}", detail))
    }
}

fn gateway_child_slot() -> &'static Mutex<Option<Child>> {
    static SLOT: OnceLock<Mutex<Option<Child>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn is_gateway_process_alive() -> bool {
    let Ok(mut guard) = gateway_child_slot().lock() else {
        return false;
    };

    match guard.as_mut() {
        Some(child) => match child.try_wait() {
            Ok(None) => true,
            Ok(Some(_)) | Err(_) => {
                *guard = None;
                false
            }
        },
        None => false,
    }
}

fn spawn_gateway_process(binary: &str) -> Result<bool, String> {
    let mut guard = gateway_child_slot()
        .lock()
        .map_err(|_| "Failed to lock gateway process state".to_string())?;

    if let Some(child) = guard.as_mut() {
        match child.try_wait() {
            Ok(None) => return Ok(false),
            Ok(Some(_)) | Err(_) => {
                *guard = None;
            }
        }
    }

    let child = Command::new(binary)
        .arg("gateway")
        .arg("run")
        .arg("--allow-unconfigured")
        .arg("--port")
        .arg("18789")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("Failed to run `openclaw gateway run`: {}", err))?;

    *guard = Some(child);
    Ok(true)
}

async fn is_official_web_ready() -> bool {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(1200))
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };

    client.get(OFFICIAL_WEB_URL).send().await.is_ok()
}

#[tauri::command]
fn list_oauth_providers() -> Vec<String> {
    let mut providers = BTreeSet::new();
    for provider in FALLBACK_OAUTH_PROVIDERS {
        if let Some(normalized) = normalize_provider_id(provider) {
            providers.insert(normalized);
        }
    }

    let Some(binary) = resolve_openclaw_binary() else {
        return providers.into_iter().collect();
    };

    let output = Command::new(&binary)
        .arg("models")
        .arg("status")
        .arg("--json")
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            if let Ok(parsed) = serde_json::from_slice::<ModelsStatusJson>(&out.stdout) {
                if let Some(auth) = parsed.auth {
                    if let Some(known) = auth.providers_with_oauth {
                        for provider in known {
                            if let Some(normalized) = normalize_provider_id(&provider) {
                                providers.insert(normalized);
                            }
                        }
                    }
                }
            }
        }
    }

    let onboard_help = Command::new(&binary).arg("onboard").arg("--help").output();
    if let Ok(help) = onboard_help {
        if help.status.success() {
            let text = String::from_utf8_lossy(&help.stdout).to_string();
            for choice in parse_onboard_auth_choices(&text) {
                if looks_like_oauth_provider(&choice) {
                    if let Some(normalized) = normalize_provider_id(&choice) {
                        providers.insert(normalized);
                    }
                }
            }
        }
    }

    providers.into_iter().collect()
}

#[tauri::command]
fn start_oauth_login(provider_id: String) -> LoginResult {
    let raw_provider_id = provider_id.trim().to_string();
    let Some(provider_id) = normalize_provider_id(&raw_provider_id) else {
        return LoginResult {
            provider_id: raw_provider_id,
            launched: false,
            command_hint: "openclaw models auth login --provider <provider-id>".to_string(),
            details: "Provider id is required.".to_string(),
        };
    };
    let command_hint = format!("openclaw models auth login --provider {}", provider_id);

    let Some(binary) = resolve_openclaw_binary() else {
        return LoginResult {
            provider_id,
            launched: false,
            command_hint,
            details: "openclaw binary not found. Install OpenClaw CLI first.".to_string(),
        };
    };

    let mut detail_lines: Vec<String> = Vec::new();
    let had_profile_before = provider_has_auth_profile(&provider_id);
    if let Some(plugin_id) = resolve_provider_plugin_id(&provider_id) {
        match run_command(&binary, &["plugins", "enable", plugin_id]) {
            Ok((true, _)) => {
                detail_lines.push(format!("Provider plugin ensured: {}", plugin_id));
            }
            Ok((false, output)) => {
                if output.is_empty() {
                    detail_lines.push(format!(
                        "WARN: failed to enable provider plugin {}.",
                        plugin_id
                    ));
                } else {
                    detail_lines.push(format!(
                        "WARN: failed to enable provider plugin {}: {}",
                        plugin_id, output
                    ));
                }
            }
            Err(err) => {
                detail_lines.push(format!(
                    "WARN: failed to enable provider plugin {}: {}",
                    plugin_id, err
                ));
            }
        }
    }

    let output = run_oauth_login_with_tty(&binary, &provider_id);

    match output {
        Ok((true, output)) => {
            let ready = provider_has_auth_profile(&provider_id);
            let looks_failed = oauth_output_looks_failed(&output);
            if ready && !looks_failed {
                let mut model_switch_ok = true;
                if let Some(model_id) = resolve_provider_default_model(&provider_id) {
                    match run_command(&binary, &["models", "set", model_id]) {
                        Ok((true, _)) => {
                            detail_lines.push(format!("Default model switched to {}.", model_id));
                        }
                        Ok((false, set_output)) => {
                            model_switch_ok = false;
                            if set_output.trim().is_empty() {
                                detail_lines.push(format!(
                                    "OAuth completed, but failed to switch default model to {}.",
                                    model_id
                                ));
                            } else {
                                detail_lines.push(format!(
                                    "OAuth completed, but failed to switch default model to {}: {}",
                                    model_id, set_output
                                ));
                            }
                        }
                        Err(err) => {
                            model_switch_ok = false;
                            detail_lines.push(format!(
                                "OAuth completed, but failed to switch default model to {}: {}",
                                model_id, err
                            ));
                        }
                    }
                }

                if !model_switch_ok {
                    return LoginResult {
                        provider_id,
                        launched: false,
                        command_hint,
                        details: detail_lines.join("\n"),
                    };
                }

                if had_profile_before {
                    detail_lines.push("OAuth login completed (existing profile refreshed/reused).".to_string());
                } else {
                    detail_lines.push("OAuth login completed and provider auth is ready.".to_string());
                }
                LoginResult {
                    provider_id,
                    launched: true,
                    command_hint,
                    details: detail_lines.join("\n"),
                }
            } else {
                detail_lines.push(
                    "OAuth command finished, but provider auth profile was not ready.".to_string(),
                );
                if !output.trim().is_empty() {
                    detail_lines.push(output);
                }
                LoginResult {
                    provider_id,
                    launched: false,
                    command_hint,
                    details: detail_lines.join("\n"),
                }
            }
        }
        Ok((false, output)) => {
            if output.is_empty() {
                detail_lines.push("OAuth login command failed.".to_string());
            } else {
                detail_lines.push(output);
            }
            LoginResult {
                provider_id,
                launched: false,
                command_hint,
                details: detail_lines.join("\n"),
            }
        }
        Err(err) => {
            detail_lines.push(err);
            LoginResult {
                provider_id,
                launched: false,
                command_hint,
                details: detail_lines.join("\n"),
            }
        }
    }
}

#[tauri::command]
async fn check_ollama() -> Result<OllamaStatus, String> {
    let endpoint = "http://127.0.0.1:11434".to_string();
    let url = format!("{}/api/tags", endpoint);

    let response = reqwest::get(url).await.map_err(|err| err.to_string())?;
    let status = response.status();

    if !status.is_success() {
        return Ok(OllamaStatus {
            endpoint,
            reachable: false,
            models: vec![],
            error: Some(format!("HTTP {}", status.as_u16())),
        });
    }

    let payload = response
        .json::<OllamaTagsResponse>()
        .await
        .map_err(|err| err.to_string())?;

    let models = payload
        .models
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| item.name)
        .collect::<Vec<_>>();

    Ok(OllamaStatus {
        endpoint,
        reachable: true,
        models,
        error: None,
    })
}

#[tauri::command]
async fn ensure_official_web_ready() -> OfficialWebStatus {
    let command_hint = "openclaw gateway".to_string();
    let url = resolve_official_dashboard_url();

    if is_official_web_ready().await {
        return OfficialWebStatus {
            ready: true,
            installed: true,
            running: true,
            started: false,
            url,
            command_hint,
            message: "Official local web is already reachable.".to_string(),
            error: None,
        };
    }

    let Some(binary) = resolve_openclaw_binary() else {
        return OfficialWebStatus {
            ready: false,
            installed: false,
            running: false,
            started: false,
            url,
            command_hint,
            message: "openclaw binary not found.".to_string(),
            error: Some("Install OpenClaw first, then retry.".to_string()),
        };
    };

    let started = match spawn_gateway_process(&binary) {
        Ok(started) => started,
        Err(error) => {
            return OfficialWebStatus {
                ready: false,
                installed: true,
                running: false,
                started: false,
                url,
                command_hint,
                message: "Failed to start local gateway.".to_string(),
                error: Some(error),
            };
        }
    };

    for _ in 0..30 {
        if is_official_web_ready().await {
            return OfficialWebStatus {
                ready: true,
                installed: true,
                running: true,
                started,
                url,
                command_hint,
                message: if started {
                    "Official local web started successfully."
                } else {
                    "Official local web is reachable."
                }
                .to_string(),
                error: None,
            };
        }
        std::thread::sleep(Duration::from_millis(400));
    }

    OfficialWebStatus {
        ready: false,
        installed: true,
        running: is_gateway_process_alive(),
        started,
        url,
        command_hint,
        message: "Gateway started, but local web did not become ready in time.".to_string(),
        error: Some("Timeout while waiting for http://127.0.0.1:18789/".to_string()),
    }
}

#[tauri::command]
async fn open_official_web_window(app: tauri::AppHandle) -> Result<OpenOfficialWebResult, String> {
    let web = ensure_official_web_ready().await;
    if !web.ready {
        let message = [web.error.clone().unwrap_or_default(), web.message]
            .into_iter()
            .filter(|item| !item.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(if message.is_empty() {
            "Official local web is not ready.".to_string()
        } else {
            message
        });
    }

    let label = "official-local-web";
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(OpenOfficialWebResult {
            opened: false,
            url: web.url,
            detail: "Official web window is already open.".to_string(),
        });
    }

    let url = reqwest::Url::parse(&web.url).map_err(|err| err.to_string())?;
    tauri::WebviewWindowBuilder::new(&app, label, tauri::WebviewUrl::External(url))
        .title("OpenClaw Official Local")
        .inner_size(1280.0, 840.0)
        .resizable(true)
        .build()
        .map_err(|err| format!("Failed to open official web window: {}", err))?;

    Ok(OpenOfficialWebResult {
        opened: true,
        url: web.url,
        detail: "Official web window opened.".to_string(),
    })
}

#[tauri::command]
async fn bootstrap_openclaw(app: tauri::AppHandle) -> BootstrapStatus {
    let mut logs: Vec<String> = Vec::new();
    push_bootstrap_log(&app, &mut logs, "Bootstrap started.");
    let mut installed = resolve_openclaw_binary().is_some();
    let installed_before = installed;
    let mut install_performed = false;

    if !installed {
        push_bootstrap_log(&app, &mut logs, "OpenClaw CLI not found. Auto install will start.");
        install_performed = true;

        match install_openclaw_from_bundle(&app, &mut logs) {
            Ok(true) => {
                installed = resolve_openclaw_binary().is_some();
            }
            Ok(false) => {
                push_bootstrap_log(
                    &app,
                    &mut logs,
                    "Offline payload unavailable, fallback to online installer.",
                );
            }
            Err(error) => {
                push_bootstrap_log(&app, &mut logs, format!("WARN: {}", error));
                push_bootstrap_log(&app, &mut logs, "Fallback to online installer.");
            }
        }

        if !installed {
            push_bootstrap_log(&app, &mut logs, "Run online installer...");
            if let Err(error) = run_installer_script(&app, &mut logs) {
                let web = OfficialWebStatus {
                    ready: false,
                    installed: false,
                    running: false,
                    started: false,
                    url: OFFICIAL_WEB_URL.to_string(),
                    command_hint: "openclaw gateway".to_string(),
                    message: "OpenClaw install failed.".to_string(),
                    error: Some(error.clone()),
                };

                return BootstrapStatus {
                    ready: false,
                    installed: false,
                    initialized: false,
                    web,
                    message: "Auto install failed.".to_string(),
                    logs,
                    error: Some(error),
                };
            }
            installed = resolve_openclaw_binary().is_some();
        }
    }

    let Some(binary) = resolve_openclaw_binary() else {
        let web = OfficialWebStatus {
            ready: false,
            installed: false,
            running: false,
            started: false,
            url: OFFICIAL_WEB_URL.to_string(),
            command_hint: "openclaw gateway".to_string(),
            message: "OpenClaw CLI still not found after install.".to_string(),
            error: Some("Binary not found".to_string()),
        };

        return BootstrapStatus {
            ready: false,
            installed: false,
            initialized: false,
            web,
            message: "OpenClaw bootstrap failed.".to_string(),
            logs,
            error: Some("openclaw binary not found".to_string()),
        };
    };

    push_bootstrap_log(&app, &mut logs, format!("Using CLI binary: {}", binary));
    if let Err(error) = ensure_browser_defaults(&app, &mut logs) {
        push_bootstrap_log(
            &app,
            &mut logs,
            format!("WARN: failed to ensure browser defaults: {}", error),
        );
    }
    ensure_browser_relay_installed(&app, &binary, &mut logs);

    if installed_before && !install_performed {
        push_bootstrap_log(&app, &mut logs, "Checking existing gateway status...");
        if let Err(error) = run_openclaw(&app, &binary, &["gateway", "start"], &mut logs) {
            push_bootstrap_log(&app, &mut logs, format!("WARN: {}", error));
        }
        let auth_ready = check_models_auth_ready(&app, &binary, &mut logs);
        let web = ensure_official_web_ready().await;
        if web.ready && auth_ready {
            return BootstrapStatus {
                ready: true,
                installed: true,
                initialized: true,
                web: web.clone(),
                message: "OpenClaw is ready.".to_string(),
                logs,
                error: None,
            };
        }
        push_bootstrap_log(
            &app,
            &mut logs,
            "Gateway/auth is not ready; running auto-repair setup.",
        );
    }

    push_bootstrap_log(&app, &mut logs, "Running setup...");
    let setup_ok = match run_openclaw(&app, &binary, &["setup"], &mut logs) {
        Ok(_) => true,
        Err(error) => {
            push_bootstrap_log(&app, &mut logs, format!("WARN: {}", error));
            false
        }
    };

    let codex_auth_detected = detect_local_codex_auth().detected;
    push_bootstrap_log(
        &app,
        &mut logs,
        format!(
            "Onboarding auth choice: {}",
            if codex_auth_detected {
                "skip (local codex detected; will sync local Codex auth after onboard)"
            } else {
                "skip (local codex not detected)"
            }
        ),
    );

    let mut onboard_ok = true;
    let onboard_args = vec![
        "onboard",
        "--non-interactive",
        "--accept-risk",
        "--mode",
        "local",
        "--auth-choice",
        "skip",
        "--install-daemon",
        "--skip-channels",
        "--skip-skills",
        "--skip-ui",
        "--skip-health",
    ];

    push_bootstrap_log(&app, &mut logs, "Running onboard...");
    if let Err(error) = run_openclaw(&app, &binary, &onboard_args, &mut logs) {
        push_bootstrap_log(&app, &mut logs, format!("WARN: {}", error));
        onboard_ok = false;
    }

    if !onboard_ok {
        push_bootstrap_log(
            &app,
            &mut logs,
            "Onboard failed, trying gateway install --force + start...",
        );
        let install_ok = match run_openclaw(
            &app,
            &binary,
            &["gateway", "install", "--force"],
            &mut logs,
        ) {
            Ok(_) => true,
            Err(error) => {
                push_bootstrap_log(&app, &mut logs, format!("WARN: {}", error));
                false
            }
        };
        let start_ok = match run_openclaw(&app, &binary, &["gateway", "start"], &mut logs) {
            Ok(_) => true,
            Err(error) => {
                push_bootstrap_log(&app, &mut logs, format!("WARN: {}", error));
                false
            }
        };
        onboard_ok = install_ok && start_ok;
    }

    if codex_auth_detected {
        push_bootstrap_log(
            &app,
            &mut logs,
            "Local Codex auth detected, syncing into OpenClaw auth-profiles...",
        );
        match sync_local_codex_auth_to_openclaw(true) {
            Ok(result) => {
                push_bootstrap_log(&app, &mut logs, format!("OK: {}", result.message));
                if let Some(profile_id) = result.profile_id {
                    push_bootstrap_log(
                        &app,
                        &mut logs,
                        format!("Codex profile synced: {}", profile_id),
                    );
                }
                if let Some(model) = result.model {
                    push_bootstrap_log(
                        &app,
                        &mut logs,
                        format!("Default model after sync: {}", model),
                    );
                }
            }
            Err(error) => {
                push_bootstrap_log(
                    &app,
                    &mut logs,
                    format!("WARN: failed to sync local Codex auth: {}", error),
                );
            }
        }
    }

    push_bootstrap_log(&app, &mut logs, "Ensuring gateway start...");
    if let Err(error) = run_openclaw(&app, &binary, &["gateway", "start"], &mut logs) {
        push_bootstrap_log(&app, &mut logs, format!("WARN: {}", error));
    }

    let model_auth_ready = check_models_auth_ready(&app, &binary, &mut logs);
    let initialized = onboard_ok && model_auth_ready;
    let web = ensure_official_web_ready().await;
    let ready = installed && initialized && web.ready;

    if !setup_ok {
        push_bootstrap_log(
            &app,
            &mut logs,
            "WARN: openclaw setup failed; continuing because onboard/model-auth checks decide readiness.",
        );
    }

    BootstrapStatus {
        ready,
        installed,
        initialized,
        web: web.clone(),
        message: if ready {
            "OpenClaw is installed and official local web is ready."
        } else if !onboard_ok {
            "OpenClaw installed, but initialization failed."
        } else if !model_auth_ready {
            "OpenClaw initialized, but no usable model auth detected."
        } else {
            "OpenClaw bootstrap incomplete. Check logs and retry."
        }
        .to_string(),
        logs,
        error: if ready {
            None
        } else if !onboard_ok {
            Some("Initialization steps failed (onboard/gateway install)".to_string())
        } else if !model_auth_ready {
            Some("Model auth is not ready (openclaw models status --check failed)".to_string())
        } else {
            web.error.clone()
        },
    }
}

#[tauri::command]
fn reuse_local_codex_auth(set_default_model: Option<bool>) -> LocalCodexReuseResult {
    match sync_local_codex_auth_to_openclaw(set_default_model.unwrap_or(true)) {
        Ok(result) => result,
        Err(error) => LocalCodexReuseResult {
            reused: false,
            profile_id: None,
            model: None,
            message: "Failed to reuse local Codex auth.".to_string(),
            error: Some(error),
        },
    }
}

#[tauri::command]
fn save_api_key(provider_id: String, api_key: String) -> Result<serde_json::Value, String> {
    if provider_id.trim().is_empty() {
        return Err("provider_id is required".to_string());
    }
    if api_key.trim().is_empty() {
        return Err("api_key is required".to_string());
    }

    Ok(serde_json::json!({ "ok": true }))
}

fn read_local_codex_auth_status() -> CodexAuthStatus {
    let path = resolve_codex_auth_path();
    let source = path.to_string_lossy().to_string();

    let content = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => {
            return CodexAuthStatus {
                detected: false,
                source,
                last_refresh: None,
                token_fields: vec![],
            }
        }
    };

    let value = match serde_json::from_str::<serde_json::Value>(&content) {
        Ok(v) => v,
        Err(_) => {
            return CodexAuthStatus {
                detected: false,
                source,
                last_refresh: None,
                token_fields: vec![],
            }
        }
    };

    let last_refresh = value
        .get("last_refresh")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let token_fields = value
        .get("tokens")
        .and_then(|v| v.as_object())
        .map(|obj| obj.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let detected = !token_fields.is_empty();

    CodexAuthStatus {
        detected,
        source,
        last_refresh,
        token_fields,
    }
}

#[tauri::command]
fn detect_local_codex_auth() -> CodexAuthStatus {
    read_local_codex_auth_status()
}

#[tauri::command]
fn detect_local_oauth_tools() -> Vec<LocalOAuthToolStatus> {
    let codex = read_local_codex_auth_status();
    let codex_cli = command_exists("codex", &["--version"]);

    let claude_path = resolve_claude_credentials_path();
    let claude_file_detected = claude_path.exists();
    let claude_cli = command_exists("claude", &["--version"])
        || command_exists("claude-code", &["--version"]);
    let claude_keychain_detected = if cfg!(target_os = "macos") {
        Command::new("security")
            .arg("find-generic-password")
            .arg("-s")
            .arg(CLAUDE_KEYCHAIN_SERVICE)
            .arg("-w")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    } else {
        false
    };

    let gemini_cli = command_exists("gemini", &["--version"]);
    let gemini_auth_probe = if gemini_cli {
        Command::new("gemini")
            .arg("--output-format")
            .arg("json")
            .arg("ok")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    } else {
        false
    };

    vec![
        LocalOAuthToolStatus {
            id: "codex".to_string(),
            label: "OpenAI Codex".to_string(),
            provider_id: "openai-codex".to_string(),
            cli_found: codex_cli,
            auth_detected: codex.detected,
            source: codex.source,
            detail: if codex.detected {
                Some("Detected local Codex auth tokens.".to_string())
            } else {
                Some("No local Codex auth token detected.".to_string())
            },
        },
        LocalOAuthToolStatus {
            id: "claude-code".to_string(),
            label: "Claude Code".to_string(),
            provider_id: "anthropic".to_string(),
            cli_found: claude_cli,
            auth_detected: claude_file_detected || claude_keychain_detected,
            source: if claude_keychain_detected && cfg!(target_os = "macos") {
                "macOS Keychain (Claude Code-credentials)".to_string()
            } else {
                claude_path.to_string_lossy().to_string()
            },
            detail: if claude_file_detected || claude_keychain_detected {
                Some("Detected reusable Claude Code credentials.".to_string())
            } else {
                Some("No reusable Claude Code credentials found.".to_string())
            },
        },
        LocalOAuthToolStatus {
            id: "gemini-cli".to_string(),
            label: "Gemini CLI".to_string(),
            provider_id: "google-gemini-cli".to_string(),
            cli_found: gemini_cli,
            auth_detected: gemini_auth_probe,
            source: "gemini".to_string(),
            detail: if gemini_auth_probe {
                Some("Gemini CLI is installed and auth probe succeeded.".to_string())
            } else if gemini_cli {
                Some("Gemini CLI detected; auth state unknown or not ready.".to_string())
            } else {
                Some("Gemini CLI is not installed.".to_string())
            },
        },
    ]
}

#[tauri::command]
fn validate_local_codex_connectivity() -> CodexConnectivityStatus {
    let expected = "CODEx_OK".to_string();
    let command = "codex exec --skip-git-repo-check -o <temp_file> \"Reply with exactly: CODEx_OK\""
        .to_string();
    let prompt = "Reply with exactly: CODEx_OK";
    let mut out_path = std::env::temp_dir();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    out_path.push(format!(
        "openclaw-desktop-codex-probe-{}-{}.txt",
        std::process::id(),
        now_ms
    ));

    let output = Command::new("codex")
        .arg("exec")
        .arg("--skip-git-repo-check")
        .arg("-o")
        .arg(&out_path)
        .arg(prompt)
        .output();

    let response = fs::read_to_string(&out_path).ok().map(|s| s.trim().to_string());
    let _ = fs::remove_file(&out_path);

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let from_stdout = if stdout.contains("CODEx_OK") {
                Some("CODEx_OK".to_string())
            } else {
                None
            };
            let normalized = response.clone().or(from_stdout);
            let ok = out.status.success() && normalized.as_deref() == Some("CODEx_OK");

            CodexConnectivityStatus {
                ok,
                expected,
                response: normalized,
                error: if ok {
                    None
                } else if !stderr.trim().is_empty() {
                    Some(stderr)
                } else if !stdout.trim().is_empty() {
                    Some(stdout)
                } else {
                    Some("No output from codex".to_string())
                },
                command,
            }
        }
        Err(err) => CodexConnectivityStatus {
            ok: false,
            expected,
            response: None,
            error: Some(err.to_string()),
            command,
        },
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            list_oauth_providers,
            start_oauth_login,
            check_ollama,
            bootstrap_openclaw,
            ensure_official_web_ready,
            open_official_web_window,
            get_browser_mode_status,
            set_browser_mode,
            get_browser_relay_status,
            prepare_browser_relay,
            diagnose_browser_relay,
            save_api_key,
            detect_local_codex_auth,
            reuse_local_codex_auth,
            detect_local_oauth_tools,
            validate_local_codex_connectivity
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

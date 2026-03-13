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
const OPENCLAW_WINDOWS_PORTABLE_BUNDLE_URL: &str =
    "https://github.com/daxiondi/openclaw-desktop/releases/latest/download/openclaw-desktop-windows-portable.zip";

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
struct OllamaApplyResult {
    endpoint: String,
    model: String,
    discovered_models: Vec<String>,
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
struct FeishuChannelStatus {
    plugin_installed: bool,
    channel_enabled: bool,
    has_credentials: bool,
    app_id: String,
    error: Option<String>,
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
            "Browser detection: no local Chromium-based browser found. Please install Google Chrome or another Chromium-based browser manually, then restart the app.",
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

fn normalize_api_key_provider_id(raw: &str) -> Option<String> {
    let normalized = normalize_provider_id(raw)?;
    let canonical = match normalized.as_str() {
        "openai-compatible" | "openai_compatible" | "openai-proxy" | "openai-gateway"
        | "gateway" | "proxy" => "openai",
        _ => normalized.as_str(),
    };
    Some(canonical.to_string())
}

fn resolve_api_key_provider_api(provider_id: &str) -> &'static str {
    match provider_id {
        "anthropic" => "anthropic-messages",
        _ => "openai-completions",
    }
}

fn normalize_api_key_base_url(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }

    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else if trimmed.starts_with("localhost")
        || trimmed.starts_with("127.0.0.1")
        || trimmed.starts_with("[::1]")
    {
        format!("http://{}", trimmed)
    } else {
        format!("https://{}", trimmed)
    };

    let normalized = with_scheme.trim_end_matches('/').to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn resolve_api_key_default_model(provider_id: &str) -> &'static str {
    match provider_id {
        "anthropic" => "anthropic/claude-sonnet-4-5",
        _ => "openai/gpt-5-mini",
    }
}

fn normalize_anthropic_model_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let lowered = trimmed.to_ascii_lowercase();
    match lowered.as_str() {
        "opus-4.5" => "claude-opus-4-5".to_string(),
        "sonnet-4.5" => "claude-sonnet-4-5".to_string(),
        _ => trimmed.to_string(),
    }
}

fn normalize_api_key_model_ref(raw: Option<&str>, provider_id: &str) -> String {
    let Some(trimmed) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return resolve_api_key_default_model(provider_id).to_string();
    };

    if let Some((provider_raw, model_raw)) = trimmed.split_once('/') {
        let provider = provider_raw.trim().to_ascii_lowercase();
        let model_raw = model_raw.trim();
        if provider.is_empty() || model_raw.is_empty() {
            return resolve_api_key_default_model(provider_id).to_string();
        }
        let model = if provider == "anthropic" {
            normalize_anthropic_model_id(model_raw)
        } else {
            model_raw.to_string()
        };
        return format!("{}/{}", provider, model);
    }

    let model = if provider_id == "anthropic" {
        normalize_anthropic_model_id(trimmed)
    } else {
        trimmed.to_string()
    };
    format!("{}/{}", provider_id, model)
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

fn path_to_string(path: &PathBuf) -> String {
    path.to_string_lossy().to_string()
}

fn append_text_once(path: &PathBuf, marker: &str, snippet: &str) -> Result<bool, String> {
    let mut content = if path.exists() {
        fs::read_to_string(path).map_err(|err| err.to_string())?
    } else {
        String::new()
    };
    if content.contains(marker) {
        return Ok(false);
    }
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(snippet);
    fs::write(path, content).map_err(|err| err.to_string())?;
    Ok(true)
}

fn ensure_unix_shell_path_config(home: &PathBuf, logs: &mut Vec<String>) -> Result<(), String> {
    let marker = "# >>> openclaw-desktop cli >>>";
    let snippet = "\
# >>> openclaw-desktop cli >>>\n\
export PATH=\"$HOME/.openclaw/bin:$HOME/.local/bin:$PATH\"\n\
# <<< openclaw-desktop cli <<<\n";
    let rc_files = [".zprofile", ".zshrc", ".bash_profile", ".bashrc", ".profile"];
    let mut touched: Vec<String> = Vec::new();
    for rc in rc_files {
        let rc_path = home.join(rc);
        match append_text_once(&rc_path, marker, snippet) {
            Ok(true) => touched.push(path_to_string(&rc_path)),
            Ok(false) => {}
            Err(error) => logs.push(format!(
                "WARN: failed to update shell profile {}: {}",
                rc, error
            )),
        }
    }
    if !touched.is_empty() {
        logs.push(format!(
            "Updated shell PATH profiles: {}",
            touched.join(", ")
        ));
    }
    Ok(())
}

fn ensure_windows_user_path_contains(local_bin: &PathBuf) -> Result<(), String> {
    let local_bin_str = path_to_string(local_bin).replace('\'', "''");
    let script = format!(
        "$localBin = '{}'\n\
         $current = [Environment]::GetEnvironmentVariable('Path','User')\n\
         if ([string]::IsNullOrWhiteSpace($current)) {{\n\
           [Environment]::SetEnvironmentVariable('Path', $localBin, 'User')\n\
         }} else {{\n\
           $parts = $current -split ';' | ForEach-Object {{ $_.Trim() }} | Where-Object {{ $_ -ne '' }}\n\
           $exists = $parts | Where-Object {{ $_.ToLowerInvariant() -eq $localBin.ToLowerInvariant() }}\n\
           if (-not $exists) {{\n\
             [Environment]::SetEnvironmentVariable('Path', \"$localBin;\" + $current, 'User')\n\
           }}\n\
         }}\n",
        local_bin_str
    );
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output()
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "failed to update user PATH: {}",
            summarize_output(&output.stdout, &output.stderr)
        ))
    }
}

fn ensure_openclaw_terminal_command(
    app: &tauri::AppHandle,
    logs: &mut Vec<String>,
    resolved_binary: &str,
) -> Result<(), String> {
    if let Ok((true, _)) = run_command("openclaw", &["--version"]) {
        push_bootstrap_log(app, logs, "Terminal command already available: openclaw");
        return Ok(());
    }

    let Some(home) = resolve_user_home() else {
        return Err("Cannot resolve user home path for terminal launcher.".to_string());
    };
    let local_bin = home.join(".local").join("bin");
    fs::create_dir_all(&local_bin).map_err(|err| err.to_string())?;

    if cfg!(target_os = "windows") {
        let launcher = local_bin.join("openclaw.cmd");
        let mut script = String::new();
        script.push_str("@echo off\r\n");
        script.push_str("setlocal\r\n");
        script.push_str("set \"TARGET=%USERPROFILE%\\.openclaw\\bin\\openclaw.cmd\"\r\n");
        script.push_str("if exist \"%TARGET%\" (\r\n");
        script.push_str("  call \"%TARGET%\" %*\r\n");
        script.push_str("  exit /b %ERRORLEVEL%\r\n");
        script.push_str(")\r\n");
        script.push_str("set \"TARGET=%USERPROFILE%\\.openclaw\\bin\\openclaw.exe\"\r\n");
        script.push_str("if exist \"%TARGET%\" (\r\n");
        script.push_str("  \"%TARGET%\" %*\r\n");
        script.push_str("  exit /b %ERRORLEVEL%\r\n");
        script.push_str(")\r\n");
        let fallback = PathBuf::from(resolved_binary);
        if fallback.is_absolute() {
            script.push_str(&format!("\"{}\" %*\r\n", fallback.to_string_lossy()));
            script.push_str("exit /b %ERRORLEVEL%\r\n");
        } else {
            script.push_str("echo OpenClaw CLI not found under %USERPROFILE%\\.openclaw\\bin.\r\n");
            script.push_str("exit /b 1\r\n");
        }
        fs::write(&launcher, script).map_err(|err| err.to_string())?;
        ensure_windows_user_path_contains(&local_bin)?;
        push_bootstrap_log(
            app,
            logs,
            format!(
                "Prepared terminal launcher at {} and updated user PATH.",
                launcher.to_string_lossy()
            ),
        );
        match run_command(&launcher.to_string_lossy(), &["--version"]) {
            Ok((true, _)) => push_bootstrap_log(app, logs, "CLI launcher validation passed."),
            Ok((false, output)) => push_bootstrap_log(
                app,
                logs,
                format!(
                    "WARN: CLI launcher validation failed: {}",
                    if output.is_empty() { "no output" } else { &output }
                ),
            ),
            Err(error) => push_bootstrap_log(
                app,
                logs,
                format!("WARN: CLI launcher validation failed: {}", error),
            ),
        }
    } else {
        let launcher = local_bin.join("openclaw");
        let mut script = String::new();
        script.push_str("#!/bin/sh\n");
        script.push_str("TARGET=\"$HOME/.openclaw/bin/openclaw\"\n");
        script.push_str("if [ -x \"$TARGET\" ]; then\n");
        script.push_str("  exec \"$TARGET\" \"$@\"\n");
        script.push_str("fi\n");
        let fallback = PathBuf::from(resolved_binary);
        if fallback.is_absolute() {
            script.push_str(&format!("exec \"{}\" \"$@\"\n", fallback.to_string_lossy()));
        } else {
            script.push_str("echo \"OpenClaw CLI not found under $HOME/.openclaw/bin\" >&2\n");
            script.push_str("exit 1\n");
        }
        fs::write(&launcher, script).map_err(|err| err.to_string())?;
        #[cfg(unix)]
        {
            fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755))
                .map_err(|err| err.to_string())?;
        }
        ensure_unix_shell_path_config(&home, logs)?;
        push_bootstrap_log(
            app,
            logs,
            format!(
                "Prepared terminal launcher at {} and synced shell PATH config.",
                launcher.to_string_lossy()
            ),
        );
        match run_command(&launcher.to_string_lossy(), &["--version"]) {
            Ok((true, _)) => push_bootstrap_log(app, logs, "CLI launcher validation passed."),
            Ok((false, output)) => push_bootstrap_log(
                app,
                logs,
                format!(
                    "WARN: CLI launcher validation failed: {}",
                    if output.is_empty() { "no output" } else { &output }
                ),
            ),
            Err(error) => push_bootstrap_log(
                app,
                logs,
                format!("WARN: CLI launcher validation failed: {}", error),
            ),
        }
    }

    match run_command("openclaw", &["--version"]) {
        Ok((true, _)) => push_bootstrap_log(app, logs, "Terminal command ready: openclaw"),
        _ => push_bootstrap_log(
            app,
            logs,
            "Terminal PATH may need refresh; reopen terminal to use `openclaw`.",
        ),
    }

    Ok(())
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
            candidates.push(exe_dir.join("bundle").join("resources").join("openclaw-bundle"));
            candidates.push(exe_dir.join("resources").join("openclaw-bundle"));
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
            // Avoid canonicalize() on Windows because it may return a verbatim path (\\?\C:\...),
            // and some native tools (e.g. robocopy) interpret that as a UNC path and fail.
            return Some(candidate);
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn windows_robocopy_path_arg(path: &PathBuf) -> String {
    // robocopy treats \\?\C:\... as a UNC/network path and fails with ERROR 53.
    // Strip the verbatim prefix for compatibility.
    let raw = path.to_string_lossy();
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{}", rest)
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        raw.to_string()
    }
}

#[cfg(not(target_os = "windows"))]
fn windows_robocopy_path_arg(path: &PathBuf) -> String {
    path.to_string_lossy().to_string()
}

fn remove_dir_all_native(dir: &PathBuf) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }

    match fs::remove_dir_all(dir) {
        Ok(_) => Ok(()),
        Err(error) => {
            if !cfg!(target_os = "windows") {
                return Err(error.to_string());
            }

            let output = Command::new("cmd")
                .args(["/C", "rmdir", "/S", "/Q"])
                .arg(dir)
                .output()
                .map_err(|err| format!("Failed to run rmdir fallback: {}", err))?;
            if output.status.success() {
                Ok(())
            } else {
                let detail = summarize_output(&output.stdout, &output.stderr);
                Err(if detail.is_empty() {
                    error.to_string()
                } else {
                    format!("{} | rmdir fallback failed: {}", error, detail)
                })
            }
        }
    }
}

fn copy_dir_with_native_tool(src: &PathBuf, dst: &PathBuf) -> Result<(), String> {
    if dst.exists() {
        remove_dir_all_native(dst)?;
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    if cfg!(target_os = "windows") {
        // Ensure destination exists: Copy-Item would create it, and robocopy behaves better when it exists.
        fs::create_dir_all(dst).map_err(|err| err.to_string())?;

        let robocopy_output = Command::new("robocopy")
            .arg(windows_robocopy_path_arg(src))
            .arg(windows_robocopy_path_arg(dst))
            .args([
                "/E",
                "/R:2",
                "/W:2",
                "/NFL",
                "/NDL",
                "/NJH",
                "/NJS",
                "/NP",
            ])
            .output();

        let mut robocopy_error: Option<String> = None;
        match robocopy_output {
            Ok(output) => {
                let exit_code = output.status.code().unwrap_or(16);
                if exit_code < 8 {
                    return Ok(());
                }
                robocopy_error = Some(format!(
                    "robocopy failed with exit code {}: {}",
                    exit_code,
                    summarize_output(&output.stdout, &output.stderr)
                ));
            }
            Err(err) => {
                robocopy_error = Some(format!("robocopy spawn failed: {}", err));
            }
        }

        // Fallback to PowerShell Copy-Item. This is slower and may be less tolerant for long paths,
        // but it avoids certain robocopy incompatibilities (e.g. verbatim paths).
        let src_escaped = src.to_string_lossy().replace('\'', "''");
        let dst_escaped = dst.to_string_lossy().replace('\'', "''");
        let script = format!(
            "$ErrorActionPreference='Stop'; Copy-Item -LiteralPath '{}' -Destination '{}' -Recurse -Force",
            src_escaped, dst_escaped
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
            .output()
            .map_err(|err| err.to_string())?;
        if output.status.success() {
            return Ok(());
        }
        let detail = summarize_output(&output.stdout, &output.stderr);
        return Err(format!(
            "{} | Copy-Item failed: {}",
            robocopy_error.unwrap_or_else(|| "robocopy failed (unknown)".to_string()),
            if detail.is_empty() { "no output".to_string() } else { detail }
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

fn bundle_payload_usable(bundle_dir: &PathBuf) -> bool {
    let prepared_prefix = bundle_dir.join("prefix");
    if prepared_prefix.exists() {
        return true;
    }

    let Some(_node_bin) = resolve_bundled_node_binary(bundle_dir) else {
        return false;
    };
    let npm_cli = bundle_dir.join("npm").join("bin").join("npm-cli.js");
    let openclaw_tgz = bundle_dir.join("openclaw.tgz");
    let npm_cache = bundle_dir.join("npm-cache");
    npm_cli.exists() && openclaw_tgz.exists() && npm_cache.exists()
}

fn find_openclaw_bundle_dir(root: &PathBuf) -> Option<PathBuf> {
    let mut stack = vec![root.clone()];
    while let Some(current) = stack.pop() {
        let Ok(read_dir) = fs::read_dir(&current) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if !meta.is_dir() {
                continue;
            }
            let is_openclaw_bundle = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name == "openclaw-bundle")
                .unwrap_or(false);
            if is_openclaw_bundle {
                return Some(path);
            }
            stack.push(path);
        }
    }
    None
}

fn resolve_openclaw_bundle_dir_from_extracted_root(extract_root: &PathBuf) -> Option<PathBuf> {
    let common_path = extract_root
        .join("bundle")
        .join("resources")
        .join("openclaw-bundle");
    if common_path.exists() {
        return Some(common_path);
    }
    find_openclaw_bundle_dir(extract_root)
}

fn download_url_to_file_native(url: &str, out_file: &PathBuf) -> Result<(), String> {
    if let Some(parent) = out_file.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let temp_file = out_file.with_extension("download");
    let _ = fs::remove_file(&temp_file);

    let curl = Command::new("curl")
        .args([
            "-L",
            "--fail",
            "--retry",
            "3",
            "--retry-all-errors",
            "--connect-timeout",
            "30",
            "-o",
        ])
        .arg(&temp_file)
        .arg(url)
        .output();

    let mut curl_error: Option<String> = None;
    match curl {
        Ok(output) => {
            if output.status.success() {
                let _ = fs::remove_file(out_file);
                fs::rename(&temp_file, out_file).map_err(|err| err.to_string())?;
                return Ok(());
            }
            let detail = summarize_output(&output.stdout, &output.stderr);
            curl_error = Some(if detail.is_empty() {
                "curl failed with no output".to_string()
            } else {
                format!("curl failed: {}", detail)
            });
        }
        Err(error) => {
            curl_error = Some(format!("curl unavailable: {}", error));
        }
    }

    let url_escaped = url.replace('\'', "''");
    let out_escaped = temp_file.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop'; \
         [Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; \
         $cmd = Get-Command Invoke-WebRequest -ErrorAction Stop; \
         $hasBasic = $cmd.Parameters.ContainsKey('UseBasicParsing'); \
         if ($hasBasic) {{ Invoke-WebRequest -UseBasicParsing -Uri '{url}' -OutFile '{out}'; }} \
         else {{ Invoke-WebRequest -Uri '{url}' -OutFile '{out}'; }}",
        url = url_escaped,
        out = out_escaped
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .output()
        .map_err(|err| format!("Failed to run download script: {}", err))?;

    if output.status.success() {
        let _ = fs::remove_file(out_file);
        fs::rename(&temp_file, out_file).map_err(|err| err.to_string())?;
        return Ok(());
    }

    let ps_detail = summarize_output(&output.stdout, &output.stderr);
    let ps_error = if ps_detail.is_empty() {
        "powershell download failed with no output".to_string()
    } else {
        format!("powershell download failed: {}", ps_detail)
    };

    Err(
        [
            "Download failed.".to_string(),
            curl_error.unwrap_or_else(|| "curl failed (unknown)".to_string()),
            ps_error,
        ]
        .join(" "),
    )
}

fn extract_zip_to_dir_native(zip_path: &PathBuf, extract_root: &PathBuf) -> Result<(), String> {
    if extract_root.exists() {
        remove_dir_all_native(extract_root)?;
    }
    fs::create_dir_all(extract_root).map_err(|err| err.to_string())?;

    let tar = Command::new("tar")
        .arg("-xf")
        .arg(zip_path)
        .arg("-C")
        .arg(extract_root)
        .output();

    let mut tar_error: Option<String> = None;
    match tar {
        Ok(output) => {
            if output.status.success() {
                return Ok(());
            }
            let detail = summarize_output(&output.stdout, &output.stderr);
            tar_error = Some(if detail.is_empty() {
                "tar failed with no output".to_string()
            } else {
                format!("tar failed: {}", detail)
            });
        }
        Err(error) => {
            tar_error = Some(format!("tar unavailable: {}", error));
        }
    }

    let zip_escaped = zip_path.to_string_lossy().replace('\'', "''");
    let extract_escaped = extract_root.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop'; Expand-Archive -LiteralPath '{zip}' -DestinationPath '{extract}';",
        zip = zip_escaped,
        extract = extract_escaped
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .output()
        .map_err(|err| format!("Failed to run zip extract script: {}", err))?;

    if output.status.success() {
        return Ok(());
    }

    let ps_detail = summarize_output(&output.stdout, &output.stderr);
    let ps_error = if ps_detail.is_empty() {
        "Expand-Archive failed with no output".to_string()
    } else {
        format!("Expand-Archive failed: {}", ps_detail)
    };

    Err(
        [
            "Zip extraction failed.".to_string(),
            tar_error.unwrap_or_else(|| "tar failed (unknown)".to_string()),
            ps_error,
        ]
        .join(" "),
    )
}

fn try_prepare_windows_downloaded_bundle(
    app: &tauri::AppHandle,
    logs: &mut Vec<String>,
) -> Result<Option<PathBuf>, String> {
    if !cfg!(target_os = "windows") {
        return Ok(None);
    }

    let cache_root = resolve_openclaw_agent_dir().join("offline-bundle-cache");
    fs::create_dir_all(&cache_root).map_err(|err| err.to_string())?;

    let zip_path = cache_root.join("openclaw-desktop-windows-portable.zip");
    let extract_root = cache_root.join("portable-extract");
    let target_bundle = cache_root.join("openclaw-bundle");

    push_bootstrap_log(
        app,
        logs,
        format!(
            "Bundled payload missing; downloading offline payload from {}",
            OPENCLAW_WINDOWS_PORTABLE_BUNDLE_URL
        ),
    );

    if !zip_path.exists() {
        download_url_to_file_native(OPENCLAW_WINDOWS_PORTABLE_BUNDLE_URL, &zip_path).map_err(
            |err| format!("Portable bundle download failed: {}", err),
        )?;
    } else {
        push_bootstrap_log(
            app,
            logs,
            format!("Using cached portable bundle: {}", zip_path.to_string_lossy()),
        );
    }

    if let Err(error) = extract_zip_to_dir_native(&zip_path, &extract_root) {
        push_bootstrap_log(app, logs, format!("WARN: {}", error));
        let _ = fs::remove_file(&zip_path);
        download_url_to_file_native(OPENCLAW_WINDOWS_PORTABLE_BUNDLE_URL, &zip_path).map_err(
            |err| format!("Portable bundle download failed: {}", err),
        )?;
        extract_zip_to_dir_native(&zip_path, &extract_root)
            .map_err(|err| format!("Portable bundle extraction failed: {}", err))?;
    }

    let Some(found_bundle) = resolve_openclaw_bundle_dir_from_extracted_root(&extract_root) else {
        push_bootstrap_log(
            app,
            logs,
            "Downloaded portable package does not contain openclaw-bundle directory.",
        );
        return Ok(None);
    };

    if target_bundle.exists() {
        remove_dir_all_native(&target_bundle)?;
    }
    match fs::rename(&found_bundle, &target_bundle) {
        Ok(_) => {}
        Err(_) => {
            copy_dir_with_native_tool(&found_bundle, &target_bundle)?;
        }
    }
    let _ = remove_dir_all_native(&extract_root);
    push_bootstrap_log(
        app,
        logs,
        format!(
            "Offline payload downloaded to {}",
            target_bundle.to_string_lossy()
        ),
    );
    Ok(Some(target_bundle))
}

fn try_prepare_windows_bundle_from_selected_zip(
    app: &tauri::AppHandle,
    logs: &mut Vec<String>,
    selected_zip: &PathBuf,
) -> Result<Option<PathBuf>, String> {
    if !cfg!(target_os = "windows") {
        return Ok(None);
    }

    if !selected_zip.exists() {
        return Err(format!(
            "Selected bundle file does not exist: {}",
            selected_zip.to_string_lossy()
        ));
    }

    let cache_root = resolve_openclaw_agent_dir().join("offline-bundle-cache");
    fs::create_dir_all(&cache_root).map_err(|err| err.to_string())?;

    let extract_root = cache_root.join("portable-extract-manual");
    let target_bundle = cache_root.join("openclaw-bundle");

    push_bootstrap_log(
        app,
        logs,
        format!(
            "Using selected portable bundle file: {}",
            selected_zip.to_string_lossy()
        ),
    );

    let extract_error = match extract_zip_to_dir_native(selected_zip, &extract_root) {
        Ok(_) => None,
        Err(error) => {
            push_bootstrap_log(app, logs, format!("WARN: {}", error));
            Some(error)
        }
    };

    let Some(found_bundle) = resolve_openclaw_bundle_dir_from_extracted_root(&extract_root) else {
        if let Some(error) = extract_error {
            return Err(format!("Selected portable bundle extraction failed: {}", error));
        }
        push_bootstrap_log(
            app,
            logs,
            "Selected portable package does not contain openclaw-bundle directory.",
        );
        return Ok(None);
    };

    if target_bundle.exists() {
        remove_dir_all_native(&target_bundle)?;
    }
    match fs::rename(&found_bundle, &target_bundle) {
        Ok(_) => {}
        Err(_) => {
            copy_dir_with_native_tool(&found_bundle, &target_bundle)?;
        }
    }
    let _ = remove_dir_all_native(&extract_root);
    push_bootstrap_log(
        app,
        logs,
        format!(
            "Selected offline payload extracted to {}",
            target_bundle.to_string_lossy()
        ),
    );
    Ok(Some(target_bundle))
}

fn install_openclaw_from_bundle_dir(
    app: &tauri::AppHandle,
    logs: &mut Vec<String>,
    bundle_dir: &PathBuf,
) -> Result<bool, String> {
    if !bundle_payload_usable(bundle_dir) {
        push_bootstrap_log(app, logs, "Bundled payload is incomplete; skip offline install.");
        return Ok(false);
    }

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Cannot resolve user home path for offline install".to_string())?;
    let prefix = PathBuf::from(home).join(".openclaw");
    fs::create_dir_all(&prefix).map_err(|err| err.to_string())?;

    let prepared_prefix = bundle_dir.join("prefix");
    if prepared_prefix.exists() {
        push_bootstrap_log(app, logs, "Installing OpenClaw from bundled prefix snapshot...");
        copy_dir_with_native_tool(&prepared_prefix, &prefix)?;
        if let Err(error) = ensure_prefix_openclaw_launcher(&prefix, bundle_dir, logs) {
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

    let Some(node_bin) = resolve_bundled_node_binary(bundle_dir) else {
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
        if let Err(error) = ensure_prefix_openclaw_launcher(&prefix, bundle_dir, logs) {
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

fn install_openclaw_from_bundle(
    app: &tauri::AppHandle,
    logs: &mut Vec<String>,
) -> Result<bool, String> {
    let mut bundle_dir = if let Some(found) = resolve_bundled_openclaw_dir(app) {
        found
    } else if let Some(downloaded) = try_prepare_windows_downloaded_bundle(app, logs)? {
        downloaded
    } else {
        push_bootstrap_log(
            app,
            logs,
            "No bundled OpenClaw payload found in installer resources.",
        );
        return Ok(false);
    };

    if !bundle_payload_usable(&bundle_dir) {
        push_bootstrap_log(
            app,
            logs,
            "Bundled payload is incomplete; trying downloadable offline payload.",
        );
        if let Some(downloaded) = try_prepare_windows_downloaded_bundle(app, logs)? {
            bundle_dir = downloaded;
        }
    }
    install_openclaw_from_bundle_dir(app, logs, &bundle_dir)
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

fn normalize_ollama_endpoint(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "http://127.0.0.1:11434".to_string();
    }
    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{}", trimmed)
    };
    with_scheme.trim_end_matches('/').to_string()
}

fn normalize_ollama_model_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(stripped) = trimmed.strip_prefix("ollama/") {
        let normalized = stripped.trim();
        if normalized.is_empty() {
            None
        } else {
            Some(normalized.to_string())
        }
    } else {
        Some(trimmed.to_string())
    }
}

async fn fetch_ollama_status(endpoint: &str) -> Result<OllamaStatus, String> {
    let url = format!("{}/api/tags", endpoint);
    let response = reqwest::get(url).await.map_err(|err| err.to_string())?;
    let status = response.status();

    if !status.is_success() {
        return Ok(OllamaStatus {
            endpoint: endpoint.to_string(),
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
        endpoint: endpoint.to_string(),
        reachable: true,
        models,
        error: None,
    })
}

#[tauri::command]
async fn check_ollama(endpoint: Option<String>) -> Result<OllamaStatus, String> {
    let endpoint = normalize_ollama_endpoint(endpoint.as_deref().unwrap_or_default());
    fetch_ollama_status(&endpoint).await
}

#[tauri::command]
async fn apply_ollama_config(
    endpoint: Option<String>,
    preferred_model: Option<String>,
) -> Result<OllamaApplyResult, String> {
    let endpoint = normalize_ollama_endpoint(endpoint.as_deref().unwrap_or_default());
    let status = fetch_ollama_status(&endpoint).await?;

    if !status.reachable {
        return Err(
            status
                .error
                .unwrap_or_else(|| "Ollama endpoint is not reachable.".to_string()),
        );
    }

    let discovered_models = status.models;
    let selected_model_name = preferred_model
        .as_deref()
        .and_then(normalize_ollama_model_name)
        .or_else(|| {
            discovered_models
                .first()
                .and_then(|name| normalize_ollama_model_name(name))
        })
        .ok_or_else(|| "No Ollama model found. Run `ollama pull <model>` first.".to_string())?;

    let selected_model_id = format!("ollama/{}", selected_model_name);

    let mut config_value = load_openclaw_config_value();
    if !config_value.is_object() {
        config_value = serde_json::json!({});
    }

    let config_obj = config_value
        .as_object_mut()
        .ok_or_else(|| "Failed to parse OpenClaw config root object.".to_string())?;

    let models_entry = config_obj
        .entry("models".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !models_entry.is_object() {
        *models_entry = serde_json::json!({});
    }
    let models_obj = models_entry
        .as_object_mut()
        .ok_or_else(|| "Failed to parse OpenClaw config models object.".to_string())?;

    let providers_entry = models_obj
        .entry("providers".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !providers_entry.is_object() {
        *providers_entry = serde_json::json!({});
    }
    let providers_obj = providers_entry
        .as_object_mut()
        .ok_or_else(|| "Failed to parse OpenClaw config models.providers object.".to_string())?;
    providers_obj.insert(
        "ollama".to_string(),
        serde_json::json!({
            "api": "ollama",
            "baseUrl": endpoint.clone(),
            "apiKey": "ollama"
        }),
    );

    let retained_models = models_obj
        .get("models")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    let provider = item.get("provider").and_then(|value| value.as_str());
                    let model_id = item.get("id").and_then(|value| value.as_str()).unwrap_or_default();
                    provider != Some("ollama") && !model_id.starts_with("ollama/")
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut merged_models = retained_models;
    merged_models.extend(
        discovered_models
            .iter()
            .filter_map(|name| normalize_ollama_model_name(name))
            .map(|name| {
                serde_json::json!({
                    "id": format!("ollama/{}", name),
                    "name": name,
                    "provider": "ollama",
                    "reasoning": false,
                    "input": ["text"],
                    "cost": {
                        "input": 0,
                        "output": 0
                    },
                    "contextWindow": 8192,
                    "maxTokens": 8192
                })
            }),
    );
    models_obj.insert("models".to_string(), serde_json::Value::Array(merged_models));

    let agents_entry = config_obj
        .entry("agents".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !agents_entry.is_object() {
        *agents_entry = serde_json::json!({});
    }
    let agents_obj = agents_entry
        .as_object_mut()
        .ok_or_else(|| "Failed to parse OpenClaw config agents object.".to_string())?;

    let defaults_entry = agents_obj
        .entry("defaults".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !defaults_entry.is_object() {
        *defaults_entry = serde_json::json!({});
    }
    let defaults_obj = defaults_entry
        .as_object_mut()
        .ok_or_else(|| "Failed to parse OpenClaw config agents.defaults object.".to_string())?;

    let model_entry = defaults_obj
        .entry("model".to_string())
        .or_insert_with(|| serde_json::json!({}));
    match model_entry {
        serde_json::Value::Object(model_obj) => {
            model_obj.insert(
                "primary".to_string(),
                serde_json::json!(selected_model_id.clone()),
            );
        }
        _ => {
            *model_entry = serde_json::json!({
                "primary": selected_model_id
            });
        }
    }

    save_openclaw_config_value(&config_value)?;

    Ok(OllamaApplyResult {
        endpoint,
        model: format!("ollama/{}", selected_model_name),
        discovered_models,
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
    if let Err(error) = ensure_openclaw_terminal_command(&app, &mut logs, &binary) {
        push_bootstrap_log(
            &app,
            &mut logs,
            format!("WARN: failed to prepare terminal `openclaw` command: {}", error),
        );
    }
    if let Err(error) = ensure_browser_defaults(&app, &mut logs) {
        push_bootstrap_log(
            &app,
            &mut logs,
            format!("WARN: failed to ensure browser defaults: {}", error),
        );
    }
    // Only install browser relay extension when using chrome profile mode (relay is not needed for openclaw profile mode)
    {
        let config_value = load_openclaw_config_value();
        let default_profile = config_value
            .pointer("/browser/defaultProfile")
            .and_then(|v| v.as_str())
            .unwrap_or("openclaw");
        if default_profile.eq_ignore_ascii_case("chrome") {
            ensure_browser_relay_installed(&app, &binary, &mut logs);
        } else {
            push_bootstrap_log(&app, &mut logs, "Browser relay skipped (profile mode, relay not needed).");
        }
    }

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
fn select_windows_portable_bundle_file() -> Result<Option<String>, String> {
    if !cfg!(target_os = "windows") {
        return Ok(None);
    }

    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.Title = 'Select openclaw-desktop portable zip'
$dialog.Filter = 'Portable Zip (*.zip)|*.zip|All Files (*.*)|*.*'
$dialog.Multiselect = $false
$dialog.CheckFileExists = $true
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  [Console]::Out.Write($dialog.FileName)
}
"#;

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-STA",
            "-Command",
            script,
        ])
        .output()
        .map_err(|err| format!("Failed to open bundle selector dialog: {}", err))?;
    if !output.status.success() {
        let detail = summarize_output(&output.stdout, &output.stderr);
        return Err(if detail.is_empty() {
            "Bundle selector dialog failed.".to_string()
        } else {
            format!("Bundle selector dialog failed: {}", detail)
        });
    }

    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if selected.is_empty() {
        Ok(None)
    } else {
        Ok(Some(selected))
    }
}

#[tauri::command]
async fn bootstrap_openclaw_with_selected_bundle(
    app: tauri::AppHandle,
    bundle_zip_path: String,
) -> BootstrapStatus {
    let mut pre_logs: Vec<String> = Vec::new();
    push_bootstrap_log(&app, &mut pre_logs, "Manual portable install started.");

    if !cfg!(target_os = "windows") {
        return BootstrapStatus {
            ready: false,
            installed: false,
            initialized: false,
            web: OfficialWebStatus {
                ready: false,
                installed: false,
                running: false,
                started: false,
                url: OFFICIAL_WEB_URL.to_string(),
                command_hint: "openclaw gateway".to_string(),
                message: "Manual portable install is only supported on Windows.".to_string(),
                error: Some("unsupported platform".to_string()),
            },
            message: "Manual portable install is only supported on Windows.".to_string(),
            logs: pre_logs,
            error: Some("unsupported platform".to_string()),
        };
    }

    let selected = bundle_zip_path.trim();
    if selected.is_empty() {
        return BootstrapStatus {
            ready: false,
            installed: false,
            initialized: false,
            web: OfficialWebStatus {
                ready: false,
                installed: false,
                running: false,
                started: false,
                url: OFFICIAL_WEB_URL.to_string(),
                command_hint: "openclaw gateway".to_string(),
                message: "No portable bundle file selected.".to_string(),
                error: Some("empty bundle path".to_string()),
            },
            message: "No portable bundle file selected.".to_string(),
            logs: pre_logs,
            error: Some("empty bundle path".to_string()),
        };
    }

    let selected_path = PathBuf::from(selected);
    let prepared_bundle =
        match try_prepare_windows_bundle_from_selected_zip(&app, &mut pre_logs, &selected_path) {
            Ok(Some(bundle)) => bundle,
            Ok(None) => {
                return BootstrapStatus {
                    ready: false,
                    installed: false,
                    initialized: false,
                    web: OfficialWebStatus {
                        ready: false,
                        installed: false,
                        running: false,
                        started: false,
                        url: OFFICIAL_WEB_URL.to_string(),
                        command_hint: "openclaw gateway".to_string(),
                        message: "Selected file does not contain openclaw-bundle payload."
                            .to_string(),
                        error: Some("bundle payload missing".to_string()),
                    },
                    message: "Selected file does not contain openclaw-bundle payload.".to_string(),
                    logs: pre_logs,
                    error: Some("bundle payload missing".to_string()),
                };
            }
            Err(error) => {
                return BootstrapStatus {
                    ready: false,
                    installed: false,
                    initialized: false,
                    web: OfficialWebStatus {
                        ready: false,
                        installed: false,
                        running: false,
                        started: false,
                        url: OFFICIAL_WEB_URL.to_string(),
                        command_hint: "openclaw gateway".to_string(),
                        message: "Failed to prepare selected bundle.".to_string(),
                        error: Some(error.clone()),
                    },
                    message: "Failed to prepare selected bundle.".to_string(),
                    logs: pre_logs,
                    error: Some(error),
                };
            }
        };

    match install_openclaw_from_bundle_dir(&app, &mut pre_logs, &prepared_bundle) {
        Ok(true) => {
            push_bootstrap_log(
                &app,
                &mut pre_logs,
                "Manual portable payload installed, continuing bootstrap.",
            );
        }
        Ok(false) => {
            return BootstrapStatus {
                ready: false,
                installed: false,
                initialized: false,
                web: OfficialWebStatus {
                    ready: false,
                    installed: false,
                    running: false,
                    started: false,
                    url: OFFICIAL_WEB_URL.to_string(),
                    command_hint: "openclaw gateway".to_string(),
                    message: "Selected bundle payload is incomplete.".to_string(),
                    error: Some("bundle payload incomplete".to_string()),
                },
                message: "Selected bundle payload is incomplete.".to_string(),
                logs: pre_logs,
                error: Some("bundle payload incomplete".to_string()),
            };
        }
        Err(error) => {
            return BootstrapStatus {
                ready: false,
                installed: false,
                initialized: false,
                web: OfficialWebStatus {
                    ready: false,
                    installed: false,
                    running: false,
                    started: false,
                    url: OFFICIAL_WEB_URL.to_string(),
                    command_hint: "openclaw gateway".to_string(),
                    message: "Failed to install selected bundle payload.".to_string(),
                    error: Some(error.clone()),
                },
                message: "Failed to install selected bundle payload.".to_string(),
                logs: pre_logs,
                error: Some(error),
            };
        }
    }

    let mut status = bootstrap_openclaw(app.clone()).await;
    let mut merged_logs = pre_logs;
    merged_logs.append(&mut status.logs);
    status.logs = merged_logs;
    status
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
fn save_api_key(
    provider_id: String,
    api_key: String,
    base_url: Option<String>,
    default_model: Option<String>,
) -> Result<serde_json::Value, String> {
    let normalized_provider_id = normalize_api_key_provider_id(&provider_id)
        .ok_or_else(|| "provider_id is required".to_string())?;
    if normalized_provider_id.trim().is_empty() {
        return Err("provider_id is required".to_string());
    }
    let normalized_api_key = api_key.trim();
    if normalized_api_key.is_empty() {
        return Err("api_key is required".to_string());
    }

    let normalized_base_url = normalize_api_key_base_url(base_url.as_deref());
    let normalized_default_model =
        normalize_api_key_model_ref(default_model.as_deref(), &normalized_provider_id);
    let provider_api = resolve_api_key_provider_api(&normalized_provider_id);

    let mut config_value = load_openclaw_config_value();
    if !config_value.is_object() {
        config_value = serde_json::json!({});
    }

    let config_obj = config_value
        .as_object_mut()
        .ok_or_else(|| "Failed to parse OpenClaw config root object.".to_string())?;

    let models_entry = config_obj
        .entry("models".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !models_entry.is_object() {
        *models_entry = serde_json::json!({});
    }
    let models_obj = models_entry
        .as_object_mut()
        .ok_or_else(|| "Failed to parse OpenClaw config models object.".to_string())?;

    let providers_entry = models_obj
        .entry("providers".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !providers_entry.is_object() {
        *providers_entry = serde_json::json!({});
    }
    let providers_obj = providers_entry
        .as_object_mut()
        .ok_or_else(|| "Failed to parse OpenClaw config models.providers object.".to_string())?;

    let existing_provider_obj = providers_obj
        .get(&normalized_provider_id)
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();
    let mut next_provider_obj = existing_provider_obj;
    next_provider_obj.insert("api".to_string(), serde_json::json!(provider_api));
    next_provider_obj.insert("apiKey".to_string(), serde_json::json!(normalized_api_key));
    if let Some(value) = &normalized_base_url {
        next_provider_obj.insert("baseUrl".to_string(), serde_json::json!(value));
    } else {
        next_provider_obj.remove("baseUrl");
    }
    providers_obj.insert(
        normalized_provider_id.clone(),
        serde_json::Value::Object(next_provider_obj),
    );

    let agents_entry = config_obj
        .entry("agents".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !agents_entry.is_object() {
        *agents_entry = serde_json::json!({});
    }
    let agents_obj = agents_entry
        .as_object_mut()
        .ok_or_else(|| "Failed to parse OpenClaw config agents object.".to_string())?;

    let defaults_entry = agents_obj
        .entry("defaults".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !defaults_entry.is_object() {
        *defaults_entry = serde_json::json!({});
    }
    let defaults_obj = defaults_entry
        .as_object_mut()
        .ok_or_else(|| "Failed to parse OpenClaw config agents.defaults object.".to_string())?;

    let model_entry = defaults_obj
        .entry("model".to_string())
        .or_insert_with(|| serde_json::json!({}));
    match model_entry {
        serde_json::Value::Object(model_obj) => {
            model_obj.insert(
                "primary".to_string(),
                serde_json::json!(normalized_default_model.clone()),
            );
        }
        _ => {
            *model_entry = serde_json::json!({
                "primary": normalized_default_model.clone()
            });
        }
    }

    save_openclaw_config_value(&config_value)?;

    Ok(serde_json::json!({
        "ok": true,
        "providerId": normalized_provider_id,
        "api": provider_api,
        "baseUrl": normalized_base_url,
        "defaultModel": normalized_default_model
    }))
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

#[tauri::command]
fn get_feishu_channel_status() -> Result<FeishuChannelStatus, String> {
    let binary = resolve_openclaw_binary()
        .ok_or_else(|| "OpenClaw binary not found.".to_string())?;

    let plugin_installed = match run_command(&binary, &["plugins", "list"]) {
        Ok((_, output)) => output.contains("openclaw-lark"),
        Err(_) => false,
    };

    let config_value = load_openclaw_config_value();
    let channel_enabled = config_value
        .pointer("/channels/feishu/enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let app_id = config_value
        .pointer("/channels/feishu/accounts/main/appId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let has_credentials = !app_id.is_empty();

    Ok(FeishuChannelStatus {
        plugin_installed,
        channel_enabled,
        has_credentials,
        app_id,
        error: None,
    })
}

#[tauri::command]
fn install_feishu_plugin() -> Result<FeishuChannelStatus, String> {
    let binary = resolve_openclaw_binary()
        .ok_or_else(|| "OpenClaw binary not found.".to_string())?;

    // Disable stock feishu plugin (ignore failure)
    let _ = run_command(&binary, &["plugins", "disable", "feishu"]);

    // Install official lark plugin
    match run_command(&binary, &["plugins", "install", "@larksuite/openclaw-lark"]) {
        Ok((true, _)) => {}
        Ok((false, output)) => {
            return Err(format!("Failed to install @larksuite/openclaw-lark: {}", output));
        }
        Err(err) => {
            return Err(format!("Failed to install @larksuite/openclaw-lark: {}", err));
        }
    }

    get_feishu_channel_status()
}

#[tauri::command]
fn save_feishu_channel_config(app_id: String, app_secret: String) -> Result<FeishuChannelStatus, String> {
    let app_id_trimmed = app_id.trim().to_string();
    let app_secret_trimmed = app_secret.trim().to_string();

    if app_id_trimmed.is_empty() || app_secret_trimmed.is_empty() {
        return Err("appId and appSecret must not be empty.".to_string());
    }

    let mut config_value = load_openclaw_config_value();
    if !config_value.is_object() {
        config_value = serde_json::json!({});
    }

    let config_obj = config_value
        .as_object_mut()
        .ok_or_else(|| "Failed to parse OpenClaw config root object.".to_string())?;

    let channels_entry = config_obj
        .entry("channels".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !channels_entry.is_object() {
        *channels_entry = serde_json::json!({});
    }

    let channels_obj = channels_entry
        .as_object_mut()
        .ok_or_else(|| "Failed to parse channels object.".to_string())?;

    channels_obj.insert("feishu".to_string(), serde_json::json!({
        "enabled": true,
        "connectionMode": "webhook",
        "dmPolicy": "allow",
        "groupPolicy": "allow",
        "accounts": {
            "main": {
                "appId": app_id_trimmed,
                "appSecret": app_secret_trimmed
            }
        }
    }));

    save_openclaw_config_value(&config_value)?;
    get_feishu_channel_status()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            list_oauth_providers,
            start_oauth_login,
            check_ollama,
            apply_ollama_config,
            bootstrap_openclaw,
            select_windows_portable_bundle_file,
            bootstrap_openclaw_with_selected_bundle,
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
            validate_local_codex_connectivity,
            get_feishu_channel_status,
            install_feishu_plugin,
            save_feishu_channel_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

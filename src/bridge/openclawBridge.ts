import { invoke } from "@tauri-apps/api/core";
import type {
  BootstrapStatus,
  CodexConnectivityStatus,
  CodexAuthStatus,
  LocalCodexReuseResult,
  LocalOAuthToolStatus,
  OpenOfficialWebResult,
  OfficialWebStatus,
  OAuthLoginResult,
  OAuthProvider,
  OllamaStatus,
  OpenClawBridge
} from "./types";

const fallbackProviders: OAuthProvider[] = [
  { id: "openai-codex", label: "OpenAI Codex" },
  { id: "anthropic", label: "Anthropic (Claude Code)" },
  { id: "github-copilot", label: "GitHub Copilot" },
  { id: "chutes", label: "Chutes" },
  { id: "google-gemini-cli", label: "Google Gemini CLI" },
  { id: "google-antigravity", label: "Google Antigravity" },
  { id: "minimax-portal", label: "MiniMax Portal" },
  { id: "qwen-portal", label: "Qwen Portal" },
  { id: "copilot-proxy", label: "Copilot Proxy" }
];

const fallbackLocalTools: LocalOAuthToolStatus[] = [
  {
    id: "codex",
    label: "OpenAI Codex",
    providerId: "openai-codex",
    cliFound: false,
    authDetected: false,
    source: "~/.codex/auth.json"
  },
  {
    id: "claude-code",
    label: "Claude Code",
    providerId: "anthropic",
    cliFound: false,
    authDetected: false,
    source: "~/.claude/.credentials.json"
  },
  {
    id: "gemini-cli",
    label: "Gemini CLI",
    providerId: "google-gemini-cli",
    cliFound: false,
    authDetected: false,
    source: "gemini"
  }
];

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && typeof window.__TAURI_INTERNALS__ !== "undefined";
}

function toHumanLabel(providerId: string): string {
  return providerId
    .split("-")
    .map((chunk) => chunk.slice(0, 1).toUpperCase() + chunk.slice(1))
    .join(" ");
}

export const openclawBridge: OpenClawBridge = {
  async listOAuthProviders() {
    if (!isTauriRuntime()) {
      return fallbackProviders;
    }

    const providers = await invoke<string[]>("list_oauth_providers");
    return providers.map((id) => ({ id, label: toHumanLabel(id) }));
  },

  async detectLocalOAuthTools() {
    if (!isTauriRuntime()) {
      return fallbackLocalTools;
    }
    return invoke<LocalOAuthToolStatus[]>("detect_local_oauth_tools");
  },

  async startOAuthLogin(providerId: string) {
    if (!isTauriRuntime()) {
      return {
        providerId,
        launched: false,
        commandHint: `openclaw models auth login --provider ${providerId}`,
        details: "Browser runtime: use native app build to trigger login."
      } satisfies OAuthLoginResult;
    }

    return invoke<OAuthLoginResult>("start_oauth_login", { providerId });
  },

  async checkOllama() {
    if (!isTauriRuntime()) {
      const endpoint = "http://127.0.0.1:11434";
      try {
        const response = await fetch(`${endpoint}/api/tags`, { method: "GET" });
        if (!response.ok) {
          return { endpoint, reachable: false, models: [], error: `HTTP ${response.status}` };
        }
        const payload = (await response.json()) as { models?: Array<{ name?: string }> };
        return {
          endpoint,
          reachable: true,
          models: (payload.models ?? []).map((model) => model.name ?? "").filter(Boolean)
        } satisfies OllamaStatus;
      } catch (error) {
        return {
          endpoint,
          reachable: false,
          models: [],
          error: error instanceof Error ? error.message : String(error)
        };
      }
    }

    return invoke<OllamaStatus>("check_ollama");
  },

  async bootstrapOpenClaw() {
    if (!isTauriRuntime()) {
      const url = "http://127.0.0.1:18789/";
      return {
        ready: false,
        installed: false,
        initialized: false,
        web: {
          ready: false,
          installed: false,
          running: false,
          started: false,
          url,
          commandHint: "openclaw gateway",
          message: "Native runtime required"
        },
        message: "Native runtime required",
        logs: ["Bootstrap is only supported in Tauri runtime."],
        error: "Native runtime required"
      } satisfies BootstrapStatus;
    }

    return invoke<BootstrapStatus>("bootstrap_openclaw");
  },

  async ensureOfficialWebReady() {
    const url = "http://127.0.0.1:18789/";

    if (!isTauriRuntime()) {
      try {
        await fetch(url, { method: "GET" });
        return {
          ready: true,
          installed: false,
          running: true,
          started: false,
          url,
          commandHint: "openclaw gateway",
          message: "Official local web is reachable."
        } satisfies OfficialWebStatus;
      } catch (error) {
        return {
          ready: false,
          installed: false,
          running: false,
          started: false,
          url,
          commandHint: "openclaw gateway",
          message: "Official local web is not reachable.",
          error: error instanceof Error ? error.message : String(error)
        } satisfies OfficialWebStatus;
      }
    }

    return invoke<OfficialWebStatus>("ensure_official_web_ready");
  },

  async openOfficialWebWindow() {
    const url = "http://127.0.0.1:18789/";

    if (!isTauriRuntime()) {
      const popup = window.open(url, "_blank", "noopener,noreferrer");
      return {
        opened: Boolean(popup),
        url,
        detail: popup ? "Opened in browser." : "Popup blocked."
      } satisfies OpenOfficialWebResult;
    }

    return invoke<OpenOfficialWebResult>("open_official_web_window");
  },

  async saveApiKey(providerId: string, apiKey: string) {
    if (!isTauriRuntime()) {
      return { ok: providerId.trim().length > 0 && apiKey.trim().length > 0 };
    }
    return invoke<{ ok: boolean }>("save_api_key", { providerId, apiKey });
  },

  async detectLocalCodexAuth() {
    if (!isTauriRuntime()) {
      return {
        detected: false,
        source: "~/.codex/auth.json",
        tokenFields: []
      } satisfies CodexAuthStatus;
    }

    return invoke<CodexAuthStatus>("detect_local_codex_auth");
  },

  async reuseLocalCodexAuth(setDefaultModel = true) {
    if (!isTauriRuntime()) {
      return {
        reused: false,
        message: "Native runtime required"
      } satisfies LocalCodexReuseResult;
    }
    return invoke<LocalCodexReuseResult>("reuse_local_codex_auth", { setDefaultModel });
  },

  async validateLocalCodexConnectivity() {
    if (!isTauriRuntime()) {
      return {
        ok: false,
        expected: "CODEx_OK",
        error: "Native runtime required",
        command: 'codex exec --skip-git-repo-check -o <temp_file> "Reply with exactly: CODEx_OK"'
      } satisfies CodexConnectivityStatus;
    }

    return invoke<CodexConnectivityStatus>("validate_local_codex_connectivity");
  }
};

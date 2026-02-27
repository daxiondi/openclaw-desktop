import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { openclawBridge } from "../../bridge/openclawBridge";
import type {
  CodexAuthStatus,
  CodexConnectivityStatus,
  LocalOAuthToolStatus,
  OAuthProvider,
  OllamaStatus
} from "../../bridge/types";

type Mode = "oauth" | "apikey" | "ollama";

type Props = {
  onStatus: (message: string) => void;
  onLoginSuccess: () => void;
};

const defaultOllamaStatus: OllamaStatus = {
  endpoint: "http://127.0.0.1:11434",
  reachable: false,
  models: []
};

const defaultCodexAuthStatus: CodexAuthStatus = {
  detected: false,
  source: "~/.codex/auth.json",
  tokenFields: []
};

const defaultCodexConnectivityStatus: CodexConnectivityStatus = {
  ok: false,
  expected: "CODEx_OK",
  command: 'codex exec --skip-git-repo-check -o <temp_file> "Reply with exactly: CODEx_OK"'
};

const defaultLocalOAuthTools: LocalOAuthToolStatus[] = [];

type ApiKeyProviderOption = {
  id: string;
  labelKey: string;
  providerId: string;
  defaultBaseUrl: string;
  defaultModel: string;
};

const apiKeyProviderOptions: ApiKeyProviderOption[] = [
  {
    id: "openai",
    labelKey: "apikey.provider.openai",
    providerId: "openai",
    defaultBaseUrl: "https://api.openai.com/v1",
    defaultModel: "gpt-5-mini"
  },
  {
    id: "anthropic",
    labelKey: "apikey.provider.anthropic",
    providerId: "anthropic",
    defaultBaseUrl: "https://api.anthropic.com",
    defaultModel: "claude-sonnet-4-5"
  }
];

export default function Onboarding({ onStatus, onLoginSuccess }: Props) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<Mode>("oauth");
  const [providers, setProviders] = useState<OAuthProvider[]>([]);
  const [selectedProvider, setSelectedProvider] = useState<string>("");
  const [apiProvider, setApiProvider] = useState(apiKeyProviderOptions[0].id);
  const [apiKey, setApiKey] = useState("");
  const [apiBaseUrl, setApiBaseUrl] = useState(apiKeyProviderOptions[0].defaultBaseUrl);
  const [apiDefaultModel, setApiDefaultModel] = useState(apiKeyProviderOptions[0].defaultModel);
  const [ollamaStatus, setOllamaStatus] = useState<OllamaStatus>(defaultOllamaStatus);
  const [ollamaEndpoint, setOllamaEndpoint] = useState(defaultOllamaStatus.endpoint);
  const [codexAuthStatus, setCodexAuthStatus] = useState<CodexAuthStatus>(defaultCodexAuthStatus);
  const [codexConnectivityStatus, setCodexConnectivityStatus] = useState<CodexConnectivityStatus>(
    defaultCodexConnectivityStatus
  );
  const [localOAuthTools, setLocalOAuthTools] = useState<LocalOAuthToolStatus[]>(defaultLocalOAuthTools);
  const [codexLoading, setCodexLoading] = useState(false);
  const [busy, setBusy] = useState(false);

  const modeCards = useMemo(
    () => [
      { id: "oauth" as const, title: t("mode.oauth"), recommended: true },
      { id: "apikey" as const, title: t("mode.apikey"), recommended: false },
      { id: "ollama" as const, title: t("mode.ollama"), recommended: false }
    ],
    [t]
  );

  const selectedApiProvider = useMemo(
    () => apiKeyProviderOptions.find((option) => option.id === apiProvider) ?? apiKeyProviderOptions[0],
    [apiProvider]
  );

  async function refreshProviders() {
    setBusy(true);
    onStatus(t("status.loading"));
    try {
      const nextProviders = await openclawBridge.listOAuthProviders();
      setProviders(nextProviders);
      onStatus(t("status.ready"));
      return nextProviders;
    } catch (error) {
      onStatus(`${t("status.error")}: ${error instanceof Error ? error.message : String(error)}`);
      return [] as OAuthProvider[];
    } finally {
      setBusy(false);
    }
  }

  async function refreshCodexAuth(providerList: OAuthProvider[]) {
    setCodexLoading(true);
    try {
      const status = await openclawBridge.detectLocalCodexAuth();
      setCodexAuthStatus(status);
      if (status.detected && providerList.some((provider) => provider.id === "openai-codex")) {
        setSelectedProvider("openai-codex");
      }
    } finally {
      setCodexLoading(false);
    }
  }

  async function refreshLocalOAuthTools(providerList: OAuthProvider[]) {
    try {
      const statuses = await openclawBridge.detectLocalOAuthTools();
      setLocalOAuthTools(statuses);

      const providerSet = new Set(providerList.map((provider) => provider.id));
      const preferredByAuth = statuses.find((status) => status.authDetected && providerSet.has(status.providerId));
      const preferredByCli = statuses.find((status) => status.cliFound && providerSet.has(status.providerId));
      const preferred = preferredByAuth ?? preferredByCli;

      if (preferred) {
        setSelectedProvider(preferred.providerId);
      } else if (providerList.length > 0 && !selectedProvider) {
        setSelectedProvider(providerList[0].id);
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      onStatus(`${t("status.error")}: ${message}`);
    }
  }

  useEffect(() => {
    void (async () => {
      const providerList = await refreshProviders();
      await refreshCodexAuth(providerList);
      await refreshLocalOAuthTools(providerList);
    })();
  }, []);

  async function handleOAuthStart(providerIdOverride?: string) {
    const providerId = providerIdOverride ?? selectedProvider;
    if (busy || !providerId) {
      return;
    }

    if (providerId === "openai-codex" && codexAuthStatus.detected) {
      setBusy(true);
      onStatus(t("status.loading"));
      try {
        const result = await openclawBridge.reuseLocalCodexAuth(true);
        if (result.reused) {
          onStatus(t("status.oauth.codex.reused"));
          onLoginSuccess();
        } else {
          onStatus(`${t("status.error")}: ${result.error ?? result.message}`);
        }
      } catch (error) {
        onStatus(`${t("status.error")}: ${error instanceof Error ? error.message : String(error)}`);
      } finally {
        setBusy(false);
      }
      return;
    }

    setBusy(true);
    onStatus(t("status.loading"));
    try {
      const result = await openclawBridge.startOAuthLogin(providerId);
      if (result.launched) {
        onStatus(`${t("status.oauth.start")}: ${result.commandHint}`);
        onLoginSuccess();
      } else {
        const details = result.details?.trim();
        onStatus(`${t("status.error")}: ${details || result.commandHint}`);
      }
    } catch (error) {
      onStatus(`${t("status.error")}: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  function useLocalCodexAuth() {
    setSelectedProvider("openai-codex");
    onStatus(t("oauth.codex.found"));
    void handleOAuthStart("openai-codex");
  }

  function useLocalToolProvider(tool: LocalOAuthToolStatus) {
    setSelectedProvider(tool.providerId);
    onStatus(t("oauth.local.selected", { provider: tool.providerId }));
    void handleOAuthStart(tool.providerId);
  }

  async function validateCodexConnectivity() {
    setBusy(true);
    onStatus(t("status.loading"));
    try {
      const result = await openclawBridge.validateLocalCodexConnectivity();
      setCodexConnectivityStatus(result);
      onStatus(result.ok ? t("oauth.codex.validate.ok") : `${t("oauth.codex.validate.fail")}: ${result.error ?? "-"}`);
    } catch (error) {
      onStatus(`${t("status.error")}: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  function handleApiProviderChange(nextProviderId: string) {
    const nextProvider =
      apiKeyProviderOptions.find((provider) => provider.id === nextProviderId) ?? apiKeyProviderOptions[0];
    setApiProvider(nextProvider.id);
    setApiBaseUrl(nextProvider.defaultBaseUrl);
    setApiDefaultModel(nextProvider.defaultModel);
  }

  async function handleApiKeySave() {
    if (!selectedApiProvider.providerId.trim() || !apiKey.trim()) {
      return;
    }
    setBusy(true);
    onStatus(t("status.loading"));
    try {
      await openclawBridge.saveApiKey(
        selectedApiProvider.providerId,
        apiKey.trim(),
        apiBaseUrl.trim() || undefined,
        apiDefaultModel.trim() || undefined
      );
      setApiKey("");
      onStatus(t("status.apikey.saved", { provider: selectedApiProvider.providerId, model: apiDefaultModel.trim() || "-" }));
      onLoginSuccess();
    } catch (error) {
      onStatus(`${t("status.error")}: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  async function handleOllamaCheck() {
    setBusy(true);
    onStatus(t("status.loading"));
    try {
      const status = await openclawBridge.checkOllama(ollamaEndpoint);
      setOllamaStatus(status);
      setOllamaEndpoint(status.endpoint);
      onStatus(status.reachable ? t("ollama.ok") : `${t("ollama.fail")}: ${status.error ?? "unknown"}`);
    } catch (error) {
      onStatus(`${t("status.error")}: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  async function handleOllamaApply() {
    setBusy(true);
    onStatus(t("status.loading"));
    try {
      const status = await openclawBridge.checkOllama(ollamaEndpoint);
      setOllamaStatus(status);
      setOllamaEndpoint(status.endpoint);

      if (!status.reachable) {
        onStatus(`${t("ollama.fail")}: ${status.error ?? "unknown"}`);
        return;
      }

      const result = await openclawBridge.applyOllamaConfig(status.endpoint, status.models[0]);
      onStatus(t("status.ollama.applied", { model: result.model }));
      onLoginSuccess();
    } catch (error) {
      onStatus(`${t("status.error")}: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="onboarding-shell">
      <div className="mode-grid">
        {modeCards.map((card) => (
          <button
            key={card.id}
            className={card.id === mode ? "mode-card is-active" : "mode-card"}
            onClick={() => setMode(card.id)}
            type="button"
          >
            <div className="mode-title">{card.title}</div>
            {card.recommended ? <span className="mode-tag">{t("mode.recommended")}</span> : null}
          </button>
        ))}
      </div>

      {mode === "oauth" ? (
        <div className="panel">
          <div className={codexAuthStatus.detected ? "status-chip success" : "status-chip warn"}>
            {codexLoading
              ? t("oauth.codex.detecting")
              : codexAuthStatus.detected
                ? t("oauth.codex.found")
                : t("oauth.codex.missing")}
          </div>
          {codexAuthStatus.detected ? (
            <p className="hint">
              {t("oauth.codex.lastRefresh")}: {codexAuthStatus.lastRefresh ?? "-"} | {codexAuthStatus.source}
            </p>
          ) : null}
          <div className={codexConnectivityStatus.ok ? "status-chip success" : "status-chip warn"}>
            {codexConnectivityStatus.ok ? t("oauth.codex.validate.ok") : t("oauth.codex.validate.fail")}
            {codexConnectivityStatus.response ? `: ${codexConnectivityStatus.response}` : ""}
          </div>
          <div className="local-oauth-tools">
            <strong>{t("oauth.local.title")}</strong>
            <ul>
              {localOAuthTools.map((tool) => {
                const statusLabel = tool.authDetected
                  ? t("oauth.local.ready")
                  : tool.cliFound
                    ? t("oauth.local.cliOnly")
                    : t("oauth.local.missing");
                return (
                  <li key={tool.id}>
                    <span>{tool.label}</span>
                    <span className={tool.authDetected ? "status-chip success" : "status-chip warn"}>{statusLabel}</span>
                    <button
                      type="button"
                      onClick={() => useLocalToolProvider(tool)}
                      disabled={busy || !providers.some((provider) => provider.id === tool.providerId)}
                    >
                      {t("oauth.local.use")}
                    </button>
                  </li>
                );
              })}
            </ul>
          </div>
          <label className="field">
            <span>{t("oauth.provider")}</span>
            <select
              value={selectedProvider}
              onChange={(event) => setSelectedProvider(event.target.value)}
              disabled={busy || providers.length === 0}
            >
              {providers.map((provider) => (
                <option key={provider.id} value={provider.id}>
                  {provider.label}
                </option>
              ))}
            </select>
          </label>
          <div className="action-row">
            <button
              type="button"
              onClick={() =>
                void (async () => {
                  const providerList = await refreshProviders();
                  await refreshCodexAuth(providerList);
                  await refreshLocalOAuthTools(providerList);
                })()
              }
              disabled={busy}
            >
              {t("oauth.refresh")}
            </button>
            <button
              type="button"
              onClick={useLocalCodexAuth}
              disabled={busy || !codexAuthStatus.detected || !providers.some((provider) => provider.id === "openai-codex")}
            >
              {t("oauth.codex.use")}
            </button>
            <button type="button" onClick={() => void validateCodexConnectivity()} disabled={busy || !codexAuthStatus.detected}>
              {t("oauth.codex.validate")}
            </button>
            <button type="button" className="primary" onClick={() => void handleOAuthStart()} disabled={busy || !selectedProvider}>
              {t("oauth.start")}
            </button>
          </div>
          <p className="hint">{t("oauth.hint")}</p>
        </div>
      ) : null}

      {mode === "apikey" ? (
        <div className="panel">
          <label className="field">
            <span>{t("apikey.provider")}</span>
            <select value={apiProvider} onChange={(event) => handleApiProviderChange(event.target.value)} disabled={busy}>
              {apiKeyProviderOptions.map((provider) => (
                <option key={provider.id} value={provider.id}>
                  {t(provider.labelKey)}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>{t("apikey.baseUrl")}</span>
            <input value={apiBaseUrl} onChange={(event) => setApiBaseUrl(event.target.value)} placeholder={selectedApiProvider.defaultBaseUrl} />
          </label>
          <label className="field">
            <span>{t("apikey.key")}</span>
            <input type="password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} />
          </label>
          <label className="field">
            <span>{t("apikey.model")}</span>
            <input
              value={apiDefaultModel}
              onChange={(event) => setApiDefaultModel(event.target.value)}
              placeholder={selectedApiProvider.defaultModel}
            />
          </label>
          <div className="action-row">
            <button type="button" className="primary" onClick={() => void handleApiKeySave()} disabled={busy || !apiKey.trim()}>
              {t("apikey.save")}
            </button>
          </div>
          <p className="hint">{t("apikey.baseUrl.hint")}</p>
          <p className="hint">{t("apikey.model.hint", { provider: selectedApiProvider.providerId })}</p>
          <p className="hint">{t("apikey.hint")}</p>
        </div>
      ) : null}

      {mode === "ollama" ? (
        <div className="panel">
          <label className="field">
            <span>{t("ollama.endpoint")}</span>
            <input
              value={ollamaEndpoint}
              onChange={(event) => {
                const nextEndpoint = event.target.value;
                setOllamaEndpoint(nextEndpoint);
                setOllamaStatus((previous) => ({ ...previous, endpoint: nextEndpoint, error: undefined }));
              }}
              placeholder={defaultOllamaStatus.endpoint}
            />
          </label>
          <p className="hint">{t("ollama.defaultHint", { endpoint: defaultOllamaStatus.endpoint })}</p>
          <div className="action-row">
            <button type="button" className="primary" onClick={() => void handleOllamaApply()} disabled={busy}>
              {t("ollama.apply")}
            </button>
            <button type="button" onClick={() => void handleOllamaCheck()} disabled={busy}>
              {t("ollama.check")}
            </button>
          </div>
          <div className="status-chip">{ollamaStatus.reachable ? t("ollama.ok") : t("ollama.fail")}</div>
          <div className="model-list">
            <strong>{t("ollama.models")}</strong>
            <ul>
              {(ollamaStatus.models.length > 0 ? ollamaStatus.models : ["-"]).map((model) => (
                <li key={model}>{model}</li>
              ))}
            </ul>
          </div>
        </div>
      ) : null}
    </section>
  );
}

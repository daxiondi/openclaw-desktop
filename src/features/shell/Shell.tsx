import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { openclawBridge } from "../../bridge/openclawBridge";
import type { BrowserModeStatus, BrowserRelayDiagnostic, BrowserRelayStatus } from "../../bridge/types";
import feedbackGroupQr from "../../assets/feedback-group-qr.png";

type Props = {
  onStatus: (message: string) => void;
  onBack: () => void;
};

type ShellTab = "help" | "official" | "settings";

const officialWebFallbackUrl = "http://127.0.0.1:18789/";

export default function Shell({ onStatus, onBack }: Props) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<ShellTab>("help");
  const [officialWebUrl, setOfficialWebUrl] = useState(officialWebFallbackUrl);
  const [officialReady, setOfficialReady] = useState(false);
  const [officialLoading, setOfficialLoading] = useState(false);
  const [officialOpening, setOfficialOpening] = useState(false);
  const [officialError, setOfficialError] = useState("");
  const [browserMode, setBrowserMode] = useState<BrowserModeStatus | null>(null);
  const [selectedMode, setSelectedMode] = useState<"openclaw" | "chrome">("openclaw");
  const [settingsLoading, setSettingsLoading] = useState(false);
  const [settingsSaving, setSettingsSaving] = useState(false);
  const [settingsError, setSettingsError] = useState("");
  const [relayStatus, setRelayStatus] = useState<BrowserRelayStatus | null>(null);
  const [relayLoading, setRelayLoading] = useState(false);
  const [relayPreparing, setRelayPreparing] = useState(false);
  const [relayError, setRelayError] = useState("");
  const [relayDiagnosing, setRelayDiagnosing] = useState(false);
  const [relayDiagnostic, setRelayDiagnostic] = useState<BrowserRelayDiagnostic | null>(null);

  async function ensureOfficialWebReady() {
    setOfficialLoading(true);
    setOfficialError("");
    onStatus(t("status.shell.official.preparing"));

    try {
      const result = await openclawBridge.ensureOfficialWebReady();
      setOfficialWebUrl(result.url || officialWebFallbackUrl);
      setOfficialReady(result.ready);
      if (result.ready) {
        onStatus(t("status.shell.official"));
        return true;
      } else {
        const message = [result.error ?? result.message, result.commandHint].filter(Boolean).join(" | ");
        setOfficialError(message);
        onStatus(`${t("status.error")}: ${message}`);
        return false;
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setOfficialError(message);
      setOfficialReady(false);
      onStatus(`${t("status.error")}: ${message}`);
      return false;
    } finally {
      setOfficialLoading(false);
    }
  }

  async function openOfficialWebWindow() {
    setOfficialOpening(true);
    setOfficialError("");

    try {
      const result = await openclawBridge.openOfficialWebWindow();
      setOfficialWebUrl(result.url || officialWebFallbackUrl);
      onStatus(t("status.shell.official.opened"));
      return true;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setOfficialError(message);
      onStatus(`${t("status.error")}: ${message}`);
      return false;
    } finally {
      setOfficialOpening(false);
    }
  }

  function maskOfficialWebUrl(url: string) {
    return url.replace(/([#?&]token=)[^&]+/i, "$1***");
  }

  function toModeText(mode: string) {
    return mode === "chrome"
      ? t("shell.settings.mode.chrome")
      : t("shell.settings.mode.openclaw");
  }

  async function loadBrowserModeStatus() {
    setSettingsLoading(true);
    setSettingsError("");
    try {
      const result = await openclawBridge.getBrowserModeStatus();
      setBrowserMode(result);
      setSelectedMode(result.mode === "chrome" ? "chrome" : "openclaw");
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setSettingsError(message);
      onStatus(`${t("status.error")}: ${message}`);
    } finally {
      setSettingsLoading(false);
    }
  }

  async function saveBrowserMode() {
    setSettingsSaving(true);
    setSettingsError("");
    try {
      const result = await openclawBridge.setBrowserMode(selectedMode);
      setBrowserMode(result);
      setSelectedMode(result.mode === "chrome" ? "chrome" : "openclaw");
      onStatus(t("status.shell.browserMode.saved", { mode: toModeText(result.mode) }));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setSettingsError(message);
      onStatus(`${t("status.error")}: ${message}`);
    } finally {
      setSettingsSaving(false);
    }
  }

  async function loadRelayStatus() {
    setRelayLoading(true);
    setRelayError("");
    try {
      const result = await openclawBridge.getBrowserRelayStatus();
      setRelayStatus(result);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setRelayError(message);
      onStatus(`${t("status.error")}: ${message}`);
    } finally {
      setRelayLoading(false);
    }
  }

  async function prepareRelay() {
    setRelayPreparing(true);
    setRelayError("");
    onStatus(t("status.shell.relay.preparing"));
    try {
      const result = await openclawBridge.prepareBrowserRelay();
      setRelayStatus(result);
      if (result.installed) {
        onStatus(t("status.shell.relay.ready"));
      } else {
        onStatus(`${t("status.error")}: ${result.error || result.message}`);
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setRelayError(message);
      onStatus(`${t("status.error")}: ${message}`);
    } finally {
      setRelayPreparing(false);
    }
  }

  async function diagnoseRelay() {
    setRelayDiagnosing(true);
    setRelayError("");
    onStatus(t("status.shell.relay.diagnosing"));
    try {
      const result = await openclawBridge.diagnoseBrowserRelay();
      setRelayDiagnostic(result);
      onStatus(t("status.shell.relay.diagnosed"));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setRelayError(message);
      onStatus(`${t("status.error")}: ${message}`);
    } finally {
      setRelayDiagnosing(false);
    }
  }

  function switchToHelpTab() {
    setActiveTab("help");
    onStatus(t("status.shell.help"));
  }

  async function switchToOfficialTab() {
    const ready = officialReady || (await ensureOfficialWebReady());
    if (!ready) {
      setActiveTab("official");
      return;
    }

    const opened = await openOfficialWebWindow();
    if (!opened) {
      setActiveTab("official");
    }
  }

  function switchToSettingsTab() {
    setActiveTab("settings");
    onStatus(t("status.shell.settings"));
  }

  useEffect(() => {
    switchToHelpTab();
    void ensureOfficialWebReady();
    void loadBrowserModeStatus();
    void loadRelayStatus();
  }, []);

  return (
    <section className="shell-root">
      <div className="shell-nav">
        <button
          type="button"
          className={`shell-tab ${activeTab === "help" ? "is-active" : ""}`}
          onClick={switchToHelpTab}
        >
          {t("shell.tab.help")}
        </button>
        <button
          type="button"
          className={`shell-tab ${activeTab === "official" ? "is-active" : ""}`}
          onClick={() => void switchToOfficialTab()}
        >
          {t("shell.tab.official")}
        </button>
        <button
          type="button"
          className={`shell-tab ${activeTab === "settings" ? "is-active" : ""}`}
          onClick={switchToSettingsTab}
        >
          {t("shell.tab.settings")}
        </button>
        <div className="shell-spacer" />
        <button type="button" onClick={onBack}>
          {t("shell.back")}
        </button>
      </div>

      <div className="shell-content">
        {activeTab === "help" ? (
          <div className="shell-custom panel">
            <h2>{t("shell.help.title")}</h2>
            <p>{t("shell.help.desc")}</p>

            <section className="help-block">
              <h3>{t("shell.help.auto.title")}</h3>
              <ul className="help-list">
                <li>{t("shell.help.auto.item.bootstrap")}</li>
                <li>{t("shell.help.auto.item.browserDefaults")}</li>
                <li>{t("shell.help.auto.item.relayAssets")}</li>
              </ul>
            </section>

            <section className="help-block">
              <h3>{t("shell.help.relay.title")}</h3>
              <p className="hint">{t("shell.help.relay.desc")}</p>
              {relayLoading ? <div className="status-chip">{t("status.loading")}</div> : null}
              {relayError ? <div className="status-chip warn">{relayError}</div> : null}
              {relayStatus ? (
                <div className={`status-chip ${relayStatus.installed ? "success" : "warn"}`}>
                  {relayStatus.installed ? t("shell.help.relay.ready") : t("shell.help.relay.missing")}
                </div>
              ) : null}
              <p className="hint">
                {t("shell.help.relay.path")}: <code>{relayStatus?.path || "-"}</code>
              </p>
              <p className="hint">
                {t("shell.help.relay.command")}: <code>{relayStatus?.commandHint || "openclaw browser extension install"}</code>
              </p>

              <div className="action-row">
                <button type="button" className="primary" onClick={() => void prepareRelay()} disabled={relayPreparing || relayLoading}>
                  {relayPreparing ? t("shell.help.relay.preparing") : t("shell.help.relay.prepare")}
                </button>
                <button type="button" onClick={() => void loadRelayStatus()} disabled={relayPreparing || relayLoading}>
                  {t("shell.help.relay.refresh")}
                </button>
                <button type="button" onClick={() => void diagnoseRelay()} disabled={relayPreparing || relayLoading || relayDiagnosing}>
                  {relayDiagnosing ? t("shell.help.relay.diagnosing") : t("shell.help.relay.diagnose")}
                </button>
              </div>

              {relayDiagnostic ? (
                <div className="relay-diagnostic">
                  <p>
                    <strong>{t("shell.help.relay.diag.cause")}</strong>: <code>{relayDiagnostic.likelyCause}</code>
                  </p>
                  <p>
                    <strong>{t("shell.help.relay.diag.relay")}</strong>: <code>{relayDiagnostic.relayUrl}</code>
                  </p>
                  <p>
                    <strong>{t("shell.help.relay.diag.reachable")}</strong>:{" "}
                    <code>{relayDiagnostic.relayReachable ? t("shell.help.relay.diag.yes") : t("shell.help.relay.diag.no")}</code>
                  </p>
                  <p>
                    <strong>{t("shell.help.relay.diag.connected")}</strong>:{" "}
                    <code>
                      {relayDiagnostic.extensionConnected === undefined
                        ? t("shell.help.relay.diag.unknown")
                        : relayDiagnostic.extensionConnected
                          ? t("shell.help.relay.diag.yes")
                          : t("shell.help.relay.diag.no")}
                    </code>
                  </p>
                  <p>
                    <strong>{t("shell.help.relay.diag.tabs")}</strong>: <code>{relayDiagnostic.tabsCount}</code>
                  </p>
                  <p className="hint">{relayDiagnostic.detail}</p>
                </div>
              ) : null}

              <ol className="help-steps">
                <li>{t("shell.help.relay.step1")}</li>
                <li>{t("shell.help.relay.step2")}</li>
                <li>{t("shell.help.relay.step3")}</li>
                <li>{t("shell.help.relay.step4")}</li>
              </ol>
            </section>

            <section className="feedback-card">
              <h3>{t("shell.feedback.title")}</h3>
              <p className="hint">{t("shell.feedback.desc")}</p>
              <img className="feedback-qr" src={feedbackGroupQr} alt={t("shell.feedback.alt")} />
            </section>
          </div>
        ) : activeTab === "official" ? (
          <div className="shell-custom panel">
            <h2>{t("shell.official.title")}</h2>
            <p>{t("shell.official.desc")}</p>
            <p className="hint">{t("shell.official.switchHint")}</p>
            {officialLoading ? <div className="status-chip">{t("shell.official.loading")}</div> : null}
            {officialError ? (
              <div className="status-chip warn">
                {t("shell.official.unavailable")}: {officialError}
              </div>
            ) : officialReady ? (
              <div className="status-chip success">{t("shell.official.ready")}</div>
            ) : null}
            <p className="hint">
              URL: <code>{maskOfficialWebUrl(officialWebUrl)}</code>
            </p>
            <div className="action-row">
              <button
                type="button"
                className="primary"
                onClick={() => void openOfficialWebWindow()}
                disabled={officialOpening || officialLoading}
              >
                {t("shell.official.open")}
              </button>
              <button type="button" onClick={() => void ensureOfficialWebReady()} disabled={officialLoading || officialOpening}>
                {t("shell.official.retry")}
              </button>
            </div>
          </div>
        ) : (
          <div className="shell-custom panel">
            <h2>{t("shell.settings.title")}</h2>
            <p>{t("shell.settings.desc")}</p>
            <p className="hint">{t("shell.settings.relayHint")}</p>
            {settingsLoading ? <div className="status-chip">{t("status.loading")}</div> : null}
            {settingsError ? <div className="status-chip warn">{settingsError}</div> : null}

            <div className="shell-mode-grid">
              <label className={`shell-mode-card ${selectedMode === "openclaw" ? "selected" : ""}`}>
                <input
                  type="radio"
                  name="browser-mode"
                  value="openclaw"
                  checked={selectedMode === "openclaw"}
                  onChange={() => setSelectedMode("openclaw")}
                />
                <div>
                  <strong>{t("shell.settings.mode.openclaw")}</strong>
                  <p className="hint">{t("shell.settings.mode.openclaw.desc")}</p>
                </div>
              </label>
              <label className={`shell-mode-card ${selectedMode === "chrome" ? "selected" : ""}`}>
                <input
                  type="radio"
                  name="browser-mode"
                  value="chrome"
                  checked={selectedMode === "chrome"}
                  onChange={() => setSelectedMode("chrome")}
                />
                <div>
                  <strong>{t("shell.settings.mode.chrome")}</strong>
                  <p className="hint">{t("shell.settings.mode.chrome.desc")}</p>
                </div>
              </label>
            </div>

            <p className="hint">
              {t("shell.settings.mode.current")}: <code>{toModeText(browserMode?.mode ?? "openclaw")}</code>
            </p>
            <p className="hint">
              {t("shell.settings.currentProfile")}: <code>{browserMode?.defaultProfile || "openclaw"}</code>
            </p>
            <p className="hint">
              {t("shell.settings.executable")}:{" "}
              <code>{browserMode?.executablePath || t("shell.settings.executable.auto")}</code>
            </p>

            <div className="detected-list">
              <strong>{t("shell.settings.detected")}</strong>
              {browserMode?.detectedBrowsers?.length ? (
                <ul>
                  {browserMode.detectedBrowsers.map((item) => (
                    <li key={`${item.kind}-${item.path}`}>
                      {item.kind}: <code>{item.path}</code>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="hint">{t("shell.settings.detected.none")}</p>
              )}
            </div>

            <div className="action-row">
              <button type="button" className="primary" onClick={() => void saveBrowserMode()} disabled={settingsSaving || settingsLoading}>
                {settingsSaving ? t("shell.settings.saving") : t("shell.settings.save")}
              </button>
              <button type="button" onClick={() => void loadBrowserModeStatus()} disabled={settingsSaving || settingsLoading}>
                {t("shell.settings.refresh")}
              </button>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { openclawBridge } from "../../bridge/openclawBridge";
import feedbackGroupQr from "../../assets/feedback-group-qr.png";

type Props = {
  onStatus: (message: string) => void;
  onBack: () => void;
};

const officialWebFallbackUrl = "http://127.0.0.1:18789/";

export default function Shell({ onStatus, onBack }: Props) {
  const { t } = useTranslation();
  const [officialWebUrl, setOfficialWebUrl] = useState(officialWebFallbackUrl);
  const [officialReady, setOfficialReady] = useState(false);
  const [officialLoading, setOfficialLoading] = useState(false);
  const [officialOpening, setOfficialOpening] = useState(false);
  const [officialError, setOfficialError] = useState("");

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
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setOfficialError(message);
      onStatus(`${t("status.error")}: ${message}`);
    } finally {
      setOfficialOpening(false);
    }
  }

  function maskOfficialWebUrl(url: string) {
    return url.replace(/([#?&]token=)[^&]+/i, "$1***");
  }

  useEffect(() => {
    onStatus(t("status.shell.custom"));
    void ensureOfficialWebReady();
  }, []);

  return (
    <section className="shell-root">
      <div className="shell-nav">
        <span className="status-chip">{t("shell.tab.custom")}</span>
        <div className="shell-spacer" />
        <button type="button" onClick={onBack}>
          {t("shell.back")}
        </button>
      </div>

      <div className="shell-content">
        <div className="shell-custom panel">
          <h2>{t("shell.custom.title")}</h2>
          <p>{t("shell.custom.desc")}</p>
          <p className="hint">{t("shell.custom.switchHint")}</p>
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
            <button type="button" className="primary" onClick={() => void openOfficialWebWindow()} disabled={officialOpening || officialLoading}>
              {t("shell.custom.openOfficial")}
            </button>
            <button type="button" onClick={() => void ensureOfficialWebReady()} disabled={officialLoading || officialOpening}>
              {t("shell.official.retry")}
            </button>
          </div>
          <section className="feedback-card">
            <h3>{t("shell.feedback.title")}</h3>
            <p className="hint">{t("shell.feedback.desc")}</p>
            <img className="feedback-qr" src={feedbackGroupQr} alt={t("shell.feedback.alt")} />
          </section>
        </div>
      </div>
    </section>
  );
}

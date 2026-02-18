import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { openclawBridge } from "../../bridge/openclawBridge";

type Props = {
  onStatus: (message: string) => void;
  onReady: () => void;
};

export default function Bootstrap({ onStatus, onReady }: Props) {
  const { t } = useTranslation();
  const [running, setRunning] = useState(false);
  const [error, setError] = useState("");
  const [logs, setLogs] = useState<string[]>([]);
  const [liveLogs, setLiveLogs] = useState<string[]>([]);
  const [elapsedSec, setElapsedSec] = useState(0);

  const visibleLogs = running ? liveLogs : logs.length > 0 ? logs : liveLogs;

  async function runBootstrap() {
    setRunning(true);
    setError("");
    setLogs([]);
    setLiveLogs([]);
    setElapsedSec(0);
    onStatus(t("status.bootstrap.running"));

    try {
      const result = await openclawBridge.bootstrapOpenClaw();
      setLogs(result.logs);
      setLiveLogs(result.logs);
      if (result.ready) {
        onStatus(t("status.bootstrap.ready"));
        onReady();
        return;
      }

      const message = result.error ?? result.message;
      setError(message);
      onStatus(`${t("status.bootstrap.failed")}: ${message}`);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      onStatus(`${t("status.bootstrap.failed")}: ${message}`);
    } finally {
      setRunning(false);
    }
  }

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    const hasTauriInternals =
      typeof window !== "undefined" &&
      typeof (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== "undefined";

    if (hasTauriInternals) {
      void listen<string>("bootstrap-log", (event) => {
        setLiveLogs((prev) => {
          const next = [...prev, event.payload];
          return next.length > 500 ? next.slice(next.length - 500) : next;
        });
      }).then((fn) => {
        if (cancelled) {
          fn();
          return;
        }
        unlisten = fn;
      });
    }

    void runBootstrap();

    return () => {
      cancelled = true;
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  useEffect(() => {
    if (!running) {
      return;
    }

    const startedAt = Date.now();
    const timer = window.setInterval(() => {
      setElapsedSec(Math.floor((Date.now() - startedAt) / 1000));
    }, 1000);

    return () => window.clearInterval(timer);
  }, [running]);

  return (
    <section className="bootstrap-root panel">
      <h2>{t("bootstrap.title")}</h2>
      <p className="hint">{t("bootstrap.desc")}</p>

      {running ? (
        <>
          <div className="status-chip">{t("bootstrap.running")}</div>
          <div className="bootstrap-progress" role="progressbar" aria-valuetext={t("bootstrap.runningDetail", { seconds: elapsedSec })}>
            <div className="bootstrap-progress-bar" />
          </div>
          <p className="hint">{t("bootstrap.runningDetail", { seconds: elapsedSec })}</p>
        </>
      ) : null}
      {error ? <div className="status-chip warn">{error}</div> : null}

      {visibleLogs.length > 0 ? (
        <div className="bootstrap-logs">
          <strong>{t("bootstrap.logs")}</strong>
          <ul>
            {visibleLogs.map((line, index) => (
              <li key={`${index}-${line}`}>{line}</li>
            ))}
          </ul>
        </div>
      ) : null}
      {running && visibleLogs.length === 0 ? <p className="hint">{t("bootstrap.waitingLogs")}</p> : null}

      {!running ? (
        <div className="action-row">
          <button type="button" className="primary" onClick={() => void runBootstrap()}>
            {t("bootstrap.retry")}
          </button>
        </div>
      ) : null}
    </section>
  );
}

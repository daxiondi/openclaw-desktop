import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

type Props = {
  onStatus: (message: string) => void;
};

type Phase = "idle" | "checking" | "available" | "none" | "downloading" | "error";

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && typeof window.__TAURI_INTERNALS__ !== "undefined";
}

export default function UpdaterWidget({ onStatus }: Props) {
  const { t } = useTranslation();
  const supported = useMemo(() => isTauriRuntime(), []);
  const updateRef = useRef<Update | null>(null);
  const totalBytesRef = useRef<number | null>(null);
  const downloadedBytesRef = useRef(0);

  const [phase, setPhase] = useState<Phase>("idle");
  const [latestVersion, setLatestVersion] = useState("");
  const [progressText, setProgressText] = useState("");
  const [errorText, setErrorText] = useState("");

  function resetProgress() {
    totalBytesRef.current = null;
    downloadedBytesRef.current = 0;
    setProgressText("");
  }

  async function checkForUpdates(manual = true) {
    if (!supported) {
      return;
    }

    setPhase("checking");
    setErrorText("");
    if (manual) {
      onStatus(t("status.update.checking"));
    }

    try {
      const update = await check({ timeout: 20000 });
      if (update) {
        updateRef.current = update;
        setLatestVersion(update.version);
        setPhase("available");
        onStatus(t("status.update.available", { version: update.version }));
      } else {
        updateRef.current = null;
        setLatestVersion("");
        setPhase(manual ? "none" : "idle");
        if (manual) {
          onStatus(t("status.update.none"));
        }
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setErrorText(message);
      setPhase("error");
      onStatus(`${t("status.update.failed")}: ${message}`);
    }
  }

  async function installUpdate() {
    if (!updateRef.current) {
      return;
    }

    setPhase("downloading");
    resetProgress();
    onStatus(t("status.update.downloading"));

    try {
      await updateRef.current.downloadAndInstall((event: DownloadEvent) => {
        if (event.event === "Started") {
          totalBytesRef.current = event.data.contentLength ?? null;
          downloadedBytesRef.current = 0;
          setProgressText(t("update.progress.start"));
          return;
        }
        if (event.event === "Progress") {
          downloadedBytesRef.current += event.data.chunkLength;
          const total = totalBytesRef.current;
          if (total && total > 0) {
            const percent = Math.min(100, Math.floor((downloadedBytesRef.current / total) * 100));
            setProgressText(t("update.progress.percent", { percent }));
          } else {
            const mb = (downloadedBytesRef.current / (1024 * 1024)).toFixed(1);
            setProgressText(t("update.progress.bytes", { mb }));
          }
          return;
        }
        setProgressText(t("update.progress.finish"));
      });

      onStatus(t("status.update.installed"));
      await relaunch();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setErrorText(message);
      setPhase("error");
      onStatus(`${t("status.update.failed")}: ${message}`);
    }
  }

  useEffect(() => {
    if (!supported) {
      return;
    }
    const timer = window.setTimeout(() => {
      void checkForUpdates(false);
    }, 2500);
    return () => window.clearTimeout(timer);
  }, [supported]);

  if (!supported) {
    return null;
  }

  return (
    <div className="update-widget">
      <button type="button" onClick={() => void checkForUpdates(true)} disabled={phase === "checking" || phase === "downloading"}>
        {phase === "checking" ? t("update.checking") : t("update.check")}
      </button>

      {phase === "available" ? (
        <>
          <span className="status-chip warn">{t("update.availableVersion", { version: latestVersion })}</span>
          <button type="button" className="primary" onClick={() => void installUpdate()}>
            {t("update.install")}
          </button>
        </>
      ) : null}

      {phase === "none" ? <span className="hint">{t("update.none")}</span> : null}
      {phase === "downloading" ? <span className="hint">{progressText || t("update.downloading")}</span> : null}
      {phase === "error" ? <span className="status-chip warn">{errorText || t("status.update.failed")}</span> : null}
    </div>
  );
}

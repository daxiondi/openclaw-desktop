import { useState } from "react";
import { useTranslation } from "react-i18next";
import Bootstrap from "./features/bootstrap/Bootstrap";
import Onboarding from "./features/onboarding/Onboarding";
import Shell from "./features/shell/Shell";
import UpdaterWidget from "./features/updater/UpdaterWidget";

export default function App() {
  const { t, i18n } = useTranslation();
  const [status, setStatus] = useState(t("status.ready"));
  const [bootstrapped, setBootstrapped] = useState(false);
  const [inShell, setInShell] = useState(false);

  function enterShell() {
    setInShell(true);
    setStatus(t("status.shell.entered"));
  }

  function backToOnboarding() {
    setInShell(false);
    setStatus(t("status.ready"));
  }

  return (
    <main className="app-root">
      <header className="hero">
        <div>
          <h1>{t("app.title")}</h1>
          <p>{t("app.subtitle")}</p>
        </div>
        <div className="hero-tools">
          <label className="lang-switch">
            <span>{t("lang.label")}</span>
            <select value={i18n.language} onChange={(event) => void i18n.changeLanguage(event.target.value)}>
              <option value="zh-CN">中文</option>
              <option value="en-US">English</option>
            </select>
          </label>
          <UpdaterWidget onStatus={setStatus} />
        </div>
      </header>

      {!bootstrapped ? (
        <Bootstrap
          onStatus={setStatus}
          onReady={() => {
            setBootstrapped(true);
            setStatus(t("status.ready"));
          }}
        />
      ) : inShell ? (
        <Shell onStatus={setStatus} onBack={backToOnboarding} />
      ) : (
        <Onboarding onStatus={setStatus} onLoginSuccess={enterShell} />
      )}

      <footer className="status-bar">
        <span>{status}</span>
      </footer>
    </main>
  );
}

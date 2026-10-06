import { useState } from "react";
import { useSettings } from "./hooks/useSettings";
import TransferPage from "./pages/TransferPage";
import SettingsPage from "./pages/SettingsPage";
import "./App.css";

type Page = "transfer" | "settings";

function App() {
  const [page, setPage] = useState<Page>("transfer");
  const { settings, loaded, updateSettings } = useSettings();

  return (
    <div className="app">
      <header className="site-header">
        <div className="site-header-inner">
          <div className="brand">
            <span className="brand-mark">移</span>
            <div>
              <span className="brand-name">Syosetu Transfer</span>
              <span className="brand-sub">小説投稿サイト移行ツール</span>
            </div>
          </div>
          <nav className="site-nav">
            <button
              type="button"
              className={page === "transfer" ? "active" : ""}
              onClick={() => setPage("transfer")}
            >
              作品を移す
            </button>
            <button
              type="button"
              className={page === "settings" ? "active" : ""}
              onClick={() => setPage("settings")}
            >
              設定
            </button>
          </nav>
        </div>
      </header>

      <main className="page">
        {!loaded ? (
          <p className="loading">読み込み中…</p>
        ) : (
          <>
            {/* 取得結果や投稿の進行状態を保つため、設定画面に切り替えてもアンマウントしない */}
            <div hidden={page !== "transfer"}>
              <TransferPage
                settings={settings}
                updateSettings={updateSettings}
                openSettings={() => setPage("settings")}
              />
            </div>
            {page === "settings" ? <SettingsPage settings={settings} updateSettings={updateSettings} /> : null}
          </>
        )}
      </main>
    </div>
  );
}

export default App;

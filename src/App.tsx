import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import "./App.css";

type Platform = "kakuyomu" | "narou";

type WorkInfo = {
  platform: Platform;
  title: string;
  author_name?: string | null;
  episode_count: number;
  first_url?: string | null;
  source_ref: string;
};

type Episode = {
  number: number;
  title: string;
  content: string;
  url?: string | null;
};

type TransferDraft = {
  work: WorkInfo;
  episodes: Episode[];
};

const platforms: Record<Platform, string> = {
  kakuyomu: "カクヨム",
  narou: "小説家になろう",
};

function App() {
  const [sourcePlatform, setSourcePlatform] = useState<Platform>("kakuyomu");
  const [targetPlatform, setTargetPlatform] = useState<Platform>("narou");
  const [source, setSource] = useState("");
  const [managementUrl, setManagementUrl] = useState("");
  const [draft, setDraft] = useState<TransferDraft | null>(null);
  const [selectedEpisode, setSelectedEpisode] = useState(0);
  const [status, setStatus] = useState("作品URLまたは ncode を入力してください。");
  const [isLoading, setIsLoading] = useState(false);

  const currentEpisode = draft?.episodes[selectedEpisode] ?? null;
  const exportJson = useMemo(() => {
    if (!draft) return "";
    return JSON.stringify(draft, null, 2);
  }, [draft]);

  async function fetchDraft() {
    if (sourcePlatform === targetPlatform) {
      setStatus("掲載元と投稿先は別のサービスを選んでください。");
      return;
    }

    setIsLoading(true);
    setStatus("作品データを取得しています。話数が多い場合は少し時間がかかります。");
    try {
      const result = await invoke<TransferDraft>("fetch_transfer_draft", {
        platform: sourcePlatform,
        source,
      });
      setDraft(result);
      setSelectedEpisode(0);
      setStatus(`${result.work.title} の ${result.episodes.length} 話を取得しました。`);
    } catch (error) {
      setStatus(String(error));
      setDraft(null);
    } finally {
      setIsLoading(false);
    }
  }

  async function openPostingPage() {
    try {
      const url = await invoke<string>("build_post_url", {
        platform: targetPlatform,
        managementUrl,
      });
      await openUrl(url);
      setStatus("投稿ページを開きました。選択中のタイトルと本文をコピーして貼り付けてください。");
    } catch (error) {
      setStatus(String(error));
    }
  }

  async function copyText(text: string, label: string) {
    await navigator.clipboard.writeText(text);
    setStatus(`${label}をコピーしました。`);
  }

  function swapPlatforms() {
    setSourcePlatform(targetPlatform);
    setTargetPlatform(sourcePlatform);
    setDraft(null);
    setSelectedEpisode(0);
    setStatus("掲載元と投稿先を入れ替えました。");
  }

  return (
    <main className="app-shell">
      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">Syosetu Transfer</p>
            <h1>小説投稿サイト移行ツール</h1>
          </div>
          <button className="ghost-button" type="button" onClick={swapPlatforms}>
            入れ替え
          </button>
        </header>

        <section className="setup-grid">
          <div className="field-group">
            <label>掲載元</label>
            <div className="segmented">
              {platformEntries().map(([value, label]) => (
                <button
                  key={value}
                  className={sourcePlatform === value ? "active" : ""}
                  type="button"
                  onClick={() => setSourcePlatform(value)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>

          <div className="field-group">
            <label>投稿先</label>
            <div className="segmented">
              {platformEntries().map(([value, label]) => (
                <button
                  key={value}
                  className={targetPlatform === value ? "active" : ""}
                  type="button"
                  onClick={() => setTargetPlatform(value)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>

          <div className="field-group wide">
            <label>{sourcePlatform === "narou" ? "ncode または作品URL" : "作品URL"}</label>
            <div className="action-row">
              <input
                value={source}
                onChange={(event) => setSource(event.currentTarget.value)}
                placeholder={
                  sourcePlatform === "narou"
                    ? "例: n5922lb"
                    : "例: https://kakuyomu.jp/works/168..."
                }
              />
              <button className="primary-button" type="button" onClick={fetchDraft} disabled={isLoading}>
                {isLoading ? "取得中" : "取得"}
              </button>
            </div>
          </div>

          <div className="field-group wide">
            <label>投稿先の作品管理URL</label>
            <div className="action-row">
              <input
                value={managementUrl}
                onChange={(event) => setManagementUrl(event.currentTarget.value)}
                placeholder={
                  targetPlatform === "kakuyomu"
                    ? "例: https://kakuyomu.jp/my/works/168..."
                    : "例: https://syosetu.com/draftepisode/input/ncode/..."
                }
              />
              <button className="secondary-button" type="button" onClick={openPostingPage}>
                開く
              </button>
            </div>
          </div>
        </section>

        <p className="status-line">{status}</p>

        {draft ? (
          <section className="transfer-layout">
            <aside className="episode-list">
              <div className="work-summary">
                <span>{platforms[draft.work.platform]}</span>
                <strong>{draft.work.title}</strong>
                <small>
                  {draft.work.author_name ? `${draft.work.author_name} / ` : ""}
                  {draft.work.episode_count} 話
                </small>
              </div>
              <div className="episode-buttons">
                {draft.episodes.map((episode, index) => (
                  <button
                    key={`${episode.number}-${episode.title}`}
                    className={selectedEpisode === index ? "active" : ""}
                    type="button"
                    onClick={() => setSelectedEpisode(index)}
                  >
                    <span>{episode.number}</span>
                    {episode.title}
                  </button>
                ))}
              </div>
            </aside>

            <section className="editor-pane">
              {currentEpisode ? (
                <>
                  <div className="editor-header">
                    <div>
                      <span>第{currentEpisode.number}話</span>
                      <h2>{currentEpisode.title}</h2>
                    </div>
                    <div className="toolbar">
                      <button type="button" onClick={() => copyText(currentEpisode.title, "タイトル")}>
                        タイトル
                      </button>
                      <button type="button" onClick={() => copyText(currentEpisode.content, "本文")}>
                        本文
                      </button>
                      <button type="button" onClick={() => copyText(`${currentEpisode.title}\n\n${currentEpisode.content}`, "タイトルと本文")}>
                        両方
                      </button>
                    </div>
                  </div>
                  <textarea readOnly value={currentEpisode.content} />
                </>
              ) : null}
            </section>
          </section>
        ) : (
          <section className="empty-state">
            <h2>移行元の作品を読み込むと、ここに各話が並びます。</h2>
            <p>取得した本文はアプリ内だけで表示します。投稿先への反映は、ログイン済みブラウザで確認しながら進められます。</p>
          </section>
        )}

        {draft ? (
          <section className="export-pane">
            <div>
              <h2>JSON 出力</h2>
              <p>バックアップや別ツール連携用に、取得結果をまとめてコピーできます。</p>
            </div>
            <button type="button" onClick={() => copyText(exportJson, "JSON")}>
              JSONをコピー
            </button>
          </section>
        ) : null}
      </section>
    </main>
  );
}

function platformEntries(): [Platform, string][] {
  return Object.entries(platforms) as [Platform, string][];
}

export default App;

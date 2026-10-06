import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { countLongRuby, renderNotation, toPlatformNotation } from "../notation";
import {
  platformEntries,
  platforms,
  type EpisodeStatus,
  type FetchProgress,
  type Platform,
  type PostProgress,
  type Settings,
  type TransferDraft,
} from "../types";

type Props = {
  settings: Settings;
  updateSettings: (patch: Partial<Settings>) => void;
  openSettings: () => void;
};

type NoticeKind = "info" | "success" | "warning" | "error";
type Notice = { kind: NoticeKind; text: string };

const statusLabels: Record<EpisodeStatus, string> = {
  opening: "開いています",
  "login-required": "ログイン待ち",
  filled: "入力済み",
  "waiting-user": "確認待ち",
  submitting: "保存中",
  posted: "投稿済み",
  submitted: "保存操作済み",
  skipped: "スキップ",
  error: "エラー",
};

const statusNotice: Partial<Record<PostProgress["status"], NoticeKind>> = {
  "login-required": "warning",
  "waiting-user": "warning",
  posted: "success",
  done: "success",
  skipped: "warning",
  submitted: "warning",
  error: "error",
};

function TransferPage({ settings, updateSettings, openSettings }: Props) {
  const [sourcePlatform, setSourcePlatform] = useState<Platform>(settings.defaultSource);
  const [targetPlatform, setTargetPlatform] = useState<Platform>(settings.defaultTarget);
  const [source, setSource] = useState("");
  const [draft, setDraft] = useState<TransferDraft | null>(null);
  const [selectedEpisode, setSelectedEpisode] = useState(0);
  const [notice, setNotice] = useState<Notice>({
    kind: "info",
    text: "移行元の作品URL（なろうは ncode でも可）を入力して「取得」を押してください。",
  });
  const [isFetching, setIsFetching] = useState(false);
  const [fetchProgress, setFetchProgress] = useState<FetchProgress | null>(null);
  const [statuses, setStatuses] = useState<Record<number, EpisodeStatus>>({});
  const [isPosting, setIsPosting] = useState(false);
  const [waitingUser, setWaitingUser] = useState(false);
  const [showSource, setShowSource] = useState(false);

  const managementUrl = settings.managementUrls[targetPlatform];
  const currentEpisode = draft?.episodes[selectedEpisode] ?? null;
  const convert = (text: string) => toPlatformNotation(text, targetPlatform, settings.rubyMode);
  const postedBody = currentEpisode ? convert(currentEpisode.content) : "";
  const longRubyCount = useMemo(
    () =>
      settings.rubyMode === "convert" && draft
        ? draft.episodes.reduce((sum, episode) => sum + countLongRuby(episode.content, targetPlatform), 0)
        : 0,
    [draft, targetPlatform, settings.rubyMode],
  );
  const exportJson = useMemo(() => (draft ? JSON.stringify(draft, null, 2) : ""), [draft]);
  const postedCount = Object.values(statuses).filter((status) => status === "posted" || status === "submitted").length;

  useEffect(() => {
    const unlistenFetch = listen<FetchProgress>("fetch-progress", (event) => setFetchProgress(event.payload));
    const unlistenPost = listen<PostProgress>("post-progress", (event) => {
      const { index, status, message } = event.payload;
      if (index !== null && status !== "done" && status !== "stopped") {
        setStatuses((current) => ({ ...current, [index]: status }));
        setSelectedEpisode(index);
      }
      if (status === "waiting-user") setWaitingUser(true);
      if (status === "opening") setWaitingUser(false);
      if (status === "done" || status === "stopped" || status === "error") {
        setIsPosting(false);
        setWaitingUser(false);
      }
      setNotice({ kind: statusNotice[status] ?? "info", text: message });
    });
    return () => {
      unlistenFetch.then((unlisten) => unlisten());
      unlistenPost.then((unlisten) => unlisten());
    };
  }, []);

  async function fetchDraft() {
    if (sourcePlatform === targetPlatform) {
      setNotice({ kind: "error", text: "掲載元と投稿先は別のサービスを選んでください。" });
      return;
    }

    setIsFetching(true);
    setFetchProgress(null);
    setNotice({ kind: "info", text: "作品データを取得しています。話数が多い場合は少し時間がかかります。" });
    try {
      const result = await invoke<TransferDraft>("fetch_transfer_draft", {
        platform: sourcePlatform,
        source,
        intervalMs: Math.round(settings.fetchIntervalSec * 1000),
      });
      setDraft(result);
      setSelectedEpisode(0);
      setStatuses({});
      setNotice({ kind: "success", text: `「${result.work.title}」の ${result.episodes.length} 話を取得しました。` });
    } catch (error) {
      setNotice({ kind: "error", text: String(error) });
      setDraft(null);
    } finally {
      setIsFetching(false);
      setFetchProgress(null);
    }
  }

  async function startPosting() {
    if (!draft) return;
    try {
      await invoke("start_posting", {
        request: {
          platform: targetPlatform,
          managementUrl,
          episodes: draft.episodes.map(({ title, content }) => ({ title: convert(title), content: convert(content) })),
          startIndex: selectedEpisode,
          mode: settings.postMode,
          intervalMs: Math.round(settings.postIntervalSec * 1000),
        },
      });
      setStatuses((current) => {
        const next = { ...current };
        for (let index = selectedEpisode; index < draft.episodes.length; index++) delete next[index];
        return next;
      });
      setIsPosting(true);
    } catch (error) {
      setNotice({ kind: "error", text: String(error) });
    }
  }

  async function copyText(text: string, label: string) {
    await navigator.clipboard.writeText(text);
    setNotice({ kind: "success", text: `${label}をコピーしました。` });
  }

  function changeSource(platform: Platform) {
    setSourcePlatform(platform);
    if (platform === targetPlatform) setTargetPlatform(sourcePlatform);
  }

  function changeTarget(platform: Platform) {
    setTargetPlatform(platform);
    if (platform === sourcePlatform) setSourcePlatform(targetPlatform);
  }

  function swapPlatforms() {
    setSourcePlatform(targetPlatform);
    setTargetPlatform(sourcePlatform);
    setDraft(null);
    setStatuses({});
    setSelectedEpisode(0);
  }

  function setManagementUrl(value: string) {
    updateSettings({ managementUrls: { ...settings.managementUrls, [targetPlatform]: value } });
  }

  return (
    <div className="transfer">
      <div className={`notice notice-${notice.kind}`} role="status">
        {notice.text}
      </div>

      <section className="card">
        <h2 className="card-title">
          <span className="step">1</span>移行元の作品を取得
        </h2>

        <div className="route">
          <PlatformPicker label="掲載元" value={sourcePlatform} onChange={changeSource} disabled={isPosting} />
          <button className="swap-button" type="button" onClick={swapPlatforms} disabled={isPosting} title="入れ替え">
            ⇄
          </button>
          <PlatformPicker label="投稿先" value={targetPlatform} onChange={changeTarget} disabled={isPosting} />
        </div>

        <label className="field-label" htmlFor="source">
          {sourcePlatform === "narou" ? "ncode または作品URL" : "作品URL"}
        </label>
        <div className="input-row">
          <input
            id="source"
            value={source}
            onChange={(event) => setSource(event.currentTarget.value)}
            onKeyDown={(event) => event.key === "Enter" && !isFetching && fetchDraft()}
            placeholder={sourcePlatform === "narou" ? "例: n5922lb" : "例: https://kakuyomu.jp/works/168..."}
          />
          <button className="button-primary" type="button" onClick={fetchDraft} disabled={isFetching || isPosting}>
            {isFetching ? "取得中…" : "取得"}
          </button>
        </div>

        {isFetching && fetchProgress ? (
          <div className="progress">
            <div className="progress-bar">
              <span
                style={{
                  width: fetchProgress.total ? `${(fetchProgress.current / fetchProgress.total) * 100}%` : "100%",
                }}
                className={fetchProgress.total ? "" : "indeterminate"}
              />
            </div>
            <small>
              第{fetchProgress.current}話{fetchProgress.total ? ` / 全${fetchProgress.total}話` : ""}：{fetchProgress.title}
            </small>
          </div>
        ) : null}
      </section>

      {draft ? (
        <>
          <section className="work-header">
            <span className="work-platform">{platforms[draft.work.platform]}</span>
            <h1 className="work-title">{draft.work.title}</h1>
            <p className="work-meta">
              {draft.work.author_name ? <span>{draft.work.author_name}</span> : null}
              <span>全{draft.episodes.length}話</span>
              {postedCount > 0 ? <span>投稿済み {postedCount}話</span> : null}
            </p>
          </section>

          <section className="card">
            <h2 className="card-title">
              <span className="step">2</span>
              {platforms[targetPlatform]}へ投稿
            </h2>

            <label className="field-label" htmlFor="management-url">
              投稿先の作品管理URL
            </label>
            <input
              id="management-url"
              value={managementUrl}
              onChange={(event) => setManagementUrl(event.currentTarget.value)}
              disabled={isPosting}
              placeholder={
                targetPlatform === "kakuyomu"
                  ? "例: https://kakuyomu.jp/my/works/168..."
                  : "例: https://syosetu.com/draftepisode/input/ncode/..."
              }
            />
            <p className="hint">
              {targetPlatform === "kakuyomu"
                ? "カクヨムの「作品の管理」ページのURLです。投稿ページは自動で開きます。"
                : "小説家になろうの「次話投稿（下書き入力）」ページのURLです。"}
            </p>

            <div className="post-mode">
              <span>
                投稿モード：
                <strong>{settings.postMode === "auto" ? "自動で保存して次へ" : "1話ずつ確認しながら"}</strong>
              </span>
              <span>
                ルビ：<strong>{settings.rubyMode === "convert" ? "投稿先の記法に変換" : "外す"}</strong>
              </span>
              <button className="link-button" type="button" onClick={openSettings}>
                変更
              </button>
            </div>
            {longRubyCount > 0 ? (
              <p className="hint hint-warning">
                なろうでは親文字・ルビとも10文字までです。超えているルビが {longRubyCount} 件あり、そのまま表示される可能性があります。
              </p>
            ) : null}

            <div className="post-actions">
              {!isPosting ? (
                <button className="button-primary" type="button" onClick={startPosting}>
                  第{draft.episodes[selectedEpisode]?.number ?? 1}話から投稿を開始
                </button>
              ) : (
                <>
                  {waitingUser ? (
                    <button className="button-primary" type="button" onClick={() => invoke("continue_posting")}>
                      次の話へ
                    </button>
                  ) : null}
                  <button className="button-secondary" type="button" onClick={() => invoke("stop_posting")}>
                    停止
                  </button>
                </>
              )}
            </div>
            <p className="hint">
              別ウィンドウで投稿ページが開きます。ログインしていない場合は、そのウィンドウでログインしてください。
            </p>
          </section>

          <div className="reader-layout">
            <section className="card toc">
              <h2 className="card-title">目次</h2>
              <ol className="toc-list">
                {draft.episodes.map((episode, index) => {
                  const status = statuses[index];
                  return (
                    <li key={`${episode.number}-${episode.title}`}>
                      <button
                        type="button"
                        className={selectedEpisode === index ? "active" : ""}
                        onClick={() => setSelectedEpisode(index)}
                      >
                        <span className="toc-number">{episode.number}</span>
                        <span className="toc-title">{episode.title}</span>
                        {status ? <span className={`badge badge-${status}`}>{statusLabels[status]}</span> : null}
                      </button>
                    </li>
                  );
                })}
              </ol>
            </section>

            <section className="card preview">
              {currentEpisode ? (
                <>
                  <p className="preview-number">第{currentEpisode.number}話</p>
                  <h2 className="preview-title">{renderNotation(currentEpisode.title)}</h2>
                  <div className="preview-switch">
                    <button type="button" className={showSource ? "" : "active"} onClick={() => setShowSource(false)}>
                      プレビュー
                    </button>
                    <button type="button" className={showSource ? "active" : ""} onClick={() => setShowSource(true)}>
                      投稿される本文
                    </button>
                  </div>
                  {showSource ? (
                    <div className="preview-body preview-source">{postedBody}</div>
                  ) : (
                    <div className="preview-body">{renderNotation(currentEpisode.content)}</div>
                  )}

                  <details className="manual-copy">
                    <summary>手動でコピー</summary>
                    <p className="hint">自動入力がうまくいかないときは、コピーして投稿ページに貼り付けてください。</p>
                    <div className="button-row">
                      <button className="button-secondary" type="button" onClick={() => copyText(convert(currentEpisode.title), "タイトル")}>
                        タイトル
                      </button>
                      <button className="button-secondary" type="button" onClick={() => copyText(postedBody, "本文")}>
                        本文
                      </button>
                      <button className="button-secondary" type="button" onClick={() => copyText(exportJson, "全話のJSON")}>
                        全話のJSON
                      </button>
                    </div>
                  </details>
                </>
              ) : null}
            </section>
          </div>
        </>
      ) : (
        <section className="empty-state">
          <p>作品を取得すると、ここに目次と本文が表示されます。</p>
        </section>
      )}
    </div>
  );
}

type PickerProps = {
  label: string;
  value: Platform;
  onChange: (platform: Platform) => void;
  disabled?: boolean;
};

function PlatformPicker({ label, value, onChange, disabled }: PickerProps) {
  return (
    <div className="picker">
      <span className="field-label">{label}</span>
      <div className="segmented">
        {platformEntries().map(([platform, name]) => (
          <button
            key={platform}
            type="button"
            className={value === platform ? "active" : ""}
            onClick={() => onChange(platform)}
            disabled={disabled}
          >
            {name}
          </button>
        ))}
      </div>
    </div>
  );
}

export default TransferPage;

import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { defaultSettings } from "../hooks/useSettings";
import { platformEntries, type Platform, type PostMode, type RubyMode, type Settings } from "../types";

type Props = {
  settings: Settings;
  updateSettings: (patch: Partial<Settings>) => void;
};

const postModes: { value: PostMode; title: string; description: string }[] = [
  {
    value: "confirm",
    title: "1話ずつ確認しながら",
    description: "タイトルと本文を入力したところで止まります。内容を確認して自分で保存し、「次の話へ」で進みます。",
  },
  {
    value: "auto",
    title: "自動で保存して次へ",
    description: "入力後に保存ボタンも自動で押し、全話を続けて投稿します。テスト用の作品で一度試してから使ってください。",
  },
];

const rubyModes: { value: RubyMode; title: string; description: string }[] = [
  {
    value: "convert",
    title: "投稿先の記法に変換する",
    description:
      "ルビは「|漢字《かんじ》」で投稿します。カクヨムの傍点《《文章》》は、なろうでは「|文章《・・》」のように文字数分の「・」のルビに置き換えます（逆方向も同様）。なろうで自動ルビになってしまう地の文の「漢字（かな）」も、ルビにならないように直します。",
  },
  {
    value: "strip",
    title: "ルビと傍点を外す",
    description: "親文字だけを残し、ルビや傍点の記法を使わずに投稿します。",
  },
];

function SettingsPage({ settings, updateSettings }: Props) {
  const [loginError, setLoginError] = useState("");

  async function openLogin(platform: Platform) {
    setLoginError("");
    try {
      await invoke("open_login_window", { platform });
    } catch (error) {
      setLoginError(String(error));
    }
  }

  return (
    <div className="settings">
      <section className="card">
        <h2 className="card-title">ログイン</h2>
        <p className="hint">
          投稿ウィンドウでログインしておくと、次回からは自動でログインした状態になります。
          パスワードはアプリに保存されません。二段階認証もそのウィンドウで入力できます。
        </p>
        <div className="button-row">
          {platformEntries().map(([platform, name]) => (
            <button key={platform} className="button-secondary" type="button" onClick={() => openLogin(platform)}>
              {name}にログイン
            </button>
          ))}
        </div>
        {loginError ? <p className="field-error">{loginError}</p> : null}
      </section>

      <section className="card">
        <h2 className="card-title">投稿モード</h2>
        <div className="choice-list">
          {postModes.map((mode) => (
            <label key={mode.value} className={`choice ${settings.postMode === mode.value ? "active" : ""}`}>
              <input
                type="radio"
                name="post-mode"
                checked={settings.postMode === mode.value}
                onChange={() => updateSettings({ postMode: mode.value })}
              />
              <span>
                <strong>{mode.title}</strong>
                <small>{mode.description}</small>
              </span>
            </label>
          ))}
        </div>
      </section>

      <section className="card">
        <h2 className="card-title">ルビ・傍点</h2>
        <div className="choice-list">
          {rubyModes.map((mode) => (
            <label key={mode.value} className={`choice ${settings.rubyMode === mode.value ? "active" : ""}`}>
              <input
                type="radio"
                name="ruby-mode"
                checked={settings.rubyMode === mode.value}
                onChange={() => updateSettings({ rubyMode: mode.value })}
              />
              <span>
                <strong>{mode.title}</strong>
                <small>{mode.description}</small>
              </span>
            </label>
          ))}
        </div>
      </section>

      <section className="card">
        <h2 className="card-title">間隔</h2>
        <p className="hint">サイトに負担をかけないよう、通信の間に待ち時間を入れます。</p>
        <div className="settings-grid">
          <NumberField
            label="各話を取得する間隔"
            value={settings.fetchIntervalSec}
            min={0.5}
            onChange={(value) => updateSettings({ fetchIntervalSec: value })}
          />
          <NumberField
            label="自動保存までの待ち時間"
            value={settings.postIntervalSec}
            min={1}
            onChange={(value) => updateSettings({ postIntervalSec: value })}
          />
        </div>
      </section>

      <section className="card">
        <h2 className="card-title">起動時の初期値</h2>
        <div className="settings-grid">
          <SelectField
            label="掲載元"
            value={settings.defaultSource}
            onChange={(value) => updateSettings({ defaultSource: value })}
          />
          <SelectField
            label="投稿先"
            value={settings.defaultTarget}
            onChange={(value) => updateSettings({ defaultTarget: value })}
          />
        </div>
      </section>

      <div className="settings-footer">
        <button
          className="link-button"
          type="button"
          onClick={() => updateSettings({ ...defaultSettings, managementUrls: settings.managementUrls })}
        >
          初期設定に戻す
        </button>
      </div>
    </div>
  );
}

type NumberFieldProps = {
  label: string;
  value: number;
  min: number;
  onChange: (value: number) => void;
};

function NumberField({ label, value, min, onChange }: NumberFieldProps) {
  return (
    <label className="setting-field">
      <span className="field-label">{label}</span>
      <span className="number-input">
        <input
          type="number"
          min={min}
          max={60}
          step={0.5}
          value={value}
          onChange={(event) => {
            const next = Number(event.currentTarget.value);
            if (Number.isFinite(next)) onChange(Math.min(60, Math.max(min, next)));
          }}
        />
        <span>秒</span>
      </span>
    </label>
  );
}

type SelectFieldProps = {
  label: string;
  value: Platform;
  onChange: (value: Platform) => void;
};

function SelectField({ label, value, onChange }: SelectFieldProps) {
  return (
    <label className="setting-field">
      <span className="field-label">{label}</span>
      <select value={value} onChange={(event) => onChange(event.currentTarget.value as Platform)}>
        {platformEntries().map(([platform, name]) => (
          <option key={platform} value={platform}>
            {name}
          </option>
        ))}
      </select>
    </label>
  );
}

export default SettingsPage;

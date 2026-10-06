//! 投稿先サイトをアプリ内の別ウィンドウ (OS 標準の WebView) で開き、
//! JS を流し込んでタイトルと本文を入力する。
//!
//! ブラウザドライバは使わない。ページの状態は `eval_with_callback` で
//! 問い合わせるだけなので、外部サイトにアプリの IPC は一切公開しない。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tokio::sync::oneshot;

use crate::fetch::build_post_url;

const POSTER_LABEL: &str = "poster";
const POLL: Duration = Duration::from_millis(500);
const FORM_TIMEOUT: Duration = Duration::from_secs(20);
const SUBMIT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Default)]
pub struct PosterState {
    running: AtomicBool,
    stop: AtomicBool,
    proceed: AtomicBool,
}

impl PosterState {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PostMode {
    /// 入力だけして、保存はユーザーが投稿ウィンドウで行う
    Confirm,
    /// 入力後に保存ボタンも自動で押す
    Auto,
}

#[derive(Debug, Deserialize)]
pub struct PostEpisode {
    title: String,
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostRequest {
    platform: String,
    management_url: String,
    episodes: Vec<PostEpisode>,
    start_index: usize,
    mode: PostMode,
    interval_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
struct PostProgress {
    index: Option<usize>,
    status: &'static str,
    message: String,
}

struct FormSpec {
    title: &'static str,
    body: &'static str,
    submit: &'static [&'static str],
}

fn form_spec(platform: &str) -> Result<FormSpec, String> {
    match platform {
        "kakuyomu" => Ok(FormSpec {
            title: "[name='title']",
            body: "[name='body']",
            submit: &["#updateButton", "button[type='submit']"],
        }),
        "narou" => Ok(FormSpec {
            title: "[name='subtitle']",
            body: "[name='novel']",
            submit: &[
                "button[form='usernoveldatainputForm']",
                "button[type='submit']",
            ],
        }),
        _ => Err("対応していない投稿先です".to_string()),
    }
}

fn login_url(platform: &str) -> Result<&'static str, String> {
    // ログイン済みならマイページ、未ログインならログイン画面へ飛ばされる URL
    match platform {
        "kakuyomu" => Ok("https://kakuyomu.jp/my"),
        "narou" => Ok("https://syosetu.com/user/top/"),
        _ => Err("対応していない投稿先です".to_string()),
    }
}

#[tauri::command]
pub async fn open_login_window(app: AppHandle, platform: String) -> Result<(), String> {
    let url = parse_url(login_url(&platform)?)?;
    poster_window(&app, url)?;
    Ok(())
}

#[tauri::command]
pub fn start_posting(
    app: AppHandle,
    state: State<'_, Arc<PosterState>>,
    request: PostRequest,
) -> Result<(), String> {
    let spec = form_spec(&request.platform)?;
    let post_url = parse_url(&build_post_url(&request.platform, &request.management_url)?)?;
    if request.start_index >= request.episodes.len() {
        return Err("投稿するエピソードがありません".to_string());
    }
    if state.running.swap(true, Ordering::SeqCst) {
        return Err("すでに投稿を進めています".to_string());
    }
    state.stop.store(false, Ordering::SeqCst);
    state.proceed.store(false, Ordering::SeqCst);

    let state = Arc::clone(&state);
    tauri::async_runtime::spawn(async move {
        let session = Session {
            app: &app,
            state: &state,
            spec,
            post_url,
        };
        let result = session.run(&request).await;
        state.running.store(false, Ordering::SeqCst);
        match result {
            Ok(Flow::Done) => session.emit(None, "done", "すべてのエピソードを処理しました"),
            Ok(Flow::Stopped) => session.emit(None, "stopped", "投稿を停止しました"),
            Err(failure) => session.emit(failure.index, "error", &failure.message),
        }
    });
    Ok(())
}

#[tauri::command]
pub fn continue_posting(state: State<'_, Arc<PosterState>>) {
    state.proceed.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn stop_posting(state: State<'_, Arc<PosterState>>) {
    state.stop.store(true, Ordering::SeqCst);
}

enum Flow {
    Done,
    Stopped,
}

struct Failure {
    index: Option<usize>,
    message: String,
}

impl Failure {
    fn at(index: usize, message: impl Into<String>) -> Self {
        Self {
            index: Some(index),
            message: message.into(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Probe {
    href: String,
    ready: String,
    login: bool,
    has_title: bool,
    has_body: bool,
}

struct Session<'a> {
    app: &'a AppHandle,
    state: &'a PosterState,
    spec: FormSpec,
    post_url: Url,
}

impl Session<'_> {
    fn emit(&self, index: Option<usize>, status: &'static str, message: &str) {
        let _ = self.app.emit(
            "post-progress",
            PostProgress {
                index,
                status,
                message: message.to_string(),
            },
        );
    }

    fn window(&self, index: usize) -> Result<WebviewWindow, Failure> {
        self.app
            .get_webview_window(POSTER_LABEL)
            .ok_or_else(|| Failure::at(index, "投稿ウィンドウが閉じられました"))
    }

    async fn run(&self, request: &PostRequest) -> Result<Flow, Failure> {
        for index in request.start_index..request.episodes.len() {
            let episode = &request.episodes[index];
            if self.state.stopped() {
                return Ok(Flow::Stopped);
            }

            self.emit(Some(index), "opening", "投稿ページを開いています");
            poster_window(self.app, self.post_url.clone()).map_err(|error| Failure::at(index, error))?;

            let Some(form_href) = self.wait_for_form(index).await? else {
                return Ok(Flow::Stopped);
            };
            self.fill(index, episode).await?;

            let flow = match request.mode {
                PostMode::Auto => self.submit(index, &form_href, request.interval_ms).await?,
                PostMode::Confirm => self.wait_for_user(index, &form_href).await?,
            };
            if let Flow::Stopped = flow {
                return Ok(Flow::Stopped);
            }
        }
        Ok(Flow::Done)
    }

    /// 投稿フォームが表示されるまで待つ。途中でログイン画面に飛ばされたら、
    /// ユーザーのログインを待ってから投稿ページを開き直す。
    async fn wait_for_form(&self, index: usize) -> Result<Option<String>, Failure> {
        let mut started = Instant::now();
        let mut waiting_login = false;
        let mut last = Probe::default();
        tokio::time::sleep(Duration::from_millis(700)).await;

        loop {
            if self.state.stopped() {
                return Ok(None);
            }
            let window = self.window(index)?;
            // ページ遷移中は応答がないことがあるので、失敗しても次の問い合わせを待つ
            if let Ok(probe) = self.probe(&window).await {
                if probe.has_title && probe.has_body {
                    return Ok(Some(probe.href));
                }
                if probe.login {
                    if !waiting_login {
                        waiting_login = true;
                        self.emit(
                            Some(index),
                            "login-required",
                            "投稿ウィンドウでログインしてください。ログインが終わると自動で再開します",
                        );
                    }
                } else if waiting_login {
                    waiting_login = false;
                    self.emit(Some(index), "opening", "ログインを確認しました。投稿ページを開き直しています");
                    window
                        .navigate(self.post_url.clone())
                        .map_err(|error| Failure::at(index, error.to_string()))?;
                    tokio::time::sleep(Duration::from_millis(700)).await;
                    started = Instant::now();
                    continue;
                }
                last = probe;
            }

            if !waiting_login && started.elapsed() > FORM_TIMEOUT && last.ready == "complete" {
                return Err(Failure::at(index, missing_fields_message(&last)));
            }
            tokio::time::sleep(POLL).await;
        }
    }

    async fn fill(&self, index: usize, episode: &PostEpisode) -> Result<(), Failure> {
        let window = self.window(index)?;
        let script = fill_script(&self.spec, episode);
        let result = eval_json(&window, script)
            .await
            .map_err(|error| Failure::at(index, error))?;
        if result.get("ok").and_then(Value::as_bool) != Some(true) {
            let detail = result
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("入力欄に書き込めませんでした");
            return Err(Failure::at(index, detail));
        }
        self.emit(Some(index), "filled", "タイトルと本文を入力しました");
        Ok(())
    }

    async fn submit(&self, index: usize, form_href: &str, interval_ms: u64) -> Result<Flow, Failure> {
        if !self.pause(Duration::from_millis(interval_ms)).await {
            return Ok(Flow::Stopped);
        }
        let window = self.window(index)?;
        let result = eval_json(&window, submit_script(&self.spec))
            .await
            .map_err(|error| Failure::at(index, error))?;
        if result.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(Failure::at(index, "保存ボタンが見つかりませんでした"));
        }
        self.emit(Some(index), "submitting", "保存ボタンを押しました");

        let started = Instant::now();
        while started.elapsed() < SUBMIT_TIMEOUT {
            if self.state.stopped() {
                return Ok(Flow::Stopped);
            }
            if self.left_form(index, form_href).await? {
                self.emit(Some(index), "posted", "保存しました");
                return Ok(Flow::Done);
            }
            tokio::time::sleep(POLL).await;
        }
        // 画面が切り替わらない保存方式の可能性もあるため、エラーにはせず未確認として進める
        self.emit(
            Some(index),
            "submitted",
            "保存ボタンを押しましたが、画面の切り替わりは確認できませんでした",
        );
        Ok(Flow::Done)
    }

    /// 確認モード: ユーザーが投稿ウィンドウで保存し、アプリで「次の話へ」を押すまで待つ。
    async fn wait_for_user(&self, index: usize, form_href: &str) -> Result<Flow, Failure> {
        self.state.proceed.store(false, Ordering::SeqCst);
        self.emit(
            Some(index),
            "waiting-user",
            "投稿ウィンドウで内容を確認して保存し、「次の話へ」を押してください",
        );
        let mut saved = false;
        loop {
            if self.state.stopped() {
                return Ok(Flow::Stopped);
            }
            if !saved && self.left_form(index, form_href).await? {
                saved = true;
                self.emit(Some(index), "posted", "保存を確認しました。「次の話へ」で進みます");
            }
            if self.state.proceed.swap(false, Ordering::SeqCst) {
                if !saved {
                    self.emit(Some(index), "skipped", "保存を確認できないまま次へ進みました");
                }
                return Ok(Flow::Done);
            }
            tokio::time::sleep(POLL).await;
        }
    }

    async fn left_form(&self, index: usize, form_href: &str) -> Result<bool, Failure> {
        let window = self.window(index)?;
        Ok(self
            .probe(&window)
            .await
            .map(|probe| probe.href != form_href)
            .unwrap_or(false))
    }

    async fn probe(&self, window: &WebviewWindow) -> Result<Probe, String> {
        let value = eval_json(window, probe_script(&self.spec)).await?;
        serde_json::from_value(value).map_err(|error| error.to_string())
    }

    /// 停止されたら false を返す
    async fn pause(&self, duration: Duration) -> bool {
        let started = Instant::now();
        while started.elapsed() < duration {
            if self.state.stopped() {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        !self.state.stopped()
    }
}

fn missing_fields_message(probe: &Probe) -> String {
    let mut missing = Vec::new();
    if !probe.has_title {
        missing.push("タイトル欄");
    }
    if !probe.has_body {
        missing.push("本文欄");
    }
    format!(
        "投稿ページで{}が見つかりませんでした。作品管理URLが正しいか確認してください。サイトの仕様が変わった場合は「手動でコピー」を使ってください",
        missing.join("・")
    )
}

fn poster_window(app: &AppHandle, url: Url) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(POSTER_LABEL) {
        window.navigate(url).map_err(|error| error.to_string())?;
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(window);
    }
    WebviewWindowBuilder::new(app, POSTER_LABEL, WebviewUrl::External(url))
        .title("投稿ウィンドウ")
        .inner_size(1000.0, 800.0)
        .build()
        .map_err(|error| format!("投稿ウィンドウを開けませんでした: {error}"))
}

fn parse_url(url: &str) -> Result<Url, String> {
    let parsed = Url::parse(url).map_err(|_| format!("URLの形式が正しくありません: {url}"))?;
    let allowed = parsed.scheme() == "https"
        && parsed.host_str().is_some_and(|host| {
            ["kakuyomu.jp", "syosetu.com"]
                .iter()
                .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
        });
    if !allowed {
        return Err("カクヨムまたは小説家になろうのURLを入力してください".to_string());
    }
    Ok(parsed)
}

async fn eval_json(window: &WebviewWindow, script: String) -> Result<Value, String> {
    let (sender, receiver) = oneshot::channel();
    let sender = Mutex::new(Some(sender));
    window
        .eval_with_callback(script, move |result| {
            if let Some(sender) = sender.lock().ok().and_then(|mut sender| sender.take()) {
                let _ = sender.send(result);
            }
        })
        .map_err(|error| error.to_string())?;
    let raw = tokio::time::timeout(Duration::from_secs(5), receiver)
        .await
        .map_err(|_| "投稿ウィンドウから応答がありません".to_string())?
        .map_err(|_| "投稿ウィンドウから応答がありません".to_string())?;
    Ok(decode_eval_result(&raw))
}

/// スクリプトは JSON.stringify した文字列を返す。プラットフォームによっては
/// それがさらに JSON 文字列として包まれて届くので、オブジェクトになるまで剥がす。
fn decode_eval_result(raw: &str) -> Value {
    let mut value = serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()));
    for _ in 0..2 {
        match &value {
            Value::String(text) => match serde_json::from_str(text) {
                Ok(inner) => value = inner,
                Err(_) => break,
            },
            _ => break,
        }
    }
    value
}

fn js_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn probe_script(spec: &FormSpec) -> String {
    format!(
        r#"(function () {{
  return JSON.stringify({{
    href: location.href,
    ready: document.readyState,
    login: /\/login|\/auth\/|2stepauth/i.test(location.pathname),
    hasTitle: !!document.querySelector({title}),
    hasBody: !!document.querySelector({body})
  }});
}})()"#,
        title = js_string(spec.title),
        body = js_string(spec.body),
    )
}

fn fill_script(spec: &FormSpec, episode: &PostEpisode) -> String {
    format!(
        r#"(function () {{
  try {{
    // React などで管理された入力欄にも反映されるよう、ネイティブの setter で値を入れてからイベントを送る
    function setValue(element, value) {{
      var proto = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      var descriptor = Object.getOwnPropertyDescriptor(proto, "value");
      element.focus();
      if (descriptor && descriptor.set) {{ descriptor.set.call(element, value); }} else {{ element.value = value; }}
      element.dispatchEvent(new Event("input", {{ bubbles: true }}));
      element.dispatchEvent(new Event("change", {{ bubbles: true }}));
      element.blur();
    }}
    var title = document.querySelector({title_selector});
    var body = document.querySelector({body_selector});
    if (!title || !body) {{
      return JSON.stringify({{ ok: false, error: "タイトル欄または本文欄が見つかりませんでした" }});
    }}
    setValue(title, {title});
    setValue(body, {body});
    return JSON.stringify({{ ok: true }});
  }} catch (error) {{
    return JSON.stringify({{ ok: false, error: String(error) }});
  }}
}})()"#,
        title_selector = js_string(spec.title),
        body_selector = js_string(spec.body),
        title = js_string(&episode.title),
        body = js_string(&episode.content),
    )
}

fn submit_script(spec: &FormSpec) -> String {
    let selectors = serde_json::to_string(spec.submit).unwrap_or_else(|_| "[]".to_string());
    format!(
        r#"(function () {{
  try {{
    var selectors = {selectors};
    for (var i = 0; i < selectors.length; i++) {{
      var button = document.querySelector(selectors[i]);
      if (button && !button.disabled) {{
        button.click();
        return JSON.stringify({{ ok: true }});
      }}
    }}
    return JSON.stringify({{ ok: false }});
  }} catch (error) {{
    return JSON.stringify({{ ok: false, error: String(error) }});
  }}
}})()"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_double_encoded_results() {
        let raw = serde_json::to_string(r#"{"ok":true}"#).unwrap();
        assert_eq!(decode_eval_result(&raw)["ok"], Value::Bool(true));
        assert_eq!(decode_eval_result(r#"{"ok":true}"#)["ok"], Value::Bool(true));
    }

    #[test]
    fn only_allows_supported_sites() {
        assert!(parse_url("https://kakuyomu.jp/my/works/1").is_ok());
        assert!(parse_url("https://syosetu.com/draftepisode/input/ncode/1/").is_ok());
        assert!(parse_url("https://evil-kakuyomu.jp/").is_err());
        assert!(parse_url("http://kakuyomu.jp/").is_err());
    }

    #[test]
    fn escapes_episode_text_for_js() {
        let spec = form_spec("narou").unwrap();
        let episode = PostEpisode {
            title: "\"});alert(1);//".to_string(),
            content: "改行\nと</script>".to_string(),
        };
        let script = fill_script(&spec, &episode);
        assert!(script.contains(r#""\"});alert(1);//""#));
        assert!(script.contains(r#""改行\nと</script>""#));
    }
}

use reqwest::Client;
use scraper::{node::Node, CaseSensitivity, ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0";

#[derive(Debug, Serialize)]
pub struct WorkInfo {
    platform: String,
    title: String,
    author_name: Option<String>,
    episode_count: usize,
    first_url: Option<String>,
    source_ref: String,
}

#[derive(Debug, Serialize)]
pub struct Episode {
    number: usize,
    title: String,
    content: String,
    url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TransferDraft {
    work: WorkInfo,
    episodes: Vec<Episode>,
}

#[derive(Debug, Clone, Serialize)]
struct FetchProgress {
    current: usize,
    total: Option<usize>,
    title: String,
}

#[derive(Debug, Deserialize)]
struct NarouNovel {
    title: String,
    ncode: String,
    general_all_no: usize,
    writer: Option<String>,
}

/// 取得の進捗をフロントへ通知しつつ、各話の間に待ち時間を入れる。
struct Fetcher {
    app: AppHandle,
    client: Client,
    interval: Duration,
}

impl Fetcher {
    fn progress(&self, current: usize, total: Option<usize>, title: &str) {
        let _ = self.app.emit(
            "fetch-progress",
            FetchProgress {
                current,
                total,
                title: title.to_string(),
            },
        );
    }

    async fn wait(&self) {
        if !self.interval.is_zero() {
            tokio::time::sleep(self.interval).await;
        }
    }
}

#[tauri::command]
pub async fn fetch_transfer_draft(
    app: AppHandle,
    platform: String,
    source: String,
    interval_ms: Option<u64>,
) -> Result<TransferDraft, String> {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let fetcher = Fetcher {
        app,
        client,
        interval: Duration::from_millis(interval_ms.unwrap_or(1000)),
    };

    match platform.as_str() {
        "narou" => fetch_narou(&fetcher, &source).await,
        "kakuyomu" => fetch_kakuyomu(&fetcher, &source).await,
        _ => Err("対応していない掲載元です".to_string()),
    }
}

pub fn build_post_url(platform: &str, management_url: &str) -> Result<String, String> {
    let trimmed = management_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("投稿先URLを入力してください".to_string());
    }

    match platform {
        "kakuyomu" => {
            if !trimmed.contains("kakuyomu.jp") {
                return Err("カクヨムの作品管理URLを入力してください".to_string());
            }
            if trimmed.ends_with("/episodes/new") {
                return Ok(trimmed.to_string());
            }
            Ok(format!("{trimmed}/episodes/new"))
        }
        "narou" => {
            if !trimmed.contains("syosetu.com") {
                return Err("小説家になろうの下書き入力URLを入力してください".to_string());
            }
            Ok(trimmed.to_string())
        }
        _ => Err("対応していない投稿先です".to_string()),
    }
}

async fn fetch_narou(fetcher: &Fetcher, source: &str) -> Result<TransferDraft, String> {
    let client = &fetcher.client;
    let ncode = normalize_ncode(source)?;
    let endpoint = format!(
        "https://api.syosetu.com/novelapi/api/?ncode={}&out=json",
        urlencoding::encode(&ncode)
    );
    let api_body: Vec<serde_json::Value> = client
        .get(endpoint)
        .send()
        .await
        .map_err(|error| format!("作品情報の取得に失敗しました: {error}"))?
        .error_for_status()
        .map_err(|error| format!("作品情報の取得に失敗しました: {error}"))?
        .json()
        .await
        .map_err(|error| format!("作品情報の読み込みに失敗しました: {error}"))?;

    let novel_value = api_body
        .get(1)
        .ok_or_else(|| "作品が見つかりませんでした".to_string())?;
    let novel: NarouNovel = serde_json::from_value(novel_value.clone())
        .map_err(|error| format!("作品情報の形式を読み取れませんでした: {error}"))?;

    let total = novel.general_all_no;
    let mut episodes = Vec::with_capacity(total);
    for number in 1..=total {
        if number > 1 {
            fetcher.wait().await;
        }
        let url = format!(
            "https://ncode.syosetu.com/{}/{}/",
            novel.ncode.to_lowercase(),
            number
        );
        let html = fetch_html(client, &url).await?;
        let (title, content) = {
            let document = Html::parse_document(&html);
            let title = first_text(&document, &[".p-novel__title", ".novel_subtitle"])
                .ok_or_else(|| format!("第{number}話のタイトルを読み取れませんでした"))?;
            let content = extract_body(
                &document,
                &[
                    ".p-novel__body .p-novel__text:not(.p-novel__text--preface):not(.p-novel__text--afterword)",
                    ".p-novel__text",
                    "#novel_honbun",
                ],
            )
                .ok_or_else(|| format!("第{number}話の本文を読み取れませんでした"))?;
            (title, content)
        };
        fetcher.progress(number, Some(total), &title);

        episodes.push(Episode {
            number,
            title,
            content,
            url: Some(url),
        });
    }

    Ok(TransferDraft {
        work: WorkInfo {
            platform: "narou".to_string(),
            title: novel.title,
            author_name: novel.writer,
            episode_count: episodes.len(),
            first_url: episodes.first().and_then(|episode| episode.url.clone()),
            source_ref: novel.ncode.to_lowercase(),
        },
        episodes,
    })
}

async fn fetch_kakuyomu(fetcher: &Fetcher, source: &str) -> Result<TransferDraft, String> {
    let client = &fetcher.client;
    let work_url = source.trim().trim_end_matches('/').to_string();
    if !work_url.contains("kakuyomu.jp/works/") {
        return Err("カクヨムの作品URLを入力してください".to_string());
    }

    let work_html = fetch_html(client, &work_url).await?;
    let (title, author_name, first_url, total) = {
        let work_doc = Html::parse_document(&work_html);
        let title = first_text(
            &work_doc,
            &["h1[class*='Heading_heading']", "h1", "[itemprop='name']"],
        )
        .ok_or_else(|| "作品タイトルを読み取れませんでした".to_string())?;
        let author_name = first_text(
            &work_doc,
            &[
                ".partialGiftWidgetActivityName a",
                "a[href*='/users/']",
                "[itemprop='author']",
            ],
        );
        let first_url = first_attr(
            &work_doc,
            &[
                "a[href*='/episodes/']",
                "a[href*='/works/'][href*='/episodes/']",
            ],
            "href",
        )
        .map(|href| absolute_kakuyomu_url(&href))
        .ok_or_else(|| "最初のエピソードURLを読み取れませんでした".to_string())?;
        let total = count_unique_attr(&work_doc, "a[href*='/episodes/']", "href");
        (title, author_name, first_url, total)
    };

    let mut next_url = Some(first_url.clone());
    let mut episodes = Vec::new();

    while let Some(url) = next_url {
        if !episodes.is_empty() {
            fetcher.wait().await;
        }
        let html = fetch_html(client, &url).await?;
        let number = episodes.len() + 1;
        let (title, content, found_next_url) = {
            let document = Html::parse_document(&html);
            let title = first_text(&document, &[".widget-episodeTitle", "h1"])
                .ok_or_else(|| format!("第{number}話のタイトルを読み取れませんでした"))?;
            let content = extract_body(
                &document,
                &[".widget-episodeBody", "[class*='episodeBody']"],
            )
            .ok_or_else(|| format!("第{number}話の本文を読み取れませんでした"))?;
            let found_next_url = first_attr(&document, &["#contentMain-readNextEpisode"], "href")
                .map(|href| absolute_kakuyomu_url(&href));
            (title, content, found_next_url)
        };
        // 作品ページのリンク数は目安なので、実際の話数が上回ったら不明扱いにする
        fetcher.progress(number, total.filter(|total| *total >= number), &title);

        episodes.push(Episode {
            number,
            title,
            content,
            url: Some(url),
        });

        next_url = found_next_url;
    }

    Ok(TransferDraft {
        work: WorkInfo {
            platform: "kakuyomu".to_string(),
            title,
            author_name,
            episode_count: episodes.len(),
            first_url: Some(first_url),
            source_ref: work_url,
        },
        episodes,
    })
}

async fn fetch_html(client: &Client, url: &str) -> Result<String, String> {
    client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("ページの取得に失敗しました: {error}"))?
        .error_for_status()
        .map_err(|error| format!("ページの取得に失敗しました: {error}"))?
        .text()
        .await
        .map_err(|error| format!("ページの読み込みに失敗しました: {error}"))
}

fn normalize_ncode(source: &str) -> Result<String, String> {
    let value = source.trim().trim_end_matches('/').to_lowercase();
    if value.is_empty() {
        return Err("ncode または作品URLを入力してください".to_string());
    }
    if let Some(index) = value.find("ncode.syosetu.com/") {
        let rest = &value[index + "ncode.syosetu.com/".len()..];
        return rest
            .split('/')
            .next()
            .filter(|part| !part.is_empty())
            .map(|part| part.to_string())
            .ok_or_else(|| "ncode を読み取れませんでした".to_string());
    }
    Ok(value)
}

/// タイトルや作者名など 1 行のテキストを取り出す
fn first_text(document: &Html, selectors: &[&str]) -> Option<String> {
    selectors.iter().find_map(|selector| {
        let selector = Selector::parse(selector).ok()?;
        document.select(&selector).find_map(|element| {
            let text = render_inline(element).replace('\n', "");
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
    })
}

/// 本文を取り出す。両サイトとも 1 行が 1 つの `<p>` なので、それを 1 行として並べる。
/// 行頭の全角スペース（字下げ）と空行はそのまま残す。
/// ルビは `|親文字《ルビ》`、カクヨムの傍点は `《《文字》》` に変換する。
fn extract_body(document: &Html, selectors: &[&str]) -> Option<String> {
    selectors.iter().find_map(|selector| {
        let selector = Selector::parse(selector).ok()?;
        document.select(&selector).find_map(|container| {
            let body = render_block(container);
            (!body.trim().is_empty()).then_some(body)
        })
    })
}

fn render_block(container: ElementRef) -> String {
    let paragraphs: Vec<ElementRef> = container
        .children()
        .filter_map(ElementRef::wrap)
        .filter(|element| element.value().name() == "p")
        .collect();
    let rendered = if paragraphs.is_empty() {
        render_inline(container)
    } else {
        paragraphs
            .into_iter()
            .map(|paragraph| render_inline(paragraph).trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let lines: Vec<&str> = rendered.lines().map(|line| line.trim_end()).collect();
    let start = lines.iter().position(|line| !line.is_empty()).unwrap_or(0);
    let end = lines.iter().rposition(|line| !line.is_empty()).map_or(0, |index| index + 1);
    lines[start..end.max(start)].join("\n")
}

fn render_inline(element: ElementRef) -> String {
    let mut out = String::new();
    push_inline(element, &mut out);
    out
}

fn push_inline(element: ElementRef, out: &mut String) {
    for child in element.children() {
        match child.value() {
            // HTML ソース上の改行は表示上の改行ではないので捨てる（改行は <br> と <p> で表す）
            Node::Text(text) => {
                for ch in text.chars().filter(|ch| *ch != '\n' && *ch != '\r') {
                    // 地の文の《 がルビ記法と誤認されないようにエスケープする
                    if ch == '《' {
                        out.push('|');
                    }
                    out.push(ch);
                }
            }
            Node::Element(value) => {
                let Some(child) = ElementRef::wrap(child) else {
                    continue;
                };
                match value.name() {
                    "br" => out.push('\n'),
                    "rt" | "rp" => {}
                    "ruby" => {
                        let base = render_inline(child);
                        let ruby: String = child
                            .children()
                            .filter_map(ElementRef::wrap)
                            .filter(|element| element.value().name() == "rt")
                            .flat_map(|element| element.text())
                            .collect();
                        if ruby.trim().is_empty() {
                            out.push_str(&base);
                        } else {
                            out.push_str(&format!("|{base}《{}》", ruby.trim()));
                        }
                    }
                    "em" if value.has_class("emphasisDots", CaseSensitivity::CaseSensitive) => {
                        out.push_str(&format!("《《{}》》", render_inline(child)));
                    }
                    _ => push_inline(child, out),
                }
            }
            _ => {}
        }
    }
}

fn first_attr(document: &Html, selectors: &[&str], attr: &str) -> Option<String> {
    selectors.iter().find_map(|selector| {
        let selector = Selector::parse(selector).ok()?;
        document
            .select(&selector)
            .find_map(|element| element.value().attr(attr).map(|value| value.to_string()))
    })
}

fn count_unique_attr(document: &Html, selector: &str, attr: &str) -> Option<usize> {
    let selector = Selector::parse(selector).ok()?;
    let mut values: Vec<&str> = document
        .select(&selector)
        .filter_map(|element| element.value().attr(attr))
        .collect();
    values.sort_unstable();
    values.dedup();
    (!values.is_empty()).then_some(values.len())
}

fn absolute_kakuyomu_url(path_or_url: &str) -> String {
    if path_or_url.starts_with("http") {
        path_or_url.to_string()
    } else {
        format!("https://kakuyomu.jp{path_or_url}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(html: &str) -> String {
        extract_body(&Html::parse_document(html), &[".body"]).unwrap()
    }

    #[test]
    fn keeps_indent_and_blank_lines() {
        let html = r#"<div class="body">
<p id="L1">　俺は34歳住所不定無職。</p>
<p id="L2"><br /></p>
<p id="L3">　人生を後悔している。</p>
</div>"#;
        assert_eq!(body(html), "　俺は34歳住所不定無職。\n\n　人生を後悔している。");
    }

    #[test]
    fn converts_ruby_to_notation() {
        let html = r#"<div class="body">
<p><ruby><rb>聖勇者</rb><rp>（</rp><rt>ホーリー・ブレイブ</rt><rp>）</rp></ruby>。それが名前。</p>
<p><ruby>愛機<rp>(</rp><rt>パソコン</rt><rp>)</rp></ruby>に</p>
</div>"#;
        assert_eq!(body(html), "|聖勇者《ホーリー・ブレイブ》。それが名前。\n|愛機《パソコン》に");
    }

    #[test]
    fn converts_emphasis_dots_and_escapes_brackets() {
        let html = r#"<div class="body"><p><em class="emphasisDots"><span>本</span><span>当</span></em>に《書名》</p></div>"#;
        assert_eq!(body(html), "《《本当》》に|《書名》");
    }
}

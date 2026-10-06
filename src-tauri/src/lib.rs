use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0";

#[derive(Debug, Serialize)]
struct WorkInfo {
    platform: String,
    title: String,
    author_name: Option<String>,
    episode_count: usize,
    first_url: Option<String>,
    source_ref: String,
}

#[derive(Debug, Serialize)]
struct Episode {
    number: usize,
    title: String,
    content: String,
    url: Option<String>,
}

#[derive(Debug, Serialize)]
struct TransferDraft {
    work: WorkInfo,
    episodes: Vec<Episode>,
}

#[derive(Debug, Deserialize)]
struct NarouNovel {
    title: String,
    ncode: String,
    general_all_no: usize,
    writer: Option<String>,
}

#[tauri::command]
async fn fetch_transfer_draft(platform: String, source: String) -> Result<TransferDraft, String> {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;

    match platform.as_str() {
        "narou" => fetch_narou(&client, &source).await,
        "kakuyomu" => fetch_kakuyomu(&client, &source).await,
        _ => Err("対応していない掲載元です".to_string()),
    }
}

#[tauri::command]
fn build_post_url(platform: String, management_url: String) -> Result<String, String> {
    let trimmed = management_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("投稿先URLを入力してください".to_string());
    }

    match platform.as_str() {
        "kakuyomu" => {
            if !trimmed.contains("kakuyomu.jp") {
                return Err("カクヨムの作品管理URLを入力してください".to_string());
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

async fn fetch_narou(client: &Client, source: &str) -> Result<TransferDraft, String> {
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

    let mut episodes = Vec::with_capacity(novel.general_all_no);
    for number in 1..=novel.general_all_no {
        let url = format!("https://ncode.syosetu.com/{}/{}/", novel.ncode.to_lowercase(), number);
        let html = fetch_html(client, &url).await?;
        let document = Html::parse_document(&html);
        let title = first_text(&document, &[".p-novel__title", ".novel_subtitle"])
            .ok_or_else(|| format!("第{number}話のタイトルを読み取れませんでした"))?;
        let content = first_text(&document, &[".p-novel__text", "#novel_honbun"])
            .ok_or_else(|| format!("第{number}話の本文を読み取れませんでした"))?;

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

async fn fetch_kakuyomu(client: &Client, source: &str) -> Result<TransferDraft, String> {
    let work_url = source.trim().trim_end_matches('/').to_string();
    if !work_url.contains("kakuyomu.jp/works/") {
        return Err("カクヨムの作品URLを入力してください".to_string());
    }

    let work_html = fetch_html(client, &work_url).await?;
    let work_doc = Html::parse_document(&work_html);
    let title = first_text(
        &work_doc,
        &[
            "h1[class*='Heading_heading']",
            "h1",
            "[itemprop='name']",
        ],
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

    let mut next_url = Some(first_url.clone());
    let mut episodes = Vec::new();

    while let Some(url) = next_url {
        let html = fetch_html(client, &url).await?;
        let document = Html::parse_document(&html);
        let number = episodes.len() + 1;
        let title = first_text(&document, &[".widget-episodeTitle", "h1"])
            .ok_or_else(|| format!("第{number}話のタイトルを読み取れませんでした"))?;
        let content = first_text(&document, &[".widget-episodeBody", "[class*='episodeBody']"])
            .ok_or_else(|| format!("第{number}話の本文を読み取れませんでした"))?;

        episodes.push(Episode {
            number,
            title,
            content,
            url: Some(url),
        });

        next_url = first_attr(&document, &["#contentMain-readNextEpisode"], "href")
            .map(|href| absolute_kakuyomu_url(&href));
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

fn first_text(document: &Html, selectors: &[&str]) -> Option<String> {
    selectors.iter().find_map(|selector| {
        let selector = Selector::parse(selector).ok()?;
        document.select(&selector).find_map(|element| {
            let text = element.text().collect::<Vec<_>>().join("\n");
            let normalized = normalize_text(&text);
            (!normalized.is_empty()).then_some(normalized)
        })
    })
}

fn first_attr(document: &Html, selectors: &[&str], attr: &str) -> Option<String> {
    selectors.iter().find_map(|selector| {
        let selector = Selector::parse(selector).ok()?;
        document
            .select(&selector)
            .find_map(|element| element.value().attr(attr).map(|value| value.to_string()))
    })
}

fn normalize_text(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn absolute_kakuyomu_url(path_or_url: &str) -> String {
    if path_or_url.starts_with("http") {
        path_or_url.to_string()
    } else {
        format!("https://kakuyomu.jp{path_or_url}")
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            fetch_transfer_draft,
            build_post_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

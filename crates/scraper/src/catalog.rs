//! Browse FitGirl Repacks: popular games, search, and game details.
//!
//! * Popular: the `widget-grid-view-image` cards on `/pop-repacks/`.
//! * Search and details: the site's WordPress REST API
//!   (`/wp-json/wp/v2/posts`). Unlike the HTML search page, it returns
//!   each post's full content, so one request gives covers, specs and
//!   download links. If the API is ever disabled, the HTML pages are used
//!   instead (without covers in search results).

use std::sync::LazyLock;

use fitdl_engine::HttpClient;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};

use crate::{collapse_ws, extract_links, fetch, folder_name, Error, Link, Result};

pub const SITE: &str = "https://fitgirl-repacks.site";
const POPULAR_URL: &str = "https://fitgirl-repacks.site/pop-repacks/";
const PER_PAGE: u32 = 12;
/// "Updates Digest" posts are changelogs, not games.
const EXCLUDED_CATEGORIES: &str = "46";

/// A game tile on the popular page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    pub title: String,
    pub url: String,
    pub slug: String,
    pub cover: Option<String>,
    pub adult: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    /// "Today", "This week" or "This month" (or the page's own heading).
    pub name: String,
    pub cards: Vec<Card>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSummary {
    pub id: u64,
    pub slug: String,
    pub url: String,
    /// Full post title, e.g. "Avatar: Frontiers of Pandora – Complete Edition, v2.7 + 10 DLCs".
    pub title: String,
    /// The game's name, e.g. "Avatar: Frontiers of Pandora".
    pub name: String,
    /// Edition/version part of the title, e.g. "Complete Edition, v2.7 + 10 DLCs".
    pub version: Option<String>,
    pub date: Option<String>,
    pub cover: Option<String>,
    pub repack_size: Option<String>,
    pub original_size: Option<String>,
    pub genres: Vec<String>,
    pub companies: Option<String>,
    pub languages: Option<String>,
    pub adult: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Screenshot {
    pub thumb: String,
    pub full: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameDetails {
    #[serde(flatten)]
    pub summary: GameSummary,
    /// Folder name for downloads.
    pub game: String,
    pub description: Option<String>,
    pub screenshots: Vec<Screenshot>,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchPage {
    pub query: String,
    pub results: Vec<GameSummary>,
    pub page: u32,
    pub total: Option<u64>,
    pub total_pages: u32,
}

/// A post as returned by `/wp-json/wp/v2/posts`.
#[derive(Debug, Clone, Deserialize)]
pub struct Post {
    pub id: u64,
    pub slug: String,
    pub link: String,
    #[serde(default)]
    pub date: Option<String>,
    pub title: Rendered,
    pub content: Rendered,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rendered {
    pub rendered: String,
}

// ---------------------------------------------------------------- popular

pub async fn popular(client: &HttpClient) -> Result<Vec<Section>> {
    let mut sections = parse_popular(&fetch(client, POPULAR_URL).await?.text().await?);
    // Titles rarely say "18+"; the site's "Adult" tag is reliable. Best effort.
    if let Ok(adult) = adult_slugs(client, &sections).await {
        for card in sections.iter_mut().flat_map(|s| s.cards.iter_mut()) {
            card.adult |= adult.contains(&card.slug);
        }
    }
    Ok(sections)
}

/// Slugs of the given cards that carry the site's "adult" tag, using two
/// small REST calls (tag id, then the tags of all cards at once).
async fn adult_slugs(client: &HttpClient, sections: &[Section]) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Tag {
        id: u64,
    }
    #[derive(Deserialize)]
    struct Tagged {
        slug: String,
        tags: Vec<u64>,
    }

    let tags: Vec<Tag> = fetch(
        client,
        &format!("{SITE}/wp-json/wp/v2/tags?slug=adult&_fields=id"),
    )
    .await?
    .json()
    .await?;
    let Some(adult) = tags.first().map(|t| t.id) else {
        return Ok(Vec::new());
    };
    let mut slugs: Vec<&str> = sections
        .iter()
        .flat_map(|s| s.cards.iter().map(|c| c.slug.as_str()))
        .filter(|s| !s.is_empty())
        .collect();
    slugs.sort_unstable();
    slugs.dedup();

    let mut result = Vec::new();
    for chunk in slugs.chunks(100) {
        let api = url::Url::parse_with_params(
            &format!("{SITE}/wp-json/wp/v2/posts"),
            &[
                ("slug", chunk.join(",").as_str()),
                ("per_page", "100"),
                ("_fields", "slug,tags"),
            ],
        )
        .expect("valid url");
        let posts: Vec<Tagged> = fetch(client, api.as_str()).await?.json().await?;
        result.extend(
            posts
                .into_iter()
                .filter(|p| p.tags.contains(&adult))
                .map(|p| p.slug),
        );
    }
    Ok(result)
}

/// Cards grouped by the `h2.widgettitle` heading above them, ordered
/// Today, This week, This month.
pub fn parse_popular(html: &str) -> Vec<Section> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse("h2.widgettitle, div.widget-grid-view-image a[href]").unwrap();
    let img = Selector::parse("img").unwrap();

    let mut sections: Vec<Section> = Vec::new();
    for el in doc.select(&sel) {
        if el.value().name() == "h2" {
            let name = section_name(&text_of(el));
            sections.push(Section {
                name,
                cards: Vec::new(),
            });
            continue;
        }
        let Some(section) = sections.last_mut() else {
            continue;
        };
        let url = el.value().attr("href").unwrap_or_default().to_owned();
        if section.cards.iter().any(|c| c.url == url) {
            continue;
        }
        let image = el.select(&img).next();
        let title = el
            .value()
            .attr("title")
            .or_else(|| image.and_then(|i| i.value().attr("alt")))
            .map(decode)
            .unwrap_or_default();
        let cover = image
            .and_then(|i| i.value().attr("src"))
            .map(|src| sized_cover(src, 300, 400));
        section.cards.push(Card {
            slug: slug_of(&url).unwrap_or_default(),
            adult: is_adult(&title, &[]),
            title,
            url,
            cover,
        });
    }
    sections.retain(|s| !s.cards.is_empty());
    sections.sort_by_key(|s| section_rank(&s.name));
    sections
}

fn section_name(heading: &str) -> String {
    let h = heading.to_ascii_lowercase();
    if h.contains("today") {
        "Today".into()
    } else if h.contains("week") {
        "This week".into()
    } else if h.contains("month") {
        "This month".into()
    } else {
        heading.to_owned()
    }
}

fn section_rank(name: &str) -> u8 {
    match name {
        "Today" => 0,
        "This week" => 1,
        "This month" => 2,
        _ => 3,
    }
}

// ----------------------------------------------------------------- search

pub async fn search(client: &HttpClient, query: &str, page: u32) -> Result<SearchPage> {
    let page = page.max(1);
    let api = url::Url::parse_with_params(
        &format!("{SITE}/wp-json/wp/v2/posts"),
        &[
            ("search", query),
            ("orderby", "relevance"),
            ("per_page", &PER_PAGE.to_string()),
            ("page", &page.to_string()),
            ("categories_exclude", EXCLUDED_CATEGORIES),
            ("_fields", "id,slug,link,date,title,content"),
        ],
    )
    .expect("valid url");

    match fetch(client, api.as_str()).await {
        Ok(resp) => {
            let header = |name: &str| {
                resp.headers()
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
            };
            let total = header("x-wp-total");
            let total_pages = header("x-wp-totalpages").unwrap_or(0) as u32;
            let posts: Vec<Post> = resp.json().await?;
            Ok(SearchPage {
                query: query.to_owned(),
                results: posts.iter().map(summary_from_post).collect(),
                page,
                total,
                total_pages,
            })
        }
        // WordPress answers 400 for a page past the end.
        Err(Error::Status(400)) => Ok(SearchPage {
            query: query.to_owned(),
            page,
            ..Default::default()
        }),
        Err(Error::Status(_)) => search_html(client, query, page).await,
        Err(e) => Err(e),
    }
}

/// Fallback: the regular `/?s=` search page (no cover images).
async fn search_html(client: &HttpClient, query: &str, page: u32) -> Result<SearchPage> {
    let base = if page > 1 {
        format!("{SITE}/page/{page}/")
    } else {
        format!("{SITE}/")
    };
    let url = url::Url::parse_with_params(&base, &[("s", query)]).expect("valid url");
    let html = fetch(client, url.as_str()).await?.text().await?;
    let (results, has_next) = parse_search_html(&html);
    Ok(SearchPage {
        query: query.to_owned(),
        results,
        page,
        total: None,
        total_pages: if has_next { page + 1 } else { page },
    })
}

pub fn parse_search_html(html: &str) -> (Vec<GameSummary>, bool) {
    let doc = Html::parse_document(html);
    let article = Selector::parse("article").unwrap();
    let heading = Selector::parse("h1.entry-title a, h2.entry-title a").unwrap();
    let summary = Selector::parse(".entry-summary, .entry-content").unwrap();
    let time = Selector::parse("time[datetime]").unwrap();
    let next = Selector::parse("a.next, .nav-previous a").unwrap();

    let mut results = Vec::new();
    for a in doc.select(&article) {
        let Some(link) = a.select(&heading).next() else {
            continue;
        };
        let url = link.value().attr("href").unwrap_or_default().to_owned();
        let title = text_of(link);
        let body = a
            .select(&summary)
            .next()
            .map(|s| s.html())
            .unwrap_or_default();
        let mut s = summarize(0, &url, &title, &body);
        s.date = a
            .select(&time)
            .next()
            .and_then(|t| t.value().attr("datetime"))
            .map(str::to_owned);
        if url.contains("updates-digest") {
            continue;
        }
        results.push(s);
    }
    let has_next = doc.select(&next).next().is_some();
    (results, has_next)
}

// ---------------------------------------------------------------- details

/// Details of one game, by slug or page URL.
pub async fn game(client: &HttpClient, slug_or_url: &str) -> Result<GameDetails> {
    let slug = slug_of(slug_or_url).unwrap_or_else(|| slug_or_url.trim_matches('/').to_owned());
    let api = url::Url::parse_with_params(
        &format!("{SITE}/wp-json/wp/v2/posts"),
        &[
            ("slug", slug.as_str()),
            ("_fields", "id,slug,link,date,title,content"),
        ],
    )
    .expect("valid url");

    match fetch(client, api.as_str()).await {
        Ok(resp) => {
            let posts: Vec<Post> = resp.json().await?;
            posts.first().map(details_from_post).ok_or(Error::NotFound)
        }
        Err(Error::Status(_)) => {
            let url = format!("{SITE}/{slug}/");
            let html = fetch(client, &url).await?.text().await?;
            details_from_page(&url, &html).ok_or(Error::NotFound)
        }
        Err(e) => Err(e),
    }
}

pub fn summary_from_post(post: &Post) -> GameSummary {
    let mut s = summarize(
        post.id,
        &post.link,
        &decode(&post.title.rendered),
        &post.content.rendered,
    );
    s.slug = post.slug.clone();
    s.date = post.date.clone();
    s
}

pub fn details_from_post(post: &Post) -> GameDetails {
    details(summary_from_post(post), &post.content.rendered)
}

/// Fallback for when the REST API is unavailable: parse the article page.
pub fn details_from_page(url: &str, html: &str) -> Option<GameDetails> {
    let doc = Html::parse_document(html);
    let title = doc
        .select(&Selector::parse("h1.entry-title").unwrap())
        .next()
        .map(text_of)?;
    let content = doc
        .select(&Selector::parse(".entry-content").unwrap())
        .next()?
        .html();
    Some(details(summarize(0, url, &title, &content), &content))
}

fn details(summary: GameSummary, content: &str) -> GameDetails {
    let doc = Html::parse_fragment(content);
    GameDetails {
        game: folder_name(&summary.title),
        description: description(&doc),
        screenshots: screenshots(&doc),
        links: extract_links(&doc),
        summary,
    }
}

/// Build a summary from a title and the post's HTML.
fn summarize(id: u64, url: &str, title: &str, content: &str) -> GameSummary {
    let (name, version) = split_title(title);
    let genres: Vec<String> = field(content, "Genres/Tags:")
        .map(|g| {
            g.split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    GameSummary {
        id,
        slug: slug_of(url).unwrap_or_default(),
        url: url.to_owned(),
        name,
        version,
        date: None,
        cover: cover(content),
        repack_size: field(content, "Repack Size:").map(|s| strip_brackets(&s)),
        original_size: field(content, "Original Size:"),
        companies: field(content, "Company:").or_else(|| field(content, "Companies:")),
        languages: field(content, "Languages:"),
        adult: is_adult(title, &genres),
        genres,
        title: title.to_owned(),
    }
}

// ---------------------------------------------------------------- helpers

static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<[^>]*>").unwrap());
static IMG_SRC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<img\b[^>]*?\bsrc=["']([^"']+)["']"#).unwrap());

/// "Name – Edition, v1.2" → ("Name", Some("Edition, v1.2")).
fn split_title(title: &str) -> (String, Option<String>) {
    for sep in [" \u{2013} ", " \u{2014} ", " - "] {
        if let Some((name, rest)) = title.split_once(sep) {
            return (
                name.trim().to_owned(),
                Some(rest.trim().to_owned()).filter(|v| !v.is_empty()),
            );
        }
    }
    // "Game, v1.0.3" style titles.
    if let Some(idx) = title.find(", v") {
        return (
            title[..idx].trim().to_owned(),
            Some(title[idx + 2..].trim().to_owned()),
        );
    }
    (title.trim().to_owned(), None)
}

/// Value after a label like `Repack Size:` up to the end of its line.
fn field(content: &str, label: &str) -> Option<String> {
    let start = content.find(label)? + label.len();
    let rest = &content[start..];
    let end = ["<br", "</p", "<p", "<h3", "\n"]
        .iter()
        .filter_map(|m| rest.find(m))
        .min()
        .unwrap_or(rest.len());
    let value = collapse_ws(&decode(&TAG.replace_all(&rest[..end], "")));
    (!value.is_empty()).then_some(value)
}

/// "from 78.5 GB [Selective Download]" → "from 78.5 GB".
fn strip_brackets(s: &str) -> String {
    s.split('[').next().unwrap_or(s).trim().to_owned()
}

/// The post's cover is its first image.
fn cover(content: &str) -> Option<String> {
    IMG_SRC
        .captures_iter(content)
        .map(|c| decode(&c[1]))
        .find(|src| !src.contains("torrent-stats") && !src.contains("fg_updates"))
        .map(|src| sized_cover(&src, 300, 0))
}

/// Serve covers through WordPress's Photon CDN (i0.wp.com): much faster
/// than the original hosts and resized for the grid.
pub fn sized_cover(src: &str, width: u32, height: u32) -> String {
    let Ok(mut url) = url::Url::parse(src) else {
        return src.to_owned();
    };
    if url.scheme() == "http" {
        let _ = url.set_scheme("https");
    }
    let host = url.host_str().unwrap_or_default().to_owned();
    let (path, keep): (String, Vec<(String, String)>) = if host.ends_with(".wp.com") {
        let keep = url
            .query_pairs()
            .filter(|(k, _)| k != "resize" && k != "w" && k != "fit")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        (format!("https://{host}{}", url.path()), keep)
    } else {
        (
            format!("https://i0.wp.com/{host}{}", url.path()),
            vec![("ssl".into(), "1".into())],
        )
    };
    let mut params = keep;
    if height > 0 {
        params.push(("resize".into(), format!("{width},{height}")));
    } else {
        params.push(("w".into(), width.to_string()));
    }
    url::Url::parse_with_params(&path, &params)
        .map(|u| u.to_string())
        .unwrap_or(path)
}

/// The "Game Description" spoiler, as plain text with line breaks.
fn description(doc: &Html) -> Option<String> {
    let spoiler = Selector::parse(".su-spoiler").unwrap();
    let title = Selector::parse(".su-spoiler-title").unwrap();
    let content = Selector::parse(".su-spoiler-content").unwrap();
    let block = doc.select(&spoiler).find(|s| {
        s.select(&title)
            .next()
            .is_some_and(|t| text_of(t).to_ascii_lowercase().contains("description"))
    })?;
    let html = block.select(&content).next()?.inner_html();
    let with_breaks = Regex::new(r"(?i)<br\s*/?>|</p>|</li>|</h\d>")
        .unwrap()
        .replace_all(&html, "\n");
    let bullets = Regex::new(r"(?i)<li[^>]*>")
        .unwrap()
        .replace_all(&with_breaks, "\n• ");
    let text = decode(&TAG.replace_all(&bullets, ""));
    let lines: Vec<String> = text
        .lines()
        .map(collapse_ws)
        .filter(|l| !l.is_empty())
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// riotpixels screenshots: thumbnails are `….jpg.240p.jpg`; the same image
/// exists as `….jpg.720p.jpg` (about 400 KB, vs 1.5 MB for the original).
fn screenshots(doc: &Html) -> Vec<Screenshot> {
    let img = Selector::parse("img[src]").unwrap();
    let mut shots: Vec<Screenshot> = Vec::new();
    for el in doc.select(&img) {
        let src = el.value().attr("src").unwrap_or_default();
        if !src.contains("riotpixels") {
            continue;
        }
        let thumb = src.replacen("http://", "https://", 1);
        let full = thumb
            .strip_suffix(".240p.jpg")
            .map(|base| format!("{base}.720p.jpg"))
            .unwrap_or_else(|| thumb.clone());
        if !shots.iter().any(|s| s.full == full) {
            shots.push(Screenshot { thumb, full });
        }
    }
    shots
}

fn is_adult(title: &str, genres: &[String]) -> bool {
    title.contains("18+")
        || genres.iter().any(|g| {
            matches!(
                g.to_ascii_lowercase().as_str(),
                "adult" | "nudity" | "sexual content" | "hentai"
            )
        })
}

/// First path segment of a fitgirl-repacks.site URL.
pub fn slug_of(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    if !u.host_str()?.ends_with("fitgirl-repacks.site") {
        return None;
    }
    let slug = u.path_segments()?.find(|s| !s.is_empty())?.to_owned();
    Some(slug)
}

fn text_of(el: ElementRef) -> String {
    collapse_ws(&el.text().collect::<String>())
}

/// Decode HTML entities (`&#8211;`, `&amp;`, …).
fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    Html::parse_fragment(&format!("<p>{}</p>", s.replace('<', "&lt;")))
        .root_element()
        .text()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_titles() {
        assert_eq!(
            split_title("Avatar: Frontiers of Pandora \u{2013} Complete Edition, v2.7 + 10 DLCs"),
            (
                "Avatar: Frontiers of Pandora".into(),
                Some("Complete Edition, v2.7 + 10 DLCs".into())
            )
        );
        assert_eq!(
            split_title("Hades II, v1.0.3"),
            ("Hades II".into(), Some("v1.0.3".into()))
        );
        assert_eq!(split_title("Tetris"), ("Tetris".into(), None));
    }

    #[test]
    fn decodes_entities() {
        assert_eq!(
            decode("Lou&#8217;s &amp; Co &#8211; v1"),
            "Lou\u{2019}s & Co \u{2013} v1"
        );
        assert_eq!(decode("plain"), "plain");
    }

    #[test]
    fn covers_go_through_photon() {
        assert_eq!(
            sized_cover("https://i3.imageban.ru/out/2026/a.jpg", 300, 0),
            "https://i0.wp.com/i3.imageban.ru/out/2026/a.jpg?ssl=1&w=300"
        );
        assert_eq!(
            sized_cover(
                "https://i0.wp.com/i3.imageban.ru/out/a.jpg?resize=150%2C200&ssl=1",
                300,
                400
            ),
            "https://i0.wp.com/i3.imageban.ru/out/a.jpg?ssl=1&resize=300%2C400"
        );
    }

    #[test]
    fn reads_fields() {
        let html = "Genres/Tags: <a href=x>Action</a>, <a>RPG</a><br /> Repack Size: <strong>from 78.5 GB</strong> [Selective Download] <p></p>";
        assert_eq!(field(html, "Genres/Tags:").as_deref(), Some("Action, RPG"));
        assert_eq!(
            field(html, "Repack Size:")
                .map(|s| strip_brackets(&s))
                .as_deref(),
            Some("from 78.5 GB")
        );
        assert_eq!(field(html, "Languages:"), None);
    }

    #[test]
    fn slugs() {
        assert_eq!(
            slug_of("https://fitgirl-repacks.site/avatar-frontiers-of-pandora/").as_deref(),
            Some("avatar-frontiers-of-pandora")
        );
        assert_eq!(slug_of("https://example.com/x/"), None);
    }
}

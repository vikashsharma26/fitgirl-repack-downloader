//! Scrapes FitGirl Repacks game pages for fuckingfast.co download links and
//! resolves those links into direct, downloadable URLs.

pub mod catalog;
pub mod fuckingfast;
mod names;

use fitdl_engine::HttpClient;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use names::{folder_name, sanitize_filename};

#[derive(Debug, Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server returned HTTP {0}")]
    Status(u16),
    #[error(
        "blocked by a Cloudflare challenge; resolve the link in the browser extension instead"
    )]
    Challenge,
    #[error("file not found on the host (removed or wrong link)")]
    NotFound,
    #[error("no download link found on the page")]
    NoDownloadLink,
    #[error("not a supported link: {0}")]
    Unsupported(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// One downloadable file listed on a game page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    /// Landing-page URL, e.g. `https://fuckingfast.co/pxf38j0jh5xp#name.rar`.
    pub url: String,
    /// The anchor text, as shown on the page.
    pub label: String,
    /// File name to save as (safe on Windows).
    pub filename: String,
    /// FitGirl's optional packs (`fg-optional-*`: language packs, bonus content).
    pub optional: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Page {
    /// Full post title, e.g. "Avatar: Frontiers of Pandora – Complete Edition, ...".
    pub title: Option<String>,
    /// Short game name usable as a folder name, e.g. "Avatar Frontiers of Pandora".
    pub game: Option<String>,
    pub links: Vec<Link>,
}

/// Is this a fuckingfast.co link (landing page or direct)?
pub fn is_fuckingfast(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|h| h == "fuckingfast.co" || h.ends_with(".fuckingfast.co"))
}

/// Parse a full game page: title plus every fuckingfast.co link.
pub fn parse_page(html: &str) -> Page {
    let doc = Html::parse_document(html);
    let heading = Selector::parse("h1.entry-title").unwrap();
    let title = doc
        .select(&heading)
        .next()
        .map(|h| collapse_ws(&h.text().collect::<String>()))
        .filter(|t| !t.is_empty());
    Page {
        game: title.as_deref().map(folder_name),
        title,
        links: extract_links(&doc),
    }
}

/// Collect every `<a>` whose href points at fuckingfast.co, keeping its text.
pub fn extract_links(doc: &Html) -> Vec<Link> {
    let anchors = Selector::parse("a[href]").unwrap();
    let mut links: Vec<Link> = Vec::new();
    for a in doc.select(&anchors) {
        let Some(href) = a.value().attr("href").map(str::trim) else {
            continue;
        };
        if !is_fuckingfast(href) || links.iter().any(|l| l.url == href) {
            continue;
        }
        let label = collapse_ws(&a.text().collect::<String>());
        let filename = filename_for(href, &label);
        links.push(Link {
            url: href.to_owned(),
            optional: filename.to_ascii_lowercase().starts_with("fg-optional"),
            label,
            filename,
        });
    }
    links
}

/// The real file name is in the URL fragment (`#Name_--_x.part01.rar`); the
/// anchor text is a prettified copy (`--` shown as `–`), so it is only a
/// fallback. Using the real name keeps FitGirl's MD5 check files valid.
pub fn filename_for(url: &str, label: &str) -> String {
    let from_fragment = url::Url::parse(url)
        .ok()
        .and_then(|u| u.fragment().map(str::to_owned))
        .map(|f| {
            percent_encoding::percent_decode_str(&f)
                .decode_utf8_lossy()
                .into_owned()
        })
        .filter(|f| !f.trim().is_empty());
    sanitize_filename(from_fragment.as_deref().unwrap_or(label))
}

/// Download and parse a game page.
pub async fn fetch_page(client: &HttpClient, url: &str) -> Result<Page> {
    Ok(parse_page(&fetch(client, url).await?.text().await?))
}

/// GET with the error handling every FitGirl request needs.
pub(crate) async fn fetch(client: &HttpClient, url: &str) -> Result<reqwest::Response> {
    let resp = client.get(url).send().await?;
    if resp.status().as_u16() == 403 && resp.headers().contains_key("cf-mitigated") {
        return Err(Error::Challenge);
    }
    if !resp.status().is_success() {
        return Err(Error::Status(resp.status().as_u16()));
    }
    Ok(resp)
}

pub(crate) fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

//! Turns a fuckingfast.co landing page (`https://fuckingfast.co/<id>`) into
//! a direct download URL (`https://dl.fuckingfast.co/dl/...`).
//!
//! The landing page's DOWNLOAD button is an htmx `hx-post` to `/f/<id>/go`,
//! answered with an `HX-Redirect` header holding the direct link. Older pages
//! embedded the link in `window.open("...")`, which is kept as a fallback.
//!
//! Direct links are signed and short-lived, so resolve right before use.

use std::sync::LazyLock;

use fitdl_engine::HttpClient;
use regex::Regex;
use reqwest::header::{ORIGIN, REFERER};
use reqwest::StatusCode;

use crate::{Error, Result};

const BASE: &str = "https://fuckingfast.co";

static LEGACY_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"window\.open\(\s*["'](https?://(?:[a-z0-9-]+\.)?fuckingfast\.co/dl/[^"']+)["']"#)
        .unwrap()
});

/// The file id of a landing-page URL, or `None` for direct or unrelated URLs.
pub fn file_id(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    if u.host_str()? != "fuckingfast.co" {
        return None;
    }
    let mut segs = u.path_segments()?.filter(|s| !s.is_empty());
    let id = segs.next()?;
    let valid = segs.next().is_none() && id.chars().all(|c| c.is_ascii_alphanumeric());
    valid.then(|| id.to_owned())
}

/// Already a direct download URL?
pub fn is_direct(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| {
        u.host_str()
            .is_some_and(|h| h == "fuckingfast.co" || h.ends_with(".fuckingfast.co"))
            && u.path().starts_with("/dl/")
    })
}

/// Direct download URL for a landing page (direct URLs are returned as is).
pub async fn resolve(client: &HttpClient, url: &str) -> Result<String> {
    if is_direct(url) {
        return Ok(url.to_owned());
    }
    let id = file_id(url).ok_or_else(|| Error::Unsupported(url.to_owned()))?;
    let page = format!("{BASE}/{id}");

    let resp = client
        .post(&format!("{BASE}/f/{id}/go"))
        .header("HX-Request", "true")
        .header("HX-Current-URL", &page)
        .header(REFERER, &page)
        .header(ORIGIN, BASE)
        .send()
        .await?;
    check(&resp)?;
    if let Some(link) = resp
        .headers()
        .get("hx-redirect")
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.starts_with("http"))
    {
        return Ok(link.to_owned());
    }

    // Fallback: older page layout with the link in an inline script.
    let resp = client.get(&page).send().await?;
    check(&resp)?;
    parse_legacy(&resp.text().await?).ok_or(Error::NoDownloadLink)
}

pub fn parse_legacy(html: &str) -> Option<String> {
    LEGACY_LINK.captures(html).map(|c| c[1].to_owned())
}

fn check(resp: &reqwest::Response) -> Result<()> {
    match resp.status() {
        s if s.is_success() => Ok(()),
        StatusCode::FORBIDDEN if resp.headers().contains_key("cf-mitigated") => {
            Err(Error::Challenge)
        }
        StatusCode::NOT_FOUND | StatusCode::GONE => Err(Error::NotFound),
        s => Err(Error::Status(s.as_u16())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_file_id() {
        assert_eq!(
            file_id("https://fuckingfast.co/pxf38j0jh5xp#Game_--_.part01.rar").as_deref(),
            Some("pxf38j0jh5xp")
        );
        assert_eq!(file_id("https://fuckingfast.co/dl/abc"), None);
        assert_eq!(file_id("https://example.com/pxf38j0jh5xp"), None);
    }

    #[test]
    fn detects_direct_links() {
        assert!(is_direct("https://dl.fuckingfast.co/dl/FjnVfF8N"));
        assert!(is_direct("https://fuckingfast.co/dl/FjnVfF8N"));
        assert!(!is_direct("https://fuckingfast.co/pxf38j0jh5xp"));
    }

    #[test]
    fn legacy_page_link() {
        let html = r#"<script>function d(){ window.open("https://fuckingfast.co/dl/AbC_123-x") }</script>"#;
        assert_eq!(
            parse_legacy(html).as_deref(),
            Some("https://fuckingfast.co/dl/AbC_123-x")
        );
        assert_eq!(parse_legacy("<html></html>"), None);
    }
}

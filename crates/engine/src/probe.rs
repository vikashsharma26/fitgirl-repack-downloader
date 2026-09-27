use percent_encoding::percent_decode_str;
use reqwest::header::{HeaderMap, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, ETAG, RANGE};
use reqwest::StatusCode;

use crate::http::HttpClient;

use crate::error::{Error, Result};

/// What the server told us about a file before downloading it.
#[derive(Debug, Clone)]
pub struct RemoteInfo {
    pub size: Option<u64>,
    pub supports_ranges: bool,
    pub filename: Option<String>,
    pub etag: Option<String>,
}

/// Probe with a 1-byte range request. Unlike HEAD this is answered by every
/// server the same way a real segment request is, so it tells us reliably
/// whether ranged (multi-connection) downloading will work.
pub async fn probe(client: &HttpClient, url: &str) -> Result<RemoteInfo> {
    let resp = client.get(url).header(RANGE, "bytes=0-0").send().await?;
    let status = resp.status();
    check_status(status)?;

    let headers = resp.headers();
    let filename = headers
        .get(CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_content_disposition);
    let etag = headers
        .get(ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let info = if status == StatusCode::PARTIAL_CONTENT {
        let range = content_range(headers)
            .ok_or_else(|| Error::BadRange("206 without a Content-Range header".into()))?;
        RemoteInfo {
            size: range.total,
            supports_ranges: range.start == 0 && range.total.is_some(),
            filename,
            etag,
        }
    } else {
        RemoteInfo {
            size: content_length(headers),
            supports_ranges: false,
            filename,
            etag,
        }
    };
    // Dropping `resp` without reading the body is fine: at most a few bytes.
    Ok(info)
}

pub(crate) fn check_status(status: StatusCode) -> Result<()> {
    match status.as_u16() {
        200..=299 => Ok(()),
        401 | 403 | 404 | 410 => Err(Error::LinkExpired(status.as_u16())),
        code => Err(Error::Status(code)),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ContentRange {
    pub start: u64,
    pub total: Option<u64>,
}

/// Parse `Content-Range: bytes <start>-<end>/<total>`.
///
/// Only `start` and `total` are used: some hosts (fuckingfast.co among them)
/// always report `<end>` as the last byte of the file even when they correctly
/// send just the requested range, so trusting `<end>` would break downloads.
pub(crate) fn content_range(headers: &HeaderMap) -> Option<ContentRange> {
    parse_content_range(headers.get(CONTENT_RANGE)?.to_str().ok()?)
}

pub(crate) fn parse_content_range(value: &str) -> Option<ContentRange> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let (start, _end) = range.split_once('-')?;
    Some(ContentRange {
        start: start.trim().parse().ok()?,
        total: total.trim().parse().ok(),
    })
}

pub(crate) fn content_length(headers: &HeaderMap) -> Option<u64> {
    headers.get(CONTENT_LENGTH)?.to_str().ok()?.parse().ok()
}

/// Extract a filename from a Content-Disposition value, preferring the
/// RFC 5987 `filename*=UTF-8''...` form.
pub fn parse_content_disposition(value: &str) -> Option<String> {
    let mut plain = None;
    for part in value.split(';').map(str::trim) {
        let Some((key, val)) = part.split_once('=') else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "filename*" => {
                let val = val.trim().trim_matches('"');
                let encoded = val.split_once("''").map_or(val, |(_, v)| v);
                if let Ok(s) = percent_decode_str(encoded).decode_utf8() {
                    if !s.is_empty() {
                        return Some(s.into_owned());
                    }
                }
            }
            "filename" => plain = Some(val.trim().trim_matches('"').to_owned()),
            _ => {}
        }
    }
    plain.filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_range_ignores_bogus_end() {
        let r = parse_content_range("bytes 1000000000-2097151999/2097152000").unwrap();
        assert_eq!(r.start, 1_000_000_000);
        assert_eq!(r.total, Some(2_097_152_000));
    }

    #[test]
    fn content_range_unknown_total() {
        let r = parse_content_range("bytes 0-99/*").unwrap();
        assert_eq!(r.total, None);
    }

    #[test]
    fn disposition_rfc5987() {
        let v = "attachment; filename*=UTF-8''Avatar_--_fitgirl-repacks.site_--_.part01.rar";
        assert_eq!(
            parse_content_disposition(v).unwrap(),
            "Avatar_--_fitgirl-repacks.site_--_.part01.rar"
        );
        let v = "attachment; filename=\"a b.bin\"; filename*=UTF-8''a%20b%C3%A9.bin";
        assert_eq!(parse_content_disposition(v).unwrap(), "a bé.bin");
        assert_eq!(
            parse_content_disposition("attachment; filename=\"x.rar\"").unwrap(),
            "x.rar"
        );
        assert_eq!(parse_content_disposition("inline"), None);
    }
}

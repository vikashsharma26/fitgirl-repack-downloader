/// Make a string safe to use as a file name on Windows (and everywhere else).
pub fn sanitize_filename(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    // Windows silently strips trailing dots and spaces.
    let trimmed = out.trim().trim_end_matches('.').trim_end();
    out = trimmed.to_owned();
    if out.is_empty() {
        out = "download".to_owned();
    }
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.as_str()) {
        out.insert(0, '_');
    }
    if out.len() > 200 {
        let mut cut = 200;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
    }
    out
}

/// Folder name from a post title: "Avatar: Frontiers of Pandora – Complete
/// Edition, v2.7" becomes "Avatar Frontiers of Pandora".
pub fn folder_name(title: &str) -> String {
    let short = title
        .split(['\u{2013}', '\u{2014}'])
        .next()
        .unwrap_or(title)
        .split(" - ")
        .next()
        .unwrap_or(title);
    let cleaned: String = short
        .chars()
        .filter(|c| !matches!(c, ':' | '"' | '?' | '*' | '<' | '>' | '|'))
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    sanitize_filename(&collapsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_windows_names() {
        assert_eq!(sanitize_filename("a:b/c?.rar"), "a_b_c_.rar");
        assert_eq!(sanitize_filename("name. "), "name");
        assert_eq!(sanitize_filename("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_filename(""), "download");
    }

    #[test]
    fn folder_from_title() {
        assert_eq!(
            folder_name(
                "Avatar: Frontiers of Pandora \u{2013} Complete Edition, v2.7 + 10 DLCs/Bonuses"
            ),
            "Avatar Frontiers of Pandora"
        );
        assert_eq!(folder_name("Hades II - v1.0"), "Hades II");
    }
}

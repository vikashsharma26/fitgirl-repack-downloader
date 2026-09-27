use fitdl_scraper::{is_fuckingfast, parse_page};

const PAGE: &str = include_str!("fixtures/game_page.html");

#[test]
fn finds_fuckingfast_links_only() {
    let page = parse_page(PAGE);
    assert_eq!(page.links.len(), 6, "{:#?}", page.links);
    assert!(page.links.iter().all(|l| is_fuckingfast(&l.url)));
}

#[test]
fn keeps_anchor_text_and_real_filename() {
    let page = parse_page(PAGE);
    let first = &page.links[0];
    assert_eq!(
        first.url,
        "https://fuckingfast.co/aaaa1111bbbb#Sample_Game_--_fitgirl-repacks.site_--_.part01.rar"
    );
    // Anchor text as displayed (with the en dash)...
    assert_eq!(
        first.label,
        "Sample_Game_\u{2013}_fitgirl-repacks.site_\u{2013}_.part01.rar"
    );
    // ...but saved under the real name from the URL fragment.
    assert_eq!(
        first.filename,
        "Sample_Game_--_fitgirl-repacks.site_--_.part01.rar"
    );
    assert!(!first.optional);
}

#[test]
fn falls_back_to_anchor_text_for_filename() {
    let page = parse_page(PAGE);
    let last = page.links.last().unwrap();
    assert_eq!(last.filename, "Mirror without fragment");
}

#[test]
fn flags_optional_files() {
    let page = parse_page(PAGE);
    let optional: Vec<_> = page
        .links
        .iter()
        .filter(|l| l.optional)
        .map(|l| l.filename.as_str())
        .collect();
    assert_eq!(
        optional,
        ["fg-optional-bonus-content.bin", "fg-optional-french-vo.bin"]
    );
}

#[test]
fn reads_title_and_game_folder() {
    let page = parse_page(PAGE);
    assert_eq!(
        page.title.as_deref(),
        Some("Sample Game: Deluxe \u{2013} Complete Edition, v1.2 + 3 DLCs")
    );
    assert_eq!(page.game.as_deref(), Some("Sample Game Deluxe"));
}

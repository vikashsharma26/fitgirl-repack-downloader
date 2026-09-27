use fitdl_scraper::catalog::{details_from_post, parse_popular, parse_search_html, Post};

const POPULAR: &str = include_str!("fixtures/pop_repacks.html");
const POST: &str = include_str!("fixtures/post.json");

#[test]
fn popular_sections_in_order_without_duplicates() {
    let sections = parse_popular(POPULAR);
    let names: Vec<_> = sections.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Today", "This week", "This month"]);
    assert_eq!(
        sections[1].cards.len(),
        1,
        "duplicate card in a section is dropped"
    );
    assert_eq!(sections[2].cards.len(), 2);
}

#[test]
fn popular_cards() {
    let sections = parse_popular(POPULAR);
    let month = &sections[2];
    let card = &month.cards[0];
    assert_eq!(
        card.title,
        "Sample Quest \u{2013} Deluxe Edition, v1.2 + 3 DLCs"
    );
    assert_eq!(card.slug, "sample-quest");
    assert_eq!(
        card.cover.as_deref(),
        Some("https://i0.wp.com/i3.imageban.ru/out/2026/01/01/aaaa.jpg?ssl=1&resize=300%2C400")
    );
    assert!(!card.adult);
    assert!(sections[0].cards[0].adult, "18+ title is flagged");
}

#[test]
fn post_details() {
    let post: Post = serde_json::from_str(POST).unwrap();
    let d = details_from_post(&post);
    let s = &d.summary;
    assert_eq!(s.id, 1234);
    assert_eq!(s.slug, "sample-quest");
    assert_eq!(s.name, "Sample Quest");
    assert_eq!(s.version.as_deref(), Some("Deluxe Edition, v1.2 + 3 DLCs"));
    assert_eq!(
        s.cover.as_deref(),
        Some("https://i0.wp.com/i3.imageban.ru/out/2026/01/01/aaaa.jpg?ssl=1&w=300")
    );
    assert_eq!(s.genres, ["Action", "RPG", "3D"]);
    assert_eq!(s.companies.as_deref(), Some("Sample Studio, Example Games"));
    assert_eq!(s.languages.as_deref(), Some("ENG/MULTI9"));
    assert_eq!(s.original_size.as_deref(), Some("40.1 GB"));
    assert_eq!(s.repack_size.as_deref(), Some("from 22.5 GB"));
    assert!(!s.adult);
    assert_eq!(d.game, "Sample Quest");
}

#[test]
fn post_description_screenshots_and_links() {
    let post: Post = serde_json::from_str(POST).unwrap();
    let d = details_from_post(&post);

    let desc = d.description.unwrap();
    assert!(
        desc.starts_with(
            "Sample Quest is an action RPG about a hero\u{2019}s & sidekick\u{2019}s journey."
        ),
        "{desc}"
    );
    assert!(desc.contains("• Huge open world"), "{desc}");

    assert_eq!(d.screenshots.len(), 2);
    assert_eq!(
        d.screenshots[0].thumb,
        "https://s01.riotpixels.net/data/aa/bb/one.jpg.240p.jpg"
    );
    assert_eq!(
        d.screenshots[0].full,
        "https://s01.riotpixels.net/data/aa/bb/one.jpg.720p.jpg"
    );

    assert_eq!(d.links.len(), 3);
    assert_eq!(
        d.links[0].filename,
        "Sample_Quest_--_fitgirl-repacks.site_--_.part01.rar"
    );
    assert!(d.links[2].optional);
}

#[test]
fn html_search_fallback() {
    let html = r#"<html><body>
      <article class="post"><header><h1 class="entry-title"><a href="https://fitgirl-repacks.site/sample-quest/">Sample Quest &#8211; v1.2</a></h1>
        <time class="entry-date" datetime="2026-01-01T12:00:00+03:00">01/01/2026</time></header>
        <div class="entry-summary"><p>Genres/Tags: Action, RPG Repack Size: from 22.5 GB [Selective Download] Download Mirrors</p></div></article>
      <article class="post"><header><h1 class="entry-title"><a href="https://fitgirl-repacks.site/updates-digest-for-january-1-2026/">Updates Digest</a></h1></header></article>
      <div class="nav-previous"><a href="https://fitgirl-repacks.site/page/2/?s=sample">Older posts</a></div>
    </body></html>"#;
    let (results, has_next) = parse_search_html(html);
    assert_eq!(results.len(), 1, "updates digest is skipped");
    assert_eq!(results[0].name, "Sample Quest");
    assert_eq!(results[0].slug, "sample-quest");
    assert_eq!(results[0].cover, None);
    assert!(has_next);
}

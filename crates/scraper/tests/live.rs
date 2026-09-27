//! Hits the real websites. Run with:
//! `cargo test -p fitdl-scraper --test live -- --ignored --nocapture`

const PAGE: &str = "https://fitgirl-repacks.site/avatar-frontiers-of-pandora/";

fn client() -> fitdl_engine::HttpClient {
    fitdl_engine::HttpClient::new(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0 Safari/537.36",
    )
    .unwrap()
}

#[tokio::test]
#[ignore = "network"]
async fn scrape_and_resolve_live() {
    let client = client();
    let page = fitdl_scraper::fetch_page(&client, PAGE).await.unwrap();
    println!(
        "title: {:?}\ngame: {:?}\nlinks: {}",
        page.title,
        page.game,
        page.links.len()
    );
    assert!(!page.links.is_empty());
    let first = &page.links[0];
    println!("first: {} -> {}", first.label, first.filename);
    let direct = fitdl_scraper::fuckingfast::resolve(&client, &first.url)
        .await
        .unwrap();
    println!("direct: {direct}");
    assert!(fitdl_scraper::fuckingfast::is_direct(&direct));

    // The direct link must support ranges for multi-connection downloads.
    let engine = fitdl_engine::Engine::with_client(
        client,
        std::sync::Arc::new(fitdl_engine::RateLimiter::unlimited()),
    );
    let info = engine.probe(&direct).await.unwrap();
    println!("probe: {info:?}");
    assert!(info.supports_ranges);
    assert_eq!(info.filename.as_deref(), Some(first.filename.as_str()));
}

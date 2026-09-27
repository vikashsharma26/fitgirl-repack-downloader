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

#[tokio::test]
#[ignore = "network"]
async fn popular_live() {
    let sections = fitdl_scraper::catalog::popular(&client()).await.unwrap();
    for s in &sections {
        println!(
            "{}: {} cards, e.g. {:?}",
            s.name,
            s.cards.len(),
            s.cards.first()
        );
    }
    assert!(sections.iter().map(|s| s.cards.len()).sum::<usize>() > 40);
    assert_eq!(sections[0].name, "Today");
    assert!(sections
        .iter()
        .flat_map(|s| &s.cards)
        .all(|c| c.cover.is_some() && !c.slug.is_empty()));
}

#[tokio::test]
#[ignore = "network"]
async fn search_and_details_live() {
    let client = client();
    let page = fitdl_scraper::catalog::search(&client, "avatar", 1)
        .await
        .unwrap();
    println!("total {:?}, pages {}", page.total, page.total_pages);
    for r in &page.results {
        println!(
            "- {} | {:?} | {:?} | {:?} | {:?}",
            r.name, r.version, r.repack_size, r.genres, r.cover
        );
    }
    let first = &page.results[0];
    assert_eq!(first.name, "Avatar: Frontiers of Pandora");
    assert!(first.cover.is_some());

    let game = fitdl_scraper::catalog::game(&client, &first.slug)
        .await
        .unwrap();
    println!("game folder: {}", game.game);
    println!(
        "companies: {:?}\nlanguages: {:?}\noriginal: {:?}",
        game.summary.companies, game.summary.languages, game.summary.original_size
    );
    println!(
        "description: {:?}",
        game.description.as_deref().map(|d| &d[..d.len().min(300)])
    );
    println!(
        "screenshots: {} e.g. {:?}",
        game.screenshots.len(),
        game.screenshots.first()
    );
    assert_eq!(game.links.len(), 46);
    assert!(game.description.is_some());
    assert!(!game.screenshots.is_empty());
}

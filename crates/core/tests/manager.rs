use std::time::Duration;

use axum::http::{header, StatusCode};
use axum::routing::get;
use axum::Router;
use fitdl_core::{AppPaths, Config, Manager, NewLink, Status};

/// Serves `/a.bin` and `/b.bin` with plain 200 responses and `/missing` as 404.
async fn file_server() -> String {
    let app = Router::new()
        .route("/a.bin", get(|| async { vec![1u8; 300_000] }))
        .route("/b.bin", get(|| async { vec![2u8; 123_456] }))
        .route("/missing", get(|| async { StatusCode::NOT_FOUND }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

fn link(url: String, filename: &str) -> NewLink {
    NewLink {
        url,
        filename: Some(filename.into()),
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn queue_downloads_into_game_folder_and_persists() {
    let base = file_server().await;
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::in_dir(dir.path().join("app"));
    let mut cfg = Config {
        download_dir: dir.path().join("downloads"),
        ..Config::default()
    };
    cfg.normalize();
    cfg.save(&paths.config).unwrap();

    let m = Manager::start(paths.clone()).unwrap();
    let res = m.add(
        vec![
            link(format!("{base}/a.bin"), "a.bin"),
            link(format!("{base}/b.bin"), "b.bin"),
            link(format!("{base}/missing"), "c.bin"),
            link(format!("{base}/a.bin"), "dupe.bin"),
        ],
        Some("Some Game: Deluxe \u{2013} v1.0".into()),
    );
    assert_eq!((res.added, res.skipped), (3, 1));
    assert_eq!(res.game.as_deref(), Some("Some Game Deluxe"));

    tokio::time::timeout(Duration::from_secs(20), m.wait_idle())
        .await
        .unwrap();
    let game_dir = dir.path().join("downloads").join("Some Game Deluxe");
    assert_eq!(
        std::fs::read(game_dir.join("a.bin")).unwrap().len(),
        300_000
    );
    assert_eq!(
        std::fs::read(game_dir.join("b.bin")).unwrap().len(),
        123_456
    );

    let items = m.items();
    let status = |name: &str| {
        items
            .iter()
            .find(|i| i.item.filename == name)
            .unwrap()
            .item
            .status
    };
    assert_eq!(status("a.bin"), Status::Completed);
    assert_eq!(status("c.bin"), Status::Failed);

    m.shutdown().await;
    let saved: Vec<fitdl_core::Item> =
        serde_json::from_slice(&std::fs::read(&paths.queue).unwrap()).unwrap();
    assert_eq!(saved.len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn api_only_accepts_extension_or_token() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::in_dir(dir.path().to_path_buf());
    let m = Manager::start(paths).unwrap();
    let token = m.config().api_token;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = fitdl_core::api::router(m.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    let ping = client.get(format!("{base}/api/ping")).send().await.unwrap();
    assert_eq!(ping.status(), 200);

    let anon = client
        .get(format!("{base}/api/items"))
        .send()
        .await
        .unwrap();
    assert_eq!(anon.status(), 403);

    let website = client
        .get(format!("{base}/api/items"))
        .header(header::ORIGIN, "https://evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(website.status(), 403);

    let ext = client
        .post(format!("{base}/api/add"))
        .header(header::ORIGIN, "chrome-extension://abcdefghijklmnop")
        .json(&serde_json::json!({ "links": [{ "url": "http://127.0.0.1:9/x.bin", "label": "x.bin" }] }))
        .send()
        .await
        .unwrap();
    assert_eq!(ext.status(), 200);
    assert_eq!(
        ext.headers()["access-control-allow-origin"],
        "chrome-extension://abcdefghijklmnop"
    );
    let body: serde_json::Value = ext.json().await.unwrap();
    assert_eq!(body["added"], 1);

    let scripted = client
        .get(format!("{base}/api/items"))
        .header("X-FitDL-Token", token)
        .send()
        .await
        .unwrap();
    assert_eq!(scripted.status(), 200);
    let body: serde_json::Value = scripted.json().await.unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
}

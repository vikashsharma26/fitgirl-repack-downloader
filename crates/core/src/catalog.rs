//! Cached access to the FitGirl catalog (popular, search, game details),
//! so switching between screens in the app is instant.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use fitdl_engine::HttpClient;
use fitdl_scraper::catalog::{self, GameDetails, SearchPage, Section};

const POPULAR_TTL: Duration = Duration::from_secs(15 * 60);
const SEARCH_TTL: Duration = Duration::from_secs(5 * 60);
const GAME_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_ENTRIES: usize = 200;

#[derive(Default)]
pub struct Catalog {
    popular: Mutex<Option<(Instant, Vec<Section>)>>,
    searches: Mutex<HashMap<(String, u32), (Instant, SearchPage)>>,
    games: Mutex<HashMap<String, (Instant, GameDetails)>>,
}

impl Catalog {
    pub async fn popular(&self, http: &HttpClient) -> Result<Vec<Section>> {
        if let Some((at, sections)) = &*self.popular.lock().unwrap() {
            if at.elapsed() < POPULAR_TTL {
                return Ok(sections.clone());
            }
        }
        let sections = catalog::popular(http)
            .await
            .context("could not load popular repacks")?;
        *self.popular.lock().unwrap() = Some((Instant::now(), sections.clone()));
        Ok(sections)
    }

    pub async fn search(&self, http: &HttpClient, query: &str, page: u32) -> Result<SearchPage> {
        let key = (query.trim().to_lowercase(), page.max(1));
        if let Some(hit) = cached(&self.searches, &key, SEARCH_TTL) {
            return Ok(hit);
        }
        let result = catalog::search(http, query.trim(), key.1)
            .await
            .with_context(|| format!("search for \"{}\" failed", query.trim()))?;
        store(&self.searches, key, result.clone());
        Ok(result)
    }

    pub async fn game(&self, http: &HttpClient, slug_or_url: &str) -> Result<GameDetails> {
        let key = catalog::slug_of(slug_or_url)
            .unwrap_or_else(|| slug_or_url.trim_matches('/').to_owned());
        if let Some(hit) = cached(&self.games, &key, GAME_TTL) {
            return Ok(hit);
        }
        let details = catalog::game(http, &key)
            .await
            .with_context(|| format!("could not load game \"{key}\""))?;
        store(&self.games, key, details.clone());
        Ok(details)
    }
}

fn cached<K: Eq + std::hash::Hash, V: Clone>(
    map: &Mutex<HashMap<K, (Instant, V)>>,
    key: &K,
    ttl: Duration,
) -> Option<V> {
    map.lock()
        .unwrap()
        .get(key)
        .filter(|(at, _)| at.elapsed() < ttl)
        .map(|(_, v)| v.clone())
}

fn store<K: Eq + std::hash::Hash, V>(map: &Mutex<HashMap<K, (Instant, V)>>, key: K, value: V) {
    let mut map = map.lock().unwrap();
    if map.len() >= MAX_ENTRIES {
        map.clear();
    }
    map.insert(key, (Instant::now(), value));
}

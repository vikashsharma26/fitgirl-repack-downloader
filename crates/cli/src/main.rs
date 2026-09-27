use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};
use fitdl_core::{AppPaths, Config, Manager, NewLink, Status};
use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressStyle};

#[derive(Parser)]
#[command(
    name = "fitdl",
    version,
    about = "Fast, resumable downloader for FitGirl Repacks (fuckingfast.co) and any direct link"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download everything from game pages, fuckingfast.co links or direct URLs.
    Get {
        /// FitGirl game page URLs, fuckingfast.co links or direct file URLs.
        #[arg(required = true)]
        urls: Vec<String>,
        /// Also download optional files (fg-optional-*: language packs, bonus content).
        #[arg(long)]
        optional: bool,
        /// Download folder (default: from config.toml).
        #[arg(short, long)]
        dir: Option<PathBuf>,
        /// Files downloaded at the same time.
        #[arg(short, long)]
        parallel: Option<usize>,
        /// Connections per file.
        #[arg(short, long)]
        connections: Option<usize>,
    },
    /// List the download links on a FitGirl game page.
    Scrape {
        page_url: String,
        /// Print JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Show the most popular repacks (today, this week, this month).
    Popular {
        #[arg(long)]
        json: bool,
    },
    /// Search FitGirl Repacks.
    Search {
        query: String,
        #[arg(short, long, default_value_t = 1)]
        page: u32,
        #[arg(long)]
        json: bool,
    },
    /// Show a game's details: size, languages, description and files.
    Game {
        /// Slug (e.g. avatar-frontiers-of-pandora) or page URL.
        game: String,
        #[arg(long)]
        json: bool,
    },
    /// Run headless with the local API, so the browser extension can queue downloads.
    Serve,
    /// Show where the config file is and what it contains.
    Config,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,fitdl_core=info".into()),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let paths = AppPaths::detect();
    match cli.command {
        Command::Get {
            urls,
            optional,
            dir,
            parallel,
            connections,
        } => {
            let mut cfg = Config::load_or_create(&paths.config)?;
            if let Some(dir) = dir {
                cfg.download_dir = dir;
            }
            if let Some(p) = parallel {
                cfg.max_parallel_files = p;
            }
            if let Some(c) = connections {
                cfg.connections_per_file = c;
            }
            cfg.normalize();
            get(Manager::start_ephemeral(paths, cfg)?, urls, optional).await
        }
        Command::Scrape { page_url, json } => {
            let cfg = Config::load_or_create(&paths.config)?;
            let m = Manager::start_ephemeral(paths, cfg)?;
            let page = m.scrape(&page_url).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&page)?);
            } else {
                println!("{}", page.title.as_deref().unwrap_or("(untitled)"));
                for (i, l) in page.links.iter().enumerate() {
                    let tag = if l.optional { " [optional]" } else { "" };
                    println!("{:>3}. {}{tag}\n     {}", i + 1, l.filename, l.url);
                }
                println!("{} link(s)", page.links.len());
            }
            Ok(())
        }
        Command::Popular { json } => {
            let m =
                Manager::start_ephemeral(paths.clone(), Config::load_or_create(&paths.config)?)?;
            let sections = m.popular().await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&sections)?);
            } else {
                for s in &sections {
                    println!("\n{}", s.name);
                    for c in &s.cards {
                        println!("  {:<42} {}", c.slug, c.title);
                    }
                }
            }
            Ok(())
        }
        Command::Search { query, page, json } => {
            let m =
                Manager::start_ephemeral(paths.clone(), Config::load_or_create(&paths.config)?)?;
            let r = m.search(&query, page).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                for g in &r.results {
                    println!(
                        "{:<42} {}  [{}]",
                        g.slug,
                        g.title,
                        g.repack_size.as_deref().unwrap_or("?")
                    );
                }
                println!("page {}/{}", r.page, r.total_pages.max(1));
            }
            Ok(())
        }
        Command::Game { game, json } => {
            let m =
                Manager::start_ephemeral(paths.clone(), Config::load_or_create(&paths.config)?)?;
            let g = m.game(&game).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&g)?);
            } else {
                let s = &g.summary;
                println!("{}\n{}", s.title, s.url);
                for (label, value) in [
                    ("Repack size", &s.repack_size),
                    ("Original size", &s.original_size),
                    ("Languages", &s.languages),
                    ("Companies", &s.companies),
                ] {
                    if let Some(v) = value {
                        println!("{label:>14}: {v}");
                    }
                }
                if !s.genres.is_empty() {
                    println!("{:>14}: {}", "Genres", s.genres.join(", "));
                }
                println!(
                    "{:>14}: {} ({} optional)",
                    "Files",
                    g.links.len(),
                    g.links.iter().filter(|l| l.optional).count()
                );
                if let Some(d) = &g.description {
                    println!("\n{d}");
                }
            }
            Ok(())
        }
        Command::Serve => {
            let m = Manager::start(paths)?;
            let port = m.config().server_port;
            println!(
                "FitDL is running. Send links from the browser extension; press Ctrl+C to stop."
            );
            tokio::select! {
                r = fitdl_core::api::serve(m.clone(), port) => r?,
                _ = tokio::signal::ctrl_c() => {}
            }
            println!("Saving progress...");
            m.shutdown().await;
            Ok(())
        }
        Command::Config => {
            let cfg = Config::load_or_create(&paths.config)?;
            println!("# {}\n", paths.config.display());
            print!("{}", std::fs::read_to_string(&paths.config)?);
            println!("\n# downloads go to: {}", cfg.download_dir.display());
            Ok(())
        }
    }
}

async fn get(m: Manager, urls: Vec<String>, optional: bool) -> Result<()> {
    for url in urls {
        let is_page = url.contains("fitgirl-repacks.site") && !fitdl_scraper::is_fuckingfast(&url);
        if is_page {
            let r = m.add_page(&url, optional).await?;
            println!(
                "{}: queued {} file(s){}",
                r.game.as_deref().unwrap_or(&url),
                r.added,
                if r.skipped > 0 {
                    format!(", {} skipped", r.skipped)
                } else {
                    String::new()
                }
            );
        } else {
            m.add(
                vec![NewLink {
                    url,
                    ..Default::default()
                }],
                None,
            );
        }
    }
    println!(
        "Saving to {}  (Ctrl+C pauses; run the same command again to resume)\n",
        m.config().download_dir.display()
    );

    let bars = MultiProgress::new();
    let style = ProgressStyle::with_template(
        "{prefix:>11.bold} {wide_msg} {bytes:>10}/{total_bytes:<10} {bar:25.cyan/blue} {binary_bytes_per_sec:>12}",
    )?
    .progress_chars("━╸─");
    let mut shown: HashMap<u64, ProgressBar> = HashMap::new();
    let mut tick = tokio::time::interval(Duration::from_millis(300));

    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = tokio::signal::ctrl_c() => {
                bars.println("Pausing, progress is saved...")?;
                m.shutdown().await;
                return Ok(());
            }
        }
        let items = m.items();
        for v in &items {
            let visible = v.item.status != Status::Queued || shown.contains_key(&v.item.id);
            if !visible {
                continue;
            }
            let bar = shown.entry(v.item.id).or_insert_with(|| {
                let b = bars.add(ProgressBar::new(0));
                b.set_style(style.clone());
                b.set_message(v.item.filename.clone());
                b
            });
            bar.set_length(v.item.size.unwrap_or(0));
            bar.set_position(v.item.downloaded);
            let prefix = match v.item.status {
                Status::Downloading => format!("{}x", v.connections),
                s => format!("{s:?}").to_lowercase(),
            };
            bar.set_prefix(prefix);
            if matches!(v.item.status, Status::Completed | Status::Failed) && !bar.is_finished() {
                if let Some(err) = &v.item.error {
                    bar.set_message(format!("{}  ✗ {err}", v.item.filename));
                }
                bar.finish();
            }
        }
        let t = m.totals();
        if t.active == 0 && t.queued == 0 {
            break;
        }
    }

    let items = m.items();
    let done = items
        .iter()
        .filter(|v| v.item.status == Status::Completed)
        .count();
    let total: u64 = items.iter().filter_map(|v| v.item.size).sum();
    println!(
        "\n{done}/{} file(s) completed ({})",
        items.len(),
        HumanBytes(total)
    );
    if done < items.len() {
        std::process::exit(1);
    }
    Ok(())
}

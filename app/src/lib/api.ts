import { invoke } from "@tauri-apps/api/core";

export type Status = "queued" | "resolving" | "downloading" | "paused" | "completed" | "failed";

export interface ItemView {
  id: number;
  url: string;
  label: string;
  filename: string;
  dir: string;
  game: string | null;
  status: Status;
  size: number | null;
  downloaded: number;
  error: string | null;
  added_at: number;
  completed_at: number | null;
  speed: number;
  connections: number;
  eta_secs: number | null;
}

export interface Totals {
  active: number;
  queued: number;
  completed: number;
  failed: number;
  speed: number;
}

export interface Link {
  url: string;
  label: string;
  filename: string;
  optional: boolean;
}

export interface Page {
  title: string | null;
  game: string | null;
  links: Link[];
}

export interface NewLink {
  url: string;
  label?: string;
  filename?: string;
}

export interface AddResult {
  game: string | null;
  added: number;
  skipped: number;
}

export interface Config {
  download_dir: string;
  max_parallel_files: number;
  connections_per_file: number;
  min_segment_size_mb: number;
  buffer_size_kb: number;
  max_retries: number;
  retry_backoff_ms: number;
  speed_limit_kbps: number;
  state_save_interval_s: number;
  subfolder_per_game: boolean;
  hide_adult: boolean;
  user_agent: string;
  server_port: number;
  api_token: string;
}

export interface AppInfo {
  version: string;
  config_path: string;
  api: { port: number; running: boolean; error: string | null };
}

export interface Card {
  title: string;
  url: string;
  slug: string;
  cover: string | null;
  adult: boolean;
}

export interface Section {
  name: string;
  cards: Card[];
}

export interface GameSummary {
  id: number;
  slug: string;
  url: string;
  title: string;
  name: string;
  version: string | null;
  date: string | null;
  cover: string | null;
  repack_size: string | null;
  original_size: string | null;
  genres: string[];
  companies: string | null;
  languages: string | null;
  adult: boolean;
}

export interface Screenshot {
  thumb: string;
  full: string;
}

export interface GameDetails extends GameSummary {
  game: string;
  description: string | null;
  screenshots: Screenshot[];
  links: Link[];
}

export interface SearchPage {
  query: string;
  results: GameSummary[];
  page: number;
  total: number | null;
  total_pages: number;
}

export const popular = () => invoke<Section[]>("popular");
export const search = (query: string, page: number) => invoke<SearchPage>("search", { query, page });
export const game = (slug: string) => invoke<GameDetails>("game", { slug });
export const openUrl = (url: string) => invoke<void>("open_url", { url });
export const getItems = () => invoke<{ items: ItemView[]; totals: Totals }>("get_items");
export const addLinks = (links: NewLink[], game: string | null) =>
  invoke<AddResult>("add_links", { links, game });
export const pause = (id: number) => invoke<void>("pause", { id });
export const resume = (id: number) => invoke<void>("resume", { id });
export const remove = (id: number, deleteFiles: boolean) => invoke<void>("remove", { id, deleteFiles });
export const pauseAll = () => invoke<void>("pause_all");
export const resumeAll = () => invoke<void>("resume_all");
export const clearCompleted = () => invoke<void>("clear_completed");
export const getConfig = () => invoke<Config>("get_config");
export const setConfig = (config: Config) => invoke<Config>("set_config", { config });
export const appInfo = () => invoke<AppInfo>("app_info");
export const showItem = (id: number) => invoke<void>("show_item", { id });
export const openDownloads = () => invoke<void>("open_downloads");

export function isGamePage(url: string): boolean {
  try {
    const u = new URL(url);
    return u.hostname.endsWith("fitgirl-repacks.site") && u.pathname.length > 1;
  } catch {
    return false;
  }
}

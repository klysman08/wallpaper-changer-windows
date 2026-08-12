/**
 * Typed client for the Python wallpaper engine.
 *
 * Every call goes through the Rust `engine_call` command, which owns the sidecar
 * process and correlates requests to responses. The webview never spawns anything.
 *
 * Method names and shapes mirror `src/wallpaper_changer/rpc.py` — that module's
 * allowlist is the source of truth, so add a method there first.
 */
import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

// ── Wire types ────────────────────────────────────────────────────────────────

export interface Monitor {
  index: number
  x: number
  y: number
  width: number
  height: number
}

export interface MonitorsResult {
  monitors: Monitor[]
  virtual_width: number
  virtual_height: number
}

export type FitMode = "fill" | "fit" | "stretch" | "center" | "span"
export type Effect = "normal" | "bw" | "vintage" | "hdr"
export type Selection = "random" | "sequential"

export interface Config {
  general: {
    mode: string
    selection: Selection
    interval: number
    collage_count: number
    collage_same_for_all: boolean
    language: string
  }
  paths: {
    wallpapers_folder: string
    output_folder: string
    default_wallpaper: string
  }
  display: { fit_mode: FitMode; effect: Effect }
  hotkeys: Record<string, string>
  video: { enabled: boolean; folder: string; loop: boolean; sound: boolean }
}

export interface ConfigResult {
  config: Config
  config_path: string
}

export interface Capabilities {
  protocol: number
  effects: Effect[]
  has_mpv: boolean
  startup_enabled: boolean
}

export interface PreviewResult {
  png_base64: string
  width: number
  height: number
  images: string[]
}

export interface ApplyResult {
  output: string
  images: string[]
}

export interface WindowInfo {
  hwnd: number
  title: string
  process: string
}

export interface VideoStatus {
  running: boolean
  current: string
  has_mpv: boolean
}

export interface Translations {
  translations: Record<string, Record<string, string>>
  supported: Record<string, string>
  default: string
  current: string
}

// ── Transport ─────────────────────────────────────────────────────────────────

/** Rust returns `Err(String)` as `"<kind>: <message>"`; keep both halves usable. */
export class EngineError extends Error {
  readonly kind: string

  constructor(raw: string) {
    const at = raw.indexOf(": ")
    super(at > 0 ? raw.slice(at + 2) : raw)
    this.name = "EngineError"
    this.kind = at > 0 ? raw.slice(0, at) : "error"
  }
}

async function call<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
  try {
    return await invoke<T>("engine_call", { method, params })
  } catch (raw) {
    throw new EngineError(typeof raw === "string" ? raw : String(raw))
  }
}

// ── Methods ───────────────────────────────────────────────────────────────────

export const engine = {
  ping: () => call<{ pong: boolean; protocol: number }>("ping"),
  getCapabilities: () => call<Capabilities>("get_capabilities"),

  getConfig: () => call<ConfigResult>("get_config"),
  saveConfig: (config: Config) => call<{ saved: boolean }>("save_config", { config }),

  getMonitors: () => call<MonitorsResult>("get_monitors"),
  getTranslations: () => call<Translations>("get_translations"),
  listFolderImages: (folder: string) =>
    call<{ count: number; images: string[] }>("list_folder_images", { folder }),

  applyWallpaper: (config?: Partial<Config>) => call<ApplyResult>("apply_wallpaper", { config }),
  applyDefaultWallpaper: (config?: Partial<Config>) =>
    call<ApplyResult>("apply_default_wallpaper", { config }),
  /** Replay the previous image set exactly, rather than picking a new one. */
  applyPreviousWallpaper: (config?: Partial<Config>) =>
    call<ApplyResult>("apply_previous_wallpaper", { config }),

  /**
   * Render the collage without touching the desktop.
   *
   * Pass `images` from a previous preview to re-render the same selection — that
   * stops effect/fit tweaks from reshuffling the picture on every change.
   */
  preview: (opts: { config?: Partial<Config>; maxWidth?: number; images?: string[] } = {}) =>
    call<PreviewResult>("preview", {
      config: opts.config,
      max_width: opts.maxWidth ?? 960,
      images: opts.images,
    }),

  watchStart: (interval?: number) =>
    call<{ watching: boolean; interval: number }>("watch_start", { interval }),
  watchStop: () => call<{ watching: boolean }>("watch_stop"),
  watchStatus: () => call<{ watching: boolean }>("watch_status"),

  listWindows: () => call<{ windows: WindowInfo[] }>("list_windows"),
  setWindowOpacity: (hwnd: number, alpha: number) =>
    call<{ hwnd: number; alpha: number }>("set_window_opacity", { hwnd, alpha }),
  getOpacitySettings: () => call<{ settings: Record<string, number> }>("get_opacity_settings"),
  saveOpacitySettings: (settings: Record<string, number>) =>
    call<{ saved: boolean }>("save_opacity_settings", { settings }),

  scanVideos: (folder: string) => call<{ count: number; videos: string[] }>("scan_videos", { folder }),
  videoStart: (config?: Partial<Config>) => call<VideoStatus>("video_start", { config }),
  videoStop: () => call<VideoStatus>("video_stop"),
  videoNext: () => call<VideoStatus>("video_next"),
  videoPrev: () => call<VideoStatus>("video_prev"),
  videoSetSound: (enabled: boolean) => call<VideoStatus>("video_set_sound", { enabled }),
  videoStatus: () => call<VideoStatus>("video_status"),

  getStartupEnabled: () => call<{ enabled: boolean }>("get_startup_enabled"),
  setStartupEnabled: (enabled: boolean) => call<{ enabled: boolean }>("set_startup_enabled", { enabled }),

  notify: (title: string, message: string) => call<{ sent: boolean }>("notify", { title, message }),
}

// ── Native shell (Rust-side, not the Python engine) ───────────────────────────

/**
 * Re-register the global hotkeys from the saved config.
 *
 * Hotkeys are owned by Rust so they survive an engine restart and can reach the
 * window itself. Call this after saving settings. Resolves to the bindings that
 * could not be registered — usually taken by another application.
 */
export async function reloadHotkeys(): Promise<string[]> {
  return invoke<string[]>("reload_hotkeys")
}

// ── Events ────────────────────────────────────────────────────────────────────

export type EngineEvent =
  | { event: "ready"; data: { protocol: number } }
  | { event: "stopped"; data: Record<string, never> }
  | { event: "wallpaper_applied"; data: ApplyResult }
  | { event: "video_status"; data: { current: string } }
  | { event: "error"; data: { source: string; message: string } }

/** Subscribe to unsolicited engine events. Returns the unlisten function. */
export function onEngineEvent(handler: (event: EngineEvent) => void): Promise<UnlistenFn> {
  return listen<EngineEvent>("engine-event", ({ payload }) => handler(payload))
}

/** A global hotkey ran (or failed). Lets the UI refresh without polling. */
export interface HotkeyEvent {
  binding: string
  result?: unknown
  error?: string
}

export function onHotkeyEvent(handler: (event: HotkeyEvent) => void): Promise<UnlistenFn> {
  return listen<HotkeyEvent>("hotkey-fired", ({ payload }) => handler(payload))
}

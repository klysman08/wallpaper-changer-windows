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
    /** Come up hidden in the tray. Always implied when Windows starts the app. */
    start_minimized?: boolean
    /** Ask GitHub for a newer release at startup. Absent in configs predating it. */
    check_updates?: boolean
    /** Written by the engine: whether rotation was running at the last shutdown. */
    rotation_active?: boolean
  }
  paths: {
    wallpapers_folder: string
    output_folder: string
    default_wallpaper: string
  }
  display: { fit_mode: FitMode; effect: Effect }
  hotkeys: Hotkeys
  video: { enabled: boolean; folder: string; loop: boolean; sound: boolean }
}

/**
 * The `[hotkeys]` table.
 *
 * Mostly shortcut strings, where empty means unbound. The two `scroll_*` keys are
 * not shortcuts — they configure modifier+wheel transparency, and live here
 * because that is where the engine and the legacy GUI both read them from.
 */
export interface Hotkeys {
  [binding: string]: string | boolean | undefined
  scroll_modifier?: ScrollModifier
  scroll_enabled?: boolean
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
  has_scroll_transparency: boolean
}

export type ScrollModifier = "alt" | "ctrl" | "shift" | "win"

export interface ScrollTransparencyStatus {
  /** What the config asks for. */
  enabled: boolean
  /** Whether the hook is actually installed — these differ on failure. */
  running: boolean
  available: boolean
  modifier: ScrollModifier
  modifiers: ScrollModifier[]
  step: number
}

/**
 * One image's slot in the collage, in composite pixels.
 *
 * Computed by the engine rather than the UI: the grid rules live in
 * `wallpaper.py`, and a second implementation here would drift the moment they
 * change. `image_index` points into `PreviewResult.images` — with
 * `collage_same_for_all` several cells share one index.
 */
export interface PreviewCell {
  monitor: number
  image_index: number
  x: number
  y: number
  width: number
  height: number
}

export interface PreviewResult {
  png_base64: string
  width: number
  height: number
  images: string[]
  cells: PreviewCell[]
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
  /**
   * Base64 JPEG thumbnails, keyed by path. The webview cannot read local files,
   * so a picture can only reach it as bytes. Paths that fail to open are absent
   * from the result rather than failing the batch.
   */
  getThumbnails: (paths: string[], size = 160) =>
    call<{ thumbnails: Record<string, string> }>("get_thumbnails", { paths, size }),

  /** Compose and apply. Pass `images` to apply exactly the set a preview showed. */
  applyWallpaper: (config?: Partial<Config>, images?: string[]) =>
    call<ApplyResult>("apply_wallpaper", { config, images }),
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

  scrollTransparencyStatus: () =>
    call<ScrollTransparencyStatus>("scroll_transparency_status"),
  /** Re-read the saved settings and install or remove the hook to match. */
  syncScrollTransparency: () => call<ScrollTransparencyStatus>("sync_scroll_transparency"),

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
  | { event: "session_restored"; data: { rotation: boolean; video: boolean } }
  | { event: "video_status"; data: { current: string } }
  | {
      event: "transparency_changed"
      data: { hwnd: number; process: string; alpha: number }
    }
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

/**
 * The tray ran an engine method.
 *
 * The window can be open behind the tray menu, so anything it shows that the action
 * changed — rotation in particular — has to be re-read rather than left to drift.
 */
export function onShellAction(handler: (action: string) => void): Promise<UnlistenFn> {
  return listen<{ action: string }>("shell-action", ({ payload }) => handler(payload.action))
}

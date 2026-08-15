/**
 * Update store: asks GitHub whether a newer release exists, then downloads and
 * installs it.
 *
 * The network work all happens in Rust (`tauri-plugin-updater`) — the webview's CSP
 * allows no remote origin, and the installer has to land on disk anyway. What lives
 * here is the state the sidebar and the Settings screen render.
 *
 * A found update never interrupts: it lights up an item in the sidebar and raises one
 * toast. The dialog opens only when the user asks for it.
 */
import * as React from "react"
import { getVersion } from "@tauri-apps/api/app"
import { relaunch } from "@tauri-apps/plugin-process"
import { check, type Update } from "@tauri-apps/plugin-updater"
import { toast } from "sonner"

import { isTauri } from "@/lib/tauri"
import type { Config } from "@/lib/engine"

/** A check that never answers must not leave the UI stuck on "checking". */
const CHECK_TIMEOUT_MS = 20_000

export type UpdateStatus =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "error"

export interface UpdateStore {
  status: UpdateStatus
  /** The running app's version, shown whether or not an update exists. */
  currentVersion: string
  /** The newer version, once one has been found. */
  version: string | null
  notes: string | null
  downloaded: number
  /** `null` until the server reports a size — some responses omit it. */
  contentLength: number | null
  error: string | null
  /** True once a manual check has run, so Settings can report "up to date". */
  checked: boolean
  dialogOpen: boolean
  setDialogOpen: (open: boolean) => void
  /** `manual` decides whether failures and "no update" are worth reporting. */
  check: (manual: boolean) => Promise<void>
  install: () => Promise<void>
}

export function useUpdate(
  config: Config | null,
  t: (key: string, vars?: Record<string, string | number>) => string,
): UpdateStore {
  const [status, setStatus] = React.useState<UpdateStatus>("idle")
  const [currentVersion, setCurrentVersion] = React.useState("")
  const [version, setVersion] = React.useState<string | null>(null)
  const [notes, setNotes] = React.useState<string | null>(null)
  const [downloaded, setDownloaded] = React.useState(0)
  const [contentLength, setContentLength] = React.useState<number | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const [checked, setChecked] = React.useState(false)
  const [dialogOpen, setDialogOpen] = React.useState(false)

  // The Update handle owns a Rust-side resource. It is kept out of state because
  // installing must use the exact object `check()` returned, not a stale render's copy.
  const update = React.useRef<Update | null>(null)
  // The automatic check runs once per launch, even though the config it waits for
  // arrives after the first render and changes again on every edit.
  const autoChecked = React.useRef(false)

  React.useEffect(() => {
    if (!isTauri()) return
    void getVersion().then(setCurrentVersion).catch(() => {})
  }, [])

  const runCheck = React.useCallback(async (manual: boolean) => {
    if (!isTauri()) return
    setStatus("checking")
    setError(null)
    try {
      const found = await check({ timeout: CHECK_TIMEOUT_MS })
      setChecked(true)
      if (!found) {
        update.current = null
        setVersion(null)
        setNotes(null)
        setStatus("idle")
        if (manual) toast.success(t("up_to_date"))
        return
      }
      update.current = found
      setVersion(found.version)
      setNotes(found.body ?? null)
      setStatus("available")
      if (!manual) {
        toast.info(t("new_version_available", { version: found.version }))
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      setChecked(true)
      setStatus("error")
      setError(message)
      // Being offline at launch is ordinary, so an automatic check stays quiet and
      // only the Settings screen shows what went wrong.
      if (manual) toast.error(t("update_check_failed"), { description: message })
    }
  }, [t])

  const install = React.useCallback(async () => {
    const found = update.current
    if (!found) return
    setStatus("downloading")
    setDownloaded(0)
    setContentLength(null)
    setError(null)
    try {
      await found.downloadAndInstall((event) => {
        if (event.event === "Started") {
          setContentLength(event.data.contentLength ?? null)
        } else if (event.event === "Progress") {
          setDownloaded((n) => n + event.data.chunkLength)
        } else if (event.event === "Finished") {
          setStatus("ready")
        }
      })
      setStatus("ready")
      // On Windows the NSIS installer takes over and this process is gone before the
      // call returns; the button in the dialog is the fallback for when it is not.
      await relaunch()
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      setStatus("error")
      setError(message)
    }
  }, [])

  React.useEffect(() => {
    if (autoChecked.current || !config) return
    autoChecked.current = true
    // A dev build's version comes from tauri.conf.json, so it would compare itself
    // against a manifest describing the very release it claims to be.
    if (import.meta.env.DEV) return
    if (config.general.check_updates === false) return
    // Off the render commit: nothing waits on this answer, and the first paint
    // should not queue behind a network round trip.
    const timer = setTimeout(() => void runCheck(false), 0)
    return () => clearTimeout(timer)
  }, [config, runCheck])

  return {
    status,
    currentVersion,
    version,
    notes,
    downloaded,
    contentLength,
    error,
    checked,
    dialogOpen,
    setDialogOpen,
    check: runCheck,
    install,
  }
}

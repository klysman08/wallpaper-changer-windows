/**
 * Thumbnails for local image files.
 *
 * The webview cannot read the disk, so every picture it shows has to come across
 * the RPC boundary as bytes. Two consequences shape this hook:
 *
 *  - **it caches for the lifetime of the window**, outside React, because the picker
 *    and the preview overlay ask for overlapping sets and a folder of a few hundred
 *    pictures must not be re-encoded each time one of them mounts
 *  - **it asks in batches**, because the engine speaks one JSON object per line and
 *    a few hundred base64 images on a single line is a stall the user can feel
 *
 * A file that fails to open is remembered as unavailable rather than retried on
 * every render.
 */
import * as React from "react"

import { engine } from "@/lib/engine"

/** Empty string marks a path the engine could not read. */
const CACHE = new Map<string, string>()

const BATCH = 24

/**
 * Paths are joined into one string so the effect keys off their contents rather
 * than the array's identity. A newline separates them because a Windows path
 * cannot contain one — a space can, and would split paths in half.
 */
const SEP = "\n"

export function useThumbnails(paths: string[], size = 160): Record<string, string> {
  const key = paths.join(SEP)
  const [, setLoaded] = React.useState(0)

  React.useEffect(() => {
    const wanted = key ? key.split(SEP) : []
    const missing = wanted.filter((path) => !CACHE.has(path))
    if (missing.length === 0) return

    let cancelled = false
    void (async () => {
      for (let at = 0; at < missing.length; at += BATCH) {
        if (cancelled) return
        const batch = missing.slice(at, at + BATCH)
        try {
          const { thumbnails } = await engine.getThumbnails(batch, size)
          for (const [path, data] of Object.entries(thumbnails)) {
            CACHE.set(path, `data:image/jpeg;base64,${data}`)
          }
        } catch {
          // The engine is down or the batch was rejected; the caller already shows
          // its own error, and a missing thumbnail degrades to a filename.
          return
        }
        // Anything the engine skipped is unreadable. Recording that is what stops
        // the next render from asking for it all over again.
        for (const path of batch) {
          if (!CACHE.has(path)) CACHE.set(path, "")
        }
        if (cancelled) return
        setLoaded((n) => n + 1)
      }
    })()

    return () => {
      cancelled = true
    }
  }, [key, size])

  const result: Record<string, string> = {}
  for (const path of paths) {
    const hit = CACHE.get(path)
    if (hit) result[path] = hit
  }
  return result
}

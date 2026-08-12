/**
 * Live wallpaper preview.
 *
 * Every render is a full composite in Python plus a base64 PNG across the IPC
 * boundary, so this must not fire on each keystroke or slider tick. Two safeguards:
 *
 *  - requests are debounced, and a newer one supersedes an in-flight response
 *  - the image *selection* is pinned between renders, so changing the effect or fit
 *    re-renders the same pictures instead of reshuffling them
 *
 * Call `reshuffle()` to deliberately pick a new set.
 */
import * as React from "react"

import { engine, type Config } from "@/lib/engine"

const DEBOUNCE_MS = 350

export interface Preview {
  src: string | null
  width: number
  height: number
  loading: boolean
  error: string | null
  reshuffle: () => void
}

export function usePreview(config: Config | null, maxWidth = 900): Preview {
  const [src, setSrc] = React.useState<string | null>(null)
  const [size, setSize] = React.useState({ width: 0, height: 0 })
  const [loading, setLoading] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [nonce, setNonce] = React.useState(0)

  const pinned = React.useRef<string[] | null>(null)
  // Guards against an older, slower response overwriting a newer one.
  const latest = React.useRef(0)

  const reshuffle = React.useCallback(() => {
    pinned.current = null
    setNonce((n) => n + 1)
  }, [])

  // Only the fields that actually change the picture belong here; watching the whole
  // config would re-render the preview when an unrelated hotkey changes.
  const signature = config
    ? JSON.stringify([
        config.paths.wallpapers_folder,
        config.display.fit_mode,
        config.display.effect,
        config.general.collage_count,
        config.general.collage_same_for_all,
        config.general.selection,
      ])
    : null

  React.useEffect(() => {
    if (!config || !signature) return
    const ticket = ++latest.current
    setLoading(true)

    const timer = setTimeout(() => {
      void engine
        .preview({ config, maxWidth, images: pinned.current ?? undefined })
        .then((result) => {
          if (ticket !== latest.current) return // superseded
          pinned.current = result.images
          setSrc(`data:image/png;base64,${result.png_base64}`)
          setSize({ width: result.width, height: result.height })
          setError(null)
        })
        .catch((e) => {
          if (ticket !== latest.current) return
          setError((e as Error).message)
          setSrc(null)
        })
        .finally(() => {
          if (ticket === latest.current) setLoading(false)
        })
    }, DEBOUNCE_MS)

    return () => clearTimeout(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [signature, nonce, maxWidth])

  return { src, ...size, loading, error, reshuffle }
}

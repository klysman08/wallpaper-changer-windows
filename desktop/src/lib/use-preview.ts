/**
 * Live wallpaper preview.
 *
 * Every render is a full composite in the engine plus a base64 PNG across the IPC
 * boundary, so this must not fire on each keystroke or slider tick. Two safeguards:
 *
 *  - requests are debounced, and a newer one supersedes an in-flight response
 *  - the image *selection* is pinned between renders, so changing the effect or fit
 *    re-renders the same pictures instead of reshuffling them
 *
 * Call `reshuffle()` to deliberately pick a new set.
 */
import * as React from "react"

import { engine, type Config, type PreviewCell } from "@/lib/engine"

const DEBOUNCE_MS = 350

export interface Preview {
  src: string | null
  width: number
  height: number
  loading: boolean
  error: string | null
  /** The images the current render used, in monitor order. */
  images: string[]
  /** Where each of those images sits, in composite pixels. */
  cells: PreviewCell[]
  reshuffle: () => void
  /** Swap two entries of the selection and re-render from the new order. */
  swap: (a: number, b: number) => void
  /** Put a specific file in one slot and re-render. */
  replace: (index: number, path: string) => void
}

export function usePreview(config: Config | null, maxWidth = 900): Preview {
  const [src, setSrc] = React.useState<string | null>(null)
  const [size, setSize] = React.useState({ width: 0, height: 0 })
  const [loading, setLoading] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [nonce, setNonce] = React.useState(0)
  const [images, setImages] = React.useState<string[]>([])
  const [cells, setCells] = React.useState<PreviewCell[]>([])

  const pinned = React.useRef<string[] | null>(null)
  // Which selection settings the pinned images were chosen under.
  const pinnedFor = React.useRef<string | null>(null)
  // Guards against an older, slower response overwriting a newer one.
  const latest = React.useRef(0)
  // Set when the pending render was asked for outright rather than inferred from a
  // settings change, which is what lets it skip the debounce.
  const urgent = React.useRef(false)

  const reshuffle = React.useCallback(() => {
    pinned.current = null
    setNonce((n) => n + 1)
  }, [])

  /**
   * Re-pin an edited selection and ask for a fresh render.
   *
   * The edit is applied to state as well as to the pin so the caption list and the
   * hit targets update on the spot, rather than a third of a second later when the
   * debounced render lands.
   */
  const edit = React.useCallback(
    (mutate: (current: string[]) => string[] | null) => {
      // The pin, not the state, is what the next request will send, so it is the
      // one that must not be read stale — and being a ref, it never is.
      const next = mutate(pinned.current ?? images)
      if (!next) return
      pinned.current = next
      // A direct edit is one deliberate act, not a slider being dragged, so it
      // skips the debounce that exists to swallow keystrokes.
      urgent.current = true
      setImages(next)
      setNonce((n) => n + 1)
    },
    [images],
  )

  const swap = React.useCallback(
    (a: number, b: number) => {
      edit((current) => {
        if (a === b || !current[a] || !current[b]) return null
        const next = [...current]
        ;[next[a], next[b]] = [next[b], next[a]]
        return next
      })
    },
    [edit],
  )

  const replace = React.useCallback(
    (index: number, path: string) => {
      edit((current) => {
        if (index < 0 || index >= current.length || current[index] === path) return null
        const next = [...current]
        next[index] = path
        return next
      })
    },
    [edit],
  )

  // Settings that decide *which* images get picked. When one of them changes the pin
  // has to be dropped: reusing it would keep showing pictures the new settings would
  // never choose, and after a folder change the paths may not even exist any more.
  const selectionSignature = config
    ? JSON.stringify([
        config.paths.wallpapers_folder,
        config.general.collage_count,
        config.general.collage_same_for_all,
        config.general.selection,
      ])
    : null

  // Settings that only change how those images are drawn. Only the fields that
  // actually change the picture belong here; watching the whole config would
  // re-render the preview when an unrelated hotkey changes.
  const signature = config
    ? JSON.stringify([selectionSignature, config.display.fit_mode, config.display.effect])
    : null

  React.useEffect(() => {
    if (!config || !signature) return

    if (pinnedFor.current !== selectionSignature) {
      pinned.current = null
      pinnedFor.current = selectionSignature
    }

    const ticket = ++latest.current
    const wait = urgent.current ? 0 : DEBOUNCE_MS
    urgent.current = false
    setLoading(true)

    const timer = setTimeout(() => {
      void engine
        .preview({ config, maxWidth, images: pinned.current ?? undefined })
        .then((result) => {
          if (ticket !== latest.current) return // superseded
          pinned.current = result.images
          setSrc(`data:image/png;base64,${result.png_base64}`)
          setSize({ width: result.width, height: result.height })
          setImages(result.images)
          setCells(result.cells ?? [])
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
    }, wait)

    return () => clearTimeout(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [signature, nonce, maxWidth])

  return { src, ...size, loading, error, images, cells, reshuffle, swap, replace }
}

/**
 * The actions the footer bar and the tabs both drive.
 *
 * Rotation and video playback are engine-side state, and three things can change
 * them: the footer, a tab, or a global hotkey. Keeping one copy here is what stops
 * the footer from claiming "Start rotation" while the Wallpaper tab shows "Stop".
 */
import * as React from "react"
import { toast } from "sonner"

import { engine, onEngineEvent, onHotkeyEvent, type Config, type VideoStatus } from "@/lib/engine"

export interface Actions {
  applying: boolean
  watching: boolean
  video: VideoStatus | null
  videoBusy: boolean
  /** Compose and set a fresh wallpaper. */
  applyNow: () => Promise<void>
  /** Re-apply the previous image set, exactly as it was. */
  applyPrevious: () => Promise<void>
  toggleWatch: () => Promise<void>
  toggleVideo: () => Promise<void>
  videoNext: () => Promise<void>
  videoPrev: () => Promise<void>
  setVideoSound: (enabled: boolean) => Promise<void>
}

export function useActions(config: Config | null, t: (key: string) => string): Actions {
  const [applying, setApplying] = React.useState(false)
  const [watching, setWatching] = React.useState(false)
  const [video, setVideo] = React.useState<VideoStatus | null>(null)
  const [videoBusy, setVideoBusy] = React.useState(false)

  // `config` is the edited copy, which changes on every keystroke. Reading it from a
  // ref keeps the callbacks stable so the footer does not re-render as you type.
  const configRef = React.useRef(config)
  configRef.current = config

  const refresh = React.useCallback(() => {
    void engine.watchStatus().then((s) => setWatching(s.watching)).catch(() => {})
    void engine.videoStatus().then(setVideo).catch(() => {})
  }, [])

  React.useEffect(() => {
    refresh()

    // A hotkey or the tray can start or stop either of these behind our back, and
    // the engine announces track changes itself.
    const unlistenHotkey = onHotkeyEvent((event) => {
      if (!event.error) refresh()
    })
    const unlistenEngine = onEngineEvent((event) => {
      if (event.event === "video_status") {
        setVideo((prev) => (prev ? { ...prev, current: event.data.current } : prev))
      }
    })
    return () => {
      void unlistenHotkey.then((fn) => fn())
      void unlistenEngine.then((fn) => fn())
    }
  }, [refresh])

  const applyNow = React.useCallback(async () => {
    setApplying(true)
    try {
      const result = await engine.applyWallpaper(configRef.current ?? undefined)
      toast.success(t("wallpaper_applied"), {
        description: `${result.images.length} ${t("images")}`,
      })
    } catch (e) {
      toast.error(t("apply_failed"), { description: (e as Error).message })
    } finally {
      setApplying(false)
    }
  }, [t])

  const applyPrevious = React.useCallback(async () => {
    setApplying(true)
    try {
      await engine.applyPreviousWallpaper(configRef.current ?? undefined)
    } catch (e) {
      // Running out of history is the normal way this fails, so it is a note.
      toast.info((e as Error).message)
    } finally {
      setApplying(false)
    }
  }, [])

  const toggleWatch = React.useCallback(async () => {
    try {
      if (watching) {
        await engine.watchStop()
        setWatching(false)
      } else {
        const r = await engine.watchStart(configRef.current?.general.interval)
        setWatching(r.watching)
      }
    } catch (e) {
      toast.error((e as Error).message)
    }
  }, [watching])

  const runVideo = React.useCallback(async (action: () => Promise<VideoStatus>) => {
    setVideoBusy(true)
    try {
      setVideo(await action())
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setVideoBusy(false)
    }
  }, [])

  const toggleVideo = React.useCallback(
    () =>
      runVideo(() =>
        video?.running ? engine.videoStop() : engine.videoStart(configRef.current ?? undefined),
      ),
    [runVideo, video?.running],
  )

  const videoNext = React.useCallback(() => runVideo(engine.videoNext), [runVideo])
  const videoPrev = React.useCallback(() => runVideo(engine.videoPrev), [runVideo])
  const setVideoSound = React.useCallback(
    (enabled: boolean) => runVideo(() => engine.videoSetSound(enabled)),
    [runVideo],
  )

  return {
    applying,
    watching,
    video,
    videoBusy,
    applyNow,
    applyPrevious,
    toggleWatch,
    toggleVideo,
    videoNext,
    videoPrev,
    setVideoSound,
  }
}

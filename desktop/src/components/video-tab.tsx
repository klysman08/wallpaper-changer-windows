import * as React from "react"
import { open } from "@tauri-apps/plugin-dialog"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { engine, type Config } from "@/lib/engine"
import type { Actions } from "@/lib/use-actions"
import type { I18n } from "@/lib/use-i18n"

interface Props {
  config: Config
  hasMpv: boolean
  i18n: I18n
  /** Shared with the footer bar, so the two transports cannot disagree. */
  actions: Actions
  onChange: <S extends keyof Config, K extends keyof Config[S]>(
    section: S,
    key: K,
    value: Config[S][K],
  ) => void
}

export function VideoTab({ config, hasMpv, i18n, actions, onChange }: Props) {
  const { t } = i18n
  const status = actions.video
  const busy = actions.videoBusy
  const [videoCount, setVideoCount] = React.useState<number | null>(null)

  React.useEffect(() => {
    let cancelled = false
    void engine
      .scanVideos(config.video.folder)
      .then((r) => !cancelled && setVideoCount(r.count))
      .catch(() => !cancelled && setVideoCount(null))
    return () => {
      cancelled = true
    }
  }, [config.video.folder])

  async function browse() {
    const picked = await open({ directory: true, defaultPath: config.video.folder })
    if (typeof picked === "string") onChange("video", "folder", picked)
  }

  if (!hasMpv) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>{t("video_wallpaper")}</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground">{t("mpv_unavailable")}</p>
        </CardContent>
      </Card>
    )
  }

  const running = status?.running ?? false

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle>{t("video_folder")}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="flex gap-2">
            <Input
              value={config.video.folder}
              onChange={(e) => onChange("video", "folder", e.target.value)}
              spellCheck={false}
            />
            <Button variant="outline" onClick={browse}>
              {t("browse")}
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">
            {videoCount === null ? "—" : `${videoCount} ${t("videos_found")}`}
          </p>
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="flex-row items-center justify-between">
          <CardTitle>{t("playback")}</CardTitle>
          {running && <Badge>{t("playing")}</Badge>}
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <p className="font-mono text-xs break-all text-muted-foreground">
            {status?.current || "—"}
          </p>

          <div className="flex flex-wrap gap-2">
            <Button variant="outline" disabled={busy} onClick={actions.videoPrev}>
              {t("previous")}
            </Button>
            <Button
              variant={running ? "destructive" : "default"}
              disabled={busy}
              onClick={actions.toggleVideo}
            >
              {running ? t("stop") : t("play")}
            </Button>
            <Button variant="outline" disabled={busy} onClick={actions.videoNext}>
              {t("next")}
            </Button>
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="video-loop" className="font-normal">{t("loop")}</Label>
            <Switch
              id="video-loop"
              checked={config.video.loop}
              onCheckedChange={(v) => onChange("video", "loop", v)}
            />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="video-sound" className="font-normal">{t("sound")}</Label>
            <Switch
              id="video-sound"
              checked={config.video.sound}
              onCheckedChange={(v) => {
                onChange("video", "sound", v)
                // Audio can be toggled live; no need to restart playback.
                if (running) void actions.setVideoSound(v)
              }}
            />
          </div>
        </CardContent>
      </Card>
    </div>
  )
}

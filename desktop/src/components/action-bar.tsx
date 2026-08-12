/**
 * The persistent action bar pinned to the bottom of the window.
 *
 * These are the things people came to do — apply a wallpaper, step through them,
 * start or stop rotation, play a video — so they stay one click away no matter
 * which settings screen is open.
 */
import type * as React from "react"
import {
  ChevronLeft,
  ChevronRight,
  Pause,
  Play,
  RefreshCw,
  SkipBack,
  SkipForward,
  Square,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import type { Actions } from "@/lib/use-actions"
import type { I18n } from "@/lib/use-i18n"

interface Props {
  actions: Actions
  hasMpv: boolean
  i18n: I18n
}

export function ActionBar({ actions, hasMpv, i18n }: Props) {
  const { t } = i18n
  const videoRunning = actions.video?.running ?? false

  return (
    <footer className="sticky bottom-0 z-10 flex shrink-0 flex-wrap items-center gap-2 border-t bg-background/80 px-4 py-2 backdrop-blur">
      <IconAction
        label={t("hk_prev_wallpaper")}
        onClick={actions.applyPrevious}
        disabled={actions.applying}
        icon={<ChevronLeft />}
      />

      <Button size="sm" onClick={actions.applyNow} disabled={actions.applying}>
        <RefreshCw className={actions.applying ? "animate-spin" : undefined} />
        {actions.applying ? t("applying") : t("apply_now")}
      </Button>

      <IconAction
        label={t("hk_next_wallpaper")}
        onClick={actions.applyNow}
        disabled={actions.applying}
        icon={<ChevronRight />}
      />

      <Separator orientation="vertical" className="mx-1 h-6" />

      <Button
        size="sm"
        variant={actions.watching ? "destructive" : "outline"}
        onClick={actions.toggleWatch}
      >
        {actions.watching ? <Pause /> : <Play />}
        {actions.watching ? t("stop_rotation") : t("start_rotation")}
      </Button>

      {/* Without libmpv there is nothing to drive, so the transport is not shown
          at all rather than sitting there permanently disabled. */}
      {hasMpv && (
        <>
          <Separator orientation="vertical" className="mx-1 h-6" />

          <IconAction
            label={t("previous")}
            onClick={actions.videoPrev}
            disabled={actions.videoBusy}
            icon={<SkipBack />}
          />
          <Button
            size="sm"
            variant={videoRunning ? "destructive" : "outline"}
            onClick={actions.toggleVideo}
            disabled={actions.videoBusy}
          >
            {videoRunning ? <Square /> : <Play />}
            {videoRunning ? t("video_stop") : t("video_play")}
          </Button>
          <IconAction
            label={t("next")}
            onClick={actions.videoNext}
            disabled={actions.videoBusy}
            icon={<SkipForward />}
          />
        </>
      )}
    </footer>
  )
}

function IconAction({
  label,
  icon,
  onClick,
  disabled,
}: {
  label: string
  icon: React.ReactNode
  onClick: () => void
  disabled?: boolean
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button variant="outline" size="icon-sm" onClick={onClick} disabled={disabled}>
            {icon}
            <span className="sr-only">{label}</span>
          </Button>
        }
      />
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}

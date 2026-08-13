/**
 * The wallpaper preview.
 *
 * The old preview was a flat thumbnail of the whole virtual desktop, which on a
 * multi-monitor setup makes every image too small to judge. This one adds the three
 * things that were missing:
 *
 *  - **framing** — the monitor outlines are drawn over the composite, so it is
 *    obvious which picture lands on which screen and where a bezel cuts one
 *  - **focus** — picking a monitor zooms the composite to exactly that screen's
 *    region, animated as a continuous move rather than a swap
 *  - **room** — an expanded view fills the window for a proper look
 *
 * Framing and focus are the same geometry: the composite *is* the virtual desktop,
 * so a monitor's rectangle is a plain percentage of the image either way.
 */
import * as React from "react"
import { Check, Maximize2, Shuffle, X } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import type { Config, MonitorsResult } from "@/lib/engine"
import type { I18n } from "@/lib/use-i18n"
import type { Preview } from "@/lib/use-preview"
import { cn } from "@/lib/utils"

interface Props {
  preview: Preview
  monitors: MonitorsResult | null
  config: Config
  i18n: I18n
  applying: boolean
  /** Apply exactly the set on screen, with no reshuffle. */
  onApply: (images: string[]) => void
}

export function PreviewStage({ preview, monitors, config, i18n, applying, onApply }: Props) {
  const { t } = i18n
  // Which monitor the stage is zoomed to; null is the whole virtual desktop.
  const [focus, setFocus] = React.useState<number | null>(null)
  const [expanded, setExpanded] = React.useState(false)

  React.useEffect(() => {
    if (!expanded) return
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setExpanded(false)
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [expanded])

  const list = monitors?.monitors ?? []
  // A display can be unplugged while the window is open. Resolving the focus against
  // the current list every render means that falls back to the whole desktop instead
  // of leaving the stage zoomed to a monitor that is gone.
  const focused = focus === null ? null : (list.find((m) => m.index === focus) ?? null)
  const activeFocus = focused?.index ?? null

  return (
    <Card>
      <CardHeader className="flex-row items-center justify-between gap-2">
        <CardTitle>{t("preview")}</CardTitle>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={preview.reshuffle}>
            <Shuffle />
            {t("shuffle")}
          </Button>
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  variant="outline"
                  size="icon-sm"
                  disabled={!preview.src}
                  onClick={() => setExpanded(true)}
                >
                  <Maximize2 />
                  <span className="sr-only">{t("preview_expand")}</span>
                </Button>
              }
            />
            <TooltipContent>{t("preview_expand")}</TooltipContent>
          </Tooltip>
          <Button
            size="sm"
            disabled={applying || !preview.src || preview.images.length === 0}
            onClick={() => onApply(preview.images)}
          >
            <Check />
            {t("preview_apply_this")}
          </Button>
        </div>
      </CardHeader>

      <CardContent className="flex flex-col gap-3">
        <Stage
          preview={preview}
          monitors={monitors}
          focus={activeFocus}
          onFocus={setFocus}
          i18n={i18n}
        />

        {list.length > 1 && (
          <div className="flex flex-wrap items-center gap-1.5">
            <FocusChip active={activeFocus === null} onClick={() => setFocus(null)}>
              {t("preview_all")}
            </FocusChip>
            {list.map((m) => (
              <FocusChip
                key={m.index}
                active={activeFocus === m.index}
                onClick={() => setFocus(activeFocus === m.index ? null : m.index)}
              >
                #{m.index + 1}
                <span className="ml-1.5 text-[10px] opacity-70">
                  {m.width}×{m.height}
                </span>
              </FocusChip>
            ))}
          </div>
        )}

        <ImageList
          images={imagesFor(preview.images, config, list.length, activeFocus)}
          label={t("preview_images_used")}
        />
      </CardContent>

      {expanded && (
        <div
          className="fixed inset-0 z-50 flex flex-col gap-3 bg-background/95 p-4 backdrop-blur preview-enter sm:p-8"
          role="dialog"
          aria-modal="true"
          aria-label={t("preview")}
        >
          <div className="flex items-center gap-2">
            <span className="flex-1 truncate text-sm font-medium">
              {t("preview")}
              {monitors && (
                <span className="ml-2 font-normal text-muted-foreground">
                  {monitors.virtual_width}×{monitors.virtual_height}
                </span>
              )}
            </span>
            {list.length > 1 && (
              <div className="flex flex-wrap items-center gap-1.5">
                <FocusChip active={activeFocus === null} onClick={() => setFocus(null)}>
                  {t("preview_all")}
                </FocusChip>
                {list.map((m) => (
                  <FocusChip
                    key={m.index}
                    active={activeFocus === m.index}
                    onClick={() => setFocus(activeFocus === m.index ? null : m.index)}
                  >
                    #{m.index + 1}
                  </FocusChip>
                ))}
              </div>
            )}
            <Button variant="outline" size="icon-sm" onClick={() => setExpanded(false)}>
              <X />
              <span className="sr-only">{t("preview_close")}</span>
            </Button>
          </div>
          {/* min-h-0 so the stage shrinks to the space left rather than pushing the
              header off the top. */}
          <div className="flex min-h-0 flex-1 items-center justify-center">
            <Stage
              preview={preview}
              monitors={monitors}
              focus={activeFocus}
              onFocus={setFocus}
              i18n={i18n}
              fit
            />
          </div>
        </div>
      )}
    </Card>
  )
}

/**
 * The image itself, framed either to the whole virtual desktop or to one monitor.
 *
 * Both cases are the same layout: a window with the target's aspect ratio, and the
 * composite oversized and offset inside it. Because every value is a percentage of
 * that window, the browser can interpolate between the two — which is what makes
 * focusing a monitor read as a zoom instead of a cut.
 */
function Stage({
  preview,
  monitors,
  focus,
  onFocus,
  i18n,
  fit = false,
  className,
}: {
  preview: Preview
  monitors: MonitorsResult | null
  focus: number | null
  onFocus: (index: number | null) => void
  i18n: I18n
  /** Size to the space available in both axes instead of filling the width. */
  fit?: boolean
  className?: string
}) {
  const { t } = i18n
  // The frame the new one fades over, so a re-render crossfades instead of showing
  // the empty stage through it. Promoted once the fade has finished — that keeps the
  // outgoing image underneath for exactly as long as it is needed, and needs no
  // effect to track it.
  const [settled, setSettled] = React.useState<string | null>(null)
  const current = preview.src
  const previous = settled === current ? null : settled

  const geometry = frameFor(monitors, focus)
  const aspect = geometry ? geometry.aspect : preview.width / (preview.height || 1) || 16 / 9
  const rootRef = React.useRef<HTMLDivElement>(null)
  const fitted = useFittedSize(rootRef, aspect, fit)

  return (
    <div
      ref={rootRef}
      className={cn("flex w-full flex-col gap-2", fit && "h-full items-center justify-center", className)}
    >
      <div
        className={cn(
          "relative overflow-hidden rounded-lg border bg-muted/30",
          !fitted && "w-full",
          preview.loading && "sheen",
        )}
        style={fitted ? { width: fitted.width, height: fitted.height } : { aspectRatio: aspect }}
      >
        {current ? (
          <>
            {previous && (
              <img src={previous} alt="" aria-hidden className="absolute" style={geometry?.style} />
            )}
            <img
              key={current}
              src={current}
              alt={t("preview")}
              className="preview-enter absolute"
              onAnimationEnd={() => setSettled(current)}
              style={{
                ...geometry?.style,
                transition:
                  "width 420ms var(--ease-out-soft), height 420ms var(--ease-out-soft), left 420ms var(--ease-out-soft), top 420ms var(--ease-out-soft)",
              }}
            />
          </>
        ) : (
          <div className="absolute inset-0 flex items-center justify-center text-sm text-muted-foreground">
            {preview.error ?? (preview.loading ? t("loading") : t("no_preview"))}
          </div>
        )}

        {/* Guides only make sense on the full desktop; zoomed in there is one
            screen and its edges are the frame. */}
        {focus === null && current && monitors && (
          <MonitorGuides monitors={monitors} onFocus={onFocus} />
        )}
      </div>

      {preview.error && current && (
        <p className="text-xs text-destructive">{preview.error}</p>
      )}
    </div>
  )
}

/**
 * Largest box of *aspect* that fits inside the element, measured rather than left
 * to CSS: `aspect-ratio` can only follow one axis, and the expanded view is bounded
 * by both. Returns null when not fitting, so the inline layout keeps its own sizing.
 */
function useFittedSize(
  ref: React.RefObject<HTMLElement | null>,
  aspect: number,
  enabled: boolean,
): { width: number; height: number } | null {
  const [box, setBox] = React.useState<{ width: number; height: number } | null>(null)

  React.useEffect(() => {
    const element = ref.current
    if (!enabled || !element) {
      setBox(null)
      return
    }
    const measure = () => {
      const { width, height } = element.getBoundingClientRect()
      if (width === 0 || height === 0) return
      const w = Math.min(width, height * aspect)
      setBox({ width: w, height: w / aspect })
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    return () => observer.disconnect()
  }, [ref, aspect, enabled])

  return box
}

/** Clickable outline per monitor, laid over the composite. */
function MonitorGuides({
  monitors,
  onFocus,
}: {
  monitors: MonitorsResult
  onFocus: (index: number) => void
}) {
  const { monitors: list, virtual_width, virtual_height } = monitors
  if (list.length === 0 || virtual_width === 0) return null
  const minX = Math.min(...list.map((m) => m.x))
  const minY = Math.min(...list.map((m) => m.y))

  return (
    <div className="absolute inset-0">
      {list.map((m) => (
        <button
          key={m.index}
          type="button"
          onClick={() => onFocus(m.index)}
          className="group absolute flex items-end justify-start rounded-sm outline outline-1 -outline-offset-1 outline-white/40 transition-colors hover:bg-primary/15 hover:outline-primary focus-visible:bg-primary/15 focus-visible:outline-primary"
          style={{
            left: `${((m.x - minX) / virtual_width) * 100}%`,
            top: `${((m.y - minY) / virtual_height) * 100}%`,
            width: `${(m.width / virtual_width) * 100}%`,
            height: `${(m.height / virtual_height) * 100}%`,
          }}
        >
          <span className="m-1.5 rounded bg-black/55 px-1.5 py-0.5 text-[10px] font-medium text-white opacity-70 transition-opacity group-hover:opacity-100">
            #{m.index + 1} · {m.width}×{m.height}
          </span>
        </button>
      ))}
    </div>
  )
}

function FocusChip({
  active,
  onClick,
  children,
}: {
  active: boolean
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "rounded-full border px-2.5 py-1 text-xs transition-colors",
        active
          ? "border-primary bg-primary text-primary-foreground"
          : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
      )}
    >
      {children}
    </button>
  )
}

function ImageList({ images, label }: { images: string[]; label: string }) {
  if (images.length === 0) return null
  return (
    <div className="flex flex-col gap-1">
      <span className="text-xs text-muted-foreground">{label}</span>
      <div className="flex flex-wrap gap-1">
        {images.map((path) => (
          <span
            key={path}
            title={path}
            className="max-w-[15rem] truncate rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
          >
            {basename(path)}
          </span>
        ))}
      </div>
    </div>
  )
}

/**
 * Where the composite sits inside the stage window.
 *
 * `null` focus frames the whole virtual desktop, so the image simply fills it. A
 * focused monitor keeps the same image scaled up by the ratio between the desktop
 * and that screen, shifted so the screen's corner lands at the window's corner.
 */
function frameFor(
  monitors: MonitorsResult | null,
  focus: number | null,
): { aspect: number; style: React.CSSProperties } | null {
  if (!monitors || monitors.virtual_width === 0) return null
  const { monitors: list, virtual_width: vw, virtual_height: vh } = monitors

  const target = focus === null ? null : list.find((m) => m.index === focus)
  if (!target) {
    return {
      aspect: vw / vh,
      style: { left: 0, top: 0, width: "100%", height: "100%" },
    }
  }

  const minX = Math.min(...list.map((m) => m.x))
  const minY = Math.min(...list.map((m) => m.y))
  return {
    aspect: target.width / target.height,
    style: {
      left: `${(-(target.x - minX) / target.width) * 100}%`,
      top: `${(-(target.y - minY) / target.height) * 100}%`,
      width: `${(vw / target.width) * 100}%`,
      height: `${(vh / target.height) * 100}%`,
    },
  }
}

/**
 * The slice of the selection that belongs to one monitor.
 *
 * Mirrors how `compose_collage` consumes the list: `collage_count` images per
 * monitor in order, unless every monitor is showing the same set.
 */
function imagesFor(
  images: string[],
  config: Config,
  monitorCount: number,
  focus: number | null,
): string[] {
  if (focus === null || monitorCount <= 1 || config.general.collage_same_for_all) return images
  const count = Math.max(1, config.general.collage_count)
  return images.slice(focus * count, (focus + 1) * count)
}

function basename(path: string): string {
  const at = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"))
  return at >= 0 ? path.slice(at + 1) : path
}

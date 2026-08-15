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
 *
 * The composite is also editable in place: every image in it carries a handle that
 * can be dragged onto another to swap the two, or clicked to choose a different
 * picture. Those handles come from the engine's own layout, never from a second
 * implementation of the grid rules here.
 */
import * as React from "react"
import { createPortal } from "react-dom"
import { Check, Maximize2, Shuffle, X } from "lucide-react"

import { ImagePickerDialog } from "@/components/image-picker-dialog"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import type { Config, MonitorsResult, PreviewCell } from "@/lib/engine"
import type { I18n } from "@/lib/use-i18n"
import type { Preview } from "@/lib/use-preview"
import { useThumbnails } from "@/lib/use-thumbnails"
import { basename, cn } from "@/lib/utils"

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
  // Which slot of the selection the picker is choosing for; null while closed.
  const [picking, setPicking] = React.useState<number | null>(null)

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
          onPick={setPicking}
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

        {preview.cells.length > 0 && (
          <p className="text-xs text-muted-foreground">{t("preview_arrange_hint")}</p>
        )}

        <ImageList
          images={imagesFor(preview.images, config, list.length, activeFocus)}
          label={t("preview_images_used")}
        />
      </CardContent>

      {/* Portalled out of the Card. Cards lift on hover, and a transformed ancestor
          becomes the containing block for its fixed descendants — left inside, this
          "full window" surface would be anchored to the card and clipped by its
          overflow for as long as the pointer was on it, which is always. */}
      {expanded && createPortal(
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
              onPick={setPicking}
              i18n={i18n}
              fit
            />
          </div>
        </div>,
        document.body,
      )}

      <ImagePickerDialog
        open={picking !== null}
        onOpenChange={(open) => !open && setPicking(null)}
        folder={config.paths.wallpapers_folder}
        current={picking === null ? null : (preview.images[picking] ?? null)}
        i18n={i18n}
        onPick={(path) => picking !== null && preview.replace(picking, path)}
      />
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
  onPick,
  i18n,
  fit = false,
  className,
}: {
  preview: Preview
  monitors: MonitorsResult | null
  focus: number | null
  onFocus: (index: number | null) => void
  /** Open the picker for one slot of the selection. */
  onPick: (imageIndex: number) => void
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
            {/* max-w-none is load-bearing: Tailwind's preflight caps every image at
                `max-width: 100%`, which silently clamps the >100% width a focused
                monitor asks for while leaving the inline height alone — the zoom
                came out squashed instead of cropped. */}
            {previous && (
              <img
                src={previous}
                alt=""
                aria-hidden
                className="absolute max-w-none"
                style={geometry?.style}
              />
            )}
            <img
              key={current}
              src={current}
              alt={t("preview")}
              className="preview-enter absolute max-w-none"
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

        {current && monitors && (
          <CollageCells
            preview={preview}
            monitors={monitors}
            focus={focus}
            geometry={geometry?.style}
            onPick={onPick}
            i18n={i18n}
          />
        )}

        {/* Guides only make sense on the full desktop; zoomed in there is one
            screen and its edges are the frame. Drawn after the cells so a bezel
            stays visible over them, and so the badge that focuses a monitor is not
            buried under a drag target. */}
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

/**
 * Monitor outlines over the composite.
 *
 * The outline is decoration and lets pointer events through: the pictures beneath
 * it are draggable now, and a full-screen click target per monitor would swallow
 * every one of those gestures. Only the corner badge stays clickable, offering the
 * same "zoom to this screen" the chips below the stage do.
 */
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
    <div className="pointer-events-none absolute inset-0">
      {list.map((m) => (
        <div
          key={m.index}
          className="absolute flex items-start justify-start rounded-sm outline outline-1 -outline-offset-1 outline-white/40"
          style={{
            left: `${((m.x - minX) / virtual_width) * 100}%`,
            top: `${((m.y - minY) / virtual_height) * 100}%`,
            width: `${(m.width / virtual_width) * 100}%`,
            height: `${(m.height / virtual_height) * 100}%`,
          }}
        >
          <button
            type="button"
            onClick={() => onFocus(m.index)}
            className="pointer-events-auto m-1.5 rounded bg-black/55 px-1.5 py-0.5 text-[10px] font-medium text-white opacity-70 transition-opacity hover:bg-black/80 hover:opacity-100 focus-visible:opacity-100 focus-visible:outline-none"
          >
            #{m.index + 1} · {m.width}×{m.height}
          </button>
        </div>
      ))}
    </div>
  )
}

/**
 * The direct-manipulation layer: one handle per picture in the collage.
 *
 * It lives inside a wrapper carrying the *same* geometry as the composite, so
 * zooming to a monitor moves the picture and its handles as one thing — measuring
 * the handles against the stage instead would let the two drift apart mid
 * transition.
 *
 * Drags use pointer events rather than HTML5 drag-and-drop: the webview reserves
 * native drags for file drops, and pointer capture keeps a gesture alive once it
 * leaves the element it began on. The cell under the cursor is found by hit-testing
 * the document, which is what lets a picture be dropped onto another monitor.
 */
function CollageCells({
  preview,
  monitors,
  focus,
  geometry,
  onPick,
  i18n,
}: {
  preview: Preview
  monitors: MonitorsResult
  focus: number | null
  geometry: React.CSSProperties | undefined
  onPick: (imageIndex: number) => void
  i18n: I18n
}) {
  const { t } = i18n
  const { virtual_width: vw, virtual_height: vh } = monitors
  // Mirrored in a ref because pointerup can land in the same frame as the move
  // before it, ahead of the re-render that would refresh a closure over state.
  const [drag, setDragState] = React.useState<Drag | null>(null)
  const dragRef = React.useRef<Drag | null>(null)
  const setDrag = (next: Drag | null) => {
    dragRef.current = next
    setDragState(next)
  }
  const thumbs = useThumbnails(preview.images)

  // Zoomed in, the other screens' handles are only clipped, not gone. Dropping the
  // ones that cannot be seen keeps them out of the tab order too.
  const cells = React.useMemo(
    () => preview.cells.filter((cell) => focus === null || cell.monitor === focus),
    [preview.cells, focus],
  )

  if (cells.length === 0 || vw === 0 || vh === 0) return null

  function handleDown(event: React.PointerEvent<HTMLButtonElement>, cell: PreviewCell) {
    // Left button only: a right-click is the system menu, not a drag.
    if (event.button !== 0) return
    event.currentTarget.setPointerCapture(event.pointerId)
    setDrag({
      pointer: event.pointerId,
      from: cell.image_index,
      over: null,
      x: event.clientX,
      y: event.clientY,
      moved: false,
    })
  }

  function handleMove(event: React.PointerEvent<HTMLButtonElement>) {
    const current = dragRef.current
    if (!current || current.pointer !== event.pointerId) return
    // A few pixels of slop, so a click that trembles still opens the picker.
    const moved =
      current.moved ||
      Math.abs(event.clientX - current.x) + Math.abs(event.clientY - current.y) > 5
    if (!moved) return
    setDrag({
      ...current,
      moved,
      x: event.clientX,
      y: event.clientY,
      over: cellUnder(event),
    })
  }

  function handleUp(event: React.PointerEvent<HTMLButtonElement>, cell: PreviewCell) {
    const current = dragRef.current
    setDrag(null)
    if (!current || current.pointer !== event.pointerId) return
    if (!current.moved) {
      onPick(cell.image_index)
      return
    }
    const target = cellUnder(event)
    if (target !== null) preview.swap(current.from, target)
  }

  return (
    <>
      {/* Same geometry and the same easing as the composite: the handles have to
          travel with the picture, not chase it. */}
      <div
        className="absolute"
        style={{
          ...geometry,
          transition:
            "width 420ms var(--ease-out-soft), height 420ms var(--ease-out-soft), left 420ms var(--ease-out-soft), top 420ms var(--ease-out-soft)",
        }}
      >
        {cells.map((cell) => {
          // With collage_same_for_all several cells share one picture, so dimming
          // by image index correctly lights up every copy of what is being moved.
          const source = drag?.moved && drag.from === cell.image_index
          const target = drag?.moved && drag.over === cell.image_index && !source
          return (
            <button
              key={`${cell.monitor}:${cell.x}:${cell.y}`}
              type="button"
              data-image-index={cell.image_index}
              title={basename(preview.images[cell.image_index] ?? "")}
              aria-label={t("preview_choose_image")}
              onPointerDown={(e) => handleDown(e, cell)}
              onPointerMove={handleMove}
              onPointerUp={(e) => handleUp(e, cell)}
              onPointerCancel={() => setDrag(null)}
              // Pointer handlers alone would leave this unreachable by keyboard.
              onKeyDown={(e) => {
                if (e.key !== "Enter" && e.key !== " ") return
                e.preventDefault()
                onPick(cell.image_index)
              }}
              className={cn(
                "absolute cursor-grab rounded-sm outline-1 -outline-offset-2 transition-colors select-none hover:bg-primary/10 hover:outline hover:outline-primary/70 focus-visible:bg-primary/10 focus-visible:outline focus-visible:outline-primary",
                source && "cursor-grabbing opacity-40",
                target && "bg-primary/25 outline-2 outline-primary",
              )}
              style={{
                left: `${(cell.x / vw) * 100}%`,
                top: `${(cell.y / vh) * 100}%`,
                width: `${(cell.width / vw) * 100}%`,
                height: `${(cell.height / vh) * 100}%`,
              }}
            />
          )
        })}
      </div>

      {/* Portalled to the body, not merely fixed: the surrounding Card lifts on
          hover, and a transformed ancestor becomes the containing block for fixed
          children — which would re-anchor these viewport coordinates and clip the
          ghost to the very card it is being dragged across. */}
      {drag?.moved &&
        thumbs[preview.images[drag.from] ?? ""] &&
        createPortal(
          <img
            src={thumbs[preview.images[drag.from]]}
            alt=""
            aria-hidden
            draggable={false}
            className="pointer-events-none fixed z-50 h-16 max-w-none -translate-x-1/2 -translate-y-1/2 rounded-md object-cover shadow-lg ring-2 ring-primary"
            style={{ left: drag.x, top: drag.y }}
          />,
          document.body,
        )}
    </>
  )
}

interface Drag {
  pointer: number
  /** Slot of the selection the gesture picked up. */
  from: number
  /** Slot currently under the cursor, if any. */
  over: number | null
  x: number
  y: number
  /** False until the pointer has travelled far enough to be a drag, not a click. */
  moved: boolean
}

/** Which slot the pointer is over, by hit-testing rather than by geometry. */
function cellUnder(event: React.PointerEvent): number | null {
  const hit = document
    .elementsFromPoint(event.clientX, event.clientY)
    .find((el) => el instanceof HTMLElement && el.dataset.imageIndex !== undefined)
  if (!(hit instanceof HTMLElement)) return null
  const index = Number(hit.dataset.imageIndex)
  return Number.isInteger(index) ? index : null
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

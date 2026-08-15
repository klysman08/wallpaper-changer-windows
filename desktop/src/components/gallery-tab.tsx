/**
 * The library of collages the user has saved from the preview.
 *
 * Every picture here is a file on disk that the webview cannot read, so both the
 * card thumbnails and the full view come across the RPC boundary as base64 — the
 * cards through the shared thumbnail cache, the full view on demand, since a
 * desktop-wide composite is far too large to hold for every card at once.
 *
 * Two deliberate asymmetries:
 *
 *  - **Applying is exact.** A saved collage goes back on the desktop as saved: the
 *    engine neither recomposes it nor re-applies an effect the file already carries.
 *  - **Removing is not deleting.** "Remove" drops the entry from the library index
 *    and leaves the image where it is. Destroying the user's picture is Explorer's
 *    job, and "Show in folder" is right there.
 */
import * as React from "react"
import { createPortal } from "react-dom"
import { open } from "@tauri-apps/plugin-dialog"
import { revealItemInDir } from "@tauri-apps/plugin-opener"
import { Check, FolderOpen, Maximize2, Trash2, X } from "lucide-react"
import { toast } from "sonner"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { engine, type Config, type SavedCollage } from "@/lib/engine"
import type { I18n } from "@/lib/use-i18n"
import { useThumbnails } from "@/lib/use-thumbnails"
import { basename, cn } from "@/lib/utils"

/** Wide enough that a four-picture collage is still readable on a card. */
const THUMB = 480

interface Props {
  config: Config
  i18n: I18n
  onChange: <S extends keyof Config, K extends keyof Config[S]>(
    section: S,
    key: K,
    value: Config[S][K],
  ) => void
}

export function GalleryTab({ config, i18n, onChange }: Props) {
  const { t } = i18n
  const [collages, setCollages] = React.useState<SavedCollage[] | null>(null)
  // Where the *next* save will land, resolved by the engine from the setting — a
  // relative or empty value is a real folder there and nowhere else.
  const [folder, setFolder] = React.useState("")
  const [error, setError] = React.useState<string | null>(null)
  // Which entry is being applied; only one at a time, and the card says so.
  const [applying, setApplying] = React.useState<string | null>(null)
  const [viewing, setViewing] = React.useState<SavedCollage | null>(null)

  // Re-runs when the setting changes so the resolved path under the input keeps up
  // with what is typed, without waiting for a save.
  const savedFolder = config.paths.saved_folder ?? ""

  React.useEffect(() => {
    let cancelled = false
    void engine
      .listSavedCollages(config)
      .then((result) => {
        if (cancelled) return
        setCollages(result.collages)
        setFolder(result.folder)
        setError(null)
      })
      .catch((e) => !cancelled && setError((e as Error).message))
    return () => {
      cancelled = true
    }
    // The whole config is not a dependency: only the folder decides this answer,
    // and watching the rest would re-list on every unrelated keystroke.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [savedFolder])

  async function browse() {
    const picked = await open({ directory: true, defaultPath: folder || undefined })
    if (typeof picked === "string") onChange("paths", "saved_folder", picked)
  }

  async function apply(collage: SavedCollage) {
    setApplying(collage.path)
    try {
      await engine.applySavedCollage(collage.path)
      toast.success(t("wallpaper_applied", { name: basename(collage.path) }))
    } catch (e) {
      toast.error(t("apply_failed"), { description: (e as Error).message })
    } finally {
      setApplying(null)
    }
  }

  async function remove(collage: SavedCollage) {
    try {
      await engine.forgetSavedCollage(collage.path)
      // Optimistic enough: the engine has already answered, and re-listing would
      // cost a round trip to learn what we just asked for.
      setCollages((current) => current?.filter((c) => c.path !== collage.path) ?? null)
      toast.success(t("gallery_removed"))
    } catch (e) {
      toast.error((e as Error).message)
    }
  }

  const paths = React.useMemo(() => (collages ?? []).map((c) => c.path), [collages])
  const thumbs = useThumbnails(paths, THUMB)

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader className="flex-row items-center justify-between gap-2">
          <CardTitle>{t("gallery")}</CardTitle>
          {collages && collages.length > 0 && (
            <Badge variant="secondary">{t("gallery_count", { count: collages.length })}</Badge>
          )}
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="saved-folder">{t("gallery_folder")}</Label>
            <div className="flex gap-2">
              <Input
                id="saved-folder"
                value={savedFolder}
                placeholder={folder}
                spellCheck={false}
                onChange={(e) => onChange("paths", "saved_folder", e.target.value)}
              />
              <Button variant="outline" onClick={browse}>
                {t("browse")}
              </Button>
            </div>
            <p className="truncate font-mono text-[11px] text-muted-foreground" title={folder}>
              {t("gallery_folder_hint", { folder })}
            </p>
          </div>

          {error && <p className="text-sm text-destructive">{error}</p>}

          {collages === null && !error && (
            <p className="text-sm text-muted-foreground">{t("loading")}</p>
          )}

          {collages?.length === 0 && (
            <div className="flex flex-col items-center gap-1 py-10 text-center">
              <p className="text-sm font-medium">{t("gallery_empty")}</p>
              <p className="max-w-sm text-xs text-muted-foreground">
                {t("gallery_empty_hint")}
              </p>
            </div>
          )}

          {collages && collages.length > 0 && (
            <div className="grid gap-3 sm:grid-cols-2">
              {collages.map((collage) => (
                <CollageCard
                  key={collage.path}
                  collage={collage}
                  thumb={thumbs[collage.path]}
                  applying={applying === collage.path}
                  busy={applying !== null}
                  i18n={i18n}
                  onApply={() => void apply(collage)}
                  onView={() => setViewing(collage)}
                  onRemove={() => void remove(collage)}
                />
              ))}
            </div>
          )}

        </CardContent>
      </Card>

      {/* Keyed on the file: opening a second collage from behind the first one
          remounts this, so the new image loads from an empty state instead of the
          previous picture lingering until its replacement arrives. */}
      {viewing && (
        <Lightbox
          key={viewing.path}
          collage={viewing}
          i18n={i18n}
          onClose={() => setViewing(null)}
        />
      )}
    </div>
  )
}

function CollageCard({
  collage,
  thumb,
  applying,
  busy,
  i18n,
  onApply,
  onView,
  onRemove,
}: {
  collage: SavedCollage
  thumb: string | undefined
  applying: boolean
  /** Something else is being applied; the whole grid waits rather than queueing. */
  busy: boolean
  i18n: I18n
  onApply: () => void
  onView: () => void
  onRemove: () => void
}) {
  const { t } = i18n
  const where =
    collage.monitor === null ? t("preview_all") : t("monitor_n", { n: collage.monitor + 1 })

  return (
    <div className="flex flex-col gap-2 rounded-lg border p-2">
      <button
        type="button"
        onClick={onView}
        title={collage.path}
        className="group relative aspect-video w-full overflow-hidden rounded-md bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
      >
        {thumb ? (
          <img
            src={thumb}
            alt={basename(collage.path)}
            className="size-full object-contain transition-transform group-hover:scale-[1.02]"
          />
        ) : (
          <span className="flex size-full items-center justify-center text-xs text-muted-foreground">
            {t("loading")}
          </span>
        )}
        <span className="absolute inset-0 flex items-center justify-center bg-black/40 opacity-0 transition-opacity group-hover:opacity-100">
          <Maximize2 className="size-5 text-white" />
        </span>
      </button>

      <div className="flex flex-col gap-0.5">
        <span className="truncate text-sm font-medium" title={collage.path}>
          {basename(collage.path)}
        </span>
        <span className="text-xs text-muted-foreground">
          {where} · {collage.width}×{collage.height} · {formatSavedAt(collage.saved_at)}
        </span>
      </div>

      <div className="flex items-center gap-1.5">
        <Button size="sm" className="flex-1" disabled={busy} onClick={onApply}>
          <Check />
          {applying ? t("applying") : t("gallery_apply")}
        </Button>
        <IconAction
          label={t("gallery_show_in_folder")}
          onClick={() => void revealItemInDir(collage.path).catch(() => {})}
        >
          <FolderOpen />
        </IconAction>
        <IconAction label={t("gallery_remove")} hint={t("gallery_remove_hint")} onClick={onRemove}>
          <Trash2 />
        </IconAction>
      </div>
    </div>
  )
}

function IconAction({
  label,
  hint,
  onClick,
  children,
}: {
  label: string
  hint?: string
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button variant="outline" size="icon-sm" onClick={onClick}>
            {children}
            <span className="sr-only">{label}</span>
          </Button>
        }
      />
      <TooltipContent>
        {label}
        {hint && <span className="block max-w-56 text-xs opacity-80">{hint}</span>}
      </TooltipContent>
    </Tooltip>
  )
}

/**
 * The saved image at a size worth looking at.
 *
 * Portalled to the body for the same reason the preview overlay is: a Card lifts on
 * hover, and a transformed ancestor becomes the containing block for its fixed
 * children — inside one, this would be anchored to the card and clipped by it.
 */
function Lightbox({
  collage,
  i18n,
  onClose,
}: {
  collage: SavedCollage
  i18n: I18n
  onClose: () => void
}) {
  const { t } = i18n
  const [src, setSrc] = React.useState<string | null>(null)
  const [error, setError] = React.useState<string | null>(null)

  React.useEffect(() => {
    let cancelled = false
    void engine
      .getImagePreview(collage.path)
      .then((r) => !cancelled && setSrc(`data:image/jpeg;base64,${r.jpeg_base64}`))
      .catch((e) => !cancelled && setError((e as Error).message))
    return () => {
      cancelled = true
    }
  }, [collage.path])

  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose()
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [onClose])

  return createPortal(
    <div
      className="preview-enter fixed inset-0 z-50 flex flex-col gap-3 bg-background/95 p-4 backdrop-blur sm:p-8"
      role="dialog"
      aria-modal="true"
      aria-label={basename(collage.path)}
    >
      <div className="flex items-center gap-2">
        <span className="flex-1 truncate text-sm font-medium">
          {basename(collage.path)}
          <span className="ml-2 font-normal text-muted-foreground">
            {collage.width}×{collage.height}
          </span>
        </span>
        <Button variant="outline" size="icon-sm" onClick={onClose}>
          <X />
          <span className="sr-only">{t("preview_close")}</span>
        </Button>
      </div>
      <div className="flex min-h-0 flex-1 items-center justify-center">
        {src ? (
          <img
            src={src}
            alt={basename(collage.path)}
            className={cn("max-h-full max-w-full rounded-lg object-contain shadow-lg")}
          />
        ) : (
          <p className="text-sm text-muted-foreground">{error ?? t("loading")}</p>
        )}
      </div>
    </div>,
    document.body,
  )
}

/** The saved timestamp in the reader's own locale, date and time, no seconds. */
function formatSavedAt(iso: string): string {
  const at = new Date(iso)
  if (Number.isNaN(at.getTime())) return iso
  return at.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })
}

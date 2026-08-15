/**
 * Choose which picture goes in one spot of the collage.
 *
 * Opened by clicking a cell in the preview. The folder listing and the thumbnails
 * both come from the engine — the webview has no filesystem access — so the grid
 * fills in progressively as batches arrive rather than blocking on the whole folder.
 */
import * as React from "react"
import { Check, Search } from "lucide-react"

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { engine } from "@/lib/engine"
import type { I18n } from "@/lib/use-i18n"
import { useThumbnails } from "@/lib/use-thumbnails"
import { basename, cn } from "@/lib/utils"

/** Beyond this the grid is unusable anyway, and the search box is the way through. */
const MAX_SHOWN = 300

/** Shared so "nothing listed yet" keeps a stable identity across renders. */
const NONE: string[] = []

/** A folder listing, tagged with the folder it describes. */
interface Listing {
  folder: string
  images: string[]
  error: string | null
}

interface Props {
  open: boolean
  onOpenChange: (open: boolean) => void
  folder: string
  /** The file currently in the slot, marked in the grid. */
  current: string | null
  i18n: I18n
  onPick: (path: string) => void
}

export function ImagePickerDialog({
  open,
  onOpenChange,
  folder,
  current,
  i18n,
  onPick,
}: Props) {
  const { t } = i18n
  const [query, setQuery] = React.useState("")
  // The folder travels with its own listing so a stale one is recognisable rather
  // than shown: after a folder change this simply reads as not-yet-loaded, and
  // nothing has to be cleared on the way in.
  const [listing, setListing] = React.useState<Listing | null>(null)

  // Listed on open rather than on mount: the folder can change under a long-lived
  // window, and a closed dialog has no reason to hold a few hundred paths.
  React.useEffect(() => {
    if (!open) return
    let cancelled = false
    void engine
      .listFolderImages(folder)
      .then(
        (result) => !cancelled && setListing({ folder, images: result.images, error: null }),
      )
      .catch(
        (e) =>
          !cancelled && setListing({ folder, images: [], error: (e as Error).message }),
      )
    return () => {
      cancelled = true
    }
  }, [open, folder])

  const loaded = listing?.folder === folder ? listing : null
  const all = loaded?.images ?? NONE
  const error = loaded?.error ?? null

  const needle = query.trim().toLowerCase()
  const matches = React.useMemo(() => {
    const filtered = needle
      ? all.filter((path) => basename(path).toLowerCase().includes(needle))
      : all
    return filtered.slice(0, MAX_SHOWN)
  }, [all, needle])

  // Only the visible slice is fetched, so narrowing the search is also what brings
  // a large folder's later pictures into view.
  const thumbs = useThumbnails(matches)

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[85vh] flex-col sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t("preview_choose_image")}</DialogTitle>
          <DialogDescription>{folder}</DialogDescription>
        </DialogHeader>

        <div className="relative">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("search_images")}
            spellCheck={false}
            className="pl-8"
          />
        </div>

        {error ? (
          <p className="py-8 text-center text-sm text-destructive">{error}</p>
        ) : matches.length === 0 ? (
          <p className="py-8 text-center text-sm text-muted-foreground">
            {loaded === null ? t("loading") : t("no_images_found")}
          </p>
        ) : (
          <div className="-mx-1 grid min-h-0 flex-1 grid-cols-[repeat(auto-fill,minmax(7rem,1fr))] gap-2 overflow-y-auto px-1 py-1">
            {matches.map((path) => (
              <button
                key={path}
                type="button"
                title={path}
                onClick={() => {
                  onPick(path)
                  onOpenChange(false)
                }}
                className={cn(
                  "group relative aspect-video overflow-hidden rounded-md border bg-muted/40 transition-colors hover:border-primary focus-visible:border-primary focus-visible:outline-none",
                  path === current && "border-primary ring-2 ring-primary/40",
                )}
              >
                {thumbs[path] ? (
                  <img
                    src={thumbs[path]}
                    alt=""
                    draggable={false}
                    className="size-full max-w-none object-cover"
                  />
                ) : (
                  <span className="sheen absolute inset-0" />
                )}
                {path === current && (
                  <span className="absolute top-1 right-1 rounded-full bg-primary p-0.5 text-primary-foreground">
                    <Check className="size-3" />
                  </span>
                )}
                <span className="absolute inset-x-0 bottom-0 truncate bg-black/60 px-1.5 py-0.5 text-left text-[10px] text-white opacity-0 transition-opacity group-hover:opacity-100">
                  {basename(path)}
                </span>
              </button>
            ))}
          </div>
        )}

        {all.length > matches.length && (
          <p className="text-xs text-muted-foreground">
            {t("showing_n_of_m", { shown: matches.length, total: all.length })}
          </p>
        )}
      </DialogContent>
    </Dialog>
  )
}

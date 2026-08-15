/**
 * The update dialog: what the sidebar's "Update available" item opens.
 *
 * It only ever appears on request — {@link useUpdate} lights up the sidebar instead of
 * interrupting, so nothing here is modal until the user chooses it.
 */
import { Download, RotateCw, TriangleAlert } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Progress, ProgressLabel, ProgressValue } from "@/components/ui/progress"
import type { I18n } from "@/lib/use-i18n"
import type { UpdateStore } from "@/lib/use-update"

function mb(bytes: number) {
  return (bytes / 1024 / 1024).toFixed(1)
}

export function UpdateDialog({ update, i18n }: { update: UpdateStore; i18n: I18n }) {
  const { t } = i18n
  const { status, downloaded, contentLength } = update
  const busy = status === "downloading"

  return (
    <Dialog open={update.dialogOpen} onOpenChange={update.setDialogOpen}>
      <DialogContent className="sm:max-w-md" showCloseButton={!busy}>
        <DialogHeader>
          <DialogTitle>{t("update_available")}</DialogTitle>
          <DialogDescription>
            {t("current_version")} {update.currentVersion || "—"} → {update.version ?? "—"}
          </DialogDescription>
        </DialogHeader>

        {status === "available" && update.notes && (
          <div className="flex flex-col gap-1.5">
            <p className="text-xs font-medium text-muted-foreground">{t("release_notes")}</p>
            {/* Plain text on purpose: the notes come from a release manifest, so they
                are never rendered as markup. */}
            <div className="max-h-56 overflow-y-auto rounded-md bg-muted/50 p-3 text-xs whitespace-pre-wrap">
              {update.notes}
            </div>
          </div>
        )}

        {busy && (
          <Progress
            // Indeterminate until the server reports a size — some responses omit it.
            value={contentLength ? Math.round((downloaded / contentLength) * 100) : null}
          >
            <ProgressLabel className="text-xs font-normal text-muted-foreground">
              {t("downloading_update")}
            </ProgressLabel>
            {contentLength ? (
              <ProgressValue className="text-xs tabular-nums" />
            ) : (
              <span className="ml-auto text-xs text-muted-foreground tabular-nums">
                {mb(downloaded)} MB
              </span>
            )}
          </Progress>
        )}

        {status === "ready" && (
          <p className="text-sm text-muted-foreground">{t("update_ready_restart")}</p>
        )}

        {status === "error" && update.error && (
          <div className="flex gap-2 rounded-md bg-destructive/10 p-3 text-xs text-destructive">
            <TriangleAlert className="mt-px size-4 shrink-0" />
            <span className="break-all">{update.error}</span>
          </div>
        )}

        <DialogFooter>
          {!busy && status !== "ready" && (
            <DialogClose render={<Button variant="outline" />}>{t("later")}</DialogClose>
          )}
          {status === "ready" ? (
            <Button onClick={() => void update.install()}>
              <RotateCw />
              {t("restart_now")}
            </Button>
          ) : (
            <Button onClick={() => void update.install()} disabled={busy || !update.version}>
              <Download />
              {busy ? t("downloading_update") : t("download_and_install")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

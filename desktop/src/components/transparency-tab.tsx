import * as React from "react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Slider } from "@/components/ui/slider"
import { engine, type WindowInfo } from "@/lib/engine"
import type { I18n } from "@/lib/use-i18n"
import { cn } from "@/lib/utils"

const FULLY_OPAQUE = 255

export function TransparencyTab({ i18n }: { i18n: I18n }) {
  const { t } = i18n
  const [windows, setWindows] = React.useState<WindowInfo[]>([])
  const [selected, setSelected] = React.useState<WindowInfo | null>(null)
  const [alpha, setAlpha] = React.useState(FULLY_OPAQUE)
  const [saved, setSaved] = React.useState<Record<string, number>>({})

  const refresh = React.useCallback(async () => {
    try {
      const [list, settings] = await Promise.all([
        engine.listWindows(),
        engine.getOpacitySettings(),
      ])
      setWindows(list.windows)
      setSaved(settings.settings)
    } catch (e) {
      toast.error((e as Error).message)
    }
  }, [])

  React.useEffect(() => {
    void refresh()
  }, [refresh])

  function select(win: WindowInfo) {
    setSelected(win)
    setAlpha(saved[win.process] ?? FULLY_OPAQUE)
  }

  // Applied live while dragging so the user sees the result on the real window;
  // persisting is a separate, explicit step.
  function onAlphaChange(next: number) {
    setAlpha(next)
    if (selected) void engine.setWindowOpacity(selected.hwnd, next).catch(() => {})
  }

  async function persist() {
    if (!selected) return
    try {
      const next = { ...saved, [selected.process]: alpha }
      await engine.saveOpacitySettings(next)
      setSaved(next)
      toast.success(t("saved"))
    } catch (e) {
      toast.error((e as Error).message)
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader className="flex-row items-center justify-between">
          <CardTitle>{t("open_windows")}</CardTitle>
          <Button variant="outline" size="sm" onClick={refresh}>
            {t("refresh")}
          </Button>
        </CardHeader>
        <CardContent>
          <ScrollArea className="h-64 rounded-md border">
            <div className="flex flex-col">
              {windows.length === 0 && (
                <p className="p-4 text-sm text-muted-foreground">{t("no_windows")}</p>
              )}
              {windows.map((win) => (
                <button
                  key={win.hwnd}
                  onClick={() => select(win)}
                  className={cn(
                    "flex flex-col items-start gap-0.5 border-b px-3 py-2 text-left last:border-b-0 hover:bg-accent",
                    selected?.hwnd === win.hwnd && "bg-accent",
                  )}
                >
                  <span className="truncate text-sm">{win.title || win.process}</span>
                  <span className="font-mono text-[11px] text-muted-foreground">
                    {win.process}
                    {saved[win.process] !== undefined && ` · ${saved[win.process]}`}
                  </span>
                </button>
              ))}
            </div>
          </ScrollArea>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("opacity")}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <p className="text-sm text-muted-foreground">
            {selected ? selected.title || selected.process : t("select_a_window")}
          </p>
          <div className="flex items-center gap-4">
            <Slider
              min={20}
              max={FULLY_OPAQUE}
              step={1}
              value={[alpha]}
              disabled={!selected}
              onValueChange={(v) => onAlphaChange(Array.isArray(v) ? v[0] : v)}
            />
            <span className="w-16 text-right font-mono text-sm">
              {Math.round((alpha / FULLY_OPAQUE) * 100)}%
            </span>
          </div>
          <div className="flex gap-2">
            <Button onClick={persist} disabled={!selected}>
              {t("save")}
            </Button>
            <Button
              variant="outline"
              disabled={!selected}
              onClick={() => onAlphaChange(FULLY_OPAQUE)}
            >
              {t("reset")}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}

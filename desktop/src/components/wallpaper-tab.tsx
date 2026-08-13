import * as React from "react"
import { open } from "@tauri-apps/plugin-dialog"

import { PreviewStage } from "@/components/preview-stage"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { engine, type Config, type Effect, type FitMode, type MonitorsResult } from "@/lib/engine"
import type { Actions } from "@/lib/use-actions"
import type { I18n } from "@/lib/use-i18n"
import { usePreview } from "@/lib/use-preview"

const COUNTS = [1, 2, 3, 4, 5, 6, 7, 8]

const FIT_MODES: { value: FitMode; labelKey: string }[] = [
  { value: "fill", labelKey: "fit_fill" },
  { value: "fit", labelKey: "fit_fit" },
  { value: "stretch", labelKey: "fit_stretch" },
  { value: "center", labelKey: "fit_center" },
  { value: "span", labelKey: "fit_span" },
]

const EFFECTS: { value: Effect; labelKey: string }[] = [
  { value: "normal", labelKey: "effect_normal" },
  { value: "bw", labelKey: "effect_bw" },
  { value: "vintage", labelKey: "effect_vintage" },
  { value: "hdr", labelKey: "effect_hdr" },
]

interface Props {
  config: Config
  monitors: MonitorsResult | null
  i18n: I18n
  actions: Actions
  onChange: <S extends keyof Config, K extends keyof Config[S]>(
    section: S,
    key: K,
    value: Config[S][K],
  ) => void
}

export function WallpaperTab({ config, monitors, i18n, actions, onChange }: Props) {
  const { t } = i18n
  const preview = usePreview(config)
  const [imageCount, setImageCount] = React.useState<number | null>(null)

  React.useEffect(() => {
    let cancelled = false
    void engine
      .listFolderImages(config.paths.wallpapers_folder)
      .then((r) => !cancelled && setImageCount(r.count))
      .catch(() => !cancelled && setImageCount(null))
    return () => {
      cancelled = true
    }
  }, [config.paths.wallpapers_folder])

  async function browseFolder() {
    const picked = await open({ directory: true, defaultPath: config.paths.wallpapers_folder })
    if (typeof picked === "string") onChange("paths", "wallpapers_folder", picked)
  }

  return (
    <div className="flex flex-col gap-4">
      <PreviewStage
        preview={preview}
        monitors={monitors}
        config={config}
        i18n={i18n}
        applying={actions.applying}
        onApply={actions.applyImages}
      />

      <Card>
        <CardHeader>
          <CardTitle>{t("images_folder")}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="flex gap-2">
            <Input
              value={config.paths.wallpapers_folder}
              onChange={(e) => onChange("paths", "wallpapers_folder", e.target.value)}
              spellCheck={false}
            />
            <Button variant="outline" onClick={browseFolder}>
              {t("browse")}
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">
            {imageCount === null ? "—" : `${imageCount} ${t("images_found")}`}
          </p>
        </CardContent>
      </Card>

      {/* Stacks until there is genuinely room for two columns — the sidebar eats
          width, so the sm breakpoint would crowd these. */}
      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>{t("layout")}</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-4">
            <Field label={t("images_per_monitor")}>
              <Select
                value={String(config.general.collage_count)}
                onValueChange={(v) => onChange("general", "collage_count", Number(v))}
              >
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  {COUNTS.map((n) => (
                    <SelectItem key={n} value={String(n)}>{n}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>

            <Field label={t("fit_mode")}>
              <Select
                value={config.display.fit_mode}
                onValueChange={(v) => v && onChange("display", "fit_mode", v as FitMode)}
              >
                <SelectTrigger>
                  {/* Base UI renders the raw value unless given a formatter. A value
                      we have no label for (older config, newer engine) shows as-is
                      rather than as an empty trigger. */}
                  <SelectValue>
                    {(v) => {
                      const key = FIT_MODES.find((m) => m.value === v)?.labelKey
                      return key ? t(key) : String(v ?? "")
                    }}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent>
                  {FIT_MODES.map((m) => (
                    <SelectItem key={m.value} value={m.value}>{t(m.labelKey)}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>

            <div className="flex items-center justify-between">
              <Label htmlFor="same-for-all" className="font-normal">
                {t("same_images_all_monitors")}
              </Label>
              <Switch
                id="same-for-all"
                checked={config.general.collage_same_for_all}
                onCheckedChange={(v) => onChange("general", "collage_same_for_all", v)}
              />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>{t("appearance")}</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-4">
            <Field label={t("effect")}>
              <Select
                value={config.display.effect}
                onValueChange={(v) => v && onChange("display", "effect", v as Effect)}
              >
                <SelectTrigger>
                  <SelectValue>
                    {(v) => {
                      const key = EFFECTS.find((e) => e.value === v)?.labelKey
                      return key ? t(key) : String(v ?? "")
                    }}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent>
                  {EFFECTS.map((e) => (
                    <SelectItem key={e.value} value={e.value}>{t(e.labelKey)}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>

            <Field label={t("selection")}>
              <Select
                value={config.general.selection}
                onValueChange={(v) =>
                  v && onChange("general", "selection", v as Config["general"]["selection"])
                }
              >
                <SelectTrigger>
                  <SelectValue>{(v) => t(String(v))}</SelectValue>
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="random">{t("random")}</SelectItem>
                  <SelectItem value="sequential">{t("sequential")}</SelectItem>
                </SelectContent>
              </Select>
            </Field>

            <Field label={t("interval_seconds")}>
              <Input
                type="number"
                min={1}
                value={config.general.interval}
                onChange={(e) =>
                  onChange("general", "interval", Math.max(1, Number(e.target.value) || 1))
                }
              />
            </Field>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label className="font-normal text-muted-foreground">{label}</Label>
      {children}
    </div>
  )
}

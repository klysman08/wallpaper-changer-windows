import * as React from "react"
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart"
import { open } from "@tauri-apps/plugin-dialog"
import { toast } from "sonner"

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
import { engine, type Config } from "@/lib/engine"
import type { I18n } from "@/lib/use-i18n"
import type { UpdateStore } from "@/lib/use-update"

interface Props {
  config: Config
  configPath: string
  i18n: I18n
  update: UpdateStore
  onChange: <S extends keyof Config, K extends keyof Config[S]>(
    section: S,
    key: K,
    value: Config[S][K],
  ) => void
}

export function SettingsTab({ config, configPath, i18n, update, onChange }: Props) {
  const { t } = i18n
  const [startup, setStartup] = React.useState<boolean | null>(null)

  React.useEffect(() => {
    void isEnabled().then(setStartup).catch(() => {})
  }, [])

  /**
   * Autostart is handled by Tauri, not the engine.
   *
   * `startup.py` registers `sys.executable`, which inside the frozen sidecar is the
   * headless engine — enabling it there would launch a background process with no
   * window. The plugin registers the actual application instead.
   */
  async function toggleStartup(next: boolean) {
    try {
      if (next) await enable()
      else await disable()
      setStartup(await isEnabled())
    } catch (e) {
      toast.error((e as Error).message)
    }
  }

  async function pickDefaultWallpaper() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "Images", extensions: ["jpg", "jpeg", "png", "bmp", "webp"] }],
    })
    if (typeof picked === "string") onChange("paths", "default_wallpaper", picked)
  }

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle>{t("general")}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label className="font-normal text-muted-foreground">{t("language")}</Label>
            <Select
              value={config.general.language}
              onValueChange={(v) => {
                // Base UI clears the value to null when the selection is dismissed.
                if (!v) return
                onChange("general", "language", v)
                i18n.setLanguage(v)
              }}
            >
              <SelectTrigger>
                {/* Show the language name, not its code. */}
                <SelectValue>{(v) => i18n.supported[String(v)] ?? String(v)}</SelectValue>
              </SelectTrigger>
              <SelectContent>
                {Object.entries(i18n.supported).map(([code, name]) => (
                  <SelectItem key={code} value={code}>{name}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("startup")}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="flex items-start justify-between gap-4">
            <div className="flex flex-col gap-0.5">
              <Label htmlFor="startup" className="font-normal">{t("start_with_windows")}</Label>
              <p className="text-xs text-muted-foreground">{t("start_with_windows_hint")}</p>
            </div>
            <Switch
              id="startup"
              checked={startup ?? false}
              disabled={startup === null}
              onCheckedChange={toggleStartup}
            />
          </div>

          <div className="flex items-start justify-between gap-4">
            <div className="flex flex-col gap-0.5">
              <Label htmlFor="start-minimized" className="font-normal">
                {t("start_minimized")}
              </Label>
              <p className="text-xs text-muted-foreground">{t("start_minimized_hint")}</p>
            </div>
            <Switch
              id="start-minimized"
              checked={config.general.start_minimized ?? false}
              onCheckedChange={(v) => onChange("general", "start_minimized", v)}
            />
          </div>

          {/* Not a setting: the engine records what was running and restores it, so
              this is here to explain the behaviour rather than to switch it. */}
          <p className="rounded-md bg-muted/50 px-3 py-2 text-xs text-muted-foreground">
            {t("resume_last_session_hint")}
          </p>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("default_wallpaper")}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="flex gap-2">
            <Input
              value={config.paths.default_wallpaper}
              onChange={(e) => onChange("paths", "default_wallpaper", e.target.value)}
              placeholder={t("no_default_wallpaper")}
              spellCheck={false}
            />
            <Button variant="outline" onClick={pickDefaultWallpaper}>
              {t("browse")}
            </Button>
          </div>
          <div>
            <Button
              variant="outline"
              size="sm"
              disabled={!config.paths.default_wallpaper}
              onClick={async () => {
                try {
                  await engine.applyDefaultWallpaper(config)
                  toast.success(t("wallpaper_applied"))
                } catch (e) {
                  toast.error((e as Error).message)
                }
              }}
            >
              {t("apply_default")}
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("storage")}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="flex flex-col gap-1">
            <Label className="font-normal text-muted-foreground">{t("config_file")}</Label>
            <p className="font-mono text-xs break-all">{configPath}</p>
          </div>
          <div className="flex flex-col gap-1.5">
            <Label className="font-normal text-muted-foreground">{t("output_folder")}</Label>
            <Input
              value={config.paths.output_folder}
              onChange={(e) => onChange("paths", "output_folder", e.target.value)}
              spellCheck={false}
            />
            <p className="text-xs text-muted-foreground">{t("output_folder_hint")}</p>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("updates")}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="flex items-center justify-between gap-4">
            <Label className="font-normal text-muted-foreground">{t("current_version")}</Label>
            <p className="font-mono text-xs">{update.currentVersion || "—"}</p>
          </div>

          <div className="flex items-start justify-between gap-4">
            <div className="flex flex-col gap-0.5">
              <Label htmlFor="check-updates" className="font-normal">
                {t("auto_check_updates")}
              </Label>
              <p className="text-xs text-muted-foreground">{t("auto_check_updates_hint")}</p>
            </div>
            <Switch
              id="check-updates"
              // Absent in configs written before this setting existed, and checking
              // is the behaviour those users were given when they upgraded.
              checked={config.general.check_updates ?? true}
              onCheckedChange={(v) => onChange("general", "check_updates", v)}
            />
          </div>

          <div className="flex items-center gap-3">
            <Button
              variant="outline"
              size="sm"
              disabled={update.status === "checking"}
              onClick={() => void update.check(true)}
            >
              {update.status === "checking" ? t("checking_for_updates") : t("check_for_updates")}
            </Button>
            {update.version && (
              <Button size="sm" onClick={() => update.setDialogOpen(true)}>
                {t("new_version_available", { version: update.version })}
              </Button>
            )}
            {/* Only after a check has actually run, so this does not claim the app is
                up to date before anything has been asked. */}
            {!update.version && update.checked && update.status !== "checking" && (
              <p className="text-xs text-muted-foreground">
                {update.status === "error" ? t("update_check_failed") : t("up_to_date")}
              </p>
            )}
          </div>

          {update.status === "error" && update.error && (
            <p className="rounded-md bg-muted/50 px-3 py-2 font-mono text-xs break-all text-muted-foreground">
              {update.error}
            </p>
          )}
        </CardContent>
      </Card>
    </div>
  )
}

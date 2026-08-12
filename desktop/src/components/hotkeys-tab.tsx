import * as React from "react"

import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import type { Config } from "@/lib/engine"
import type { I18n } from "@/lib/use-i18n"
import { cn } from "@/lib/utils"

/** Bindings in the order the old GUI listed them, with their translation keys. */
const BINDINGS: { key: string; labelKey: string }[] = [
  { key: "next_wallpaper", labelKey: "hk_next_wallpaper" },
  { key: "prev_wallpaper", labelKey: "hk_prev_wallpaper" },
  { key: "stop_watch", labelKey: "hk_stop_watch" },
  { key: "default_wallpaper", labelKey: "hk_default_wallpaper" },
  { key: "toggle_transparency", labelKey: "hk_toggle_transparency" },
  { key: "toggle_window", labelKey: "hk_toggle_window" },
  { key: "effect_normal", labelKey: "hk_effect_normal" },
  { key: "effect_bw", labelKey: "hk_effect_bw" },
  { key: "effect_vintage", labelKey: "hk_effect_vintage" },
  { key: "effect_hdr", labelKey: "hk_effect_hdr" },
  { key: "toggle_video", labelKey: "hk_toggle_video" },
  { key: "toggle_video_sound", labelKey: "hk_toggle_video_sound" },
  { key: "next_video", labelKey: "hk_next_video" },
  { key: "prev_video", labelKey: "hk_prev_video" },
]

/** Browser key names that differ from the engine's `keyboard`-library spelling. */
const KEY_ALIASES: Record<string, string> = {
  arrowright: "right",
  arrowleft: "left",
  arrowup: "up",
  arrowdown: "down",
  escape: "esc",
  " ": "space",
}

const MODIFIER_KEYS = new Set(["control", "alt", "shift", "meta"])

/** Render a KeyboardEvent as the engine's binding syntax, e.g. `ctrl+alt+right`. */
function describe(event: React.KeyboardEvent): string | null {
  const raw = event.key.toLowerCase()
  if (MODIFIER_KEYS.has(raw)) return null // still waiting for the real key

  const parts: string[] = []
  if (event.ctrlKey) parts.push("ctrl")
  if (event.altKey) parts.push("alt")
  if (event.shiftKey) parts.push("shift")
  if (event.metaKey) parts.push("windows")
  parts.push(KEY_ALIASES[raw] ?? raw)
  return parts.join("+")
}

interface Props {
  config: Config
  i18n: I18n
  onChange: (key: string, value: string) => void
}

export function HotkeysTab({ config, i18n, onChange }: Props) {
  const { t } = i18n
  const [recording, setRecording] = React.useState<string | null>(null)

  function onKeyDown(binding: string, event: React.KeyboardEvent) {
    event.preventDefault()
    if (event.key === "Escape") {
      setRecording(null)
      return
    }
    const combo = describe(event)
    if (!combo) return
    onChange(binding, combo)
    setRecording(null)
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("hotkeys")}</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        <p className="mb-2 text-sm text-muted-foreground">{t("hotkey_hint")}</p>
        {BINDINGS.map(({ key, labelKey }) => {
          const isRecording = recording === key
          return (
            <div key={key} className="flex items-center justify-between gap-4">
              {/* Several of these keys are shared with the old GUI, whose layout
                  baked a trailing colon into the label text. */}
              <Label className="font-normal">{t(labelKey).replace(/:$/, "")}</Label>
              <Button
                variant="outline"
                size="sm"
                className={cn("min-w-40 font-mono", isRecording && "ring-2 ring-ring")}
                onClick={() => setRecording(isRecording ? null : key)}
                onKeyDown={(e) => isRecording && onKeyDown(key, e)}
              >
                {isRecording ? t("press_keys") : config.hotkeys[key] || "—"}
              </Button>
            </div>
          )
        })}
      </CardContent>
    </Card>
  )
}

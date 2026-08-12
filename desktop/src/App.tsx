import * as React from "react"
import { Image, KeyRound, Layers, Settings2, Video } from "lucide-react"
import { Toaster, toast } from "sonner"

import { HotkeysTab } from "@/components/hotkeys-tab"
import { SettingsTab } from "@/components/settings-tab"
import { TransparencyTab } from "@/components/transparency-tab"
import { VideoTab } from "@/components/video-tab"
import { WallpaperTab } from "@/components/wallpaper-tab"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
  SidebarTrigger,
} from "@/components/ui/sidebar"
import { TooltipProvider } from "@/components/ui/tooltip"
import {
  engine,
  onEngineEvent,
  onHotkeyEvent,
  reloadHotkeys,
  type Capabilities,
  type MonitorsResult,
} from "@/lib/engine"
import { useConfig } from "@/lib/use-config"
import { useI18n } from "@/lib/use-i18n"

type SectionId = "wallpaper" | "video" | "transparency" | "hotkeys" | "settings"

const SECTIONS: { id: SectionId; icon: React.ComponentType<{ className?: string }> }[] = [
  { id: "wallpaper", icon: Image },
  { id: "video", icon: Video },
  { id: "transparency", icon: Layers },
  { id: "hotkeys", icon: KeyRound },
  { id: "settings", icon: Settings2 },
]

export function App() {
  const cfg = useConfig()
  const i18n = useI18n(cfg.config?.general.language)
  const [caps, setCaps] = React.useState<Capabilities | null>(null)
  const [monitors, setMonitors] = React.useState<MonitorsResult | null>(null)
  const [engineDown, setEngineDown] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  const [section, setSection] = React.useState<SectionId>("wallpaper")

  React.useEffect(() => {
    void engine.getCapabilities().then(setCaps).catch(() => setEngineDown(true))
    void engine.getMonitors().then(setMonitors).catch(() => {})

    const unlistenEngine = onEngineEvent((event) => {
      if (event.event === "stopped") setEngineDown(true)
      if (event.event === "error") toast.error(event.data.message)
    })
    // A hotkey can change state while the window is open, so reload the config
    // rather than letting the UI drift out of step with the engine.
    const unlistenHotkey = onHotkeyEvent((event) => {
      if (!event.error) void cfg.reload()
    })
    return () => {
      void unlistenEngine.then((fn) => fn())
      void unlistenHotkey.then((fn) => fn())
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const { t } = i18n

  async function save() {
    setSaving(true)
    try {
      await cfg.save()
      // Rust owns the shortcuts, so it has to re-read them for an edit to take
      // effect without a restart.
      const failed = await reloadHotkeys()
      if (failed.length > 0) {
        toast.warning(t("hotkeys_unavailable"), { description: failed.join("\n") })
      } else {
        toast.success(t("settings_saved"))
      }
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setSaving(false)
    }
  }

  if (engineDown || cfg.error) {
    return (
      <Centered>
        <div className="max-w-md rounded-lg border border-destructive/50 p-4">
          <h2 className="font-medium">{t("engine_unavailable")}</h2>
          <p className="mt-1 text-sm text-muted-foreground">{cfg.error ?? t("engine_stopped")}</p>
        </div>
      </Centered>
    )
  }

  if (cfg.loading || !cfg.config) {
    return (
      <Centered>
        <p className="text-sm text-muted-foreground">{t("loading")}</p>
      </Centered>
    )
  }

  const config = cfg.config

  return (
    // Required by the sidebar: collapsed menu items render their label in a tooltip.
    <TooltipProvider>
    <SidebarProvider>
      <Sidebar collapsible="icon">
        <SidebarHeader>
          <div className="flex items-center gap-2 px-1 py-1.5">
            <img src="/icon.png" alt="" className="size-8 shrink-0 rounded-md" />
            {/* Hidden when collapsed to icon width. */}
            <div className="grid flex-1 text-left leading-tight group-data-[collapsible=icon]:hidden">
              <span className="truncate text-sm font-medium">Wallpaper Changer</span>
              <span className="truncate text-xs text-muted-foreground">
                {monitors
                  ? `${monitors.monitors.length} ${t("monitors")} · ${monitors.virtual_width}×${monitors.virtual_height}`
                  : "—"}
              </span>
            </div>
          </div>
        </SidebarHeader>

        <SidebarContent>
          <SidebarGroup>
            <SidebarGroupContent>
              <SidebarMenu>
                {SECTIONS.map(({ id, icon: Icon }) => (
                  <SidebarMenuItem key={id}>
                    <SidebarMenuButton
                      isActive={section === id}
                      tooltip={t(id)}
                      onClick={() => setSection(id)}
                    >
                      <Icon />
                      <span>{t(id)}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </SidebarContent>

        <SidebarFooter>
          <div className="px-1 pb-1 text-[11px] text-muted-foreground group-data-[collapsible=icon]:hidden">
            {caps ? `protocol v${caps.protocol}` : "—"}
          </div>
        </SidebarFooter>
        <SidebarRail />
      </Sidebar>

      <SidebarInset>
        {/* Sticky so Save stays reachable however far the content scrolls. */}
        <header className="sticky top-0 z-10 flex h-14 shrink-0 items-center gap-2 border-b bg-background/80 px-4 backdrop-blur">
          <SidebarTrigger />
          <h1 className="flex-1 truncate text-sm font-medium">{t(section)}</h1>
          {cfg.dirty && <Badge variant="secondary">{t("unsaved_changes")}</Badge>}
          <Button size="sm" onClick={save} disabled={!cfg.dirty || saving}>
            {saving ? t("saving") : t("save")}
          </Button>
        </header>

        <div className="mx-auto w-full max-w-5xl flex-1 p-4 sm:p-6">
          {section === "wallpaper" && (
            <WallpaperTab config={config} monitors={monitors} i18n={i18n} onChange={cfg.set} />
          )}
          {section === "video" && (
            <VideoTab
              config={config}
              hasMpv={caps?.has_mpv ?? false}
              i18n={i18n}
              onChange={cfg.set}
            />
          )}
          {section === "transparency" && <TransparencyTab i18n={i18n} />}
          {section === "hotkeys" && (
            <HotkeysTab
              config={config}
              i18n={i18n}
              onChange={(key, value) =>
                cfg.set("hotkeys", key as keyof typeof config.hotkeys, value)
              }
            />
          )}
          {section === "settings" && (
            <SettingsTab
              config={config}
              configPath={cfg.configPath}
              i18n={i18n}
              onChange={cfg.set}
            />
          )}
        </div>
      </SidebarInset>
      <Toaster position="bottom-right" richColors />
    </SidebarProvider>
    </TooltipProvider>
  )
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex min-h-svh items-center justify-center p-6">
      {children}
      <Toaster position="bottom-right" richColors />
    </div>
  )
}

export default App

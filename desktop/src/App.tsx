import * as React from "react"
import { Code2, Heart, Image, KeyRound, Layers, Settings2, Video } from "lucide-react"
import { Toaster, toast } from "sonner"

import { ActionBar } from "@/components/action-bar"
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
import { useActions } from "@/lib/use-actions"
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

const REPO_URL = "https://github.com/klysman08/wallpaper-changer-windows"
const AUTHOR_URL = "https://github.com/klysman08"
const SUPPORT_URL = "https://buy.stripe.com/4gMdRa7XW6dt8Ph9KX9Ve01"

export function App() {
  const cfg = useConfig()
  const i18n = useI18n(cfg.config?.general.language)
  const [caps, setCaps] = React.useState<Capabilities | null>(null)
  const [monitors, setMonitors] = React.useState<MonitorsResult | null>(null)
  const [engineDown, setEngineDown] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  // The direction travels with the selection: which way a screen slides in depends
  // on where the sidebar moved from, which is only known at the moment of the click.
  const [screen, setScreen] = React.useState<{ id: SectionId; direction: number }>({
    id: "wallpaper",
    direction: 1,
  })
  const { id: section, direction } = screen
  // Bumped after every successful save. Screens that show engine-side state the
  // save changes (the scroll hook) re-read it when this moves.
  const [savedAt, setSavedAt] = React.useState(0)
  // One copy of the running state, shared by the footer bar and the tabs.
  const actions = useActions(cfg.config, i18n.t)

  /** Select a screen, remembering which way the selection travelled. */
  function goTo(id: SectionId) {
    setScreen((prev) => ({
      id,
      direction:
        SECTIONS.findIndex((s) => s.id === id) >= SECTIONS.findIndex((s) => s.id === prev.id)
          ? 1
          : -1,
    }))
  }

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
      setSavedAt((n) => n + 1)
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
                      onClick={() => goTo(id)}
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
          {/* Plain anchors: ExternalLinkGuard intercepts them and hands the URL to
              the system browser, so nothing ever navigates inside the webview. */}
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton
                tooltip={t("support_project")}
                className="text-rose-600 dark:text-rose-400"
                render={<a href={SUPPORT_URL} target="_blank" rel="noreferrer noopener" />}
              >
                <Heart />
                <span>{t("support_project")}</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
            <SidebarMenuItem>
              <SidebarMenuButton
                tooltip={t("source_code")}
                render={<a href={REPO_URL} target="_blank" rel="noreferrer noopener" />}
              >
                <Code2 />
                <span>{t("source_code")}</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
          <div className="px-2 pb-1 text-[11px] text-muted-foreground group-data-[collapsible=icon]:hidden">
            {t("made_by")}{" "}
            <a
              href={AUTHOR_URL}
              target="_blank"
              rel="noreferrer noopener"
              className="underline underline-offset-2 hover:text-foreground"
            >
              klysman08
            </a>
            {caps ? ` · protocol v${caps.protocol}` : ""}
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

        {/* Keyed on the section so React remounts it: that is what replays the
            entrance animation, and the direction follows the sidebar movement. */}
        <div
          key={section}
          className="screen-enter mx-auto w-full max-w-5xl flex-1 p-4 sm:p-6"
          style={{ "--motion-dir": direction } as React.CSSProperties}
        >
          {section === "wallpaper" && (
            <WallpaperTab
              config={config}
              monitors={monitors}
              i18n={i18n}
              actions={actions}
              onChange={cfg.set}
            />
          )}
          {section === "video" && (
            <VideoTab
              config={config}
              hasMpv={caps?.has_mpv ?? false}
              i18n={i18n}
              actions={actions}
              onChange={cfg.set}
            />
          )}
          {section === "transparency" && (
            <TransparencyTab
              config={config}
              i18n={i18n}
              savedAt={savedAt}
              onChange={cfg.set}
            />
          )}
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

        <ActionBar actions={actions} hasMpv={caps?.has_mpv ?? false} i18n={i18n} />
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

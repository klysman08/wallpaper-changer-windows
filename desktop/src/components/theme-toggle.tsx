/**
 * The visible way to change the theme.
 *
 * {@link ThemeProvider} has always resolved light/dark/system and kept the choice
 * in localStorage, but the only way to reach it was an undocumented "d" keypress.
 * This is the same state with a control attached.
 *
 * "System" is a real third option rather than a checkbox: it keeps following
 * Windows when the user changes it there, which a plain light/dark switch cannot.
 */
import { Monitor, Moon, Sun } from "lucide-react"

import { useTheme } from "@/components/theme-provider"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import type { I18n } from "@/lib/use-i18n"

const OPTIONS = [
  { value: "light", icon: Sun, labelKey: "theme_light" },
  { value: "dark", icon: Moon, labelKey: "theme_dark" },
  { value: "system", icon: Monitor, labelKey: "theme_system" },
] as const

export function ThemeToggle({ i18n }: { i18n: I18n }) {
  const { t } = i18n
  const { theme, setTheme } = useTheme()
  const active = OPTIONS.find((option) => option.value === theme) ?? OPTIONS[2]
  const ActiveIcon = active.icon

  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger
          render={
            <DropdownMenuTrigger
              render={
                <Button variant="ghost" size="icon-sm">
                  <ActiveIcon />
                  <span className="sr-only">{t("theme")}</span>
                </Button>
              }
            />
          }
        />
        <TooltipContent>{t("theme")}</TooltipContent>
      </Tooltip>
      {/* The content defaults to the trigger's width, which for an icon button is
          too narrow to read a label in. */}
      <DropdownMenuContent align="end" className="w-auto min-w-36">
        <DropdownMenuRadioGroup
          value={theme}
          onValueChange={(value) => setTheme(value as typeof theme)}
        >
          {OPTIONS.map(({ value, icon: Icon, labelKey }) => (
            <DropdownMenuRadioItem key={value} value={value}>
              <Icon />
              {t(labelKey)}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

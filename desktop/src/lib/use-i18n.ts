/**
 * Translations, sourced from the engine.
 *
 * `i18n.py` stays the single source of truth for all three languages — the front end
 * fetches the tables at startup rather than keeping a parallel copy that would drift.
 */
import * as React from "react"

import { engine } from "@/lib/engine"

type Tables = Record<string, Record<string, string>>

export interface I18n {
  /** Translate a key, with optional {placeholder} substitution. */
  t: (key: string, vars?: Record<string, string | number>) => string
  language: string
  setLanguage: (lang: string) => void
  /** Language code -> display name, for the picker. */
  supported: Record<string, string>
  ready: boolean
}

export function useI18n(configLanguage: string | undefined): I18n {
  const [tables, setTables] = React.useState<Tables>({})
  const [supported, setSupported] = React.useState<Record<string, string>>({})
  const [fallback, setFallback] = React.useState("en")
  const [language, setLanguage] = React.useState("en")

  React.useEffect(() => {
    void engine
      .getTranslations()
      .then((result) => {
        setTables(result.translations)
        setSupported(result.supported)
        setFallback(result.default)
        setLanguage(result.current)
      })
      .catch(() => {
        // Leave the tables empty; t() then echoes keys, which is still legible.
      })
  }, [])

  // The saved config wins once it arrives, so the picker reflects the real setting.
  React.useEffect(() => {
    if (configLanguage) setLanguage(configLanguage)
  }, [configLanguage])

  const t = React.useCallback(
    (key: string, vars?: Record<string, string | number>) => {
      const text = tables[language]?.[key] ?? tables[fallback]?.[key] ?? key
      if (!vars) return text
      return Object.entries(vars).reduce(
        (acc, [name, value]) => acc.replaceAll(`{${name}}`, String(value)),
        text,
      )
    },
    [tables, language, fallback],
  )

  return {
    t,
    language,
    setLanguage,
    supported,
    ready: Object.keys(tables).length > 0,
  }
}

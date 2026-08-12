/**
 * Config store: loads settings from the engine, tracks unsaved edits, saves back.
 *
 * Edits are held locally so the UI stays responsive and the user can change several
 * things before committing. `dirty` drives the Save button; `apply` and `preview`
 * send the *draft*, so you can try a setting before saving it.
 */
import * as React from "react"

import { engine, type Config } from "@/lib/engine"

type Section = keyof Config

interface ConfigStore {
  config: Config | null
  configPath: string
  dirty: boolean
  loading: boolean
  error: string | null
  /** Update one key within a section, marking the draft dirty. */
  set: <S extends Section, K extends keyof Config[S]>(
    section: S,
    key: K,
    value: Config[S][K],
  ) => void
  save: () => Promise<void>
  reload: () => Promise<void>
}

export function useConfig(): ConfigStore {
  const [config, setConfig] = React.useState<Config | null>(null)
  const [saved, setSaved] = React.useState<Config | null>(null)
  const [configPath, setConfigPath] = React.useState("")
  const [loading, setLoading] = React.useState(true)
  const [error, setError] = React.useState<string | null>(null)

  const reload = React.useCallback(async () => {
    setLoading(true)
    try {
      const result = await engine.getConfig()
      setConfig(result.config)
      setSaved(result.config)
      setConfigPath(result.config_path)
      setError(null)
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => {
    void reload()
  }, [reload])

  const set = React.useCallback<ConfigStore["set"]>((section, key, value) => {
    setConfig((prev) =>
      prev ? { ...prev, [section]: { ...prev[section], [key]: value } } : prev,
    )
  }, [])

  const save = React.useCallback(async () => {
    if (!config) return
    await engine.saveConfig(config)
    setSaved(config)
  }, [config])

  // Structural comparison rather than reference equality: `set` always produces a
  // new object, so identity would report dirty even after editing a value back.
  const dirty = React.useMemo(
    () => Boolean(config && saved) && JSON.stringify(config) !== JSON.stringify(saved),
    [config, saved],
  )

  return { config, configPath, dirty, loading, error, set, save, reload }
}

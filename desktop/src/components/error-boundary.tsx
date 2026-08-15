/**
 * The last line of defence between a render error and a black window.
 *
 * React unmounts the whole tree when a render throws. In a browser that leaves a
 * white page next to a console full of explanation; here it leaves the window
 * painted in the theme background with no console anywhere near it, while the tray,
 * the hotkeys and the engine all carry on working — which reads as "the app broke
 * my screen" rather than "the interface crashed". That is not a hypothetical: a
 * misused menu part shipped exactly that.
 *
 * So: catch it, say what happened, write it to the app log where a bug report can
 * reach it, and offer the one action that helps.
 *
 * Deliberately free of app dependencies. Translations arrive from the engine over
 * IPC, and a crash screen that needs the engine to render is no crash screen at all.
 */
import { Component, type ErrorInfo, type ReactNode } from "react"

interface Props {
  children: ReactNode
}

interface State {
  error: Error | null
  stack: string
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, stack: "" }

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    this.setState({ stack: info.componentStack ?? "" })
    // Best effort, and in that order: the console is there in dev, the log file is
    // what a user can actually send back.
    console.error("interface crashed", error, info.componentStack)
    void import("@tauri-apps/plugin-log")
      .then((log) => log.error(`interface crashed: ${error.stack ?? error.message}`))
      .catch(() => {})
  }

  render() {
    const { error, stack } = this.state
    if (!error) return this.props.children

    return (
      <div className="flex min-h-svh items-center justify-center p-6">
        <div className="flex max-w-xl flex-col gap-3 rounded-lg border border-destructive/50 p-5">
          <h2 className="font-medium">The interface stopped responding</h2>
          <p className="text-sm text-muted-foreground">
            Wallpaper Changer is still running in the tray — your wallpaper and hotkeys
            are unaffected. Reloading rebuilds the window.
          </p>
          <p className="rounded bg-muted px-2 py-1 font-mono text-xs break-words">
            {error.message || String(error)}
          </p>
          {stack && (
            <details className="text-xs text-muted-foreground">
              <summary className="cursor-pointer select-none">Details</summary>
              <pre className="mt-2 max-h-48 overflow-auto font-mono text-[11px] whitespace-pre-wrap">
                {stack}
              </pre>
            </details>
          )}
          <div>
            <button
              type="button"
              onClick={() => window.location.reload()}
              className="rounded-md bg-primary px-3 py-1.5 text-sm text-primary-foreground"
            >
              Reload
            </button>
          </div>
        </div>
      </div>
    )
  }
}

import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import "./index.css"
import App from "./App.tsx"
import { ThemeProvider } from "@/components/theme-provider.tsx"
import { ErrorBoundary } from "./components/error-boundary.tsx"
import { ExternalLinkGuard } from "./components/external-link-guard.tsx"
import { DebugPanel } from "./components/debug-panel.tsx"

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider>
      <ExternalLinkGuard />
      {import.meta.env.DEV ? <DebugPanel /> : null}
      {/* Inside the theme provider so the fallback is themed, but around App so a
          crash anywhere in the interface still leaves something on screen — an
          unmounted tree is an unlit window, and the app keeps running behind it. */}
      <main data-ui-scroll-container>
        <ErrorBoundary>
          <App />
        </ErrorBoundary>
      </main>
    </ThemeProvider>
  </StrictMode>
)

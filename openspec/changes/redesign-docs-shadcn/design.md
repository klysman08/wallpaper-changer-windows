## Context

The `/docs` directory hosts the static GitHub Pages website for WallpaperChanger. The desktop app in `desktop/` is a modern Tauri v2 application built with React 19, Tailwind CSS v4, and shadcn/ui. The docs website currently employs a retro neobrutalist aesthetic that diverges from the desktop app.

This design outlines the technical approach to redesign the docs website to strictly follow shadcn/ui patterns, design tokens, typography, and UI paradigms while remaining a high-performance, zero-build static website for GitHub Pages.

## Goals / Non-Goals

**Goals:**
- Implement the complete shadcn/ui design token system (`--background`, `--foreground`, `--card`, `--primary`, `--secondary`, `--muted`, `--accent`, `--border`, `--ring`, `--radius`) with seamless dark and light theme switching.
- Recreate shadcn UI component aesthetics: Cards, Badges, Buttons (primary, outline, ghost, secondary), Tabs, Tables, Kbd badges, and Code blocks.
- Build an authentic interactive desktop app mockup that mirrors the Tauri app structure from `desktop/src/App.tsx` and its tab views.
- Replace retro/pixel fonts with `Inter` and `JetBrains Mono`.
- Replace emojis with inline SVG icons styled after Lucide icons used throughout the desktop app.
- Provide fluid interactions: copy-to-clipboard feedback, tab switching, and mobile sheet/drawer navigation.

**Non-Goals:**
- Modifying the desktop application (`desktop/`) or Python backend (`src/wallpaper_changer/`).
- Introducing heavy JavaScript bundler dependencies (e.g. Webpack, Next.js) to `/docs` — keeping deployment lightweight and static for GitHub Pages.

## Decisions

### 1. Token-Driven Vanilla CSS Architecture
- **Decision**: Define the exact OKLCH/HSL color tokens from `desktop/src/index.css` inside `docs/css/style.css` under `:root` and `.dark`.
- **Rationale**: Provides 1:1 visual fidelity with the desktop application while eliminating the need for a compile step for GitHub Pages.
- **Alternative considered**: Setting up a Vite/Tailwind build pipeline for `/docs`. Rejected because standalone HTML/CSS ensures zero-overhead deployment on GitHub Pages with instant load times and no build step maintenance.

### 2. Lucide-Style SVG Iconography
- **Decision**: Replace all emoji icons with crisp, semantic SVG icons based on Lucide (e.g., `Image`, `Video`, `Layers`, `KeyRound`, `Settings2`, `Monitor`, `Check`, `X`, `Copy`, `ExternalLink`, `Moon`, `Sun`).
- **Rationale**: Matches the icon library used in `desktop/src/App.tsx` and enhances the sleek, professional feel of the site.
- **Alternative considered**: Loading an external icon font or CDN bundle. Rejected to avoid layout shifts, network requests, and external points of failure.

### 3. Desktop Application Window Mockup
- **Decision**: Reconstruct the interactive preview component to model the actual Tauri window layout:
  - Collapsible Sidebar with app icon, monitor resolution badge, navigation items, and footer links.
  - Sticky header bar with breadcrumb/section title and Save button.
  - Interactive multi-monitor stage view with labeled screen overlays (`#1`, `#2`, `#3`), action chips (`All monitors`, `Expand`, `Shuffle`, `Set as wallpaper`), and bottom action bar.
- **Rationale**: Gives potential users an immediate and realistic understanding of the application's capabilities and interface before downloading.

### 4. Theme System & Interaction Script
- **Decision**: Implement a streamlined `docs/js/main.js` that handles:
  - Light/Dark theme toggling using `.dark` class on `<html>`, synced with `localStorage` and `prefers-color-scheme`.
  - Preview tab switcher and install tab switcher with smooth transitions.
  - Clipboard copy interaction with temporary checkmark and feedback state.
  - Mobile hamburger drawer toggle.
  - IntersectionObserver for subtle scroll reveal animations.

## Risks / Trade-offs

- **[Risk] High volume of static HTML/SVG markup** → *Mitigation*: Structure `docs/index.html` with clean semantic sections and reusable CSS utility classes to keep markup readable and maintainable.
- **[Risk] Theme transition flashing on page load** → *Mitigation*: Include an inline theme initializer script in `<head>` to read `localStorage` / system theme before DOM render.

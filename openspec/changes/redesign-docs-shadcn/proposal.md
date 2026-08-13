## Why

The project's landing and documentation website located in `/docs` currently uses a retro pixel/neobrutalist aesthetic with warm paper backgrounds and pixel fonts. This heavily contrasts with the actual desktop application, which was rebuilt with Tauri v2, React 19, Tailwind CSS, and shadcn/ui featuring a sleek, modern zinc/neutral design language.

Redesigning `/docs` to follow shadcn/ui design patterns, semantic tokens, and typography creates a unified, professional brand experience and authentically showcases the modern desktop application to prospective users.

## What Changes

- **Design System & Tokens**: Replace retro paper/pixel styling with shadcn/ui-inspired CSS variables (zinc/slate color scale, semantic tokens for background, foreground, card, popover, primary, secondary, muted, accent, destructive, border, input, ring, and radius).
- **Typography & Icons**: Adopt `Inter` / `Inter Variable` as the primary font and `JetBrains Mono` for code blocks; replace emoji icons with clean SVG icons styled after Lucide icons used in the desktop app.
- **Navbar & Theme System**: Implement a sticky backdrop-blur header with shadcn-styled navigation links, GitHub button, and a dark/light theme toggle.
- **Hero & App Showcase**: Redesign the hero section with modern badges, action buttons, and an interactive desktop app window mockup that closely mirrors `desktop/src/App.tsx`.
- **Feature Cards & Comparison Table**: Overhaul the feature cards and problem/solution comparison using shadcn `Card` patterns, subtle borders, and refined hover states.
- **Interactive Preview Panels**: Redesign the preview tab switcher to match the desktop app's sidebar and stage controls (`Wallpaper`, `Video`, `Transparency`, `Hotkeys`, `Settings`, `System Tray`).
- **Install & CLI Sections**: Modernize the installation and CLI code snippets using shadcn `Tabs`, badge indicators, and copyable code blocks.
- **Hotkeys & Changelog**: Reformat keyboard shortcuts with clean `<kbd>` tags and modernize the release timeline.
- **Responsive & Accessible**: Ensure full responsiveness across mobile, tablet, and desktop screens with smooth transitions and respect for `prefers-reduced-motion`.
- **Zero-Dependency Static Delivery**: Keep the documentation website completely self-contained within `/docs` for GitHub Pages hosting.

## Capabilities

### New Capabilities
- `docs-website`: Redesign and modernize the `/docs` website to adopt shadcn/ui design tokens, component patterns, and aesthetic parity with the WallpaperChanger desktop application.

### Modified Capabilities
*(None - no existing capability requirements are modified)*

## Impact

- **Affected Directory**: `docs/` (`docs/index.html`, `docs/css/style.css`, `docs/js/main.js`, and associated assets).
- **APIs / Dependencies**: No backend or engine changes required. Static site continues to work with GitHub Pages.
- **Brand Consistency**: Unifies the visual design language between the Tauri desktop application and the web documentation.

## Purpose

Provides a modern, responsive landing page and documentation website for WallpaperChanger matching the shadcn/ui design language of the desktop application.

## ADDED Requirements

### Requirement: Modern shadcn/ui visual theme and tokens
The website SHALL adopt the shadcn/ui design system with zinc/neutral color palette, clean typography (`Inter` / `JetBrains Mono`), refined borders, and seamless dark and light theme modes.

#### Scenario: User toggles theme
- **WHEN** user clicks the theme toggle button in the navigation bar
- **THEN** the site switches immediately between light and dark themes with persistent storage in `localStorage`

#### Scenario: System prefers dark mode
- **WHEN** a user visits the site without a prior saved theme preference
- **THEN** the site respects the system's `prefers-color-scheme` setting

### Requirement: Interactive desktop app preview and tab switcher
The website SHALL render an interactive desktop app window mockup that showcases key desktop application screens (Live Preview, Collage Mode, Video Wallpaper, Settings, System Tray) mirroring the actual Tauri UI.

#### Scenario: User navigates preview tabs
- **WHEN** user selects a preview tab (e.g., Live Preview, Video Wallpaper, Settings)
- **THEN** the mock window updates smoothly to reflect that section's interface and displays an explanatory caption

### Requirement: Feature showcase and comparison matrix
The website SHALL display application features and a problem-solution comparison using shadcn `Card` patterns, subtle borders, clean badges, and SVG icons.

#### Scenario: User views feature cards
- **WHEN** user scrolls through the features section
- **THEN** feature cards display clean SVG iconography, descriptive headings, and structured benefit descriptions

### Requirement: Installation and CLI guides with code copy
The website SHALL provide tabbed installation instructions (Windows Installer vs Build from Source) and terminal command cards with one-click copy functionality.

#### Scenario: User copies code snippet
- **WHEN** user clicks the "Copy" button on any code block
- **THEN** the snippet text is copied to clipboard and the button shows visual confirmation for 2 seconds

### Requirement: Keyboard shortcuts reference and release changelog
The website SHALL present an organized hotkeys cheat sheet with styled `<kbd>` elements and a chronological timeline of version releases.

#### Scenario: User reviews keyboard shortcuts
- **WHEN** user inspects the hotkeys section
- **THEN** shortcuts are displayed in styled key badges with accompanying action labels

### Requirement: Responsive layout and accessibility
The website SHALL provide a fully responsive layout across desktop, tablet, and mobile viewports, including an accessible mobile navigation menu and respect for `prefers-reduced-motion`.

#### Scenario: Mobile viewport navigation
- **WHEN** user opens the website on a viewport narrower than 768px
- **THEN** the navigation collapses into a mobile menu toggle that opens a drawer navigation overlay

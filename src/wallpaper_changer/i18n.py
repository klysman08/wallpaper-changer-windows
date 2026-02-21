"""Internationalization (i18n) support for WallpaperChanger."""
from __future__ import annotations

SUPPORTED_LANGUAGES = {
    "en": "English",
    "pt_BR": "Português (Brasil)",
    "ja": "日本語",
}

DEFAULT_LANGUAGE = "en"

# ── Translation dictionaries ──────────────────────────────────────────────────

_TRANSLATIONS: dict[str, dict[str, str]] = {
    # ── English ───────────────────────────────────────────────────────────────
    "en": {
        # Window
        "window_title": "WallpaperChanger",
        "header_subtitle": "Control Panel  |  Windows",
        "detecting": "detecting...",

        # Monitor panel
        "monitors": "Monitors",
        "detect": "Detect",
        "no_monitor_detected": "No monitor detected",
        "monitors_count": "{n} monitor(s)",
        "monitor_singular": "monitor",
        "monitor_plural": "monitors",

        # Collage
        "collage_title": "Collage — Images per Monitor",
        "collage_same": "Same images on all monitors",

        # Selection
        "selection_title": "Image Selection",
        "sel_random": "Random",
        "sel_sequential": "Sequential",

        # Fit mode
        "fit_title": "Fit Mode",
        "fit_fill": "Fill",
        "fit_fill_desc": "Expands to cover, crops excess",
        "fit_fit": "Fit",
        "fit_fit_desc": "Fits without cropping, adds black bars",
        "fit_stretch": "Stretch",
        "fit_stretch_desc": "Distorts to fill exactly",
        "fit_center": "Center",
        "fit_center_desc": "No resize, centers on screen",
        "fit_span": "Span",
        "fit_span_desc": "Image distributed across all space",

        # Rotation
        "rotation_title": "Automatic Rotation",
        "interval_label": "Interval:",
        "seconds": "seconds",
        "start_with_windows": "Start with Windows",

        # Hotkeys
        "hotkeys_title": "Global Hotkeys",
        "hk_next": "Next wallpaper:",
        "hk_prev": "Previous wallpaper:",
        "hk_stop": "Stop/Start Watch:",
        "hk_default": "Default wallpaper:",
        "hk_record": "Record",
        "hk_recording": "Press...",
        "hk_disabled_warning": "\u26a0 'keyboard' library not installed. Hotkeys disabled.",

        # Default wallpaper
        "default_wp_title": "Default Wallpaper",
        "default_wp_desc": "Image applied by the 'Default wallpaper' hotkey.",
        "select_default_wp": "Select default wallpaper",

        # Folder
        "folder_title": "Wallpapers Folder",
        "folder_formats": "Supported formats: jpg  jpeg  png  bmp  webp",
        "folder_not_found": "Folder not found.",
        "folder_scanning": "Scanning...",
        "folder_images_found": "{n} image(s) found",
        "folder_more_images": "... and {n} more images",
        "images_found_header": "Images found",
        "select_folder": "Select wallpapers folder",

        # Actions
        "apply_now": "Apply Now",
        "applying": "Applying...",
        "save_config": "Save Config",
        "start_watch": "Start Watch",
        "stop_watch": "Stop Watch",
        "tray_btn": "Tray",

        # Status
        "ready": "Ready.",
        "wallpaper_applied": "Wallpaper applied: {name}",
        "error_prefix": "Error: {msg}",
        "no_monitor_action": "No monitor. Click Detect.",
        "config_saved": "Config saved.",
        "save_error": "Error saving: {msg}",
        "watch_active": "Watch active — changing every {n}s.",
        "watch_disabled": "Watch disabled.",
        "startup_enabled": "Auto-start enabled.",
        "startup_disabled": "Auto-start disabled.",
        "startup_error": "Error configuring auto-start: {msg}",
        "no_prev_wallpaper": "No previous wallpaper in history.",
        "prev_applied": "Previous wallpaper applied: {name}",
        "default_wp_applied": "Default wallpaper applied: {name}",
        "default_wp_not_found": "Default wallpaper not configured or file not found.",
        "no_monitor_error": "No monitor detected.",
        "hk_lib_unavailable": "'keyboard' library not available.",

        # Tray
        "tray_show": "Show",
        "tray_apply": "Apply Now",
        "tray_quit": "Quit",

        # Single instance
        "already_running": "The application is already running.",

        # Language
        "language_title": "Language",
        "language_restart_note": "Language change requires restart.",
    },

    # ── Brazilian Portuguese ──────────────────────────────────────────────────
    "pt_BR": {
        # Window
        "window_title": "WallpaperChanger",
        "header_subtitle": "Painel de controle  |  Windows",
        "detecting": "detectando...",

        # Monitor panel
        "monitors": "Monitores",
        "detect": "Detectar",
        "no_monitor_detected": "Nenhum monitor detectado",
        "monitors_count": "{n} monitor(es)",
        "monitor_singular": "monitor",
        "monitor_plural": "monitores",

        # Collage
        "collage_title": "Collage — Imagens por Monitor",
        "collage_same": "Mesmas imagens em todos os monitores",

        # Selection
        "selection_title": "Seleção de Imagens",
        "sel_random": "Aleatório",
        "sel_sequential": "Sequencial",

        # Fit mode
        "fit_title": "Ajuste na Tela",
        "fit_fill": "Preencher",
        "fit_fill_desc": "Expande para cobrir, corta o excesso",
        "fit_fit": "Ajustar",
        "fit_fit_desc": "Encaixa sem cortar, adiciona barras pretas",
        "fit_stretch": "Ampliar",
        "fit_stretch_desc": "Distorce para preencher exatamente",
        "fit_center": "Centralizar",
        "fit_center_desc": "Sem redimensionar, centraliza na tela",
        "fit_span": "Estender",
        "fit_span_desc": "Imagem distribuída por todo o espaço",

        # Rotation
        "rotation_title": "Rotação Automática",
        "interval_label": "Intervalo:",
        "seconds": "segundos",
        "start_with_windows": "Iniciar com o Windows",

        # Hotkeys
        "hotkeys_title": "Atalhos Globais",
        "hk_next": "Próximo wallpaper:",
        "hk_prev": "Wallpaper anterior:",
        "hk_stop": "Parar/Iniciar Watch:",
        "hk_default": "Wallpaper padrão:",
        "hk_record": "Gravar",
        "hk_recording": "Pressione...",
        "hk_disabled_warning": "\u26a0 Biblioteca 'keyboard' não instalada. Atalhos desativados.",

        # Default wallpaper
        "default_wp_title": "Wallpaper Padrão",
        "default_wp_desc": "Imagem aplicada pelo atalho 'Wallpaper padrão'.",
        "select_default_wp": "Selecione o wallpaper padrão",

        # Folder
        "folder_title": "Pasta de Wallpapers",
        "folder_formats": "Formatos suportados: jpg  jpeg  png  bmp  webp",
        "folder_not_found": "Pasta não encontrada.",
        "folder_scanning": "Escaneando...",
        "folder_images_found": "{n} imagem(ns) encontrada(s)",
        "folder_more_images": "... e mais {n} imagens",
        "images_found_header": "Imagens encontradas",
        "select_folder": "Selecione a pasta de wallpapers",

        # Actions
        "apply_now": "Aplicar Agora",
        "applying": "Aplicando...",
        "save_config": "Salvar Config",
        "start_watch": "Iniciar Watch",
        "stop_watch": "Parar Watch",
        "tray_btn": "Bandeja",

        # Status
        "ready": "Pronto.",
        "wallpaper_applied": "Wallpaper aplicado: {name}",
        "error_prefix": "Erro: {msg}",
        "no_monitor_action": "Nenhum monitor. Clique em Detectar.",
        "config_saved": "Configurações salvas.",
        "save_error": "Erro ao salvar: {msg}",
        "watch_active": "Watch ativo — trocando a cada {n}s.",
        "watch_disabled": "Watch desativado.",
        "startup_enabled": "Início automático ativado.",
        "startup_disabled": "Início automático desativado.",
        "startup_error": "Erro ao configurar início automático: {msg}",
        "no_prev_wallpaper": "Nenhum wallpaper anterior no histórico.",
        "prev_applied": "Wallpaper anterior aplicado: {name}",
        "default_wp_applied": "Wallpaper padrão aplicado: {name}",
        "default_wp_not_found": "Wallpaper padrão não configurado ou arquivo não encontrado.",
        "no_monitor_error": "Nenhum monitor detectado.",
        "hk_lib_unavailable": "Biblioteca 'keyboard' não disponível.",

        # Tray
        "tray_show": "Mostrar",
        "tray_apply": "Aplicar Agora",
        "tray_quit": "Sair",

        # Single instance
        "already_running": "O aplicativo já está em execução.",

        # Language
        "language_title": "Idioma",
        "language_restart_note": "Mudança de idioma requer reinicialização.",
    },

    # ── Japanese ──────────────────────────────────────────────────────────────
    "ja": {
        # Window
        "window_title": "WallpaperChanger",
        "header_subtitle": "コントロールパネル  |  Windows",
        "detecting": "検出中...",

        # Monitor panel
        "monitors": "モニター",
        "detect": "検出",
        "no_monitor_detected": "モニターが検出されませんでした",
        "monitors_count": "モニター {n} 台",
        "monitor_singular": "台",
        "monitor_plural": "台",

        # Collage
        "collage_title": "コラージュ — モニターあたりの画像数",
        "collage_same": "すべてのモニターで同じ画像を使用",

        # Selection
        "selection_title": "画像の選択",
        "sel_random": "ランダム",
        "sel_sequential": "順次",

        # Fit mode
        "fit_title": "フィットモード",
        "fit_fill": "塗りつぶし",
        "fit_fill_desc": "拡大して覆い、余分を切り取る",
        "fit_fit": "フィット",
        "fit_fit_desc": "切り取らずに収める、黒帯を追加",
        "fit_stretch": "引き伸ばし",
        "fit_stretch_desc": "歪めて正確に埋める",
        "fit_center": "中央",
        "fit_center_desc": "リサイズなし、画面中央に配置",
        "fit_span": "スパン",
        "fit_span_desc": "画像を全領域に分散配置",

        # Rotation
        "rotation_title": "自動ローテーション",
        "interval_label": "間隔：",
        "seconds": "秒",
        "start_with_windows": "Windows起動時に開始",

        # Hotkeys
        "hotkeys_title": "グローバルホットキー",
        "hk_next": "次の壁紙：",
        "hk_prev": "前の壁紙：",
        "hk_stop": "監視の停止/開始：",
        "hk_default": "デフォルト壁紙：",
        "hk_record": "記録",
        "hk_recording": "押してください...",
        "hk_disabled_warning": "\u26a0 'keyboard'ライブラリ未インストール。ホットキー無効。",

        # Default wallpaper
        "default_wp_title": "デフォルト壁紙",
        "default_wp_desc": "「デフォルト壁紙」ホットキーで適用される画像。",
        "select_default_wp": "デフォルト壁紙を選択",

        # Folder
        "folder_title": "壁紙フォルダ",
        "folder_formats": "対応形式: jpg  jpeg  png  bmp  webp",
        "folder_not_found": "フォルダが見つかりません。",
        "folder_scanning": "スキャン中...",
        "folder_images_found": "{n} 枚の画像が見つかりました",
        "folder_more_images": "... 他 {n} 枚の画像",
        "images_found_header": "見つかった画像",
        "select_folder": "壁紙フォルダを選択",

        # Actions
        "apply_now": "今すぐ適用",
        "applying": "適用中...",
        "save_config": "設定を保存",
        "start_watch": "監視を開始",
        "stop_watch": "監視を停止",
        "tray_btn": "トレイ",

        # Status
        "ready": "準備完了。",
        "wallpaper_applied": "壁紙を適用しました: {name}",
        "error_prefix": "エラー: {msg}",
        "no_monitor_action": "モニターなし。検出をクリックしてください。",
        "config_saved": "設定を保存しました。",
        "save_error": "保存エラー: {msg}",
        "watch_active": "監視中 — {n}秒ごとに変更。",
        "watch_disabled": "監視を停止しました。",
        "startup_enabled": "自動起動を有効にしました。",
        "startup_disabled": "自動起動を無効にしました。",
        "startup_error": "自動起動の設定エラー: {msg}",
        "no_prev_wallpaper": "履歴に前の壁紙がありません。",
        "prev_applied": "前の壁紙を適用しました: {name}",
        "default_wp_applied": "デフォルト壁紙を適用しました: {name}",
        "default_wp_not_found": "デフォルト壁紙が未設定またはファイルが見つかりません。",
        "no_monitor_error": "モニターが検出されませんでした。",
        "hk_lib_unavailable": "'keyboard'ライブラリが利用できません。",

        # Tray
        "tray_show": "表示",
        "tray_apply": "今すぐ適用",
        "tray_quit": "終了",

        # Single instance
        "already_running": "アプリケーションは既に実行中です。",

        # Language
        "language_title": "言語",
        "language_restart_note": "言語の変更には再起動が必要です。",
    },
}


# ── Active language state ─────────────────────────────────────────────────────

_current_lang: str = DEFAULT_LANGUAGE


def set_language(lang: str) -> None:
    """Set the active language. Falls back to English if unsupported."""
    global _current_lang
    _current_lang = lang if lang in _TRANSLATIONS else DEFAULT_LANGUAGE


def get_language() -> str:
    """Return the current language code."""
    return _current_lang


def t(key: str, **kwargs: object) -> str:
    """Translate a key using the current language.

    Supports simple {placeholder} substitution via keyword arguments.
    Falls back to English, then to the key itself.
    """
    text = _TRANSLATIONS.get(_current_lang, {}).get(key)
    if text is None:
        text = _TRANSLATIONS["en"].get(key, key)
    if kwargs:
        try:
            text = text.format(**kwargs)
        except (KeyError, IndexError):
            pass
    return text

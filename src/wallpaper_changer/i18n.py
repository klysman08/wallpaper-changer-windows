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
        "tab_wallpaper": "Wallpaper",
        "tab_video": "Video",
        "tab_tools": "Tools & Shortcuts",

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
        "effect_title": "Image Effect",
        "effect_normal": "Normal",
        "effect_bw": "Black & White",
        "effect_vintage": "Vintage",
        "effect_hdr": "HDR",

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
        "hk_transp": "Toggle transparency:",
        "hk_toggle_window": "Open/Close app window:",
        "hk_scroll_modifier": "Transparency scroll modifier:",
        "hk_effects_group": "Image Effects",
        "hk_effect_normal": "Normal:",
        "hk_effect_bw": "Black & White:",
        "hk_effect_vintage": "Vintage:",
        "hk_effect_hdr": "HDR:",
        "hk_video_group": "Video Wallpaper",
        "hk_toggle_video": "Start/Stop video:",
        "hk_toggle_video_sound": "Toggle video sound:",
        "hk_next_video": "Next video:",
        "hk_prev_video": "Previous video:",
        "video_sound_on": "Video sound on.",
        "video_sound_off": "Video sound off.",
        "video_prev": "⏮  Prev",
        "video_next": "⏭  Next",
        "hk_record": "Record",
        "hk_recording": "Press...",
        "hk_disabled_warning": "\u26a0 Native Windows hotkeys are unavailable.",

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
        "apply_already_running": "A wallpaper change is already in progress.",
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
        "hk_lib_unavailable": "Native Windows hotkeys are unavailable.",
        "hk_registration_error": "Shortcut error: {msg}",
        "notif_watch_started": "Auto-rotation started ({n}s interval).",
        "notif_watch_stopped": "Auto-rotation stopped.",
        "notif_default_set": "Default wallpaper set to {name}.",
        "notif_default_applied": "Default wallpaper applied: {name}.",

        # Tray
        "tray_show": "Show",
        "tray_apply": "Apply Now",
        "tray_quit": "Quit",

        # Single instance
        "already_running": "The application is already running.",

        # Language
        "language_title": "Language",
        "language_restart_note": "Language change requires restart.",

        # Transparency
        "transp_title": "Window Transparency",
        "transp_refresh": "Refresh",
        "transp_select": "Select a window",
        "transp_shortcut_info": "Alt+A: 50%  ·  Alt+Scroll: adjust",
        "transp_applied": "Opacity {alpha} applied",
        "transp_saved": "Transparency settings saved.",
        "transp_restored": "Restored opacity for {n} window(s).",

        # Video wallpaper
        "video_title": "Video Wallpaper",
        "video_enable": "Enable video wallpaper",
        "video_folder_label": "Video folder",
        "video_folder_formats": "Supported: mp4  mkv  avi  mov  wmv  webm  m4v",
        "video_files_found": "{n} video(s) found",
        "video_files_header": "Videos found",
        "video_select_folder": "Select video folder",
        "video_loop": "Loop",
        "video_next_on_end": "Play once",
        "video_sound": "Sound",
        "video_sound_note": "Plays the video's own audio track.",
        "video_play": "▶  Play",
        "video_stop": "■  Stop",
        "video_playing": "Playing: {name}",
        "video_stopped": "Video stopped.",
        "video_no_files": "No video files found in the selected folder.",
        "video_mpv_missing": "Video wallpaper requires python-mpv + libmpv-2.dll.",
        "video_minimize_hint": "Video playing — minimize this window to see it on the desktop.",

        # Tauri desktop UI
        "wallpaper": "Wallpaper",
        "video": "Video",
        "transparency": "Transparency",
        "hotkeys": "Hotkeys",
        "settings": "Settings",
        "general": "General",
        "preview": "Preview",
        "no_preview": "No preview available",
        "loading": "Loading...",
        "shuffle": "Shuffle",
        "images_folder": "Images folder",
        "browse": "Browse",
        "images_found": "images found",
        "images": "images",
        "layout": "Layout",
        "appearance": "Appearance",
        "images_per_monitor": "Images per monitor",
        "fit_mode": "Fit mode",
        "same_images_all_monitors": "Same images on all monitors",
        "effect": "Effect",
        "selection": "Selection",
        "random": "Random",
        "sequential": "Sequential",
        "interval_seconds": "Interval (seconds)",
        "start_rotation": "Start rotation",
        "stop_rotation": "Stop rotation",
        "apply_failed": "Could not apply the wallpaper",
        "save": "Save",
        "saving": "Saving...",
        "saved": "Saved",
        "settings_saved": "Settings saved",
        "unsaved_changes": "Unsaved changes",
        "reset": "Reset",
        "refresh": "Refresh",
        "video_wallpaper": "Video wallpaper",
        "video_folder": "Video folder",
        "videos_found": "videos found",
        "playback": "Playback",
        "play": "Play",
        "stop": "Stop",
        "next": "Next",
        "previous": "Previous",
        "playing": "Playing",
        "loop": "Loop",
        "sound": "Sound",
        "mpv_unavailable": "libmpv is not available, so video wallpaper is disabled.",
        "open_windows": "Open windows",
        "no_windows": "No windows found",
        "opacity": "Opacity",
        "select_a_window": "Select a window above",
        "hotkey_hint": "Click a shortcut, then press the new key combination. Esc cancels.",
        "hotkey_none_hint": "Nothing is bound out of the box — assign only the shortcuts you want.",
        "press_keys": "Press keys...",
        "hotkey_not_set": "Not set",
        "hotkey_clear": "Clear shortcut",
        "quick_actions": "Quick actions",
        "video_play": "Play video",
        "video_stop": "Stop video",
        "support_project": "Support this project",
        "source_code": "Source code",
        "made_by": "Made by",
        "hk_next_wallpaper": "Next wallpaper",
        "hk_prev_wallpaper": "Previous wallpaper",
        "hk_stop_watch": "Stop rotation",
        "hk_default_wallpaper": "Default wallpaper",
        "hk_toggle_transparency": "Toggle transparency",
        "language": "Language",
        "default_wallpaper": "Default wallpaper",
        "no_default_wallpaper": "No default wallpaper set",
        "apply_default": "Apply default wallpaper",
        "storage": "Storage",
        "config_file": "Configuration file",
        "output_folder": "Output folder",
        "output_folder_hint": "Relative paths are stored in your local app data folder.",
        "engine_unavailable": "Engine unavailable",
        "engine_stopped": "The wallpaper engine stopped running.",

        "hotkeys_unavailable": "Some hotkeys are already in use",

    },

    # ── Brazilian Portuguese ──────────────────────────────────────────────────
    "pt_BR": {
        # Window
        "window_title": "WallpaperChanger",
        "header_subtitle": "Painel de controle  |  Windows",
        "detecting": "detectando...",
        "tab_wallpaper": "Wallpaper",
        "tab_video": "Vídeo",
        "tab_tools": "Ferramentas e Atalhos",

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
        "effect_title": "Efeito de Imagem",
        "effect_normal": "Normal",
        "effect_bw": "Preto e Branco",
        "effect_vintage": "Vintage",
        "effect_hdr": "HDR",

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
        "hk_transp": "Alternar transparência:",
        "hk_toggle_window": "Abrir/fechar janela do app:",
        "hk_scroll_modifier": "Modificador do scroll de transparência:",
        "hk_effects_group": "Efeitos de Imagem",
        "hk_effect_normal": "Normal:",
        "hk_effect_bw": "Preto e Branco:",
        "hk_effect_vintage": "Vintage:",
        "hk_effect_hdr": "HDR:",
        "hk_video_group": "Wallpaper em Vídeo",
        "hk_toggle_video": "Iniciar/Parar vídeo:",
        "hk_toggle_video_sound": "Ativar/Desativar som do vídeo:",
        "hk_next_video": "Próximo vídeo:",
        "hk_prev_video": "Vídeo anterior:",
        "video_sound_on": "Som do vídeo ativado.",
        "video_sound_off": "Som do vídeo desativado.",
        "video_prev": "⏮  Anterior",
        "video_next": "⏭  Próximo",
        "hk_record": "Gravar",
        "hk_recording": "Pressione...",
        "hk_disabled_warning": "\u26a0 Atalhos nativos do Windows indisponíveis.",

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
        "apply_already_running": "Uma troca de wallpaper já está em andamento.",
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
        "hk_lib_unavailable": "Atalhos nativos do Windows indisponíveis.",
        "hk_registration_error": "Erro no atalho: {msg}",
        "notif_watch_started": "Rotação automática iniciada (intervalo de {n}s).",
        "notif_watch_stopped": "Rotação automática parada.",
        "notif_default_set": "Wallpaper padrão definido para {name}.",
        "notif_default_applied": "Wallpaper padrão aplicado: {name}.",

        # Tray
        "tray_show": "Mostrar",
        "tray_apply": "Aplicar Agora",
        "tray_quit": "Sair",

        # Single instance
        "already_running": "O aplicativo já está em execução.",

        # Language
        "language_title": "Idioma",
        "language_restart_note": "Mudança de idioma requer reinicialização.",

        # Transparency
        "transp_title": "Transparência de Janela",
        "transp_refresh": "Atualizar",
        "transp_select": "Selecione uma janela",
        "transp_shortcut_info": "Alt+A: 50%  ·  Alt+Scroll: ajustar",
        "transp_applied": "Opacidade {alpha} aplicada",
        "transp_saved": "Configurações de transparência salvas.",
        "transp_restored": "Restaurada opacidade de {n} janela(s).",

        # Video wallpaper
        "video_title": "Wallpaper em Vídeo",
        "video_enable": "Ativar wallpaper em vídeo",
        "video_folder_label": "Pasta de vídeos",
        "video_folder_formats": "Suportados: mp4  mkv  avi  mov  wmv  webm  m4v",
        "video_files_found": "{n} vídeo(s) encontrado(s)",
        "video_files_header": "Vídeos encontrados",
        "video_select_folder": "Selecionar pasta de vídeos",
        "video_loop": "Repetir",
        "video_next_on_end": "Reproduzir uma vez",
        "video_sound": "Som",
        "video_sound_note": "Reproduz o áudio do próprio vídeo.",
        "video_play": "▶  Reproduzir",
        "video_stop": "■  Parar",
        "video_playing": "Reproduzindo: {name}",
        "video_stopped": "Vídeo parado.",
        "video_no_files": "Nenhum arquivo de vídeo encontrado na pasta.",
        "video_mpv_missing": "Wallpaper em vídeo requer python-mpv + libmpv-2.dll.",
        "video_minimize_hint": "Vídeo em reprodução — minimize esta janela para vê-lo na área de trabalho.",

        # Tauri desktop UI
        "wallpaper": "Wallpaper",
        "video": "Vídeo",
        "transparency": "Transparência",
        "hotkeys": "Atalhos",
        "settings": "Configurações",
        "general": "Geral",
        "preview": "Pré-visualização",
        "no_preview": "Pré-visualização indisponível",
        "loading": "Carregando...",
        "shuffle": "Sortear",
        "images_folder": "Pasta de imagens",
        "browse": "Procurar",
        "images_found": "imagens encontradas",
        "images": "imagens",
        "layout": "Layout",
        "appearance": "Aparência",
        "images_per_monitor": "Imagens por monitor",
        "fit_mode": "Modo de ajuste",
        "same_images_all_monitors": "Mesmas imagens em todos os monitores",
        "effect": "Efeito",
        "selection": "Seleção",
        "random": "Aleatória",
        "sequential": "Sequencial",
        "interval_seconds": "Intervalo (segundos)",
        "start_rotation": "Iniciar rotação",
        "stop_rotation": "Parar rotação",
        "apply_failed": "Não foi possível aplicar o wallpaper",
        "save": "Salvar",
        "saving": "Salvando...",
        "saved": "Salvo",
        "settings_saved": "Configurações salvas",
        "unsaved_changes": "Alterações não salvas",
        "reset": "Redefinir",
        "refresh": "Atualizar",
        "video_wallpaper": "Wallpaper em vídeo",
        "video_folder": "Pasta de vídeos",
        "videos_found": "vídeos encontrados",
        "playback": "Reprodução",
        "play": "Reproduzir",
        "stop": "Parar",
        "next": "Próximo",
        "previous": "Anterior",
        "playing": "Reproduzindo",
        "loop": "Repetir",
        "sound": "Som",
        "mpv_unavailable": "libmpv não está disponível, então o wallpaper em vídeo está desativado.",
        "open_windows": "Janelas abertas",
        "no_windows": "Nenhuma janela encontrada",
        "opacity": "Opacidade",
        "select_a_window": "Selecione uma janela acima",
        "hotkey_hint": "Clique em um atalho e pressione a nova combinação. Esc cancela.",
        "hotkey_none_hint": "Nada vem configurado — defina apenas os atalhos que você quiser.",
        "press_keys": "Pressione as teclas...",
        "hotkey_not_set": "Não definido",
        "hotkey_clear": "Limpar atalho",
        "quick_actions": "Ações rápidas",
        "video_play": "Reproduzir vídeo",
        "video_stop": "Parar vídeo",
        "support_project": "Apoie este projeto",
        "source_code": "Código-fonte",
        "made_by": "Feito por",
        "hk_next_wallpaper": "Próximo wallpaper",
        "hk_prev_wallpaper": "Wallpaper anterior",
        "hk_stop_watch": "Parar rotação",
        "hk_default_wallpaper": "Wallpaper padrão",
        "hk_toggle_transparency": "Alternar transparência",
        "language": "Idioma",
        "default_wallpaper": "Wallpaper padrão",
        "no_default_wallpaper": "Nenhum wallpaper padrão definido",
        "apply_default": "Aplicar wallpaper padrão",
        "storage": "Armazenamento",
        "config_file": "Arquivo de configuração",
        "output_folder": "Pasta de saída",
        "output_folder_hint": "Caminhos relativos ficam na pasta de dados locais do aplicativo.",
        "engine_unavailable": "Motor indisponível",
        "engine_stopped": "O motor de wallpaper parou de funcionar.",

        "hotkeys_unavailable": "Alguns atalhos já estão em uso",

    },

    # ── Japanese ──────────────────────────────────────────────────────────────
    "ja": {
        # Window
        "window_title": "WallpaperChanger",
        "header_subtitle": "コントロールパネル  |  Windows",
        "detecting": "検出中...",
        "tab_wallpaper": "壁紙",
        "tab_video": "動画",
        "tab_tools": "ツールとショートカット",

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
        "effect_title": "画像エフェクト",
        "effect_normal": "標準",
        "effect_bw": "白黒",
        "effect_vintage": "ヴィンテージ",
        "effect_hdr": "HDR",

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
        "hk_transp": "透過度の切り替え：",
        "hk_toggle_window": "アプリウィンドウの開閉：",
        "hk_scroll_modifier": "透過スクロール修飾キー：",
        "hk_effects_group": "画像エフェクト",
        "hk_effect_normal": "標準：",
        "hk_effect_bw": "白黒：",
        "hk_effect_vintage": "ヴィンテージ：",
        "hk_effect_hdr": "HDR：",
        "hk_video_group": "動画壁紙",
        "hk_toggle_video": "動画の開始/停止：",
        "hk_toggle_video_sound": "動画の音声切り替え：",
        "hk_next_video": "次の動画：",
        "hk_prev_video": "前の動画：",
        "video_sound_on": "動画の音声をオンにしました。",
        "video_sound_off": "動画の音声をオフにしました。",
        "video_prev": "⏮  前へ",
        "video_next": "⏭  次へ",
        "hk_record": "記録",
        "hk_recording": "押してください...",
        "hk_disabled_warning": "\u26a0 Windowsのホットキーが利用できません。",

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
        "apply_already_running": "壁紙の変更はすでに実行中です。",
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
        "hk_lib_unavailable": "Windowsのホットキーが利用できません。",
        "hk_registration_error": "ショートカットエラー: {msg}",
        "notif_watch_started": "自動ローテーションを開始しました（間隔 {n}秒）。",
        "notif_watch_stopped": "自動ローテーションを停止しました。",
        "notif_default_set": "デフォルト壁紙を {name} に設定しました。",
        "notif_default_applied": "デフォルト壁紙を適用しました: {name}。",

        # Tray
        "tray_show": "表示",
        "tray_apply": "今すぐ適用",
        "tray_quit": "終了",

        # Single instance
        "already_running": "アプリケーションは既に実行中です。",

        # Language
        "language_title": "言語",
        "language_restart_note": "言語の変更には再起動が必要です。",

        # Transparency
        "transp_title": "ウィンドウの透過度",
        "transp_refresh": "更新",
        "transp_select": "ウィンドウを選択",
        "transp_shortcut_info": "Alt+A: 50%  ·  Alt+Scroll: 調整",
        "transp_applied": "透過度 {alpha} を適用",
        "transp_saved": "透過度設定を保存しました。",
        "transp_restored": "{n} 件のウィンドウの透過度を復元しました。",

        # Video wallpaper
        "video_title": "動画壁紙",
        "video_enable": "動画壁紙を有効にする",
        "video_folder_label": "動画フォルダ",
        "video_folder_formats": "対応形式: mp4  mkv  avi  mov  wmv  webm  m4v",
        "video_files_found": "{n} 本の動画が見つかりました",
        "video_files_header": "見つかった動画",
        "video_select_folder": "動画フォルダを選択",
        "video_loop": "ループ",
        "video_next_on_end": "一度だけ再生",
        "video_sound": "サウンド",
        "video_sound_note": "動画自体の音声を再生します。",
        "video_play": "▶  再生",
        "video_stop": "■  停止",
        "video_playing": "再生中: {name}",
        "video_stopped": "動画を停止しました。",
        "video_no_files": "選択したフォルダに動画ファイルが見つかりません。",
        "video_mpv_missing": "動画壁紙には python-mpv と libmpv-2.dll が必要です。",
        "video_minimize_hint": "動画再生中 — このウィンドウを最小化してデスクトップで確認してください。",

        # Tauri desktop UI
        "wallpaper": "壁紙",
        "video": "動画",
        "transparency": "透明度",
        "hotkeys": "ショートカット",
        "settings": "設定",
        "general": "全般",
        "preview": "プレビュー",
        "no_preview": "プレビューはありません",
        "loading": "読み込み中...",
        "shuffle": "シャッフル",
        "images_folder": "画像フォルダー",
        "browse": "参照",
        "images_found": "件の画像",
        "images": "枚の画像",
        "layout": "レイアウト",
        "appearance": "外観",
        "images_per_monitor": "モニターあたりの画像数",
        "fit_mode": "フィットモード",
        "same_images_all_monitors": "すべてのモニターで同じ画像",
        "effect": "エフェクト",
        "selection": "選択方法",
        "random": "ランダム",
        "sequential": "順番",
        "interval_seconds": "間隔（秒）",
        "start_rotation": "ローテーション開始",
        "stop_rotation": "ローテーション停止",
        "apply_failed": "壁紙を適用できませんでした",
        "save": "保存",
        "saving": "保存中...",
        "saved": "保存しました",
        "settings_saved": "設定を保存しました",
        "unsaved_changes": "未保存の変更",
        "reset": "リセット",
        "refresh": "更新",
        "video_wallpaper": "動画壁紙",
        "video_folder": "動画フォルダー",
        "videos_found": "件の動画",
        "playback": "再生",
        "play": "再生",
        "stop": "停止",
        "next": "次へ",
        "previous": "前へ",
        "playing": "再生中",
        "loop": "ループ",
        "sound": "音声",
        "mpv_unavailable": "libmpv が利用できないため、動画壁紙は無効です。",
        "open_windows": "開いているウィンドウ",
        "no_windows": "ウィンドウが見つかりません",
        "opacity": "不透明度",
        "select_a_window": "上のウィンドウを選択してください",
        "hotkey_hint": "ショートカットをクリックし、新しいキーの組み合わせを押します。Esc で取り消し。",
        "hotkey_none_hint": "初期状態では何も割り当てられていません。必要なショートカットだけを設定してください。",
        "press_keys": "キーを押してください...",
        "hotkey_not_set": "未設定",
        "hotkey_clear": "ショートカットを消去",
        "quick_actions": "クイック操作",
        "video_play": "動画を再生",
        "video_stop": "動画を停止",
        "support_project": "このプロジェクトを支援",
        "source_code": "ソースコード",
        "made_by": "制作",
        "hk_next_wallpaper": "次の壁紙",
        "hk_prev_wallpaper": "前の壁紙",
        "hk_stop_watch": "ローテーション停止",
        "hk_default_wallpaper": "既定の壁紙",
        "hk_toggle_transparency": "透明度の切り替え",
        "language": "言語",
        "default_wallpaper": "既定の壁紙",
        "no_default_wallpaper": "既定の壁紙は未設定です",
        "apply_default": "既定の壁紙を適用",
        "storage": "保存先",
        "config_file": "設定ファイル",
        "output_folder": "出力フォルダー",
        "output_folder_hint": "相対パスはローカルアプリデータフォルダーに保存されます。",
        "engine_unavailable": "エンジンを利用できません",
        "engine_stopped": "壁紙エンジンが停止しました。",

        "hotkeys_unavailable": "一部のショートカットは既に使用されています",

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


def get_translations() -> dict[str, dict[str, str]]:
    """Return every translation table, keyed by language code.

    Lets a non-Python front end (the Tauri UI) reuse this module as the single
    source of truth instead of maintaining a parallel copy of the strings.
    """
    return {lang: dict(table) for lang, table in _TRANSLATIONS.items()}


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

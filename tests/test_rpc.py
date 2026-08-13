"""Tests for the stdio JSON-RPC adapter.

No Win32 API is exercised here: every method that would reach ctypes is patched at
the ``wallpaper_changer.rpc`` boundary, per the repo convention.
"""

import base64
import io
import json
from pathlib import Path

import pytest
from PIL import Image

from wallpaper_changer import rpc
from wallpaper_changer.config import load_config
from wallpaper_changer.monitor import Monitor


@pytest.fixture
def engine():
    """An Engine whose emitted events are captured instead of written to stdout."""
    events: list[tuple[str, dict]] = []
    eng = rpc.Engine(lambda name, data: events.append((name, data)))
    eng.events = events  # type: ignore[attr-defined]
    return eng


@pytest.fixture
def cfg(tmp_path):
    # A real, throwaway path: the engine persists session flags (rotation, video) to
    # this file on its own, so a fictional one would be written for real.
    return {
        "_config_path": str(tmp_path / "settings.toml"),
        "general": {"selection": "random", "interval": 300, "collage_count": 4},
        "paths": {"wallpapers_folder": "C:/pics", "output_folder": "assets/output"},
        "display": {"fit_mode": "fill", "effect": "normal"},
        "video": {"enabled": False, "folder": "C:/vids", "loop": True, "sound": False},
    }


# ── Framing ───────────────────────────────────────────────────────────────────

def _run(lines: list[dict]) -> list[dict]:
    """Feed request objects through serve() and return the emitted objects."""
    stdin = io.StringIO("\n".join(json.dumps(o) for o in lines) + "\n")
    stdout = io.StringIO()
    rpc.serve(stdin, stdout)
    return [json.loads(line) for line in stdout.getvalue().splitlines() if line]


def test_serve_emits_ready_event_before_any_response():
    out = _run([{"id": 1, "method": "ping"}])

    assert out[0] == {"event": "ready", "data": {"protocol": rpc.PROTOCOL_VERSION}}


def test_serve_answers_request_with_matching_id():
    out = _run([{"id": 7, "method": "ping"}])
    response = next(o for o in out if o.get("id") == 7)

    assert response["ok"] is True
    assert response["result"]["pong"] is True


def test_serve_reports_malformed_json_without_dying():
    stdin = io.StringIO('{"id": 1, "method": "ping"}\nnot json\n{"id": 2, "method": "ping"}\n')
    stdout = io.StringIO()

    rpc.serve(stdin, stdout)
    out = [json.loads(line) for line in stdout.getvalue().splitlines() if line]

    assert any(o.get("error", {}).get("type") == "parse" for o in out)
    # the loop survived and still served the request that followed the bad line
    assert any(o.get("id") == 2 and o.get("ok") for o in out)


def test_serve_tolerates_a_utf8_bom_on_the_first_request():
    """PowerShell pipes prefix a BOM; it must not break the opening request."""
    stdin = io.StringIO('﻿{"id": 1, "method": "ping"}\n')
    stdout = io.StringIO()

    rpc.serve(stdin, stdout)
    out = [json.loads(line) for line in stdout.getvalue().splitlines() if line]

    response = next(o for o in out if o.get("id") == 1)
    assert response["ok"] is True


def test_serve_rejects_non_object_params():
    out = _run([{"id": 1, "method": "ping", "params": [1, 2]}])
    response = next(o for o in out if o.get("id") == 1)

    assert response["ok"] is False
    assert response["error"]["type"] == "bad_params"


def test_serve_stops_after_shutdown_and_ignores_later_requests():
    out = _run([{"id": 1, "method": "shutdown"}, {"id": 2, "method": "ping"}])

    assert any(o.get("id") == 1 and o.get("ok") for o in out)
    assert not any(o.get("id") == 2 for o in out)


# ── Dispatch ──────────────────────────────────────────────────────────────────

def test_dispatch_rejects_unknown_method(engine):
    with pytest.raises(rpc.RpcError) as exc:
        engine.dispatch("definitely_not_a_method", {})

    assert exc.value.kind == "unknown_method"


def test_dispatch_rejects_private_attribute_as_method(engine):
    """The allowlist must keep internals such as _merged unreachable from the wire."""
    with pytest.raises(rpc.RpcError) as exc:
        engine.dispatch("_merged", {"config": {}})

    assert exc.value.kind == "unknown_method"


def test_dispatch_turns_signature_mismatch_into_bad_params(engine):
    with pytest.raises(rpc.RpcError) as exc:
        engine.dispatch("set_window_opacity", {"nope": 1})

    assert exc.value.kind == "bad_params"


def test_error_payload_maps_file_not_found_to_not_found():
    assert rpc._error_payload(FileNotFoundError("gone"))["type"] == "not_found"


def test_error_payload_includes_traceback_for_unexpected_errors():
    payload = rpc._error_payload(RuntimeError("boom"))

    assert payload["type"] == "internal"
    assert "traceback" in payload


# ── Config ────────────────────────────────────────────────────────────────────

def test_get_config_strips_internal_keys_and_exposes_path(engine, cfg, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)

    result = engine.get_config()

    assert "_config_path" not in result["config"]
    assert result["config_path"] == cfg["_config_path"]
    assert result["config"]["display"]["effect"] == "normal"


def test_save_config_persists_and_updates_language(engine, cfg, monkeypatch):
    saved: dict = {}
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "save_config", lambda c: saved.update(c))
    monkeypatch.setattr(rpc.i18n, "set_language", lambda lang: saved.update(lang=lang))

    new = dict(cfg)
    new["general"] = {**cfg["general"], "language": "ja"}
    engine.save_config(new)

    assert saved["lang"] == "ja"
    assert saved["_config_path"] == cfg["_config_path"]


def test_merged_overlays_sections_without_dropping_siblings(engine, cfg, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)

    merged = engine._merged({"display": {"effect": "bw"}})

    assert merged["display"]["effect"] == "bw"
    assert merged["display"]["fit_mode"] == "fill"       # sibling key survived
    assert merged["paths"]["wallpapers_folder"] == "C:/pics"  # sibling section survived


def test_merged_does_not_mutate_the_saved_config(engine, cfg, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)

    engine._merged({"display": {"effect": "hdr"}})

    assert cfg["display"]["effect"] == "normal"


# ── Monitors ──────────────────────────────────────────────────────────────────

def test_get_monitors_reports_virtual_desktop_size(engine, monkeypatch):
    monkeypatch.setattr(
        rpc, "get_monitors", lambda: [Monitor(0, 0, 0, 1920, 1080), Monitor(1, 1920, 0, 1280, 1024)]
    )

    result = engine.get_monitors()

    assert len(result["monitors"]) == 2
    assert result["virtual_width"] == 3200
    assert result["virtual_height"] == 1080


def test_get_monitors_handles_no_displays(engine, monkeypatch):
    monkeypatch.setattr(rpc, "get_monitors", lambda: [])

    result = engine.get_monitors()

    assert result["monitors"] == []
    assert result["virtual_width"] == 0


# ── Preview ───────────────────────────────────────────────────────────────────

def test_preview_returns_decodable_png_and_never_applies(engine, cfg, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "get_monitors", lambda: [Monitor(0, 0, 0, 400, 200)])
    monkeypatch.setattr(
        rpc,
        "compose_collage",
        lambda *a, **k: (Image.new("RGB", (400, 200), (10, 20, 30)), ["a.png"]),
    )
    # If preview ever reached the applier the test would fail loudly.
    monkeypatch.setattr(
        rpc, "apply_wallpaper", lambda *a, **k: pytest.fail("preview must not apply")
    )

    result = engine.preview(max_width=100)

    decoded = Image.open(io.BytesIO(base64.b64decode(result["png_base64"])))
    assert decoded.format == "PNG"
    assert result["width"] == 100
    assert result["height"] == 50          # aspect ratio preserved
    assert result["images"] == ["a.png"]


def test_preview_uses_throwaway_state_file_to_protect_rotation_history(engine, cfg, monkeypatch):
    captured: dict = {}
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "get_monitors", lambda: [Monitor(0, 0, 0, 40, 20)])

    def fake_compose(cfg_, monitors, preset_images=None, state_file=None):
        captured["state_file"] = state_file
        captured["preset"] = preset_images
        return Image.new("RGB", (40, 20)), []

    monkeypatch.setattr(rpc, "compose_collage", fake_compose)

    engine.preview(images=["keep.png"])

    assert captured["state_file"] == rpc._PREVIEW_STATE
    assert captured["preset"] == ["keep.png"]   # re-render honours the pinned selection


def test_preview_without_monitors_raises_clean_error(engine, cfg, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "get_monitors", lambda: [])

    with pytest.raises(rpc.RpcError) as exc:
        engine.preview()

    assert exc.value.kind == "no_monitors"


# ── Apply ─────────────────────────────────────────────────────────────────────

def test_apply_wallpaper_emits_event_with_result(engine, cfg, tmp_path, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "get_monitors", lambda: [Monitor(0, 0, 0, 40, 20)])
    monkeypatch.setattr(rpc, "resolve_output_dir", lambda cfg_: tmp_path)
    monkeypatch.setattr(
        rpc, "apply_wallpaper", lambda *a, **k: (tmp_path / "out.bmp", ["x.png"])
    )

    result = engine.apply_wallpaper()

    assert result["images"] == ["x.png"]
    assert ("wallpaper_applied", result) in engine.events


def test_apply_wallpaper_can_replay_a_preview_selection(engine, cfg, tmp_path, monkeypatch):
    """"Set as wallpaper" in the preview must apply what is on screen, not a reshuffle."""
    seen: dict = {}
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "get_monitors", lambda: [Monitor(0, 0, 0, 40, 20)])
    monkeypatch.setattr(rpc, "resolve_output_dir", lambda cfg_: tmp_path)

    def record(cfg_, monitors, out, preset_images=None):
        seen["preset"] = preset_images
        return tmp_path / "out.bmp", list(preset_images or [])

    monkeypatch.setattr(rpc, "apply_wallpaper", record)

    result = engine.apply_wallpaper(images=["a.png", "b.png"])

    assert seen["preset"] == ["a.png", "b.png"]
    assert result["images"] == ["a.png", "b.png"]


def test_apply_wallpaper_rejects_concurrent_run(engine, cfg, tmp_path, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "get_monitors", lambda: [Monitor(0, 0, 0, 40, 20)])
    monkeypatch.setattr(rpc, "resolve_output_dir", lambda cfg_: tmp_path)
    engine._apply_lock.acquire()

    with pytest.raises(rpc.RpcError) as exc:
        engine.apply_wallpaper()

    assert exc.value.kind == "busy"


def test_apply_wallpaper_releases_lock_after_failure(engine, cfg, tmp_path, monkeypatch):
    """A failed apply must not wedge the engine into a permanent 'busy' state."""
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "get_monitors", lambda: [Monitor(0, 0, 0, 40, 20)])
    monkeypatch.setattr(rpc, "resolve_output_dir", lambda cfg_: tmp_path)

    def boom(*a, **k):
        raise RuntimeError("compose failed")

    monkeypatch.setattr(rpc, "apply_wallpaper", boom)

    with pytest.raises(RuntimeError):
        engine.apply_wallpaper()

    assert engine._apply_lock.acquire(blocking=False) is True


def test_apply_default_wallpaper_without_configured_path(engine, cfg, monkeypatch):
    cfg["paths"]["default_wallpaper"] = ""
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)

    with pytest.raises(rpc.RpcError) as exc:
        engine.apply_default_wallpaper()

    assert exc.value.kind == "not_configured"


def test_apply_default_wallpaper_with_missing_file(engine, cfg, tmp_path, monkeypatch):
    cfg["paths"]["default_wallpaper"] = str(tmp_path / "nope.png")
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)

    with pytest.raises(rpc.RpcError) as exc:
        engine.apply_default_wallpaper()

    assert exc.value.kind == "not_found"


# ── History / hotkey actions ──────────────────────────────────────────────────

@pytest.fixture
def applying_engine(engine, cfg, tmp_path, monkeypatch):
    """An engine whose apply path is stubbed, recording what it was asked to apply."""
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "get_monitors", lambda: [Monitor(0, 0, 0, 40, 20)])
    monkeypatch.setattr(rpc, "resolve_output_dir", lambda cfg_: tmp_path)

    calls: list[list[str] | None] = []
    counter = {"n": 0}

    def fake_apply(cfg_, monitors, out_dir, preset_images=None):
        calls.append(preset_images)
        if preset_images:
            return tmp_path / "out.bmp", list(preset_images)
        counter["n"] += 1
        return tmp_path / "out.bmp", [f"img{counter['n']}.png"]

    monkeypatch.setattr(rpc, "apply_wallpaper", fake_apply)
    engine.calls = calls  # type: ignore[attr-defined]
    return engine


def test_apply_records_history(applying_engine):
    applying_engine.apply_wallpaper()
    applying_engine.apply_wallpaper()

    assert applying_engine._history == [["img1.png"], ["img2.png"]]
    assert applying_engine._history_idx == 1


def test_previous_wallpaper_replays_the_earlier_selection(applying_engine):
    applying_engine.apply_wallpaper()
    applying_engine.apply_wallpaper()

    result = applying_engine.apply_previous_wallpaper()

    assert result["images"] == ["img1.png"]
    # Replayed exactly, not re-picked.
    assert applying_engine.calls[-1] == ["img1.png"]


def test_previous_wallpaper_can_step_back_repeatedly(applying_engine):
    for _ in range(3):
        applying_engine.apply_wallpaper()

    assert applying_engine.apply_previous_wallpaper()["images"] == ["img2.png"]
    assert applying_engine.apply_previous_wallpaper()["images"] == ["img1.png"]


def test_previous_wallpaper_at_the_start_of_history_errors(applying_engine):
    applying_engine.apply_wallpaper()

    with pytest.raises(rpc.RpcError) as exc:
        applying_engine.apply_previous_wallpaper()

    assert exc.value.kind == "no_history"


def test_previous_wallpaper_without_any_history_errors(applying_engine):
    with pytest.raises(rpc.RpcError) as exc:
        applying_engine.apply_previous_wallpaper()

    assert exc.value.kind == "no_history"


def test_applying_after_stepping_back_discards_the_forward_entries(applying_engine):
    """Going back then applying fresh must not leave an unreachable branch."""
    applying_engine.apply_wallpaper()
    applying_engine.apply_wallpaper()
    applying_engine.apply_previous_wallpaper()

    applying_engine.apply_wallpaper()

    assert applying_engine._history == [["img1.png"], ["img3.png"]]
    assert applying_engine._history_idx == 1


def test_history_is_bounded(applying_engine, monkeypatch):
    monkeypatch.setattr(rpc, "_HISTORY_LIMIT", 3)

    for _ in range(5):
        applying_engine.apply_wallpaper()

    assert len(applying_engine._history) == 3
    assert applying_engine._history[0] == ["img3.png"]   # oldest dropped


def test_set_effect_applies_and_updates_live_config(applying_engine):
    result = applying_engine.set_effect("bw")

    assert result["effect"] == "bw"
    assert applying_engine._config()["display"]["effect"] == "bw"


def test_set_effect_rejects_an_unknown_effect(applying_engine):
    with pytest.raises(rpc.RpcError) as exc:
        applying_engine.set_effect("sepia")

    assert exc.value.kind == "invalid"


def test_toggle_foreground_opacity_dims_then_restores(engine, monkeypatch):
    stored: dict[str, int] = {}
    applied: list[tuple[int, int]] = []
    monkeypatch.setattr(rpc.transparency, "get_foreground_window", lambda: 4242)
    monkeypatch.setattr(rpc.transparency, "_get_process_name_for_hwnd", lambda h: "app.exe")
    monkeypatch.setattr(rpc.transparency, "load_opacity_settings", lambda: dict(stored))
    monkeypatch.setattr(rpc.transparency, "save_opacity_settings", lambda s: stored.update(s))
    monkeypatch.setattr(
        rpc.transparency, "set_window_opacity", lambda h, a: applied.append((h, a))
    )

    first = engine.toggle_foreground_opacity()
    second = engine.toggle_foreground_opacity()

    assert first["alpha"] == rpc._HALF_OPACITY
    assert second["alpha"] == rpc._FULLY_OPAQUE
    assert applied == [(4242, rpc._HALF_OPACITY), (4242, rpc._FULLY_OPAQUE)]
    assert stored["app.exe"] == rpc._FULLY_OPAQUE   # persisted immediately


def test_toggle_foreground_opacity_without_focus_errors(engine, monkeypatch):
    monkeypatch.setattr(rpc.transparency, "get_foreground_window", lambda: 0)

    with pytest.raises(rpc.RpcError) as exc:
        engine.toggle_foreground_opacity()

    assert exc.value.kind == "not_found"


def test_watch_toggle_starts_then_stops(engine, cfg, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)

    assert engine.watch_toggle()["watching"] is True
    assert engine.watch_toggle()["watching"] is False


def test_video_toggle_sound_flips_the_saved_flag(engine, cfg, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "has_mpv", lambda: True)

    first = engine.video_toggle_sound()
    second = engine.video_toggle_sound()

    assert first["sound"] is True     # cfg fixture starts with sound off
    assert second["sound"] is False


def test_video_toggle_starts_when_stopped(engine, cfg, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "has_mpv", lambda: True)
    monkeypatch.setattr(rpc, "scan_video_folder", lambda folder: [])

    # Reaches video_start (which then fails on an empty folder) rather than stopping.
    with pytest.raises(rpc.RpcError) as exc:
        engine.video_toggle()

    assert exc.value.kind == "not_found"


# ── Rotation timer ────────────────────────────────────────────────────────────

def test_watch_start_and_stop_toggle_status(engine, cfg, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)

    assert engine.watch_start(interval=999)["interval"] == 999
    assert engine.watch_status()["watching"] is True

    engine.watch_stop()
    assert engine.watch_status()["watching"] is False


def test_watch_start_falls_back_to_configured_interval(engine, cfg, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)

    result = engine.watch_start()
    engine.watch_stop()

    assert result["interval"] == 300


def test_watch_start_and_stop_persist_the_rotation_flag(engine, cfg, monkeypatch):
    """The next launch has to know rotation was running, without an explicit Save."""
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)

    engine.watch_start(interval=999)
    started = load_config(Path(cfg["_config_path"]))
    engine.watch_stop()
    stopped = load_config(Path(cfg["_config_path"]))

    assert started["general"]["rotation_active"] is True
    assert stopped["general"]["rotation_active"] is False


def test_save_config_ignores_a_stale_rotation_flag_from_the_client(engine, cfg, monkeypatch):
    """A draft read before the hotkey fired must not switch rotation back off."""
    saved: dict = {}
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    engine.watch_start(interval=999)
    monkeypatch.setattr(rpc, "save_config", lambda c: saved.update(c))

    engine.save_config({**cfg, "general": {**cfg["general"], "rotation_active": False}})
    # Read before stopping: watch_stop writes through the same patched save_config.
    persisted = saved["general"]["rotation_active"]
    engine.watch_stop()

    assert persisted is True


def test_restore_session_starts_rotation_when_it_was_left_running(engine, cfg, monkeypatch):
    cfg["general"]["rotation_active"] = True
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "has_mpv", lambda: False)

    restored = engine.restore_session()
    watching = engine.watch_status()["watching"]
    engine.watch_stop()

    assert restored == {"rotation": True, "video": False}
    assert watching is True


def test_restore_session_leaves_everything_idle_when_nothing_was_running(
    engine, cfg, monkeypatch
):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "has_mpv", lambda: True)

    restored = engine.restore_session()

    assert restored == {"rotation": False, "video": False}
    assert engine.watch_status()["watching"] is False


def test_restore_session_survives_a_video_that_cannot_start(engine, cfg, monkeypatch):
    """No videos on disk must not cost the user their rotation timer."""
    cfg["general"]["rotation_active"] = True
    cfg["video"]["enabled"] = True
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "has_mpv", lambda: True)
    monkeypatch.setattr(rpc, "scan_video_folder", lambda folder: [])

    restored = engine.restore_session()
    engine.watch_stop()

    assert restored == {"rotation": True, "video": False}


def test_watch_tick_emits_error_event_when_apply_fails(engine, cfg, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)

    def boom(*a, **k):
        raise RuntimeError("no images")

    monkeypatch.setattr(engine, "apply_wallpaper", boom)
    engine._watch_tick(300)
    engine.watch_stop()

    assert any(name == "error" for name, _ in engine.events)


# ── Video ─────────────────────────────────────────────────────────────────────

def test_video_start_without_mpv_raises_clean_error(engine, monkeypatch):
    monkeypatch.setattr(rpc, "has_mpv", lambda: False)

    with pytest.raises(rpc.RpcError) as exc:
        engine.video_start()

    assert exc.value.kind == "no_mpv"


def test_video_start_without_videos_raises_not_found(engine, cfg, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "has_mpv", lambda: True)
    monkeypatch.setattr(rpc, "scan_video_folder", lambda folder: [])

    with pytest.raises(rpc.RpcError) as exc:
        engine.video_start()

    assert exc.value.kind == "not_found"


def test_shutdown_stops_video_and_cancels_watch(engine, cfg, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    stopped: list[bool] = []
    monkeypatch.setattr(engine._video, "stop", lambda: stopped.append(True))
    engine.watch_start(interval=999)

    engine.shutdown()

    assert stopped == [True]
    assert engine.watch_status()["watching"] is False


def test_shutdown_survives_video_teardown_failure(engine, monkeypatch):
    """Shutdown is the last chance to clean up; one failing step must not abort it."""
    def boom():
        raise RuntimeError("mpv already gone")

    monkeypatch.setattr(engine._video, "stop", boom)

    assert engine.shutdown()["bye"] is True


# ── i18n ──────────────────────────────────────────────────────────────────────

def test_get_translations_exposes_every_supported_language(engine):
    result = engine.get_translations()

    assert set(result["translations"]) == set(result["supported"])
    assert result["translations"]["en"]  # non-empty


def test_get_translations_returns_a_copy(engine):
    """Callers must not be able to mutate the module's translation tables."""
    engine.get_translations()["translations"]["en"]["__probe__"] = "x"

    assert "__probe__" not in rpc.i18n.get_translations()["en"]

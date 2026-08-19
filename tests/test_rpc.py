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


def _stub_two_monitors(monkeypatch, cfg):
    """Two 400x200 screens side by side, composed without touching any Win32 API."""
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(
        rpc,
        "get_monitors",
        lambda: [Monitor(0, 0, 0, 400, 200), Monitor(1, 400, 0, 400, 200)],
    )
    monkeypatch.setattr(
        rpc,
        "compose_collage",
        lambda *a, **k: (Image.new("RGB", (800, 200)), ["a.png"]),
    )


def test_preview_reports_a_cell_for_every_image_slot(engine, cfg, monkeypatch):
    """The UI lays hit targets over these, so a missing cell is an uneditable image."""
    cfg["general"]["collage_count"] = 4
    _stub_two_monitors(monkeypatch, cfg)

    cells = engine.preview(max_width=0)["cells"]

    assert len(cells) == 8
    assert sorted(c["image_index"] for c in cells) == list(range(8))
    # The second monitor's cells are offset into its half of the composite.
    assert all(c["x"] >= 400 for c in cells if c["monitor"] == 1)


def test_preview_cells_repeat_short_list_when_sharing(engine, cfg, monkeypatch):
    cfg["general"]["collage_count"] = 2
    cfg["general"]["collage_same_for_all"] = True
    _stub_two_monitors(monkeypatch, cfg)

    cells = engine.preview(max_width=0)["cells"]

    # Four cells, but only two pictures — both screens point at the same pair.
    assert len(cells) == 4
    assert sorted(c["image_index"] for c in cells) == [0, 0, 1, 1]


def test_preview_without_monitors_raises_clean_error(engine, cfg, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "get_monitors", lambda: [])

    with pytest.raises(rpc.RpcError) as exc:
        engine.preview()

    assert exc.value.kind == "no_monitors"


# ── Thumbnails ────────────────────────────────────────────────────────────────

def test_get_thumbnails_returns_decodable_jpegs_within_the_box(engine, tmp_path):
    source = tmp_path / "wide.png"
    Image.new("RGB", (800, 200), (5, 10, 15)).save(source)

    result = engine.get_thumbnails([str(source)], size=100)

    thumb = Image.open(io.BytesIO(base64.b64decode(result["thumbnails"][str(source)])))
    assert thumb.format == "JPEG"
    assert max(thumb.size) <= 100
    assert thumb.size == (100, 25)          # aspect ratio preserved


def test_get_thumbnails_skips_unreadable_files_instead_of_failing_the_batch(
    engine, tmp_path
):
    good = tmp_path / "good.png"
    Image.new("RGB", (40, 40)).save(good)
    broken = tmp_path / "broken.png"
    broken.write_text("not an image")

    result = engine.get_thumbnails([str(broken), str(good), str(tmp_path / "gone.png")])

    # One folder with one bad file must still show every other picture in it.
    assert list(result["thumbnails"]) == [str(good)]


def test_get_thumbnails_clamps_an_absurd_size(engine, tmp_path):
    source = tmp_path / "square.png"
    Image.new("RGB", (2000, 2000)).save(source)

    result = engine.get_thumbnails([str(source)], size=99999)

    thumb = Image.open(io.BytesIO(base64.b64decode(result["thumbnails"][str(source)])))
    assert max(thumb.size) <= 512


# ── Saving a collage ──────────────────────────────────────────────────────────

def test_save_collage_writes_the_whole_desktop_at_full_resolution(
    engine, cfg, tmp_path, monkeypatch
):
    """The preview PNG is sized for a window; a saved file must not inherit that."""
    _stub_two_monitors(monkeypatch, cfg)
    target = tmp_path / "saved.png"

    result = engine.save_collage(path=str(target))

    assert Image.open(target).size == (800, 200)
    assert result["collage"]["monitor"] is None
    assert result["collage"]["width"] == 800


def test_save_collage_crops_to_the_requested_monitor(engine, cfg, tmp_path, monkeypatch):
    _stub_two_monitors(monkeypatch, cfg)
    target = tmp_path / "second-screen.png"

    result = engine.save_collage(monitor=1, path=str(target))

    assert Image.open(target).size == (400, 200)
    assert result["collage"]["monitor"] == 1


def test_save_collage_lists_only_the_images_inside_the_crop(engine, cfg, monkeypatch, tmp_path):
    cfg["general"]["collage_count"] = 2
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(
        rpc,
        "get_monitors",
        lambda: [Monitor(0, 0, 0, 400, 200), Monitor(1, 400, 0, 400, 200)],
    )
    monkeypatch.setattr(
        rpc,
        "compose_collage",
        lambda *a, **k: (Image.new("RGB", (800, 200)), ["a.png", "b.png", "c.png", "d.png"]),
    )

    result = engine.save_collage(monitor=1, path=str(tmp_path / "right.png"))

    # Two per screen, in order: the right-hand screen holds the second pair.
    assert result["collage"]["images"] == ["c.png", "d.png"]


def test_save_collage_rejects_a_format_pillow_cannot_write(engine, cfg, tmp_path, monkeypatch):
    _stub_two_monitors(monkeypatch, cfg)

    with pytest.raises(rpc.RpcError) as exc:
        engine.save_collage(path=str(tmp_path / "collage.pdf"))

    assert exc.value.kind == "invalid"


def test_save_collage_rejects_a_monitor_that_is_not_there(engine, cfg, tmp_path, monkeypatch):
    _stub_two_monitors(monkeypatch, cfg)

    with pytest.raises(rpc.RpcError) as exc:
        engine.save_collage(monitor=7, path=str(tmp_path / "x.png"))

    assert exc.value.kind == "invalid"


def test_save_collage_never_touches_the_desktop_or_rotation_history(
    engine, cfg, tmp_path, monkeypatch
):
    captured: dict = {}
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    monkeypatch.setattr(rpc, "get_monitors", lambda: [Monitor(0, 0, 0, 40, 20)])
    monkeypatch.setattr(
        rpc, "apply_wallpaper", lambda *a, **k: pytest.fail("saving must not apply")
    )

    def fake_compose(cfg_, monitors, preset_images=None, state_file=None):
        captured["state_file"] = state_file
        captured["preset"] = preset_images
        return Image.new("RGB", (40, 20)), ["pinned.png"]

    monkeypatch.setattr(rpc, "compose_collage", fake_compose)

    engine.save_collage(images=["pinned.png"], path=str(tmp_path / "out.png"))

    assert captured["state_file"] == rpc._PREVIEW_STATE
    assert captured["preset"] == ["pinned.png"]   # exactly what the preview showed


def test_saved_collages_are_listed_newest_first(engine, cfg, tmp_path, monkeypatch):
    _stub_two_monitors(monkeypatch, cfg)
    engine.save_collage(path=str(tmp_path / "first.png"))
    engine.save_collage(monitor=0, path=str(tmp_path / "second.png"))

    listed = engine.list_saved_collages()["collages"]

    assert [Path(c["path"]).name for c in listed] == ["second.png", "first.png"]


def test_forget_saved_collage_keeps_the_image_file(engine, cfg, tmp_path, monkeypatch):
    _stub_two_monitors(monkeypatch, cfg)
    target = tmp_path / "kept.png"
    engine.save_collage(path=str(target))

    assert engine.forget_saved_collage(str(target))["removed"] is True
    assert engine.list_saved_collages()["collages"] == []
    assert target.exists()


def test_suggest_collage_path_creates_the_folder_the_dialog_will_open(engine):
    """A default path in a folder that does not exist is one the dialog ignores."""
    suggested = Path(engine.suggest_collage_path(monitor=0)["path"])

    assert suggested.parent.is_dir()
    assert suggested.name.endswith("_monitor1.png")


def test_suggest_collage_path_follows_an_unsaved_library_folder(engine, cfg, tmp_path, monkeypatch):
    """The dialog must open where the screen says pictures go, saved or not."""
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    chosen = tmp_path / "Pictures" / "Collages"

    result = engine.suggest_collage_path(
        monitor=None, config={"paths": {"saved_folder": str(chosen)}}
    )

    assert Path(result["path"]).parent == chosen


def test_save_collage_without_a_path_lands_in_the_configured_library(
    engine, cfg, tmp_path, monkeypatch
):
    _stub_two_monitors(monkeypatch, cfg)
    chosen = tmp_path / "my-collages"

    result = engine.save_collage(config={"paths": {"saved_folder": str(chosen)}})

    saved = Path(result["collage"]["path"])
    assert saved.parent == chosen
    assert saved.is_file()


def test_list_saved_collages_reports_the_folder_the_next_save_will_use(
    engine, cfg, tmp_path, monkeypatch
):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)
    chosen = tmp_path / "elsewhere"

    listed = engine.list_saved_collages(config={"paths": {"saved_folder": str(chosen)}})

    assert listed["folder"] == str(chosen)


def test_get_image_preview_only_ever_shrinks(engine, tmp_path):
    small = tmp_path / "small.png"
    Image.new("RGB", (120, 60)).save(small)

    result = engine.get_image_preview(str(small), max_width=1400)

    assert (result["width"], result["height"]) == (120, 60)


# ── Applying a saved collage ──────────────────────────────────────────────────

def test_apply_saved_collage_spans_a_whole_desktop_export(
    engine, cfg, tmp_path, monkeypatch
):
    _stub_two_monitors(monkeypatch, cfg)
    saved = tmp_path / "desktop.png"
    engine.save_collage(path=str(saved))
    placed: dict = {}

    def fake_span(path, *_args, **_kwargs):
        placed["spanned"] = Path(path)
        return tmp_path / "out.bmp"

    monkeypatch.setattr(rpc, "apply_desktop_image", fake_span)
    monkeypatch.setattr(
        rpc,
        "apply_single_wallpaper",
        lambda *a, **k: pytest.fail("a desktop-wide export must not be repeated per screen"),
    )

    result = engine.apply_saved_collage(str(saved))

    assert placed["spanned"] == saved
    assert result["images"] == [str(saved)]
    assert engine.events[-1][0] == "wallpaper_applied"


def test_apply_saved_collage_repeats_a_single_screen_crop(engine, cfg, tmp_path, monkeypatch):
    _stub_two_monitors(monkeypatch, cfg)
    saved = tmp_path / "one-screen.png"
    engine.save_collage(monitor=0, path=str(saved))
    placed: dict = {}

    def fake_each_screen(path, *_args, **_kwargs):
        placed["each"] = Path(path)
        return tmp_path / "out.bmp"

    monkeypatch.setattr(rpc, "apply_single_wallpaper", fake_each_screen)

    engine.apply_saved_collage(str(saved))

    assert placed["each"] == saved


def test_apply_saved_collage_does_not_pollute_the_wallpaper_history(
    engine, cfg, tmp_path, monkeypatch
):
    """History replays selections through the composer; a flat picture is not one."""
    _stub_two_monitors(monkeypatch, cfg)
    saved = tmp_path / "flat.png"
    engine.save_collage(path=str(saved))
    monkeypatch.setattr(rpc, "apply_desktop_image", lambda *a, **k: tmp_path / "out.bmp")

    engine.apply_saved_collage(str(saved))

    assert engine._history == []


def test_apply_saved_collage_reports_a_missing_file_cleanly(engine, cfg, tmp_path, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)

    with pytest.raises(rpc.RpcError) as exc:
        engine.apply_saved_collage(str(tmp_path / "never.png"))

    assert exc.value.kind == "not_found"


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


def test_watch_tick_emits_error_event_when_apply_fails(engine, cfg, monkeypatch):
    monkeypatch.setattr(rpc, "load_config", lambda: cfg)

    def boom(*a, **k):
        raise RuntimeError("no images")

    monkeypatch.setattr(engine, "apply_wallpaper", boom)
    engine._watch_tick(300)
    engine.watch_stop()

    assert any(name == "error" for name, _ in engine.events)


# ── Video ─────────────────────────────────────────────────────────────────────


# ── i18n ──────────────────────────────────────────────────────────────────────

def test_get_translations_exposes_every_supported_language(engine):
    result = engine.get_translations()

    assert set(result["translations"]) == set(result["supported"])
    assert result["translations"]["en"]  # non-empty


def test_get_translations_returns_a_copy(engine):
    """Callers must not be able to mutate the module's translation tables."""
    engine.get_translations()["translations"]["en"]["__probe__"] = "x"

    assert "__probe__" not in rpc.i18n.get_translations()["en"]

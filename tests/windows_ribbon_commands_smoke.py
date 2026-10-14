#!/usr/bin/env python3
"""Verify shared Ribbon command state through the gallery's own Win32 window.

Uses opt-in application diagnostics to observe command state, semantic placement
bounds and invocation counts. Input still follows the production pointer/KeyTip
paths. Captures and cursor restoration use the existing gallery smoke helpers.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import sys

from windows_gallery_smoke import GallerySession, GalleryWindows, latest_command_state, visible_control_center
from windows_retained_smoke import WM_MOUSEMOVE, WM_KEYDOWN, WM_KEYUP, key_message
from windows_smoke import SmokeFailure, WM_CLOSE


def latest_snapshot(path: Path) -> dict | None:
    return latest_command_state(path.read_text(encoding="utf-8", errors="replace"))


def background_color(state: dict, placement: dict) -> tuple[int, int, int]:
    dark = state["theme"] == "dark"
    if state["checked"]:
        return (37, 72, 109) if dark else (199, 222, 246)
    # Flat QAT commands expose the canvas; group commands expose the common
    # Ribbon surface. Determine the QAT row from observed command positions.
    qat_y = min(item["bounds"][1] for item in state["controls"] if item["command"] is not None)
    if abs(placement["bounds"][1] - qat_y) < 0.001:
        return (25, 29, 37) if dark else (240, 244, 249)
    return (40, 46, 57) if dark else (255, 255, 255)


def verify_shared(state: dict, sequence: int, checked: bool) -> None:
    if state["sequence"] != sequence or state["checked"] is not checked:
        raise SmokeFailure(f"Unexpected command invocation/state: {state}")
    for placement in state["placements"]:
        if placement["checked"] is not checked:
            raise SmokeFailure(f"Placement disagrees with shared command: {placement}")
        if placement["role"] not in ("ToggleButton", "MenuItem"):
            raise SmokeFailure(f"Incorrect command placement role: {placement}")


def group_stage(state: dict, group: str) -> str:
    """Classify the measured model group, never a coincidentally equal caption."""
    matches = [entry["kind"] for entry in state["ribbon"] if group in entry["groups"]]
    if len(matches) != 1 or matches[0] not in ("full", "collapsed", "overflow"):
        raise SmokeFailure(f"Invalid measured stage for {group}: {state['ribbon']}")
    return matches[0]


def discover_group_layout(sample, group: str = "editing") -> dict:
    """Bound the search and resolve the final full-to-overflow interval at 1px.

    Adaptive icon columns may already be narrower than a collapsed header.
    In that case report overflow as the reachable menu placement, explicitly
    recording that a distinct collapsed stage was not observed.
    """
    samples = []

    def stage_at(width):
        stage = group_stage(sample(width), group)
        samples.append({"width": width, "stage": stage})
        return stage

    previous_full = None
    for width in [*range(1200, 63, -32), 64]:
        stage = stage_at(width)
        if previous_full is None and stage != "full":
            raise SmokeFailure(f"Expected expanded {group} at 1200px, got {stage}")
        if stage == "full":
            previous_full = width
            continue
        if stage == "collapsed":
            return {"group": group, "kind": stage, "width": width, "collapsed_observed": True,
                    "samples": samples}
        # The planner only collapses after all profitable item reductions.
        # A short collapsed interval can sit between these two coarse samples.
        for boundary in range(previous_full - 1, width, -1):
            refined = stage_at(boundary)
            if refined == "collapsed":
                return {"group": group, "kind": refined, "width": boundary, "collapsed_observed": True,
                        "samples": samples}
        return {"group": group, "kind": "overflow", "width": width, "collapsed_observed": False,
                "reason": "No separate collapsed group between expanded and overflow, including 1px boundary refinement",
                "samples": samples}
    raise SmokeFailure(f"Neither collapsed nor overflow {group} found in bounded widths 64..1200")


class CommandSession(GallerySession):
    def snapshot(self, description: str, predicate=lambda _: True) -> dict:
        def matches():
            state = latest_snapshot(self.output / "stdout.log")
            return state if state is not None and predicate(state) else None
        state = self.wait(description, matches)
        self.report["checks"].append({"check": description, "command_state": state})
        return state

    def state(self) -> dict:
        state = latest_snapshot(self.output / "stdout.log")
        if state is None:
            raise SmokeFailure("Missing command diagnostics")
        return state

    def activate_placement(self, placement: dict, description: str) -> dict:
        before = self.state()
        self.click_item(placement)
        state = self.snapshot(description, lambda s: s["sequence"] > before["sequence"])
        self.settle()
        # Re-read after idle so a duplicate invocation cannot pass unnoticed.
        state = self.state()
        verify_shared(state, before["sequence"] + 1, not before["checked"])
        self.report["checks"].append({"check": description + " exactly once, all placements agree",
                                      "command_state": state})
        return state

    def click_item(self, item: dict) -> None:
        viewport = tuple(value / self.scale for value in self.windows.client_size(self.hwnd))
        self.click(*visible_control_center(item, viewport), self.scale)

    def capture(self, output: Path, name: str) -> dict:
        width, height = self.windows.client_size(self.hwnd)
        self.pointer(WM_MOUSEMOVE, round(width / self.scale) - 5,
                     round(height / self.scale) - 5, self.scale)
        result = super().capture(output, name)
        bitmap = Path(result["bmp"]).read_bytes()
        offset = struct.unpack_from("<I", bitmap, 10)[0]
        state = self.state()
        def encoded(color):
            return tuple(round(255 * (12.92 * (v / 255) if v / 255 <= 0.0031308
                         else 1.055 * (v / 255) ** (1 / 2.4) - 0.055)) for v in color)
        samples = []
        for placement in state["placements"]:
            if placement["role"] != "ToggleButton":
                continue
            expected = background_color(state, placement)
            x, y, _, _ = placement["bounds"]
            px, py = round((x + 5) * self.scale), round((y + 5) * self.scale)
            cx, cy, cw, ch = placement["clip"]
            if not (0 <= px < width and 0 <= py < height and
                    cx <= x + 5 < cx + cw and cy <= y + 5 < cy + ch):
                raise SmokeFailure(f"Checked-background sample is clipped: {placement}")
            index = offset + ((height - 1 - py) * width + px) * 4
            b, g, r = bitmap[index:index + 3]
            actual = (r, g, b)
            if not any(all(abs(a - e) <= 5 for a, e in zip(actual, candidate, strict=True))
                       for candidate in (expected, encoded(expected))):
                raise SmokeFailure(f"Checked background differs at {placement['label']}: {actual}, expected {expected}")
            samples.append({"label": placement["label"], "point": [px, py], "rgb": actual})
        result["checked_backgrounds"] = samples
        return result

    def control(self, predicate) -> dict:
        matches = [item for item in self.state()["controls"] if predicate(item)]
        if len(matches) != 1:
            raise SmokeFailure(f"Expected exactly one control, found {matches}")
        return matches[0]

    def resize(self, width: int) -> None:
        target = (round(width * self.scale), round(680 * self.scale))
        self.windows.resize_client(self.hwnd, *target)
        self.wait("native client resize", lambda: self.windows.client_size(self.hwnd) == target)
        self.snapshot(f"layout at {width}px", lambda s: abs(s["viewport"][0] - width) < 1)
        self.settle()

    def resize_until(self, description: str, widths: tuple[int, ...], predicate) -> None:
        for width in widths:
            self.resize(width)
            if predicate(self.state()):
                self.report["checks"].append({"check": description, "logical_width": width})
                return
        raise SmokeFailure(f"No {description} at tested widths {widths}")

    def editing_menu_layout(self) -> dict:
        initial = self.state()

        def sample(width):
            self.resize(width)
            state = self.state()
            verify_shared(state, initial["sequence"], initial["checked"])
            if state["popup_open"]:
                raise SmokeFailure("Resize unexpectedly opened a command menu")
            return state

        result = discover_group_layout(sample)
        # Boundary refinement may finish at another width than its result.
        self.resize(result["width"])
        if group_stage(self.state(), "editing") != result["kind"]:
            raise SmokeFailure("Measured group plan changed when restoring the discovered width")
        result["theme"] = self.state()["theme"]
        self.report.setdefault("adaptive_layout", []).append(result)
        return result

    def capture_discovered_layout(self, output: Path, name: str, layout: dict) -> None:
        if layout["width"] >= 300:
            self.capture(output, f"{name}-{layout['kind']}")
        else:
            self.report.setdefault("capture_limits", []).append({
                "name": name, "kind": layout["kind"], "width": layout["width"],
                "reason": "Existing gallery capture validation requires a width of at least 300px; command assertions still run",
            })

    def menu_favorite(self, anchor: dict, description: str, required_commands=()) -> None:
        before = self.state()
        self.click_item(anchor)
        state = self.snapshot(description + " opens", lambda s: s["popup_open"])
        verify_shared(state, before["sequence"], before["checked"])
        commands = {item["command"] for item in state["items"]}
        missing = set(required_commands) - commands
        if missing:
            raise SmokeFailure(f"{description} loses group commands: {sorted(missing)}")
        matches = [p for p in state["placements"] if p["role"] == "MenuItem"]
        if len(matches) != 1:
            raise SmokeFailure(f"Expected one favorite menu item: {matches}")
        verify_shared(state, state["sequence"], state["checked"])
        self.activate_placement(matches[0], description + " activation")
        self.snapshot(description + " closes on success", lambda s: not s["popup_open"])

    def run(self, output: Path) -> None:
        self.output = output
        self.hwnd = self.wait("visible gallery child", lambda: self.windows.find_window(self.process.pid))
        self.report["window_handle"] = self.hwnd
        self.wait_state("first GPU frame", lambda s: s["frames"] >= 1 and s["glyphs"] > 100)
        dpi = self.windows.user.GetDpiForWindow(self.hwnd)
        if dpi <= 0:
            raise SmokeFailure("GetDpiForWindow returned zero")
        self.scale = dpi / 96.0
        self.report["dpi"] = dpi
        self.windows.topmost_without_activation(self.hwnd, True)
        self.promoted = True
        self.focus_messages(True)
        width, height = self.windows.client_size(self.hwnd)
        self.pointer(WM_MOUSEMOVE, round(width / self.scale) - 5, round(height / self.scale) - 5, self.scale)
        initial = self.snapshot("initial shared command", lambda s: len(s["placements"]) >= 2)
        verify_shared(initial, 0, False)
        self.capture(output, "initial-light")

        body = max(initial["placements"], key=lambda p: p["bounds"][1])
        self.activate_placement(body, "Ribbon body toggles command")
        self.capture(output, "body-checked-light")
        qat = min(self.state()["placements"], key=lambda p: p["bounds"][1])
        self.activate_placement(qat, "QAT toggles same command")
        self.capture(output, "qat-unchecked-light")

        before = self.state()
        self.key(0x79)  # F10 opens KeyTips, independent of keyboard layout.
        self.snapshot("QAT KeyTips displayed", lambda s:
                      any(p["key_tip"] == "3" and p["label"] == "Избранное" for p in s["placements"]))
        # Let the child's TranslateMessage enqueue WM_CHAR before KEYUP. Winit
        # assembles printable logical keys by looking ahead in that queue; an
        # already queued KEYUP would end the key as Unidentified before CHAR.
        scan = self.windows.user.MapVirtualKeyW(0x33, 4)
        self.windows.post(self.hwnd, WM_KEYDOWN, 0x33, key_message(scan, False))
        try:
            self.snapshot("QAT KeyTip invokes command", lambda s: s["sequence"] > before["sequence"])
        finally:
            self.windows.post(self.hwnd, WM_KEYUP, 0x33, key_message(scan, True))
        self.settle()
        verify_shared(self.state(), before["sequence"] + 1, not before["checked"])
        self.report["checks"].append({"check": "QAT KeyTip exactly once", "command_state": self.state()})

        self.resize_until("QAT overflow", (96, 112, 128), lambda state:
                          any(item["label"] == "More" for item in state["controls"]))
        self.menu_favorite(self.control(lambda p: p["label"] == "More"),
                           "QAT More")
        editing_layout = self.editing_menu_layout()
        editing_anchor = "Редактирование" if editing_layout["kind"] == "collapsed" else "All commands"
        self.menu_favorite(self.control(lambda p: p["label"] == editing_anchor),
                           "Editing commands in " + editing_layout["kind"], ("edit", "copy", "favorite"))
        self.capture_discovered_layout(output, "editing-light", editing_layout)

        self.resize_until("All commands overflow", tuple(range(256, 63, -16)), lambda state:
                          any(item["label"] == "All commands" for item in state["controls"]))
        self.menu_favorite(self.control(lambda p: p["label"] == "All commands"), "All commands overflow")
        # Existing gallery capture checker intentionally requires >=300px width.
        self.resize(1200)
        self.capture(output, "restored-light")
        self.click_item(self.control(lambda p: p["label"] == "Вид" and p["role"] == "Tab"))
        self.snapshot("View tab", lambda s: any(p["label"] == "Тёмная" for p in s["controls"]))
        self.click_item(self.control(lambda p: p["label"] == "Тёмная"))
        self.wait_state("Dark theme", lambda s: s["theme"] == "Тёмная")
        self.snapshot("Dark diagnostics", lambda s: s["theme"] == "dark")
        self.click_item(self.control(lambda p: p["label"] == "Главная" and p["role"] == "Tab"))
        self.snapshot("Home tab after theme rebuild", lambda s:
                      len(s["placements"]) >= 2)
        verify_shared(self.state(), 6, False)
        qat = min(self.state()["placements"], key=lambda p: p["bounds"][1])
        self.activate_placement(qat, "Dark QAT after rebuild")
        self.capture(output, "checked-dark")
        self.resize_until("Dark QAT overflow", (96, 112, 128), lambda state:
                          any(item["label"] == "More" for item in state["controls"]))
        self.menu_favorite(self.control(lambda p: p["label"] == "More"),
                           "Dark QAT More")
        editing_layout = self.editing_menu_layout()
        self.capture_discovered_layout(output, "editing-dark", editing_layout)
        final = self.wait_state("no missing glyphs", lambda s: s["missing"] == 0)
        self.report["final_render_state"] = final
        verify_shared(self.state(), 8, False)
        self.windows.post(self.hwnd, WM_CLOSE)
        code = self.process.wait(timeout=self.timeout)
        if code != 0:
            raise SmokeFailure(f"Gallery exited with status {code}")
        self.report["checks"].append({"check": "native close", "exit_code": code})


def self_test() -> None:
    qat = {"command": "favorite", "bounds": [20, 500, 32, 32]}
    body = {"command": "favorite", "bounds": [20, 600, 84, 88]}
    state = {"theme": "light", "checked": False, "controls": [body, qat]}
    assert background_color(state, qat) == (240, 244, 249)
    assert background_color(state, body) == (255, 255, 255)
    state["checked"] = True
    assert background_color(state, qat) == background_color(state, body) == (199, 222, 246)
    state.update(theme="dark", checked=False)
    assert background_color(state, qat) == (25, 29, 37)
    assert background_color(state, body) == (40, 46, 57)
    state["checked"] = True
    assert background_color(state, qat) == background_color(state, body) == (37, 72, 109)

    def fixture(width, *, collapsed):
        kind = "full" if width > 302 else "collapsed" if collapsed and width >= 300 else "overflow"
        return {"ribbon": [{"kind": kind, "groups": ["file", "editing", "tree"]}]}

    narrow = discover_group_layout(lambda width: fixture(width, collapsed=True))
    assert narrow["kind"] == "collapsed" and narrow["width"] == 302
    assert narrow["collapsed_observed"]
    direct = discover_group_layout(lambda width: fixture(width, collapsed=False))
    assert direct["kind"] == "overflow" and not direct["collapsed_observed"]
    assert len(direct["samples"]) <= 70
    for invalid in ([], [{"kind": "tiny", "groups": ["editing"]}],
                    [{"kind": "full", "groups": ["editing"]}, {"kind": "overflow", "groups": ["editing"]}]):
        try:
            group_stage({"ribbon": invalid}, "editing")
        except SmokeFailure:
            pass
        else:
            raise AssertionError("Missing or ambiguous measured group stage accepted")
    print("PASS: Ribbon command palettes and bounded adaptive discovery, including narrow or absent collapsed stages")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--timeout", type=float, default=20)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if not args.exe or not args.output_dir:
        parser.error("--exe and --output-dir are required")
    if sys.platform != "win32" or not args.exe.is_file():
        parser.error("An interactive Windows desktop and gallery executable are required")
    if not math.isfinite(args.timeout) or not 0 < args.timeout <= 60:
        parser.error("--timeout must be in (0, 60]")
    executable, output = args.exe.resolve(), args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    report = {"status": "failed", "started_utc": datetime.now(timezone.utc).isoformat(),
              "executable": str(executable), "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
              "checks": [], "captures": {}, "backend_environment": os.environ.get("WGPU_BACKEND"),
              "limits": ["No native IME or screen reader test", "Letter KeyTips covered by Rust tests",
                         "Narrow overflow verified by state/input; captures require at least 300px width"]}
    session = None
    try:
        windows = GalleryWindows()
        cursor = windows.cursor_position()
        with (output / "stdout.log").open("wb") as stdout, (output / "stderr.log").open("wb") as stderr:
            process = subprocess.Popen([str(executable), "--diagnostics", "--allow-small-window", "--rows", "100000",
                                        "--state-file", str(output / "columns.txt")],
                                       stdout=stdout, stderr=stderr, creationflags=subprocess.CREATE_NO_WINDOW)
            report["process_id"] = process.pid
            session = CommandSession(windows, process, report, args.timeout, cursor)
            try:
                session.run(output)
                report["status"] = "passed"
            finally:
                session.cleanup()
    except (SmokeFailure, OSError, subprocess.SubprocessError, KeyError, StopIteration) as error:
        report["error"] = str(error)
    finally:
        report["finished_utc"] = datetime.now(timezone.utc).isoformat()
        if session is not None:
            report["exit_code"] = session.process.poll()
        destination = output / "report.json"
        destination.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"{report['status'].upper()}: {destination}")
    if "error" in report:
        print(report["error"], file=sys.stderr)
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

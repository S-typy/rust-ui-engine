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

from windows_gallery_smoke import GallerySession, GalleryWindows
from windows_retained_smoke import WM_MOUSEMOVE, WM_KEYDOWN, WM_KEYUP, key_message
from windows_smoke import SmokeFailure, WM_CLOSE


def latest_snapshot(path: Path) -> dict | None:
    # An incomplete final stdout write is allowed while the application runs.
    snapshots = []
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines(keepends=True):
        if line.startswith("command-state "):
            if not line.endswith("\n"):
                continue
            try:
                snapshots.append(json.loads(line.removeprefix("command-state ")))
            except json.JSONDecodeError as error:
                raise SmokeFailure(f"Malformed complete command snapshot: {error}") from error
    return snapshots[-1] if snapshots else None


def verify_shared(state: dict, sequence: int, checked: bool) -> None:
    if state["sequence"] != sequence or state["checked"] is not checked:
        raise SmokeFailure(f"Unexpected command invocation/state: {state}")
    for placement in state["placements"]:
        if placement["checked"] is not checked:
            raise SmokeFailure(f"Placement disagrees with shared command: {placement}")
        if placement["role"] not in ("ToggleButton", "MenuItem"):
            raise SmokeFailure(f"Incorrect command placement role: {placement}")


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
        if not item["enabled"]:
            raise SmokeFailure(f"Target is disabled: {item}")
        x, y, width, height = item["bounds"]
        if item.get("clip"):
            cx, cy, cw, ch = item["clip"]
            right, bottom = min(x + width, cx + cw), min(y + height, cy + ch)
            x, y = max(x, cx), max(y, cy)
            width, height = right - x, bottom - y
        if width <= 0 or height <= 0:
            raise SmokeFailure(f"Target has no visible hit area: {item}")
        self.click(round(x + width / 2), round(y + height / 2), self.scale)

    def capture(self, output: Path, name: str) -> dict:
        width, height = self.windows.client_size(self.hwnd)
        self.pointer(WM_MOUSEMOVE, round(width / self.scale) - 5,
                     round(height / self.scale) - 5, self.scale)
        result = super().capture(output, name)
        bitmap = Path(result["bmp"]).read_bytes()
        offset = struct.unpack_from("<I", bitmap, 10)[0]
        state = self.state()
        dark = state["theme"] == "dark"
        expected = ((37, 72, 109) if dark else (199, 222, 246)) if state["checked"] else (
            (40, 46, 57) if dark else (255, 255, 255))
        def encoded(color):
            return tuple(round(255 * (12.92 * (v / 255) if v / 255 <= 0.0031308
                         else 1.055 * (v / 255) ** (1 / 2.4) - 0.055)) for v in color)
        samples = []
        for placement in state["placements"]:
            if placement["role"] != "ToggleButton":
                continue
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

    def menu_favorite(self, anchor: dict, description: str) -> None:
        self.click_item(anchor)
        state = self.snapshot(description + " opens", lambda s: s["popup_open"])
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
        self.pointer(WM_MOUSEMOVE, 1100, 700, self.scale)
        initial = self.snapshot("initial shared command", lambda s: len(s["placements"]) >= 2)
        verify_shared(initial, 0, False)
        self.capture(output, "initial-light")

        body = next(p for p in initial["placements"] if p["bounds"][1] > 70)
        self.activate_placement(body, "Ribbon body toggles command")
        self.capture(output, "body-checked-light")
        qat = next(p for p in self.state()["placements"] if p["bounds"][1] < 45)
        self.activate_placement(qat, "QAT toggles same command")
        self.capture(output, "qat-unchecked-light")

        before = self.state()
        self.key(0x79)  # F10 opens KeyTips, independent of keyboard layout.
        self.snapshot("QAT KeyTips displayed", lambda s:
                      any("[3]" in p["label"] for p in s["placements"]))
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

        self.resize(320)
        self.menu_favorite(self.control(lambda p: p["label"] == "More" and p["bounds"][1] < 45),
                           "QAT More")
        self.menu_favorite(self.control(lambda p: p["label"].startswith("Редактирование\n")),
                           "Collapsed Editing group")
        self.capture(output, "collapsed-light")

        self.resize(240)
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
        qat = next(p for p in self.state()["placements"] if p["bounds"][1] < 45)
        self.activate_placement(qat, "Dark QAT after rebuild")
        self.capture(output, "checked-dark")
        self.resize(320)
        self.menu_favorite(self.control(lambda p: p["label"] == "More" and p["bounds"][1] < 45),
                           "Dark QAT More")
        self.capture(output, "unchecked-dark-collapsed")
        final = self.wait_state("no missing glyphs", lambda s: s["missing"] == 0)
        self.report["final_render_state"] = final
        verify_shared(self.state(), 8, False)
        self.windows.post(self.hwnd, WM_CLOSE)
        code = self.process.wait(timeout=self.timeout)
        if code != 0:
            raise SmokeFailure(f"Gallery exited with status {code}")
        self.report["checks"].append({"check": "native close", "exit_code": code})


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--timeout", type=float, default=20)
    args = parser.parse_args()
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
                         "240px overflow verified by state/input; capture after restoring width"]}
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

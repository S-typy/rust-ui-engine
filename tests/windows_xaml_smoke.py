#!/usr/bin/env python3
"""Exercise the compiled XAML form through child-only Win32 messages.

Captures share the bounded PrintWindow worker with the gallery. Input is posted
only to the process this test starts; no global keyboard or clipboard is used.
The retained helpers restore the temporarily aligned cursor during cleanup.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import time

from windows_gallery_smoke import GalleryWindows, capture_client
from windows_retained_smoke import (
    RetainedSession, WM_KEYDOWN, WM_KEYUP, WM_MOUSEMOVE, WM_LBUTTONDOWN,
    WM_LBUTTONUP, key_message,
)
from windows_smoke import SmokeFailure, WM_CLOSE

TITLE = re.compile(r"^Форма · Rust UI \| frames=(\d+) glyphs=(\d+) missing=(\d+)$")
WM_CHAR = 0x0102
VK_BACK = 0x08
VK_END = 0x23
VK_F24 = 0x87
TEXT = "Привет Rust"


def parse_state(title: str) -> dict | None:
    match = TITLE.fullmatch(title)
    return None if match is None else dict(zip(("frames", "glyphs", "missing"),
                                               map(int, match.groups()), strict=True))


def last_edit_value(log: str) -> str | None:
    """Read printable fixture values without evaluating diagnostic source text.

    The fixture uses the JSON-compatible subset of Rust Debug string escaping.
    Unrecognized escapes are errors, rather than guessed editor contents.
    """
    values = re.findall(r"^edit-value=(.*)$", log, re.MULTILINE)
    if not values:
        return None
    try:
        value = json.loads(values[-1])
    except json.JSONDecodeError as error:
        raise SmokeFailure("Invalid edit-value diagnostic for this fixture") from error
    if not isinstance(value, str):
        raise SmokeFailure("edit-value diagnostic is not a string")
    return value


def capture_checkpoints(pixels: bytes, width: int, height: int, scale: float,
                        dark: bool, focused: bool) -> dict:
    """Check direct sRGB palette, actual glyph coverage and antialiased corners."""
    if (not math.isfinite(scale) or scale <= 0 or width < 320 * scale
            or height < 160 * scale or len(pixels) != width * height * 4):
        raise SmokeFailure("Invalid form capture dimensions or DPI")

    def pixel(x: int, y: int) -> tuple[int, int, int]:
        offset = ((height - 1 - y) * width + x) * 4
        blue, green, red = pixels[offset:offset + 3]
        return red, green, blue

    def rgb(x: float, y: float) -> tuple[int, int, int]:
        return pixel(round(x * scale), round(y * scale))

    def near(actual, expected, tolerance=3):
        return all(abs(a - b) <= tolerance for a, b in zip(actual, expected, strict=True))

    canvas = (32, 32, 32) if dark else (243, 243, 243)
    surface = (45, 45, 45) if dark else (255, 255, 255)
    border = (85, 85, 85) if dark else (205, 205, 205)
    foreground = (245, 245, 245) if dark else (27, 27, 27)
    muted = (170, 170, 170) if dark else (105, 105, 105)
    accent = (96, 205, 255) if dark else (0, 95, 184)
    samples = {
        "canvas_rgb": (rgb(8, 8), canvas),
        "surface_rgb": (rgb(280, 50), surface),
        "rounded_corner_rgb": (rgb(32, 32), canvas),
        "top_border_rgb": (rgb(200, 32), border),
    }
    if focused:
        samples["focus_underline_rgb"] = (rgb(200, 66.5), accent)
    for name, (actual, expected) in samples.items():
        if not near(actual, expected):
            raise SmokeFailure(f"Unexpected {name}: {actual}, expected sRGB {expected}")
    ink = 0
    shades = set()
    for y in range(round(37 * scale), round(63 * scale)):
        for x in range(round(44 * scale), round(245 * scale)):
            color = pixel(x, y)
            if near(color, foreground, 10) or near(color, muted, 10):
                ink += 1
                shades.add(color)
    if ink < 30 or len(shades) < 5:
        raise SmokeFailure(f"No credible form glyph coverage: ink={ink}, shades={len(shades)}")
    corner_shades = set()
    for y in range(round(32 * scale), round(37 * scale)):
        for x in range(round(32 * scale), round(37 * scale)):
            color = pixel(x, y)
            if color not in (canvas, surface, border):
                corner_shades.add(color)
    if len(corner_shades) < 2:
        raise SmokeFailure("Rounded corner lacks antialiased coverage")
    return {**{name: actual for name, (actual, _) in samples.items()},
            "text_pixels": ink, "text_shades": len(shades),
            "corner_shades": len(corner_shades),
            "sha256_pixels": hashlib.sha256(pixels).hexdigest()}


class FormWindows(GalleryWindows):
    def __init__(self, dark: bool, focused: bool = False):
        super().__init__()
        self.dark = dark
        self.focused = focused

    def find_window(self, pid: int) -> int | None:
        found = []

        @self.enum_callback
        def collect(hwnd: int, _parameter: int) -> bool:
            if (self.owns(hwnd, pid) and self.user.IsWindowVisible(hwnd)
                    and self.title(hwnd).startswith("Форма · Rust UI")):
                found.append(hwnd)
            return True

        self.require(self.user.EnumWindows(collect, 0), "EnumWindows")
        return found[0] if found else None

    def capture_state(self, hwnd: int) -> dict | None:
        state = parse_state(self.title(hwnd))
        return None if state is None else {
            **state, "page": "form", "theme": "dark" if self.dark else "light"
        }

    def verify_capture(self, pixels, width, height, scale, state):
        if state["missing"] != 0:
            raise SmokeFailure("Form reported missing glyphs")
        return capture_checkpoints(pixels, width, height, scale, self.dark, self.focused)


def form_capture_worker(hwnd: int, pid: int, destination: Path, result_queue,
                        dark: bool, focused: bool) -> None:
    try:
        result_queue.put(FormWindows(dark, focused).screenshot(hwnd, pid, destination))
    except Exception as error:
        result_queue.put({"status": "unavailable", "reason": str(error)})


class FormSession(RetainedSession):
    def wait_state(self, description: str, predicate) -> dict:
        def matches():
            state = parse_state(self.windows.title(self.hwnd))
            if state is not None and state["missing"] != 0:
                raise SmokeFailure(f"Missing form glyphs: {state}")
            return state if state is not None and predicate(state) else None
        state = self.wait(description, matches)
        self.report["checks"].append({"check": description, "state": state})
        return state

    def log(self) -> str:
        return self.stdout_path.read_text(encoding="utf-8", errors="strict")

    def wait_value(self, value: str, description: str) -> None:
        self.wait(description, lambda: last_edit_value(self.log()) == value)
        self.report["checks"].append({"check": description, "edit_value": value})

    def type_text(self, text: str) -> None:
        self.alive()
        # winit assembles a KeyEvent from KEYDOWN followed by CHAR messages.
        # F24 is an inert carrier: TranslateMessage supplies no extra text.
        scan = self.windows.user.MapVirtualKeyW(VK_F24, 4)
        self.windows.post(self.hwnd, WM_KEYDOWN, VK_F24, key_message(scan, False))
        units = text.encode("utf-16-le")
        for offset in range(0, len(units), 2):
            self.windows.post(self.hwnd, WM_CHAR,
                              int.from_bytes(units[offset:offset + 2], "little"),
                              key_message(scan, False))
        self.windows.post(self.hwnd, WM_KEYUP, VK_F24, key_message(scan, True))

    def click_editor(self, scale: float) -> None:
        self.focus_messages(True)
        self.pointer(WM_MOUSEMOVE, 60, 50, scale)
        self.pointer(WM_LBUTTONDOWN, 60, 50, scale, held=True)
        self.pointer(WM_LBUTTONUP, 60, 50, scale)
        # Capture an unhovered editor so palette checks are deterministic.
        self.pointer(WM_MOUSEMOVE, 20, 100, scale)

    def capture(self, output: Path, name: str, focused: bool) -> None:
        # A focused field blinks every 500ms, so idle-frame equality is not a
        # valid settling condition. Require a live frame and allow queued input.
        self.wait_state("live frame before " + name, lambda s: s["frames"] > 0)
        time.sleep(0.15)
        result = capture_client(self.hwnd, self.process.pid, output / f"{name}.bmp",
                                self.timeout, worker_target=form_capture_worker,
                                worker_args=(self.windows.dark, focused))
        self.report["captures"][name] = result
        if result["status"] != "available":
            raise SmokeFailure(f"Required {name} capture unavailable: {result.get('reason')}")

    def run(self, output: Path) -> None:
        self.stdout_path = output / "stdout.log"
        self.hwnd = self.wait("visible form child window", lambda:
                             self.windows.find_window(self.process.pid))
        self.report["window_handle"] = self.hwnd
        self.wait_state("first form frame has placeholder glyphs", lambda s:
                        s["frames"] >= 1 and s["glyphs"] >= 5)
        self.wait("compiled x:Name resolves", lambda:
                  "named-editor=Message resolved=true" in self.log())
        self.report["checks"].append({"check": "compiled x:Name Message resolves"})
        dpi = self.windows.user.GetDpiForWindow(self.hwnd)
        if dpi <= 0:
            raise SmokeFailure("GetDpiForWindow returned zero")
        scale = dpi / 96.0
        self.report["dpi"] = dpi
        actual = self.windows.client_size(self.hwnd)
        expected = (round(640 * scale), round(280 * scale))
        if any(abs(a - b) > 1 for a, b in zip(actual, expected, strict=True)):
            raise SmokeFailure(f"Initial client size {actual} differs from XAML 640x280 at DPI {dpi}: {expected}")
        self.report["checks"].append({"check": "initial client dimensions match compiled XAML",
                                      "actual_physical": actual, "expected_physical": expected,
                                      "rounding_tolerance_pixels": 1})
        self.windows.topmost_without_activation(self.hwnd, True)
        self.promoted = True
        self.pointer(WM_MOUSEMOVE, 20, 100, scale)
        self.capture(output, "empty", focused=False)

        self.click_editor(scale)
        self.type_text(TEXT)
        self.wait_value(TEXT, "posted Unicode input reaches editor")
        self.capture(output, "typed", focused=True)
        self.key(VK_BACK)
        self.wait_value(TEXT[:-1], "Backspace removes final grapheme")

        before = parse_state(self.windows.title(self.hwnd))["frames"]
        target = (round(480 * scale), round(240 * scale))
        self.windows.resize_client(self.hwnd, *target)
        self.wait("native client resize", lambda: self.windows.client_size(self.hwnd) == target)
        self.wait_state("resized form redraw", lambda s: s["frames"] > before)
        if last_edit_value(self.log()) != TEXT[:-1]:
            raise SmokeFailure("Resize changed editor contents")
        self.capture(output, "resized", focused=True)

        self.focus_messages(False)
        self.type_text("IGNORED")
        time.sleep(0.2)
        self.alive()
        if last_edit_value(self.log()) != TEXT[:-1]:
            raise SmokeFailure("Blurred editor accepted text")
        self.report["checks"].append({"check": "blur prevents input and preserves value"})
        self.click_editor(scale)
        self.key(VK_END)
        self.type_text(TEXT[-1])
        self.wait_value(TEXT, "focus restore permits editing preserved value")

        self.windows.require(self.windows.user.ShowWindowAsync(self.hwnd, 6), "minimize")
        self.wait("native minimize", lambda: self.windows.user.IsIconic(self.hwnd))
        self.windows.require(self.windows.user.ShowWindowAsync(self.hwnd, 9), "restore")
        self.wait("native restore", lambda: not self.windows.user.IsIconic(self.hwnd))
        self.click_editor(scale)
        self.capture(output, "restored", focused=True)
        if last_edit_value(self.log()) != TEXT:
            raise SmokeFailure("Minimize/restore changed editor contents")
        self.windows.post(self.hwnd, WM_CLOSE)
        try:
            code = self.process.wait(timeout=self.timeout)
        except subprocess.TimeoutExpired as error:
            raise SmokeFailure("Form did not exit after WM_CLOSE") from error
        if code != 0:
            raise SmokeFailure(f"Form exited with status {code}")
        self.report["checks"].append({"check": "native close exits successfully", "exit_code": code})


def self_test() -> None:
    title = "Форма · Rust UI | frames=12 glyphs=10 missing=0"
    assert parse_state(title) == {"frames": 12, "glyphs": 10, "missing": 0}
    assert parse_state(title + " extra") is None
    assert parse_state(title.replace("Форма", "Other")) is None
    assert last_edit_value('named-editor=Message resolved=true\nedit-value="Привет Rust"\n') == TEXT
    assert last_edit_value('edit-value="one"\nedit-value="two"\n') == "two"
    try:
        last_edit_value('edit-value=__import__("os")')
    except SmokeFailure:
        pass
    else:
        raise AssertionError("Diagnostic expressions must be rejected")
    width, height = 640, 280
    for dark in (False, True):
        canvas = 32 if dark else 243
        surface = 45 if dark else 255
        border = 85 if dark else 205
        foreground = 245 if dark else 27
        pixels = bytearray(bytes((canvas, canvas, canvas, 255)) * width * height)

        def set_rgb(x, y, color):
            offset = ((height - 1 - y) * width + x) * 4
            pixels[offset:offset + 4] = bytes((*reversed(color), 255))

        set_rgb(280, 50, (surface,) * 3)
        set_rgb(200, 32, (border,) * 3)
        set_rgb(33, 32, (border + 1,) * 3)
        set_rgb(34, 32, (border + 2,) * 3)
        for y in range(40, 45):
            for x in range(45, 80):
                set_rgb(x, y, (foreground - x % 6 if dark else foreground + x % 6,) * 3)
        assert capture_checkpoints(pixels, width, height, 1.0, dark, False)["text_pixels"] > 30
        direct = bytes(pixels)
        # Reproduce the previous accidental linear->sRGB palette encoding.
        for x, y in ((8, 8), (32, 32)):
            wrong = round(255 * (1.055 * (canvas / 255) ** (1 / 2.4) - 0.055))
            set_rgb(x, y, (wrong,) * 3)
        try:
            capture_checkpoints(pixels, width, height, 1.0, dark, False)
        except SmokeFailure:
            pass
        else:
            raise AssertionError("Washed-out palette must be rejected")
        try:
            capture_checkpoints(direct, width, height, float("nan"), dark, False)
        except SmokeFailure:
            pass
        else:
            raise AssertionError("Invalid scale must be rejected")
    print("PASS: XAML title, diagnostic, direct-sRGB and corner/glyph capture fixtures")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--dark", action="store_true")
    parser.add_argument("--timeout", type=float, default=20)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if sys.platform != "win32":
        parser.error("An interactive Windows desktop is required")
    if not args.exe or not args.output_dir:
        parser.error("--exe and --output-dir are required")
    if not math.isfinite(args.timeout) or not 0 < args.timeout <= 60:
        parser.error("--timeout must be in (0, 60]")
    executable = args.exe.resolve()
    if not executable.is_file():
        parser.error("--exe must name a file")
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = {"status": "failed", "started_utc": datetime.now(timezone.utc).isoformat(),
              "executable": str(executable), "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
              "mode": "xaml-form", "theme": "dark" if args.dark else "light",
              "checks": [], "captures": {}, "backend_environment": os.environ.get("WGPU_BACKEND"),
              "input_method": "Posted child-only Win32 pointer, KEYDOWN/CHAR/KEYUP messages; restored cursor alignment",
              "limits": ["No physical keyboard, native IME, screen reader or external clipboard test",
                         "No multi-monitor DPI or frame-time benchmark", "Captures require separate visual review"]}
    session = None
    try:
        windows = FormWindows(args.dark)
        original_cursor = windows.cursor_position()
        with (output / "stdout.log").open("wb") as stdout, (output / "stderr.log").open("wb") as stderr:
            command = [str(executable), "--diagnostics"] + (["--dark"] if args.dark else [])
            process = subprocess.Popen(command, stdout=stdout, stderr=stderr,
                                       creationflags=subprocess.CREATE_NO_WINDOW)
            report["process_id"] = process.pid
            session = FormSession(windows, process, report, args.timeout, original_cursor)
            try:
                session.run(output)
                report["status"] = "passed"
            finally:
                session.cleanup()
    except (SmokeFailure, OSError, subprocess.SubprocessError) as error:
        report["status"] = "failed"
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

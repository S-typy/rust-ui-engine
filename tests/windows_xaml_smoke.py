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
VK_TAB = 0x09
VK_RETURN = 0x0D
VK_ESCAPE = 0x1B
VK_SPACE = 0x20
VK_LEFT = 0x25
VK_UP = 0x26
VK_RIGHT = 0x27
VK_DOWN = 0x28
VK_END = 0x23
VK_F24 = 0x87
TEXT = "Привет Rust"
REQUIRED_MESSAGE = "Введите текст перед подтверждением."


def parse_state(title: str) -> dict | None:
    match = TITLE.fullmatch(title)
    return None if match is None else dict(zip(("frames", "glyphs", "missing"),
                                               map(int, match.groups()), strict=True))


def last_edit_value(log: str) -> str | None:
    return last_edit_diagnostic(log, "edit-value")


def last_edit_diagnostic(log: str, name: str) -> str | None:
    """Read printable fixture values without evaluating diagnostic source text.

    The fixture uses the JSON-compatible subset of Rust Debug string escaping.
    Unrecognized escapes are errors, rather than guessed editor contents.
    """
    values = re.findall(r"^" + re.escape(name) + r"=(.*)$", log, re.MULTILINE)
    if not values:
        return None
    try:
        value = json.loads(values[-1])
    except json.JSONDecodeError as error:
        raise SmokeFailure(f"Invalid {name} diagnostic for this fixture") from error
    if not isinstance(value, str):
        raise SmokeFailure(f"{name} diagnostic is not a string")
    return value


def last_checkbox_state(log: str) -> bool | None:
    values = re.findall(r"^checkbox-state=Option checked=(.*)$", log, re.MULTILINE)
    if not values:
        return None
    if values[-1] not in ("true", "false"):
        raise SmokeFailure("Invalid Option checkbox diagnostic")
    return values[-1] == "true"


def capture_checkpoints(pixels: bytes, width: int, height: int, scale: float,
                        dark: bool, focused: bool, invalid: bool = False,
                        read_only: bool = False) -> dict:
    """Check direct sRGB palette, actual glyph coverage and antialiased corners."""
    if (not math.isfinite(scale) or scale <= 0 or width < 320 * scale
            or height < 340 * scale or len(pixels) != width * height * 4):
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
    error = (255, 153, 164) if dark else (196, 43, 28)
    control_border, check_mark = border, accent
    if invalid:
        border = accent = error
    samples = {
        "canvas_rgb": (rgb(8, 8), canvas),
        "surface_rgb": (rgb(280, 122), canvas if read_only else surface),
        "rounded_corner_rgb": (rgb(32, 104), canvas),
        "top_border_rgb": (rgb(200, 104), border),
        "confirm_surface_rgb": (rgb(180, 210), surface),
        "confirm_border_rgb": (rgb(112, 188), control_border),
        "checkbox_surface_rgb": (rgb(300, 164), canvas),
        "checkbox_indicator_rgb": (rgb(42, 159), check_mark if read_only else surface),
        "confirm_rounded_corner_rgb": (rgb(32, 188), canvas),
    }
    if focused:
        samples["focus_underline_rgb"] = (rgb(200, 138.5), accent)
    for name, (actual, expected) in samples.items():
        if not near(actual, expected):
            raise SmokeFailure(f"Unexpected {name}: {actual}, expected sRGB {expected}")
    ink = 0
    shades = set()
    for y in range(round(109 * scale), round(135 * scale)):
        for x in range(round(44 * scale), round(245 * scale)):
            color = pixel(x, y)
            if near(color, foreground, 10) or near(color, muted, 10):
                ink += 1
                shades.add(color)
    if ink < 30 or len(shades) < 5:
        raise SmokeFailure(f"No credible form glyph coverage: ink={ink}, shades={len(shades)}")
    corner_shades = set()
    for y in range(round(104 * scale), round(109 * scale)):
        for x in range(round(32 * scale), round(37 * scale)):
            color = pixel(x, y)
            if color not in (canvas, surface, border):
                corner_shades.add(color)
    if len(corner_shades) < 2:
        raise SmokeFailure("Rounded corner lacks antialiased coverage")
    label_ink = {}
    # Independent fixed fixture regions prove that the nested tree paints its
    # TextBlocks, checkbox caption and button, beyond the editor's glyphs.
    for name, (left, top, right, bottom) in {
        "title": (40, 32, 300, 64),
        "message_label": (40, 72, 300, 96),
        "option": (64, 151, 270, 179),
        "confirm": (40, 193, 184, 219),
    }.items():
        count = sum(near(pixel(x, y), foreground, 10)
                    for y in range(round(top * scale), round(bottom * scale))
                    for x in range(round(left * scale), round(right * scale)))
        if count < 30:
            raise SmokeFailure(f"Nested {name} has no credible glyph coverage: {count}")
        label_ink[name] = count
    message_ink = 0
    if invalid:
        # The example places its validation status after the fixed-height help
        # label. Check readable message glyphs separately from the field chrome.
        # This is coverage, not OCR; the exact text is checked in diagnostics.
        for y in range(round(369 * scale), min(height, round(405 * scale))):
            for x in range(round(40 * scale), min(width, round(420 * scale))):
                color = pixel(x, y)
                if near(color, foreground, 10) or near(color, error, 10):
                    message_ink += 1
        if message_ink < 30:
            raise SmokeFailure(f"Validation message has no credible glyph coverage: {message_ink}")
    return {**{name: actual for name, (actual, _) in samples.items()},
            "text_pixels": ink, "text_shades": len(shades),
            "corner_shades": len(corner_shades),
            "nested_label_pixels": label_ink,
            "validation_message_pixels": message_ink,
            "sha256_pixels": hashlib.sha256(pixels).hexdigest()}


def button_capture_checkpoints(pixels: bytes, width: int, height: int, scale: float,
                               dark: bool, checked: bool, phase: str, focused: bool,
                               option_checked: bool = False) -> dict:
    """Verify the fixed Fluent fixture, independently of input diagnostics."""
    def pixel(x, y):
        offset = ((height - 1 - y) * width + x) * 4
        blue, green, red = pixels[offset:offset + 3]
        return red, green, blue

    def rgb(x, y):
        return pixel(round(x * scale), round(y * scale))

    def near(actual, expected):
        return all(abs(a - b) <= 3 for a, b in zip(actual, expected, strict=True))

    canvas = (32, 32, 32) if dark else (243, 243, 243)
    surface = (45, 45, 45) if dark else (255, 255, 255)
    hover = (50, 50, 50) if dark else (250, 250, 250)
    pressed = (40, 40, 40) if dark else (235, 235, 235)
    selected = (35, 75, 105) if dark else (196, 225, 245)
    accent = (96, 205, 255) if dark else (0, 95, 184)
    muted = (170, 170, 170) if dark else (105, 105, 105)
    background = {"normal": selected if checked else surface,
                  "hover": hover, "pressed": pressed}[phase]
    samples = {
        "pin_background": (rgb(308, 214), background),
        "pin_corner": (rgb(200, 188), canvas),
        "pin_focus": (rgb(203, 206), accent if focused else background),
        "disabled_background": (rgb(432, 214), canvas),
        "disabled_corner": (rgb(328, 188), canvas),
    }
    for name, (actual, expected) in samples.items():
        if not near(actual, expected):
            raise SmokeFailure(f"Unexpected {name}: {actual}, expected {expected}")
    disabled_ink = sum(near(pixel(x, y), muted)
                       for y in range(round(194 * scale), round(220 * scale))
                       for x in range(round(338 * scale), round(431 * scale)))
    if disabled_ink < 30:
        raise SmokeFailure(f"Disabled caption lacks muted glyphs: {disabled_ink}")
    foreground = (245, 245, 245) if dark else (27, 27, 27)
    pin_ink = sum(near(pixel(x, y), foreground)
                  for y in range(round(194 * scale), round(220 * scale))
                  for x in range(round(214 * scale), round(300 * scale)))
    if pin_ink < 30:
        raise SmokeFailure(f"ToggleButton caption lacks glyphs: {pin_ink}")
    mark_ink = 0
    if option_checked:
        mark = canvas if dark else (255, 255, 255)
        mark_ink = sum(near(pixel(x, y), mark)
                       for y in range(round(157 * scale), round(172 * scale))
                       for x in range(round(42 * scale), round(56 * scale)))
        if mark_ink < 4:
            raise SmokeFailure(f"Checked indicator lacks a contrasting checkmark: {mark_ink}")
    return {**{name: actual for name, (actual, _) in samples.items()},
            "disabled_glyph_pixels": disabled_ink, "pin_glyph_pixels": pin_ink,
            "checkmark_pixels": mark_ink,
            "pin_checked": checked, "pin_phase": phase, "pin_focused": focused}


def radio_capture_checkpoints(pixels: bytes, width: int, height: int, scale: float,
                              dark: bool, selected: str, focused: str | None) -> dict:
    """Check circular indicators, exclusive dots and separate keyboard focus."""
    if selected not in ("AlignLeft", "AlignCenter"):
        raise SmokeFailure("Invalid radio fixture selection")
    canvas = (32, 32, 32) if dark else (243, 243, 243)
    surface = (45, 45, 45) if dark else (255, 255, 255)
    accent = (96, 205, 255) if dark else (0, 95, 184)
    dot = canvas if dark else (255, 255, 255)
    samples = {}
    for name, left in (("AlignLeft", 32), ("AlignCenter", 160), ("AlignRight", 288)):
        background = accent if name == selected else canvas if name == "AlignRight" else surface
        for part, (x, y, expected) in {
            "dot": (left + 17, 248, dot if name == selected else background),
            "fill": (left + 17, 242, background),
            "circle_corner": (left + 8, 239, canvas),
            "row": (left + 108, 258, canvas),
            "focus": (left + 3, 248, accent if name == focused else canvas),
        }.items():
            px, py = round(x * scale), round(y * scale)
            if not 0 <= px < width or not 0 <= py < height:
                raise SmokeFailure("Radio checkpoint lies outside the capture")
            offset = ((height - 1 - py) * width + px) * 4
            blue, green, red = pixels[offset:offset + 3]
            actual = (red, green, blue)
            if any(abs(a - b) > 3 for a, b in zip(actual, expected, strict=True)):
                raise SmokeFailure(f"Unexpected {name} {part}: {actual}, expected {expected}")
            samples[f"{name}_{part}"] = actual
    return samples


def primitives_capture_checkpoints(pixels: bytes, width: int, height: int,
                                  scale: float, dark: bool) -> dict:
    """Check visible vector silhouettes and thin rules at independent fixture bounds."""
    if (not math.isfinite(scale) or scale <= 0 or width < 440 * scale
            or height < 316 * scale or len(pixels) != width * height * 4):
        raise SmokeFailure("Invalid primitive capture dimensions or DPI")
    canvas = (32, 32, 32) if dark else (243, 243, 243)
    foreground = (245, 245, 245) if dark else (27, 27, 27)
    muted = (170, 170, 170) if dark else (105, 105, 105)
    border = (85, 85, 85) if dark else (205, 205, 205)

    def pixel(x, y):
        offset = ((height - 1 - y) * width + x) * 4
        blue, green, red = pixels[offset:offset + 3]
        return red, green, blue

    def near(actual, expected):
        return all(abs(a - b) <= 3 for a, b in zip(actual, expected, strict=True))

    def require(x, y, expected, description):
        actual = pixel(math.floor(x * scale), math.floor(y * scale))
        if not near(actual, expected):
            raise SmokeFailure(f"Unexpected {description}: {actual}, expected {expected}")

    icons, masks = {}, set()
    for name, left in (("Check", 32), ("Close", 64), ("Add", 96), ("Remove", 128),
                       ("ChevronDown", 160), ("ChevronRight", 192), ("Save", 224),
                       ("DisabledPin", 265)):
        expected = muted if name == "DisabledPin" else foreground
        colors = [pixel(x, y)
                  for y in range(round(276 * scale), round(300 * scale))
                  for x in range(round(left * scale), round((left + 24) * scale))]
        ink = sum(near(color, expected) for color in colors)
        shades = {color for color in colors
                  if not near(color, expected) and not near(color, canvas)}
        if ink < 12 * scale * scale:
            raise SmokeFailure(f"Missing {name} vector silhouette: ink={ink}")
        if name in ("Check", "Close", "ChevronDown", "ChevronRight") and len(shades) < 2:
            raise SmokeFailure(f"{name} diagonal lacks antialiased coverage")
        if name == "DisabledPin" and any(near(color, foreground) for color in colors):
            raise SmokeFailure("Disabled icon contains enabled foreground")
        for x in (left, left + 23):
            for y in (276, 299):
                require(x, y, canvas, name + " transparent corner")
        mask = bytes(not near(color, canvas) for color in colors)
        digest = hashlib.sha256(mask).hexdigest()
        if digest in masks:
            raise SmokeFailure(f"{name} repeats another icon silhouette")
        masks.add(digest)
        icons[name] = {"ink_pixels": ink, "edge_shades": len(shades), "mask_sha256": digest}
    for x in (40, 100, 200, 300, 400):
        require(x, 312, border, "horizontal separator")
        for y in (310, 314):
            require(x, y, canvas, "horizontal separator outside")
    for y in (280, 288, 296):
        require(256, y, border, "vertical separator")
        for x in (254, 259):
            require(x, y, canvas, "vertical separator outside")
    return {"icons": icons, "horizontal_separator": True, "vertical_separator": True}


class FormWindows(GalleryWindows):
    def __init__(self, dark: bool, focused: bool = False, invalid: bool = False,
                 read_only: bool = False, buttons: dict | None = None,
                 radios: dict | None = None):
        super().__init__()
        self.dark = dark
        self.focused = focused
        self.invalid = invalid
        self.read_only = read_only
        self.buttons = buttons
        self.radios = radios

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
        result = capture_checkpoints(pixels, width, height, scale, self.dark, self.focused,
                                     self.invalid, self.read_only)
        result["primitives"] = primitives_capture_checkpoints(pixels, width, height,
                                                             scale, self.dark)
        if self.buttons is not None:
            result["buttons"] = button_capture_checkpoints(pixels, width, height, scale,
                                                          self.dark, option_checked=self.read_only,
                                                          **self.buttons)
        if self.radios is not None:
            result["radios"] = radio_capture_checkpoints(pixels, width, height, scale,
                                                        self.dark, **self.radios)
        return result


def form_capture_worker(hwnd: int, pid: int, destination: Path, result_queue,
                        dark: bool, focused: bool, invalid: bool, read_only: bool,
                        buttons: dict | None = None, radios: dict | None = None) -> None:
    try:
        result_queue.put(FormWindows(dark, focused, invalid, read_only, buttons, radios).screenshot(hwnd, pid, destination))
    except Exception as error:
        result_queue.put({"status": "unavailable", "reason": str(error)})


class FormSession(RetainedSession):
    def pointer(self, message: int, x: int, y: int, scale: float, held: bool = False) -> None:
        super().pointer(message, x, y, scale, held)
        self.expected_cursor = self.windows.cursor_position()

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

    def wait_edit_diagnostic(self, name: str, value: str, description: str,
                             after: int = 0) -> None:
        def matches():
            log = self.log()
            return (log.count(name + "=") > after
                    and last_edit_diagnostic(log, name) == value)
        self.wait(description, matches)
        self.report["checks"].append({"check": description, "diagnostic": name, "value": value})

    def wait_focus(self, name: str, description: str) -> None:
        self.wait_edit_diagnostic("focus-name", name, description)

    def tab_from_radio_to_message(self) -> None:
        self.key(VK_TAB)
        self.wait_focus("ActionMenu", "Tab leaves radio group for DropDownButton")
        for popup in (False, True):
            before = self.log().count("popup-state ")
            self.key(VK_TAB)
            def focused_part():
                snapshots = [json.loads(line.removeprefix("popup-state "))
                             for line in self.log().splitlines(keepends=True)
                             if line.startswith("popup-state ") and line.endswith("\n")]
                return len(snapshots) > before and any(
                    item["focused"] and item["has_popup"] == popup
                    and item["bounds"][0] >= 195 and item["bounds"][1] >= 430
                    for item in snapshots[-1]["controls"])
            self.wait("Tab focuses the separate SplitButton part", focused_part)
        self.key(VK_TAB)
        self.wait_focus("Message", "Tab leaves SplitButton and wraps to Message")

    def wait_checkbox(self, checked: bool, description: str, after: int) -> None:
        def matches():
            log = self.log()
            read_only = re.findall(r"^editor-read-only=(true|false)$", log, re.MULTILINE)
            return (log.count("checkbox-state=Option checked=") > after
                    and last_checkbox_state(log) == checked
                    and bool(read_only) and read_only[-1] == str(checked).lower())
        self.wait(description, matches)
        self.report["checks"].append({"check": description, "checked": checked,
                                      "editor_read_only": checked})

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

    def click_control(self, x: int, y: int, scale: float) -> None:
        self.focus_messages(True)
        self.pointer(WM_MOUSEMOVE, x, y, scale)
        self.pointer(WM_LBUTTONDOWN, x, y, scale, held=True)
        self.pointer(WM_LBUTTONUP, x, y, scale)
        # Capture an unhovered editor so palette checks are deterministic.
        self.pointer(WM_MOUSEMOVE, 20, 100, scale)

    def click_editor(self, scale: float) -> None:
        self.click_control(60, 122, scale)
        self.wait_focus("Message", "pointer focuses the nested Message editor")

    def capture(self, output: Path, name: str, focused: bool, invalid: bool = False,
                read_only: bool = False, buttons: dict | None = None,
                radios: dict | None = None) -> None:
        # A focused field blinks every 500ms, so idle-frame equality is not a
        # valid settling condition. Require a live frame and allow queued input.
        self.wait_state("live frame before " + name, lambda s: s["frames"] > 0)
        time.sleep(0.15)
        cursor_before = self.windows.cursor_position()
        result = capture_client(self.hwnd, self.process.pid, output / f"{name}.bmp",
                                self.timeout, worker_target=form_capture_worker,
                                worker_args=(self.windows.dark, focused, invalid, read_only, buttons, radios))
        result["cursor"] = {"expected": list(self.expected_cursor),
                            "before": list(cursor_before),
                            "after": list(self.windows.cursor_position())}
        self.report["captures"][name] = result
        if buttons is not None and buttons["phase"] in ("hover", "pressed"):
            if any(result["cursor"][phase] != result["cursor"]["expected"]
                   for phase in ("before", "after")):
                raise SmokeFailure(f"Pointer moved during required {name} state: {result['cursor']}")
        if result["status"] != "available":
            raise SmokeFailure(f"Required {name} capture unavailable: {result.get('reason')}")

    def wait_pin(self, checked: bool, after: int, description: str) -> None:
        expected = str(checked).lower()
        def matches():
            values = re.findall(r"^toggle-state=Pin checked=(true|false)$", self.log(), re.MULTILINE)
            return len(values) > after and values[-1] == expected
        self.wait(description, matches)
        self.report["checks"].append({"check": description, "pin_checked": checked})

    def run_controls(self, output: Path, scale: float) -> None:
        self.click_control(112, 206, scale)
        self.wait("Confirm command", lambda: "command=Confirm" in self.log())
        self.wait_edit_diagnostic("edit-invalid", REQUIRED_MESSAGE, "Confirm rejects required empty value")
        self.wait_focus("Confirm", "pointer focuses Confirm")
        self.key(VK_TAB)
        self.wait_focus("Pin", "Tab reaches ToggleButton")
        self.capture(output, "pin-focused", focused=False, invalid=True,
                     buttons={"checked": False, "phase": "normal", "focused": True})

        self.pointer(WM_MOUSEMOVE, 260, 206, scale)
        self.capture(output, "pin-hover", focused=False, invalid=True,
                     buttons={"checked": False, "phase": "hover", "focused": True})
        self.pointer(WM_LBUTTONDOWN, 260, 206, scale, held=True)
        self.capture(output, "pin-pressed", focused=False, invalid=True,
                     buttons={"checked": False, "phase": "pressed", "focused": True})
        toggles = self.log().count("toggle-state=Pin checked=")
        self.pointer(WM_MOUSEMOVE, 20, 100, scale, held=True)
        self.pointer(WM_LBUTTONUP, 20, 100, scale)
        self.capture(output, "pin-canceled", focused=False, invalid=True,
                     buttons={"checked": False, "phase": "normal", "focused": True})
        if self.log().count("toggle-state=Pin checked=") != toggles:
            raise SmokeFailure("Pointer release outside Pin changed checked state")
        self.report["checks"].append({"check": "pointer release outside Pin cancels activation"})

        self.key(VK_SPACE)
        self.wait_pin(True, toggles, "Space checks Pin on key release")
        self.capture(output, "pin-checked", focused=False, invalid=True,
                     buttons={"checked": True, "phase": "normal", "focused": True})
        toggles = self.log().count("toggle-state=Pin checked=")
        self.key(VK_RETURN)
        self.wait_pin(False, toggles, "Enter clears Pin")
        self.run_radios(output, scale)
        self.tab_from_radio_to_message()
        for name, x, y in (("SaveIcon", 236, 288), ("HRule", 400, 312),
                            ("VRule", 256, 280)):
            commands = self.log().count("command=")
            self.click_control(x, y, scale)
            self.wait_focus("", name + " does not accept pointer focus")
            self.key(VK_TAB)
            self.wait_focus("Message", "Tab skips " + name + " and starts at Message")
            if self.log().count("command=") != commands:
                raise SmokeFailure(name + " activated a command")
        self.click_control(380, 206, scale)
        time.sleep(0.15)
        if last_edit_diagnostic(self.log(), "focus-name") == "Disabled":
            raise SmokeFailure("Disabled button accepted focus")
        self.report["checks"].append({"check": "disabled button rejects pointer focus"})
        self.click_editor(scale)
        self.key(VK_TAB)
        self.wait_focus("Option", "Tab reaches Option")
        toggles = self.log().count("checkbox-state=Option checked=")
        self.key(VK_SPACE)
        self.wait_checkbox(True, "Space checks Option and makes editor read-only", toggles)
        for name in ("Confirm", "Pin", "AlignLeft"):
            self.key(VK_TAB)
            self.wait_focus(name, "Tab reaches " + name)
        self.tab_from_radio_to_message()
        self.capture(output, "check-checked", focused=True, invalid=True, read_only=True,
                     buttons={"checked": False, "phase": "normal", "focused": False})

        toggles = self.log().count("checkbox-state=Option checked=")
        self.click_control(60, 164, scale)
        self.wait_checkbox(False, "pointer clears Option", toggles)
        self.click_editor(scale)
        before = parse_state(self.windows.title(self.hwnd))["frames"]
        target = (round(480 * scale), round(448 * scale))
        self.windows.resize_client(self.hwnd, *target)
        self.wait("client resize", lambda: self.windows.client_size(self.hwnd) == target)
        self.wait_state("resize redraw", lambda state: state["frames"] > before)
        self.capture(output, "resized-controls", focused=True, invalid=True,
                     buttons={"checked": False, "phase": "normal", "focused": False})
        self.windows.post(self.hwnd, WM_CLOSE)
        code = self.process.wait(timeout=self.timeout)
        if code != 0:
            raise SmokeFailure(f"Unexpected close status {code}")
        self.report["checks"].append({"check": "native close", "exit_code": code})

    def wait_radio(self, selected: str, description: str) -> None:
        def matches():
            log = self.log()
            for name in ("AlignLeft", "AlignCenter", "AlignRight"):
                values = re.findall(r"^radio-state=" + name + r" checked=(true|false)$",
                                    log, re.MULTILINE)
                if not values or values[-1] != str(name == selected).lower():
                    return False
            return True
        self.wait(description, matches)
        self.report["checks"].append({"check": description, "selected_radio": selected})

    def run_radios(self, output: Path, scale: float) -> None:
        self.key(VK_TAB)
        self.wait_focus("AlignLeft", "Tab skips Disabled and enters selected radio")
        self.wait_radio("AlignLeft", "XAML initializes exactly one selected radio")
        self.capture(output, "radio-left", focused=False, invalid=True,
                     radios={"selected": "AlignLeft", "focused": "AlignLeft"})
        for key, selected, description in (
            (VK_RIGHT, "AlignCenter", "Right selects next radio"),
            (VK_RIGHT, "AlignLeft", "Right skips disabled radio and wraps"),
            (VK_LEFT, "AlignCenter", "Left wraps backwards skipping disabled radio"),
            (VK_UP, "AlignLeft", "Up selects previous radio"),
            (VK_DOWN, "AlignCenter", "Down selects next radio"),
        ):
            self.key(key)
            self.wait_focus(selected, description + " and moves focus")
            self.wait_radio(selected, description + " exclusively")
        self.key(VK_SPACE)
        self.capture(output, "radio-center-space", focused=False, invalid=True,
                     radios={"selected": "AlignCenter", "focused": "AlignCenter"})
        self.wait_radio("AlignCenter", "Space preserves an already selected radio")
        self.tab_from_radio_to_message()
        self.click_control(340, 248, scale)
        self.capture(output, "radio-disabled", focused=False, invalid=True,
                     radios={"selected": "AlignCenter", "focused": None})
        if last_edit_diagnostic(self.log(), "focus-name") == "AlignRight":
            raise SmokeFailure("Disabled radio accepted pointer focus")
        self.wait_radio("AlignCenter", "Disabled radio rejects pointer selection")
        self.click_control(90, 248, scale)
        self.wait_focus("AlignLeft", "Mouse focuses the first radio")
        self.wait_radio("AlignLeft", "Mouse selects first radio and clears second")
        self.key(VK_RETURN)
        self.capture(output, "radio-left-enter", focused=False, invalid=True,
                     radios={"selected": "AlignLeft", "focused": "AlignLeft"})
        self.wait_radio("AlignLeft", "Enter preserves the selected radio")

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
        names = ("Message", "Option", "Confirm", "Pin", "Disabled",
                 "AlignLeft", "AlignCenter", "AlignRight", "CheckIcon", "CloseIcon",
                 "AddIcon", "RemoveIcon", "ChevronDownIcon", "ChevronRightIcon",
                 "SaveIcon", "DisabledIcon", "VRule", "HRule")
        for name in names:
            self.wait("compiled nested x:Name " + name + " resolves", lambda name=name:
                      f"named-control={name} resolved=true" in self.log())
        self.report["checks"].append({"check": "compiled nested control names resolve",
                                      "names": list(names)})
        dpi = self.windows.user.GetDpiForWindow(self.hwnd)
        if dpi <= 0:
            raise SmokeFailure("GetDpiForWindow returned zero")
        scale = dpi / 96.0
        self.report["dpi"] = dpi
        actual = self.windows.client_size(self.hwnd)
        expected = (round(640 * scale), round(512 * scale))
        if any(abs(a - b) > 1 for a, b in zip(actual, expected, strict=True)):
            raise SmokeFailure(f"Initial client size {actual} differs from XAML 640x512 at DPI {dpi}: {expected}")
        self.report["checks"].append({"check": "initial client dimensions match compiled XAML",
                                      "actual_physical": actual, "expected_physical": expected,
                                      "rounding_tolerance_pixels": 1})
        self.windows.topmost_without_activation(self.hwnd, True)
        self.promoted = True
        self.pointer(WM_MOUSEMOVE, 20, 100, scale)
        self.capture(output, "empty", focused=False)

        if self.report["mode"] == "xaml-controls":
            self.run_controls(output, scale)
            return

        self.click_editor(scale)
        self.key(VK_RETURN)
        self.wait_edit_diagnostic("edit-invalid", REQUIRED_MESSAGE,
                                  "Enter validates the required empty editor")
        self.capture(output, "invalid", focused=True, invalid=True)
        failures_before = self.log().count("edit-invalid=")
        commands_before = self.log().count("command=Confirm")
        self.click_control(112, 206, scale)
        self.wait("Confirm command runs for an invalid draft", lambda:
                  self.log().count("command=Confirm") > commands_before)
        # Leaving Message validates once; the button's explicit commit must
        # validate again. This distinguishes its action from blur alone.
        self.wait_edit_diagnostic("edit-invalid", REQUIRED_MESSAGE,
                                  "Confirm explicitly validates the invalid draft after blur",
                                  after=failures_before + 1)
        self.click_editor(scale)
        failures_before = self.log().count("edit-invalid=")
        self.focus_messages(False)
        self.wait_edit_diagnostic("edit-invalid", REQUIRED_MESSAGE,
                                  "invalid draft survives focus loss without closing the form",
                                  after=failures_before)
        if last_edit_value(self.log()) is not None or "edit-committed=" in self.log():
            raise SmokeFailure("Invalid empty draft changed or was accepted on focus loss")
        self.click_editor(scale)
        self.type_text(TEXT)
        self.wait_value(TEXT, "posted Unicode input reaches editor")
        self.key(VK_RETURN)
        self.wait_edit_diagnostic("edit-committed", TEXT,
                                  "Enter accepts the valid Unicode draft")
        self.capture(output, "typed", focused=True)

        # The radio group contributes one stop after Pin. Structural panels
        # and labels must not introduce tab stops; the final stop wraps.
        self.key(VK_TAB)
        self.wait_focus("Option", "Tab moves from Message to nested Option")
        toggles_before = self.log().count("checkbox-state=Option checked=")
        self.key(VK_SPACE)
        self.wait_checkbox(True, "Space checks Option and makes Message read-only",
                           after=toggles_before)
        self.key(VK_TAB)
        self.wait_focus("Confirm", "Tab moves from Option to nested Confirm")
        self.key(VK_TAB)
        self.wait_focus("Pin", "Tab moves from Confirm to Pin")
        self.key(VK_TAB)
        self.wait_focus("AlignLeft", "Tab skips Disabled and enters the radio group")
        self.tab_from_radio_to_message()
        changes_before = self.log().count("edit-value=")
        text_before = self.log().count('edit-input=Text("READONLY")')
        self.type_text("READONLY")
        self.wait("posted read-only text reaches the form", lambda:
                  self.log().count('edit-input=Text("READONLY")') > text_before)
        backspace_before = self.log().count("edit-input=Input(KeyUp { key: Backspace,")
        self.key(VK_BACK)
        self.wait("posted read-only Backspace completes", lambda:
                  self.log().count("edit-input=Input(KeyUp { key: Backspace,") > backspace_before)
        if last_edit_value(self.log()) != TEXT or self.log().count("edit-value=") != changes_before:
            raise SmokeFailure("Read-only Message accepted typing/deletion or emitted TextChanged")
        self.report["checks"].append({"check": "read-only Message rejects posted text and Backspace without TextChanged"})
        self.capture(output, "read-only", focused=True, read_only=True)
        toggles_before = self.log().count("checkbox-state=Option checked=")
        self.click_control(60, 164, scale)
        self.wait_checkbox(False, "mouse unchecks Option and restores editing",
                           after=toggles_before)
        self.click_editor(scale)
        self.key(VK_END)
        self.type_text("#")
        self.wait_value(TEXT + "#", "editable Message accepts a draft after Option is cleared")
        commands_before = self.log().count("command=Confirm")
        commits_before = self.log().count("edit-committed=")
        self.click_control(112, 206, scale)
        self.wait("mouse invokes the compiled Confirm button", lambda:
                  self.log().count("command=Confirm") > commands_before)
        self.wait_focus("Confirm", "mouse focuses the compiled Confirm button")
        self.wait_edit_diagnostic("edit-committed", TEXT + "#",
                                  "Confirm interaction accepts the valid draft with auto-commit enabled",
                                  after=commits_before)
        self.report["checks"].append({"check": "mouse invokes named Confirm command"})
        self.click_editor(scale)
        self.key(VK_END)
        self.key(VK_BACK)
        self.wait_value(TEXT, "Message keeps its editor session after button focus")
        commits_before = self.log().count("edit-committed=")
        self.key(VK_RETURN)
        self.wait_edit_diagnostic("edit-committed", TEXT,
                                  "Enter restores the accepted Unicode fixture after Confirm",
                                  after=commits_before)
        self.key(VK_END)
        self.type_text("!")
        self.wait_value(TEXT + "!", "editing changes the draft after acceptance")
        cancellations_before = self.log().count("edit-canceled=true")
        self.key(VK_ESCAPE)
        self.wait_value(TEXT, "Escape restores the accepted value")
        self.wait("Escape reports cancellation", lambda:
                  self.log().count("edit-canceled=true") > cancellations_before)
        self.report["checks"].append({"check": "Escape reports canceled editing"})
        self.key(VK_BACK)
        self.wait_value(TEXT[:-1], "Backspace removes final grapheme")

        before = parse_state(self.windows.title(self.hwnd))["frames"]
        target = (round(480 * scale), round(448 * scale))
        self.windows.resize_client(self.hwnd, *target)
        self.wait("native client resize", lambda: self.windows.client_size(self.hwnd) == target)
        self.wait_state("resized form redraw", lambda s: s["frames"] > before)
        if last_edit_value(self.log()) != TEXT[:-1]:
            raise SmokeFailure("Resize changed editor contents")
        self.capture(output, "resized", focused=True)

        commits_before = self.log().count("edit-committed=")
        self.focus_messages(False)
        self.wait_edit_diagnostic("edit-committed", TEXT[:-1],
                                  "focus loss accepts the valid draft",
                                  after=commits_before)
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

        # The compiled example sets MaxLength=64. Fill the field, then verify
        # rejected input neither changes the value nor emits TextChanged.
        self.key(VK_END)
        suffix = "x" * (64 - len(TEXT))
        self.type_text(suffix)
        self.wait_value(TEXT + suffix, "compiled MaxLength accepts exactly 64 graphemes")
        changes_before = self.log().count("edit-value=")
        self.type_text("!")
        time.sleep(0.2)
        self.alive()
        if last_edit_value(self.log()) != TEXT + suffix:
            raise SmokeFailure("MaxLength accepted a 65th grapheme")
        if self.log().count("edit-value=") != changes_before:
            raise SmokeFailure("Rejected input emitted an edit-value event")
        self.report["checks"].append({"check": "MaxLength rejects overflow without TextChanged"})
        for _ in suffix:
            self.key(VK_BACK)
        self.wait_value(TEXT, "editing continues after rejected overflow")

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
    assert last_edit_diagnostic('edit-committed="old"\nedit-value="draft"\n',
                                 "edit-committed") == "old"
    assert last_edit_diagnostic('edit-invalid="Введите текст перед подтверждением."\n',
                                 "edit-invalid") == REQUIRED_MESSAGE
    assert last_edit_diagnostic('focus-name="Message"\nfocus-name="Option"\n',
                                "focus-name") == "Option"
    assert last_checkbox_state("checkbox-state=Option checked=true\n") is True
    assert last_checkbox_state("checkbox-state=Other checked=true\n") is None
    assert last_checkbox_state("checkbox-state=Option checked=true\n"
                               "checkbox-state=Option checked=false\n") is False
    try:
        last_checkbox_state("checkbox-state=Option checked=1\n")
    except SmokeFailure:
        pass
    else:
        raise AssertionError("Invalid checkbox booleans must be rejected")
    try:
        last_edit_value('edit-value=__import__("os")')
    except SmokeFailure:
        pass
    else:
        raise AssertionError("Diagnostic expressions must be rejected")
    width, height = 640, 488
    for dark in (False, True):
        canvas = 32 if dark else 243
        surface = 45 if dark else 255
        border = 85 if dark else 205
        foreground = 245 if dark else 27
        pixels = bytearray(bytes((canvas, canvas, canvas, 255)) * width * height)

        def set_rgb(x, y, color):
            offset = ((height - 1 - y) * width + x) * 4
            pixels[offset:offset + 4] = bytes((*reversed(color), 255))

        set_rgb(280, 122, (surface,) * 3)
        set_rgb(200, 104, (border,) * 3)
        set_rgb(33, 104, (border + 1,) * 3)
        set_rgb(34, 104, (border + 2,) * 3)
        set_rgb(180, 210, (surface,) * 3)
        set_rgb(112, 188, (border,) * 3)
        set_rgb(42, 159, (surface,) * 3)
        for y in range(112, 117):
            for x in range(45, 80):
                set_rgb(x, y, (foreground - x % 6 if dark else foreground + x % 6,) * 3)
        for left, top in ((45, 36), (45, 76), (68, 156), (90, 199)):
            for y in range(top, top + 5):
                for x in range(left, left + 35):
                    set_rgb(x, y, (foreground,) * 3)
        assert capture_checkpoints(pixels, width, height, 1.0, dark, False)["text_pixels"] > 30
        direct = bytes(pixels)
        for y in range(193, 219):
            for x in range(40, 184):
                set_rgb(x, y, (surface,) * 3)
        try:
            capture_checkpoints(pixels, width, height, 1.0, dark, False)
        except SmokeFailure:
            pass
        else:
            raise AssertionError("Missing nested button glyphs must be rejected")
        pixels[:] = direct
        accent = (96, 205, 255) if dark else (0, 95, 184)
        set_rgb(280, 122, (canvas,) * 3)
        set_rgb(42, 159, accent)
        set_rgb(200, 138, accent)
        assert capture_checkpoints(pixels, width, height, 1.0, dark, True,
                                   read_only=True)["checkbox_indicator_rgb"] == accent
        set_rgb(42, 159, (surface,) * 3)
        try:
            capture_checkpoints(pixels, width, height, 1.0, dark, True, read_only=True)
        except SmokeFailure:
            pass
        else:
            raise AssertionError("An unchecked Option must fail the read-only capture")
        pixels[:] = direct
        error = (255, 153, 164) if dark else (196, 43, 28)
        set_rgb(200, 104, error)
        set_rgb(200, 138, error)
        for y in range(373, 378):
            for x in range(45, 80):
                set_rgb(x, y, (foreground,) * 3)
        invalid_fixture = bytes(pixels)
        assert capture_checkpoints(pixels, width, height, 1.0, dark, True,
                                   invalid=True)["validation_message_pixels"] >= 30
        set_rgb(200, 104, (border,) * 3)
        try:
            capture_checkpoints(pixels, width, height, 1.0, dark, True, invalid=True)
        except SmokeFailure:
            pass
        else:
            raise AssertionError("A normal border must not pass as invalid appearance")
        pixels[:] = invalid_fixture
        for y in range(369, 405):
            for x in range(40, 420):
                set_rgb(x, y, (canvas,) * 3)
        try:
            capture_checkpoints(pixels, width, height, 1.0, dark, True, invalid=True)
        except SmokeFailure:
            pass
        else:
            raise AssertionError("Missing validation message glyphs must be rejected")
        pixels[:] = direct
        # The button checker must reject a wrong state palette and missing
        # focus ring independently of otherwise valid editor pixels.
        selected = (35, 75, 105) if dark else (196, 225, 245)
        muted = (170, 170, 170) if dark else (105, 105, 105)
        for y in range(199, 205):
            for x in range(350, 385):
                set_rgb(x, y, muted)
            for x in range(230, 270):
                set_rgb(x, y, (foreground,) * 3)
        set_rgb(308, 214, selected)
        set_rgb(203, 206, accent)
        assert button_capture_checkpoints(pixels, width, height, 1.0, dark, True,
                                          "normal", True)["disabled_glyph_pixels"] > 30
        for damaged in ((308, 214), (203, 206)):
            set_rgb(*damaged, (surface,) * 3)
            try:
                button_capture_checkpoints(pixels, width, height, 1.0, dark, True,
                                           "normal", True)
            except SmokeFailure:
                pass
            else:
                raise AssertionError("Wrong checked/focus palette must fail")
            set_rgb(*damaged, selected if damaged[0] == 308 else accent)
        pixels[:] = direct
        dot = (canvas,) * 3 if dark else (255, 255, 255)
        for left in (32, 160, 288):
            background = accent if left == 32 else (canvas,) * 3 if left == 288 else (surface,) * 3
            set_rgb(left + 17, 248, dot if left == 32 else background)
            set_rgb(left + 17, 242, background)
            set_rgb(left + 3, 248, accent if left == 32 else (canvas,) * 3)
        radio_fixture = bytes(pixels)
        radio_capture_checkpoints(pixels, width, height, 1.0, dark, "AlignLeft", "AlignLeft")
        for x, y, wrong in ((49, 248, accent), (40, 239, accent),
                            (35, 248, (canvas,) * 3), (177, 242, accent)):
            pixels[:] = radio_fixture
            set_rgb(x, y, wrong)
            try:
                radio_capture_checkpoints(pixels, width, height, 1.0, dark, "AlignLeft", "AlignLeft")
            except SmokeFailure:
                pass
            else:
                raise AssertionError("Missing dot/focus, square indicator or duplicate selection must fail")
        pixels[:] = direct
        # Independent masks exercise missing/wrong-color/placeholder/rule
        # rejection without duplicating the vector path implementation.
        for index, left in enumerate((32, 64, 96, 128, 160, 192, 224, 265)):
            color = muted if index == 7 else (foreground,) * 3
            for y in range(281, 285 + index):
                for x in range(left + 5, left + 10):
                    set_rgb(x, y, color)
            set_rgb(left + 12, 290, (120, 120, 120))
            set_rgb(left + 13, 290, (140, 140, 140))
        for x in range(32, 608):
            set_rgb(x, 312, (border,) * 3)
        for y in range(272, 304):
            set_rgb(256, y, (border,) * 3)
        primitive_fixture = bytes(pixels)
        primitives_capture_checkpoints(pixels, width, height, 1.0, dark)
        for damage in ("missing-icon", "enabled-disabled-icon", "opaque-corner",
                       "missing-horizontal-rule", "missing-vertical-rule", "duplicate-icon"):
            pixels[:] = primitive_fixture
            if damage == "missing-icon":
                for y in range(276, 300):
                    for x in range(32, 56):
                        set_rgb(x, y, (canvas,) * 3)
            elif damage == "enabled-disabled-icon":
                set_rgb(270, 282, (foreground,) * 3)
            elif damage == "opaque-corner":
                set_rgb(32, 276, (foreground,) * 3)
            elif damage == "missing-horizontal-rule":
                set_rgb(100, 312, (canvas,) * 3)
            elif damage == "missing-vertical-rule":
                set_rgb(256, 288, (canvas,) * 3)
            else:
                for y in range(276, 300):
                    for x in range(24):
                        source = ((height - 1 - y) * width + 32 + x) * 4
                        target = ((height - 1 - y) * width + 64 + x) * 4
                        pixels[target:target + 4] = pixels[source:source + 4]
            try:
                primitives_capture_checkpoints(pixels, width, height, 1.0, dark)
            except SmokeFailure:
                pass
            else:
                raise AssertionError("Primitive checker accepted " + damage)
        pixels[:] = direct
        # Reproduce the previous accidental linear->sRGB palette encoding.
        for x, y in ((8, 8), (32, 104)):
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
    print("PASS: XAML tree diagnostics, direct-sRGB, nested controls, read-only/invalid palettes, radio states, vector icons/separators and corner/glyph fixtures")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--dark", action="store_true")
    parser.add_argument("--timeout", type=float, default=20)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--controls-only", action="store_true",
                        help="Test buttons/check box/radio groups, focus and resize without Unicode text injection")
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
              "mode": "xaml-controls" if args.controls_only else "xaml-form", "theme": "dark" if args.dark else "light",
              "checks": [], "captures": {}, "backend_environment": os.environ.get("WGPU_BACKEND"),
              "input_method": "Posted child-only Win32 pointer, KEYDOWN/CHAR/KEYUP messages; restored cursor alignment",
              "limits": ["No physical keyboard, native IME, screen reader or external clipboard test",
                         "No native Shift+Tab modifier verification; backward traversal requires separate form model verification",
                         "No multi-monitor DPI or frame-time benchmark", "Captures require separate visual review"]}
    if args.controls_only:
        report["input_method"] = "Posted child-only Win32 pointer and key messages; no CHAR injection; restored cursor alignment"
        report["limits"].append("Controls-only mode does not test text entry, accepted values or Escape rollback")
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

#!/usr/bin/env python3
"""Exercise the compiled XAML Ribbon through its own native Windows child.

The fixture observes opt-in diagnostics and uses acknowledged native cursor
moves, posted child-only mouse/key messages, and bounded client captures. It
does not use the system clipboard or send global keyboard input.
"""
from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes as wt
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import time

from windows_gallery_smoke import capture_client
from windows_popup_smoke import PopupSession, PopupWindows
from windows_retained_smoke import (
    WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, key_message,
)
from windows_smoke import SmokeFailure, WM_CLOSE
from windows_xaml_smoke import last_edit_value, VK_LEFT, VK_END


TITLE_PREFIX = "Документ · Rust UI"
TITLE = re.compile(r"^Документ · Rust UI \| frames=(\d+) glyphs=(\d+) missing=(\d+)$")
VK_HOME = 0x24
VK_F10 = 0x79
VK_ESCAPE = 0x1B
INITIAL_TEXT = "1234567890"
PRESERVED_TEXT = "0123456789"


def parse_render_state(title: str) -> dict | None:
    match = TITLE.fullmatch(title)
    return None if match is None else dict(zip(
        ("frames", "glyphs", "missing"), map(int, match.groups()), strict=True))


def latest_diagnostic(log: str, prefix: str) -> dict | None:
    latest = None
    for line in log.splitlines(keepends=True):
        if not line.startswith(prefix) or not line.endswith("\n"):
            continue
        try:
            latest = json.loads(line.removeprefix(prefix))
        except json.JSONDecodeError as error:
            raise SmokeFailure(f"Invalid complete {prefix.strip()} diagnostic: {error}") from error
        if not isinstance(latest, dict):
            raise SmokeFailure(f"Expected object in {prefix.strip()} diagnostic")
    return latest


def command_invocations(log: str) -> list[str]:
    return [line.removeprefix("command=").rstrip("\r\n")
            for line in log.splitlines(keepends=True)
            if line.startswith("command=") and line.endswith("\n")]


def pin_placements(state: dict) -> list[dict]:
    return [item for item in state["controls"]
            if item.get("command") == "Pin" or item["label"] == "Закрепить"
            or item["label"].endswith(" / Закрепить")]


def item_sizes(ribbon: dict) -> dict[tuple[str, str, str], str]:
    """Read actual adaptive placements; absent items can be collapsed or hidden."""
    result = {}
    for item in ribbon["item_presentations"]:
        identity = (item["tab_id"], item["group_id"], item["key_tip"])
        size = item["size"]
        if any(not isinstance(value, str) or not value for value in identity):
            raise SmokeFailure(f"Invalid Ribbon placement identity: {item}")
        if size not in ("Large", "Small", "IconOnly") or identity in result:
            raise SmokeFailure(f"Invalid or duplicate Ribbon placement: {item}")
        result[identity] = size
    if not ribbon["body_visible"] and result:
        raise SmokeFailure("Hidden Ribbon body exposes item placements")
    return result


def control_geometry(state: dict) -> list[tuple]:
    """Compare semantic names and layout without transient focus/check state."""
    return sorted((item["role"], item["label"], *item["bounds"])
                  for item in state["controls"] if item["role"] != "MenuItem")


def verify_geometry(before: list[tuple], after: list[tuple]) -> None:
    if len(before) != len(after):
        raise SmokeFailure(f"KeyTips changed control count: {len(before)} -> {len(after)}")
    for old, new in zip(before, after, strict=True):
        if old[:2] != new[:2] or any(abs(a - b) > 0.001 for a, b in zip(old[2:], new[2:], strict=True)):
            raise SmokeFailure(f"KeyTips changed a control name or geometry: {old} -> {new}")


def split_primary(controls: list[dict], arrow: dict) -> dict:
    """Resolve the Split sibling in either large or compact presentation."""
    x, y, width, height = arrow["bounds"]

    def adjacent(item):
        px, py, pw, ph = item["bounds"]
        vertical = abs(px - x) < 0.001 and abs(pw - width) < 0.001 and abs(py + ph - y) < 0.001
        horizontal = abs(py - y) < 0.001 and abs(ph - height) < 0.001 and abs(px + pw - x) < 0.001
        same_parent = ("parent_id" not in arrow
                       or item.get("parent_id") == arrow["parent_id"])
        return same_parent and (vertical or horizontal)

    matches = [item for item in controls if item["role"] in ("Button", "ToggleButton")
               and item["command"] is not None and arrow["label"] == item["label"] + " menu"
               and adjacent(item)]
    if len(matches) != 1:
        raise SmokeFailure(f"Expected the adjacent Split primary, found {matches} for {arrow}")
    return matches[0]


def verify_shared_pin(state: dict, expected: bool) -> None:
    placements = pin_placements(state)
    if state["pin_checked"] is not expected or not placements:
        raise SmokeFailure(f"Missing or incorrect shared Pin state: {state}")
    for item in placements:
        if item["checked"] is not expected or item["role"] not in ("ToggleButton", "MenuItem"):
            raise SmokeFailure(f"Pin placement disagrees with command state: {item}")


def visible_rectangle(bounds, clip, viewport) -> list[float]:
    """Intersect diagnostic bounds with the actual client and ancestor clip."""
    rectangles = [bounds, [0, 0, *viewport]] + ([] if clip is None else [clip])
    if any(len(rect) != 4 or any(not math.isfinite(value) for value in rect)
           or rect[2] < 0 or rect[3] < 0 for rect in rectangles):
        raise SmokeFailure("Invalid diagnostic rectangle")
    x, y = max(rect[0] for rect in rectangles), max(rect[1] for rect in rectangles)
    right = min(rect[0] + rect[2] for rect in rectangles)
    bottom = min(rect[1] + rect[3] for rect in rectangles)
    if right <= x or bottom <= y:
        raise SmokeFailure(f"Control has no visible client area: {bounds}, clip={clip}")
    return [x, y, right - x, bottom - y]


def capture_ink(bitmap: bytes, width: int, height: int, scale: float,
                regions: dict[str, list[float]], dark: bool) -> dict:
    if (bitmap[:2] != b"BM" or not math.isfinite(scale) or scale <= 0
            or struct.unpack_from("<ii", bitmap, 18) != (width, height)):
        raise SmokeFailure("Unexpected native capture encoding or dimensions")
    offset = struct.unpack_from("<I", bitmap, 10)[0]
    if len(bitmap) != offset + width * height * 4:
        raise SmokeFailure("Truncated native client capture")
    foreground = (245, 245, 245) if dark else (27, 27, 27)
    muted = (170, 170, 170) if dark else (105, 105, 105)
    accent = (96, 205, 255) if dark else (0, 95, 184)
    canvas = (32, 32, 32) if dark else (243, 243, 243)

    def linear(channel):
        value = channel / 255
        return value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4

    def srgb(value):
        return round(255 * (12.92 * value if value <= 0.0031308 else 1.055 * value ** (1 / 2.4) - 0.055))

    # Coverage is blended in linear color space. Counting only near-solid
    # accent pixels discards legitimate antialiased glyph edges, especially
    # when the light accent's red channel starts at zero. Require >=25% ink.
    accent_shades = tuple(tuple(srgb(linear(fg) * (step / 20) + linear(bg) * (1 - step / 20))
                                for fg, bg in zip(accent, canvas, strict=True))
                          for step in range(5, 21))
    samples = {}
    for name, bounds in regions.items():
        x, y, w, h = visible_rectangle(bounds, None, (width / scale, height / scale))
        if any(abs(a - b) > 1 / scale for a, b in zip((x, y, w, h), bounds, strict=True)):
            raise SmokeFailure(f"Capture region exceeds the native client: {name}: {bounds}")
        ink = 0
        shades = set()
        is_tab = name.endswith("_tab")
        palette = (foreground, muted, *accent_shades) if is_tab else (foreground, muted)
        # Tab captions can use primary text or an accent. Exclude the entire
        # underline so a colored rule alone cannot pass the text-coverage check.
        bottom_inset = 4 if is_tab else 2
        for py in range(max(0, math.ceil((y + 2) * scale)),
                        min(height, math.floor((y + h - bottom_inset) * scale))):
            for px in range(max(0, math.ceil((x + 2) * scale)),
                            min(width, math.floor((x + w - 2) * scale))):
                index = offset + ((height - 1 - py) * width + px) * 4
                b, g, r = bitmap[index:index + 3]
                color = (r, g, b)
                if any(all(abs(a - b) <= 12 for a, b in zip(color, candidate, strict=True))
                       for candidate in palette):
                    ink += 1
                    shades.add(color)
        if ink < 20 or len(shades) < 3:
            raise SmokeFailure(f"No credible text/vector coverage in {name}: {ink} pixels, {len(shades)} shades")
        samples[name] = {"bounds": bounds, "ink_pixels": ink, "ink_shades": len(shades),
                         "accent_caption": is_tab, "bottom_inset": bottom_inset}
    return samples


class MonitorInfo(ctypes.Structure):
    _fields_ = [("size", wt.DWORD), ("monitor", wt.RECT), ("work", wt.RECT), ("flags", wt.DWORD)]


class RibbonWindows(PopupWindows):
    def place_child(self, hwnd, pid):
        if not self.owns(hwnd, pid):
            raise SmokeFailure("Window positioning requires the owned Ribbon child")
        self.user.MonitorFromWindow.argtypes = [wt.HWND, wt.DWORD]
        self.user.MonitorFromWindow.restype = wt.HANDLE
        self.user.GetMonitorInfoW.argtypes = [wt.HANDLE, ctypes.POINTER(MonitorInfo)]
        self.user.GetMonitorInfoW.restype = wt.BOOL
        monitor = self.user.MonitorFromWindow(hwnd, 2)
        info = MonitorInfo()
        info.size = ctypes.sizeof(info)
        self.require(self.user.GetMonitorInfoW(monitor, ctypes.byref(info)), "GetMonitorInfoW")
        # Keep the command area and editor visible even when Windows cascades a
        # tall child near the desktop's bottom edge. No other window is moved.
        self.require(self.user.SetWindowPos(hwnd, None, info.work.left + 8, info.work.top + 8,
                                           0, 0, 0x0001 | 0x0004 | 0x0010 | 0x4000), "SetWindowPos")

    def find_window(self, pid):
        found = []

        @self.enum_callback
        def collect(hwnd, _parameter):
            if (self.owns(hwnd, pid) and self.user.IsWindowVisible(hwnd)
                    and self.title(hwnd).startswith(TITLE_PREFIX)):
                found.append(hwnd)
            return True

        self.require(self.user.EnumWindows(collect, 0), "EnumWindows")
        return found[0] if found else None

    def capture_state(self, hwnd):
        state = parse_render_state(self.title(hwnd))
        return None if state is None else {
            **state, "page": "xaml-ribbon", "theme": "dark" if self.dark else "light"}


def ribbon_capture_worker(hwnd, pid, destination, result_queue, dark):
    try:
        result_queue.put(RibbonWindows(dark).screenshot(hwnd, pid, destination))
    except Exception as error:
        result_queue.put({"status": "unavailable", "reason": str(error)})


class RibbonSession(PopupSession):
    def wait_state(self, description, predicate):
        def matches():
            state = parse_render_state(self.windows.title(self.hwnd))
            if state is not None and state["missing"]:
                raise SmokeFailure(f"Missing XAML Ribbon glyphs: {state}")
            return state if state is not None and predicate(state) else None
        state = self.wait(description, matches)
        self.report["checks"].append({"check": description, "render_state": state})
        return state

    def ribbon_state(self):
        state = latest_diagnostic(self.log(), "ribbon-state ")
        if state is None:
            raise SmokeFailure("No Ribbon diagnostic")
        return state

    def ribbon_snapshot(self, description, predicate=lambda _: True):
        def matches():
            state = latest_diagnostic(self.log(), "ribbon-state ")
            return state if state is not None and predicate(state) else None
        state = self.wait(description, matches)
        self.report["checks"].append({"check": description, "ribbon_state": state})
        return state

    def invocations(self):
        return command_invocations(self.log())

    def commands(self):
        return len(self.invocations())

    def viewport(self):
        return tuple(value / self.scale for value in self.windows.client_size(self.hwnd))

    def control(self, predicate):
        matches = [item for item in self.state()["controls"] if predicate(item)]
        if len(matches) != 1:
            raise SmokeFailure(f"Expected one diagnostic control, found {matches}")
        return matches[0]

    def in_ribbon(self, item):
        x, y, width, height = self.ribbon_state()["bounds"]
        ix, iy, iw, ih = item["bounds"]
        return (x <= ix < x + width and y <= iy < y + height
                and iw > 0 and ih > 0 and item["role"] != "MenuItem")

    def editor(self):
        return self.control(lambda item: item["role"] == "TextBox" and item["label"] == "Текст")

    def tab(self, label):
        return self.control(lambda item: item["role"] == "Tab" and item["label"] == label)

    def pin(self, qat=False):
        matches = [item for item in pin_placements(self.state()) if self.in_ribbon(item)]
        if not matches:
            raise SmokeFailure("No visible Pin placement")
        ordered = sorted(matches, key=lambda item: (item["bounds"][1], item["bounds"][0]))
        if qat:
            return ordered[0]
        if len(ordered) < 2:
            raise SmokeFailure(f"No body Pin placement: {matches}")
        return ordered[1]

    def menu_leaf(self, command):
        matches = [item for item in self.state()["controls"]
                   if item["role"] == "MenuItem" and item.get("command") == command
                   and not item["has_popup"] and item["enabled"]]
        if not matches:
            raise SmokeFailure(f"No enabled visible menu leaf for {command}")
        # Multiple leaves may deliberately invoke the same registry command.
        return sorted(matches, key=lambda item: (item["bounds"][1], item["bounds"][0]))[0]

    def click_control(self, x, y, scale):
        self.focus_messages(True)
        self.pointer(WM_MOUSEMOVE, x, y, scale)
        self.pointer(WM_LBUTTONDOWN, x, y, scale, held=True)
        self.pointer(WM_LBUTTONUP, x, y, scale)
        # Move to the outer canvas; no hard-coded control coordinates are used.
        self.pointer(WM_MOUSEMOVE, 8, 8, scale)

    def click_item(self, item, *, allow_disabled=False):
        if not item["enabled"] and not allow_disabled:
            raise SmokeFailure(f"Target is unexpectedly disabled: {item}")
        x, y, width, height = visible_rectangle(item["bounds"], item["clip"], self.viewport())
        self.click_control(round(x + width / 2), round(y + height / 2), self.scale)

    def exact_command(self, before, command, description, pin=None):
        self.wait(description, lambda: self.commands() > before)
        time.sleep(0.15)
        self.alive()
        new_commands = self.invocations()[before:]
        if new_commands != [command]:
            raise SmokeFailure(f"{description}: expected one {command}, got {new_commands}")
        state = self.state()
        if pin is not None:
            verify_shared_pin(state, pin)
        self.report["checks"].append({"check": description + " exactly once",
                                      "command": command, "pin": state["pin_checked"]})

    def activate_pin(self, item, description):
        before, expected = self.commands(), not self.state()["pin_checked"]
        self.click_item(item)
        self.snapshot(description, lambda state: state["pin_checked"] is expected)
        self.exact_command(before, "Pin", description, pin=expected)

    def letter_keytip(self, character, description, predicate):
        """Allow the child's TranslateMessage to generate CHAR before KEYUP.

        Posting a second CHAR would risk duplicate committed text. This checks
        the actual child layout's logical character and reports a non-Latin
        layout explicitly instead of changing the user's keyboard layout.
        """
        before = len(self.log())
        virtual_key = ord(character.upper())
        scan = self.windows.user.MapVirtualKeyW(virtual_key, 4)
        self.windows.post(self.hwnd, WM_KEYDOWN, virtual_key, key_message(scan, False))

        def delivered():
            fragment = self.log()[before:]
            match = re.search(r'edit-input=Input\(KeyDown \{ key: Character\(("(?:[^"\\]|\\.)*")\)', fragment)
            if match and json.loads(match[1]).upper() != character.upper():
                raise SmokeFailure(f"KeyTip {character} produced {match[1]} in the child keyboard layout; use a Latin layout")
            return predicate()

        try:
            self.wait(description, delivered)
        finally:
            self.windows.post(self.hwnd, WM_KEYUP, virtual_key, key_message(scan, True))
        self.report["checks"].append({"check": description, "native_character": character})

    def type_text(self, text):
        # Use the child's ordinary TranslateMessage path. Separate posted CHAR
        # messages can arrive after winit has already assembled a KeyEvent.
        # Digits make the editing/caret probe independent of Latin/Cyrillic layout.
        if not text or any(character not in "0123456789" for character in text):
            raise SmokeFailure("The native editing probe accepts digit keys only")
        for character in text:
            before = len(self.log())
            self.letter_keytip(character, "native editor key " + character,
                               lambda character=character, before=before:
                               'edit-input=Text(' + json.dumps(character) + ')' in self.log()[before:])

    def capture_regions(self, output, name, regions):
        self.wait_state("live frame before " + name, lambda state: state["frames"] > 0)
        time.sleep(0.15)
        result = capture_client(self.hwnd, self.process.pid, output / f"{name}.bmp", self.timeout,
                                worker_target=ribbon_capture_worker, worker_args=(self.windows.dark,))
        self.report["captures"][name] = result
        if result["status"] != "available":
            raise SmokeFailure(f"{name} capture unavailable: {result}")
        bitmap = Path(result["bmp"]).read_bytes()
        result["regions"] = capture_ink(bitmap, result["width"], result["height"],
                                         self.scale, regions, self.windows.dark)
        result["pixel_sha256"] = hashlib.sha256(bitmap).hexdigest()

    def capture_ribbon(self, output, name):
        ribbon = self.ribbon_state()
        tabs = [item for item in self.state()["controls"] if item["role"] == "Tab"]
        self.capture_regions(output, name, {
            "ribbon": ribbon["bounds"],
            "first_tab": visible_rectangle(tabs[0]["bounds"], tabs[0]["clip"], self.viewport()),
        })

    def capture_menu(self, output, name):
        menus = self.state()["menus"]
        if not menus:
            raise SmokeFailure("Expected an open Ribbon menu for capture")
        self.capture_regions(output, name, {f"menu_{index}": bounds for index, bounds in enumerate(menus)})

    def resize(self, width):
        target = (round(width * self.scale), round(720 * self.scale))
        self.windows.resize_client(self.hwnd, *target)
        self.wait("native Ribbon client resize", lambda: self.windows.client_size(self.hwnd) == target)
        self.ribbon_snapshot(f"Ribbon reflows at {width}px", lambda state:
                             abs(state["bounds"][2] - (width - 32)) < 1)
        # Await a snapshot from the new geometry, not the previous frame.
        self.snapshot("controls match resized Ribbon", lambda state: any(
            item["role"] == "TextBox" and abs(item["bounds"][2] - (width - 64)) < 1
            for item in state["controls"]))

    def adaptive_resize(self):
        """Discover real stages without assigning a layout mode to 360/180 px."""
        seen = {}
        collapsed = overflow = None
        previous_width = 1040
        for width in range(1008, 79, -16):
            self.resize(width)
            ribbon = self.ribbon_state()
            for identity, size in item_sizes(ribbon).items():
                seen.setdefault(identity, set()).add(size)
            self.snapshot("adaptive resize preserves editor focus", lambda state: state["focused"] == "Текст")
            self.assert_editor_text(PRESERVED_TEXT, "adaptive resize preserves editor draft")
            if "Collapsed" in ribbon["presentations"] and collapsed is None:
                collapsed = width
            if "Overflow" in ribbon["presentations"]:
                overflow = width
                if collapsed is None:
                    # IconOnly groups can make the remaining collapse interval
                    # narrower than the coarse scan step. Inspect that final
                    # interval before concluding that the stage is missing.
                    for boundary_width in range(width + 1, previous_width):
                        self.resize(boundary_width)
                        boundary = self.ribbon_state()
                        for identity, size in item_sizes(boundary).items():
                            seen.setdefault(identity, set()).add(size)
                        self.snapshot("boundary resize preserves editor focus", lambda state: state["focused"] == "Текст")
                        self.assert_editor_text(PRESERVED_TEXT, "boundary resize preserves editor draft")
                        if "Collapsed" in boundary["presentations"]:
                            collapsed = boundary_width
                            break
                break
            previous_width = width
        transitions = [identity for identity, sizes in seen.items() if sizes == {"Large", "Small", "IconOnly"}]
        if not transitions or overflow is None:
            raise SmokeFailure(f"Incomplete adaptive sequence: sizes={seen}, collapsed={collapsed}, overflow={overflow}")
        self.report["adaptive_layout"] = {"items": [{"identity": list(identity), "sizes": sorted(sizes)}
                                                    for identity, sizes in seen.items()],
                                           "collapsed_width": collapsed,
                                           "overflow_width": overflow}
        if collapsed is None:
            self.report["adaptive_layout"]["collapsed_limit"] = (
                "No separate collapsed group observed; the final expanded-to-overflow interval was refined at 1px. "
                "IconOnly columns can already be narrower than collapsed headers. Overflow command coverage remains required.")
        self.resize(1040)
        restored = self.ribbon_state()
        if "Collapsed" in restored["presentations"] or "Overflow" in restored["presentations"]:
            raise SmokeFailure("Wide Ribbon failed to restore expanded groups")
        return overflow

    def assert_editor_text(self, text, description):
        if last_edit_value(self.log()) != text:
            raise SmokeFailure(f"{description}: editor draft changed unexpectedly")
        self.report["checks"].append({"check": description, "edit_value": text})

    def run(self, output):
        self.stdout_path = output / "stdout.log"
        self.hwnd = self.wait("visible XAML Ribbon child", lambda: self.windows.find_window(self.process.pid))
        self.report["window_handle"] = self.hwnd
        self.windows.place_child(self.hwnd, self.process.pid)
        self.wait_state("first XAML Ribbon GPU frame", lambda state: state["frames"] > 0 and state["glyphs"] > 0)
        dpi = self.windows.user.GetDpiForWindow(self.hwnd)
        if dpi <= 0:
            raise SmokeFailure("Invalid child DPI")
        self.report["dpi"] = dpi
        self.scale = dpi / 96.0
        self.windows.topmost_without_activation(self.hwnd, True)
        self.promoted = True
        self.focus_messages(True)
        self.wait("child receives focus notification", lambda: "Input(WindowFocused(true))" in self.log())
        time.sleep(0.2)
        self.pointer(WM_MOUSEMOVE, 8, 8, self.scale)
        initial = self.snapshot("initial shared Pin and closed popup", lambda state:
                                state["pin_checked"] is False and not state["popup_open"])
        verify_shared_pin(initial, False)
        if self.report["minimized"]:
            self.run_minimized(output)
            return
        self.ribbon_snapshot("Home tab has full groups", lambda state:
                             state["active_tab"] == "Home" and "Collapsed" not in state["presentations"]
                             and "Overflow" not in state["presentations"])
        self.capture_ribbon(output, "main-ribbon")

        self.click_item(self.editor())
        self.snapshot("editor receives pointer focus", lambda state: state["focused"] == "Текст")
        self.type_text(INITIAL_TEXT)
        self.wait_value(INITIAL_TEXT, "initial editor draft")
        self.activate_pin(self.pin(), "Ribbon body Pin")
        self.activate_pin(self.pin(qat=True), "QAT shares the same Pin state")

        self.click_item(self.tab("Вид"))
        self.ribbon_snapshot("pointer selects View tab", lambda state: state["active_tab"] == "View")
        self.snapshot("View commands are mounted", lambda state: any(item["label"] == "Тёмная" for item in state["controls"]))
        self.capture_ribbon(output, "view-tab")
        for key, tab, label in [(VK_LEFT, "Home", "Главная"), (VK_END, "View", "Вид"),
                                (VK_HOME, "Home", "Главная")]:
            self.key(key)
            self.ribbon_snapshot("tab navigation selects " + tab, lambda state, tab=tab: state["active_tab"] == tab)
            self.snapshot("selected tab keeps keyboard focus", lambda state, label=label: any(
                item["role"] == "Tab" and item["label"] == label and item["focused"] for item in state["controls"]))

        disabled = self.control(lambda item: item["label"] == "Недоступно" and self.in_ribbon(item))
        if disabled["enabled"]:
            raise SmokeFailure("Disabled Ribbon command is enabled")
        before = self.commands()
        self.click_item(disabled, allow_disabled=True)
        self.ensure_no_command(before, "Disabled Ribbon command")

        self.click_item(self.editor())
        self.snapshot("editor focus before KeyTips", lambda state: state["focused"] == "Текст")
        before_keytips = control_geometry(self.state())
        before = self.commands()
        self.key(VK_F10)
        self.ribbon_snapshot("F10 displays independent tab and QAT KeyTips", lambda state:
                             state["key_tips"] and {"H", "V", "1", "2", "3"}.issubset(state["key_tip_labels"]))
        verify_geometry(before_keytips, control_geometry(self.state()))
        self.report["checks"].append({"check": "KeyTip badges preserve names and layout"})
        self.capture_ribbon(output, "key-tips")
        if self.report["numeric_keytips"]:
            self.letter_keytip("2", "QAT 2 invokes the Pin command", lambda: self.commands() > before)
        else:
            self.letter_keytip("H", "H opens Home command KeyTips", lambda:
                               "P" in self.ribbon_state()["key_tip_labels"])
            verify_geometry(before_keytips, control_geometry(self.state()))
            self.letter_keytip("P", "P invokes the Pin command", lambda: self.commands() > before)
        self.ribbon_snapshot("command KeyTip hides the hints", lambda state: not state["key_tips"])
        self.snapshot("KeyTip command returns focus to editor", lambda state: state["focused"] == "Текст")
        self.exact_command(before, "Pin", self.report["key_tip_sequence"], pin=True)
        self.assert_editor_text(INITIAL_TEXT, "KeyTip letters do not enter the editor")
        verify_geometry(before_keytips, control_geometry(self.state()))

        arrow = self.control(lambda item:
                             item["has_popup"] and item["label"] == "Подтвердить menu"
                             and item["command"] is None and self.in_ribbon(item))
        primary = split_primary(self.state()["controls"], arrow)
        before = self.commands()
        self.click_item(primary)
        self.exact_command(before, "Confirm", "Split primary")
        self.snapshot("Split primary does not open a menu", lambda state: not state["popup_open"])
        before = self.commands()
        self.click_item(arrow)
        self.snapshot("Split arrow opens the secondary menu", lambda state: len(state["menus"]) == 1)
        self.ensure_no_command(before, "Split arrow")
        self.capture_menu(output, "split-menu")
        self.click_item(self.control(lambda item: item["role"] == "MenuItem" and item["label"] == "Очистить"))
        self.wait_value("", "Split secondary clears the editor")
        self.exact_command(before, "Clear", "Split secondary")
        self.snapshot("Split secondary closes the menu", lambda state: not state["popup_open"])

        self.click_item(self.editor())
        self.type_text(PRESERVED_TEXT)
        self.wait_value(PRESERVED_TEXT, "draft before adaptive reflow")
        left_before = self.log().count("key: ArrowLeft")
        self.key(VK_LEFT)
        self.key(VK_LEFT)
        self.wait("two caret-left keys are delivered", lambda: self.log().count("key: ArrowLeft") >= left_before + 4)
        overflow_width = self.adaptive_resize()
        self.type_text("5")
        preserved_with_caret = PRESERVED_TEXT[:-2] + "5" + PRESERVED_TEXT[-2:]
        self.wait_value(preserved_with_caret, "caret survives collapse, overflow and expansion")

        self.resize(overflow_width)
        before = self.commands()
        self.click_item(self.control(lambda item: item["label"] == "All commands" and self.in_ribbon(item)))
        self.snapshot("All commands opens inside the narrow client", lambda state: len(state["menus"]) == 1)
        self.ensure_no_command(before, "All commands opening")
        menu_commands = {item.get("command") for item in self.state()["controls"] if item["role"] == "MenuItem"}
        required_commands = {"Confirm", "Clear", "Pin", "Disabled", "Actions"}
        if not required_commands.issubset(menu_commands):
            raise SmokeFailure(f"Overflow lost Home command owners: {sorted(required_commands - menu_commands)}")
        self.capture_menu(output, "narrow-menu")
        menu_pin = self.menu_leaf("Pin")
        self.activate_pin(menu_pin, "Overflow shares the same Pin state")
        self.snapshot("Overflow activation closes the menu", lambda state: not state["popup_open"])
        self.resize(1040)
        self.assert_editor_text(preserved_with_caret, "menu activation preserves the editor document")
        verify_shared_pin(self.state(), False)
        self.nested_dropdown(output)
        self.capture_ribbon(output, "restored-ribbon")
        self.finish()

    def nested_dropdown(self, output):
        before = self.commands()
        owner = self.control(lambda item: item.get("command") == "Actions"
                             and item["has_popup"] and self.in_ribbon(item))
        self.click_item(owner)
        self.snapshot("DropDown opens its root menu", lambda state: len(state["menus"]) == 1)
        self.ensure_no_command(before, "DropDown opening")
        submenu = self.control(lambda item: item["role"] == "MenuItem" and item["label"] == "Документ"
                               and item["has_popup"])
        self.click_item(submenu)
        self.snapshot("Ribbon submenu opens", lambda state: len(state["menus"]) == 2)
        self.ensure_no_command(before, "Submenu opening")
        self.capture_menu(output, "nested-dropdown")
        self.key(VK_ESCAPE)
        self.snapshot("Escape closes only the nested menu", lambda state: len(state["menus"]) == 1)
        self.ensure_no_command(before, "Nested Escape")
        submenu = self.control(lambda item: item["role"] == "MenuItem" and item["label"] == "Документ"
                               and item["has_popup"])
        self.click_item(submenu)
        self.snapshot("Ribbon submenu reopens", lambda state: len(state["menus"]) == 2)
        self.activate_pin(self.menu_leaf("Pin"), "Nested Ribbon leaf")
        self.snapshot("Nested leaf closes the complete menu", lambda state: not state["popup_open"])

    def run_minimized(self, output):
        initial = self.ribbon_snapshot("initial minimized header only", lambda state:
                                       state["minimized"] and not state["body_visible"])
        if item_sizes(initial):
            raise SmokeFailure("Minimized Ribbon still exposes body items")
        self.capture_ribbon(output, "minimized-header")
        self.click_item(self.editor())
        self.snapshot("editor receives focus while Ribbon is minimized", lambda state: state["focused"] == "Текст")
        self.type_text(INITIAL_TEXT)
        self.wait_value(INITIAL_TEXT, "minimized editor draft")
        self.click_item(self.tab("Главная"))
        self.ribbon_snapshot("tab transiently reveals minimized body", lambda state:
                             state["minimized"] and state["body_visible"])
        self.capture_ribbon(output, "minimized-revealed")
        self.key(VK_ESCAPE)
        self.ribbon_snapshot("Escape hides transient body", lambda state: not state["body_visible"])
        self.snapshot("Escape restores editor focus", lambda state: state["focused"] == "Текст")
        self.assert_editor_text(INITIAL_TEXT, "minimized reveal preserves editor draft")
        before = self.commands()
        self.key(VK_F10)
        self.ribbon_snapshot("minimized QAT KeyTips", lambda state:
                             state["key_tips"] and "2" in state["key_tip_labels"])
        self.letter_keytip("2", "minimized QAT toggles Pin", lambda: self.commands() > before)
        self.exact_command(before, "Pin", "Minimized QAT KeyTip", pin=True)
        self.ribbon_snapshot("QAT activation leaves body hidden", lambda state:
                             state["minimized"] and not state["body_visible"] and not state["key_tips"])
        self.snapshot("QAT returns to editor", lambda state: state["focused"] == "Текст")
        self.click_item(self.tab("Главная"))
        self.ribbon_snapshot("reveal before nested menu", lambda state: state["body_visible"])
        self.nested_dropdown(output)
        self.ribbon_snapshot("nested leaf closes transient body", lambda state:
                             state["minimized"] and not state["body_visible"])
        self.snapshot("nested leaf restores editor", lambda state: state["focused"] == "Текст")
        for width in (360, 180, 1040):
            self.resize(width)
            ribbon = self.ribbon_state()
            if not ribbon["minimized"] or ribbon["body_visible"] or item_sizes(ribbon):
                raise SmokeFailure(f"Resize unexpectedly reveals minimized body: {ribbon}")
            self.assert_editor_text(INITIAL_TEXT, "minimized resize preserves editor draft")
        verify_shared_pin(self.state(), False)
        self.capture_ribbon(output, "minimized-restored")
        self.finish()

    def finish(self):
        self.report["final_render_state"] = self.wait_state("final frame has no missing glyphs", lambda state: state["missing"] == 0)
        self.report["command_invocations"] = self.invocations()
        self.report["final_edit_value"] = last_edit_value(self.log())
        self.windows.post(self.hwnd, WM_CLOSE)
        code = self.process.wait(timeout=self.timeout)
        if code:
            raise SmokeFailure(f"XAML Ribbon child exited with status {code}")
        self.report["checks"].append({"check": "native close", "exit_code": code})


def self_test():
    assert parse_render_state("Документ · Rust UI | frames=12 glyphs=340 missing=0") == {
        "frames": 12, "glyphs": 340, "missing": 0}
    assert parse_render_state("Форма · Rust UI | frames=12 glyphs=340 missing=0") is None
    assert latest_diagnostic('ribbon-state {"active_tab":"Home"}\n', "ribbon-state ") == {"active_tab": "Home"}
    assert latest_diagnostic('ribbon-state {"active_tab":"Home"}\nribbon-state {', "ribbon-state ") == {"active_tab": "Home"}
    for malformed in ('ribbon-state nope\n', 'ribbon-state []\n'):
        try:
            latest_diagnostic(malformed, "ribbon-state ")
        except SmokeFailure:
            pass
        else:
            raise AssertionError("Malformed complete diagnostic was accepted")
    assert command_invocations("command=Pin\r\nother-command=Clear\ncommand=Confirm\ncommand=Pin") == ["Pin", "Confirm"]
    assert visible_rectangle([5, 5, 20, 20], [10, 8, 40, 30], (24, 25)) == [10, 8, 14, 17]
    try:
        visible_rectangle([30, 30, 10, 10], None, (20, 20))
    except SmokeFailure:
        pass
    else:
        raise AssertionError("Fully clipped control was accepted")
    state = {"pin_checked": True, "controls": [
        {"label": "Закрепить", "role": "ToggleButton", "checked": True},
        {"label": "Редактирование / Закрепить", "role": "MenuItem", "checked": True},
    ]}
    verify_shared_pin(state, True)
    state["controls"][1]["checked"] = False
    try:
        verify_shared_pin(state, True)
    except SmokeFailure:
        pass
    else:
        raise AssertionError("Disagreeing shared command state was accepted")
    geometry_state = {"controls": [
        {"role": "Tab", "label": "Главная", "bounds": [16, 48, 84, 34], "focused": False},
        {"role": "ToggleButton", "label": "Закрепить", "bounds": [48, 16, 32, 32], "checked": False},
    ]}
    baseline = control_geometry(geometry_state)
    geometry_state["controls"][0]["focused"] = True
    geometry_state["controls"][1]["checked"] = True
    verify_geometry(baseline, control_geometry(geometry_state))
    for change in ({"label": "Главная [H]"}, {"bounds": [16, 48, 110, 34]}):
        altered = {"controls": [{**geometry_state["controls"][0], **change}, geometry_state["controls"][1]]}
        try:
            verify_geometry(baseline, control_geometry(altered))
        except SmokeFailure:
            pass
        else:
            raise AssertionError("Changed KeyTip caption/layout was accepted")
    arrow = {"role": "Button", "label": "Подтвердить menu", "command": None,
             "has_popup": True, "bounds": [20, 120, 84, 40]}
    primary = {"role": "Button", "label": "Подтвердить", "command": "Confirm",
               "has_popup": True, "bounds": [20, 80, 84, 40]}
    qat = {**primary, "bounds": [20, 20, 32, 32]}
    unrelated = {**primary, "bounds": [200, 80, 84, 80]}
    assert split_primary([qat, unrelated, primary, arrow], arrow) is primary
    try:
        split_primary([qat, unrelated, arrow], arrow)
    except SmokeFailure:
        pass
    else:
        raise AssertionError("A nonadjacent button was accepted as the Split primary")
    compact_primary = {**primary, "id": "primary", "parent_id": "split", "bounds": [20, 80, 80, 28]}
    compact_arrow = {**arrow, "id": "arrow", "parent_id": "split", "bounds": [100, 80, 20, 28]}
    same_position_other_parent = {**compact_primary, "parent_id": "unrelated"}
    assert split_primary([same_position_other_parent, compact_primary, compact_arrow], compact_arrow) is compact_primary
    try:
        split_primary([same_position_other_parent, compact_arrow], compact_arrow)
    except SmokeFailure:
        pass
    else:
        raise AssertionError("A neighboring unrelated button was accepted as a Split sibling")
    placement = {"tab_id": "Home", "group_id": "Actions", "key_tip": "A", "size": "IconOnly"}
    assert item_sizes({"body_visible": True, "item_presentations": [placement]}) == {
        ("Home", "Actions", "A"): "IconOnly"}
    assert not item_sizes({"body_visible": False, "item_presentations": []})
    for invalid in (
        {"body_visible": False, "item_presentations": [placement]},
        {"body_visible": True, "item_presentations": [placement, placement]},
        {"body_visible": True, "item_presentations": [{**placement, "size": "Tiny"}]},
    ):
        try:
            item_sizes(invalid)
        except SmokeFailure:
            pass
        else:
            raise AssertionError("Invalid adaptive placement diagnostics were accepted")
    for dark in (False, True):
        width, height = 60, 40
        background = (32, 32, 32) if dark else (243, 243, 243)
        accent = (96, 205, 255) if dark else (0, 95, 184)
        pixels = bytearray(bytes((*reversed(background), 255)) * width * height)
        header = struct.pack("<2sIHHI", b"BM", 54 + len(pixels), 0, 0, 54)
        header += struct.pack("<IiiHHIIiiII", 40, width, height, 1, 32, 0, len(pixels), 0, 0, 0, 0)

        def accent_pixels(top, bottom):
            for y in range(top, bottom):
                for x in range(10, 40):
                    offset = ((height - 1 - y) * width + x) * 4
                    color = tuple(min(255, value + x % 4) for value in accent)
                    pixels[offset:offset + 4] = bytes((*reversed(color), 255))

        accent_pixels(height - 3, height)
        try:
            capture_ink(header + pixels, width, height, 1.0, {"first_tab": [0, 0, width, height]}, dark)
        except SmokeFailure:
            pass
        else:
            raise AssertionError("An accent underline alone passed the tab caption check")
        accent_pixels(12, 18)
        evidence = capture_ink(header + pixels, width, height, 1.0,
                               {"first_tab": [0, 0, width, height]}, dark)
        assert evidence["first_tab"]["ink_pixels"] >= 20
        assert evidence["first_tab"]["ink_shades"] >= 3
    print("PASS: Ribbon diagnostics, adaptive sizes, stable KeyTips, Split siblings, shared state and accent tab ink")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--dark", action="store_true")
    parser.add_argument("--ribbon-minimized", action="store_true", help="Exercise the initially minimized Ribbon")
    parser.add_argument("--timeout", type=float, default=20)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--numeric-keytips", action="store_true",
                        help="Use F10/2 in a non-Latin keyboard layout; letter KeyTips remain untested natively")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if args.exe is None or args.output_dir is None:
        parser.error("--exe and --output-dir are required")
    if sys.platform != "win32" or not math.isfinite(args.timeout) or not 0 < args.timeout <= 60:
        parser.error("Windows and a finite timeout in (0, 60] are required")
    executable = args.exe.resolve()
    if not executable.is_file():
        parser.error("--exe must name a built xaml-form executable")
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    report = {
        "status": "failed", "started_utc": datetime.now(timezone.utc).isoformat(),
        "executable": str(executable), "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
        "backend_environment": os.environ.get("WGPU_BACKEND"), "theme": "dark" if args.dark else "light",
        "numeric_keytips": args.numeric_keytips,
        "key_tip_sequence": "F10/2" if args.numeric_keytips or args.ribbon_minimized else "F10/H/P",
        "minimized": args.ribbon_minimized,
        "checks": [], "captures": {},
        "input_method": "Acknowledged native cursor moves and child-only posted buttons/keys; native translated KeyTip characters",
        "limits": ["No physical keyboard, native IME or screen reader", "Printable H/P KeyTips require the child to use a Latin keyboard layout",
                   "No multi-monitor DPI test; screenshots require visual review"],
    }
    if args.numeric_keytips or args.ribbon_minimized:
        report["limits"].append("Letter KeyTips were not exercised natively in this run; F10/2 covers QAT activation")
    session = None
    try:
        windows = RibbonWindows(args.dark)
        cursor = windows.cursor_position()
        with (output / "stdout.log").open("wb") as stdout, (output / "stderr.log").open("wb") as stderr:
            process = subprocess.Popen([str(executable), "--diagnostics"] + (["--dark"] if args.dark else [])
                                       + (["--ribbon-minimized"] if args.ribbon_minimized else []),
                                       stdout=stdout, stderr=stderr, creationflags=subprocess.CREATE_NO_WINDOW)
            report["process_id"] = process.pid
            session = RibbonSession(windows, process, report, args.timeout, cursor)
            try:
                session.run(output)
                report["status"] = "passed"
            finally:
                session.cleanup()
    except (SmokeFailure, OSError, subprocess.SubprocessError, KeyError, ValueError) as error:
        report["error"] = str(error)
    finally:
        report["finished_utc"] = datetime.now(timezone.utc).isoformat()
        if session:
            report["exit_code"] = session.process.poll()
        (output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"{report['status'].upper()}: {output / 'report.json'}")
    if "error" in report:
        print(report["error"], file=sys.stderr)
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

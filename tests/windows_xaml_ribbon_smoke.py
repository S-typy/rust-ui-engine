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


TITLE_PREFIX = "Форма · XAML Ribbon"
TITLE = re.compile(r"^Форма · XAML Ribbon \| frames=(\d+) glyphs=(\d+) missing=(\d+)$")
VK_HOME = 0x24
VK_F10 = 0x79
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
            if item["label"] == "Закрепить" or item["label"].startswith("Закрепить [")
            or item["label"].endswith(" / Закрепить")]


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
    samples = {}
    for name, bounds in regions.items():
        x, y, w, h = visible_rectangle(bounds, None, (width / scale, height / scale))
        if any(abs(a - b) > 1 / scale for a, b in zip((x, y, w, h), bounds, strict=True)):
            raise SmokeFailure(f"Capture region exceeds the native client: {name}: {bounds}")
        ink = 0
        shades = set()
        for py in range(max(0, math.ceil((y + 2) * scale)),
                        min(height, math.floor((y + h - 2) * scale))):
            for px in range(max(0, math.ceil((x + 2) * scale)),
                            min(width, math.floor((x + w - 2) * scale))):
                index = offset + ((height - 1 - py) * width + px) * 4
                b, g, r = bitmap[index:index + 3]
                color = (r, g, b)
                if any(all(abs(a - b) <= 12 for a, b in zip(color, candidate, strict=True))
                       for candidate in (foreground, muted)):
                    ink += 1
                    shades.add(color)
        if ink < 20 or len(shades) < 3:
            raise SmokeFailure(f"No credible text/vector coverage in {name}: {ink} pixels, {len(shades)} shades")
        samples[name] = {"bounds": bounds, "ink_pixels": ink, "ink_shades": len(shades)}
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
        if len(matches) != 2:
            raise SmokeFailure(f"Expected QAT and body Pin placements: {matches}")
        return sorted(matches, key=lambda item: item["bounds"][1])[0 if qat else 1]

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
        before = self.commands()
        self.key(VK_F10)
        self.ribbon_snapshot("F10 displays tab KeyTips", lambda state: state["key_tips"])
        self.snapshot("tab KeyTip captions are visible", lambda state: any(
            item["role"] == "Tab" and item["label"] == "Главная [H]" for item in state["controls"]))
        self.capture_ribbon(output, "key-tips")
        if self.report["numeric_keytips"]:
            self.letter_keytip("2", "QAT 2 invokes the Pin command", lambda: self.commands() > before)
        else:
            self.letter_keytip("H", "H opens Home command KeyTips", lambda: any(
                item["label"] == "Закрепить [P]" for item in self.state()["controls"]))
            self.letter_keytip("P", "P invokes the Pin command", lambda: self.commands() > before)
        self.ribbon_snapshot("command KeyTip hides the hints", lambda state: not state["key_tips"])
        self.snapshot("KeyTip command returns focus to editor", lambda state: state["focused"] == "Текст")
        self.exact_command(before, "Pin", self.report["key_tip_sequence"], pin=True)
        self.assert_editor_text(INITIAL_TEXT, "KeyTip letters do not enter the editor")

        primaries = [item for item in self.state()["controls"]
                     if item["label"] == "Подтвердить" and self.in_ribbon(item)]
        primary = max(primaries, key=lambda item: item["bounds"][3])
        before = self.commands()
        self.click_item(primary)
        self.exact_command(before, "Confirm", "Split primary")
        self.snapshot("Split primary does not open a menu", lambda state: not state["popup_open"])
        arrow = self.control(lambda item: item["label"] == "More" and self.in_ribbon(item)
                             and abs(item["bounds"][0] - primary["bounds"][0]) < 1)
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
        for width in (360, 180, 1040):
            self.resize(width)
            self.snapshot("reflow preserves editor focus", lambda state: state["focused"] == "Текст")
            self.assert_editor_text(PRESERVED_TEXT, "reflow preserves the same draft")
            presentation = self.ribbon_state()["presentations"]
            if width == 360 and "Collapsed" not in presentation:
                raise SmokeFailure("Medium width did not collapse a group")
            if width == 180 and "Overflow" not in presentation:
                raise SmokeFailure("Narrow width did not produce All commands")
            if width == 1040 and ("Overflow" in presentation or "Collapsed" in presentation):
                raise SmokeFailure("Wide Ribbon failed to restore full groups")
        self.type_text("5")
        preserved_with_caret = PRESERVED_TEXT[:-2] + "5" + PRESERVED_TEXT[-2:]
        self.wait_value(preserved_with_caret, "caret survives collapse, overflow and expansion")

        self.resize(180)
        before = self.commands()
        self.click_item(self.control(lambda item: item["label"] == "All commands" and self.in_ribbon(item)))
        self.snapshot("All commands opens inside the narrow client", lambda state: len(state["menus"]) == 1)
        self.ensure_no_command(before, "All commands opening")
        self.capture_menu(output, "narrow-menu")
        menu_pin = self.control(lambda item: item["role"] == "MenuItem" and item["label"] == "Редактирование / Закрепить")
        self.activate_pin(menu_pin, "Overflow shares the same Pin state")
        self.snapshot("Overflow activation closes the menu", lambda state: not state["popup_open"])
        self.resize(1040)
        self.assert_editor_text(preserved_with_caret, "menu activation preserves the editor document")
        verify_shared_pin(self.state(), False)
        self.capture_ribbon(output, "restored-ribbon")
        self.report["final_render_state"] = self.wait_state("final frame has no missing glyphs", lambda state: state["missing"] == 0)
        self.report["command_invocations"] = self.invocations()
        self.report["final_edit_value"] = last_edit_value(self.log())
        self.windows.post(self.hwnd, WM_CLOSE)
        code = self.process.wait(timeout=self.timeout)
        if code:
            raise SmokeFailure(f"XAML Ribbon child exited with status {code}")
        self.report["checks"].append({"check": "native close", "exit_code": code})


def self_test():
    assert parse_render_state("Форма · XAML Ribbon | frames=12 glyphs=340 missing=0") == {
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
    print("PASS: Ribbon diagnostics, command counting, clipping and shared-state fixtures")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--dark", action="store_true")
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
        "numeric_keytips": args.numeric_keytips, "key_tip_sequence": "F10/2" if args.numeric_keytips else "F10/H/P",
        "checks": [], "captures": {},
        "input_method": "Acknowledged native cursor moves and child-only posted buttons/keys; native translated KeyTip characters",
        "limits": ["No physical keyboard, native IME or screen reader", "Printable H/P KeyTips require the child to use a Latin keyboard layout",
                   "No multi-monitor DPI test; screenshots require visual review"],
    }
    if args.numeric_keytips:
        report["limits"].append("Letter KeyTips were not exercised natively in this run; F10/2 covers QAT activation")
    session = None
    try:
        windows = RibbonWindows(args.dark)
        cursor = windows.cursor_position()
        with (output / "stdout.log").open("wb") as stdout, (output / "stderr.log").open("wb") as stderr:
            process = subprocess.Popen([str(executable), "--diagnostics"] + (["--dark"] if args.dark else []),
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

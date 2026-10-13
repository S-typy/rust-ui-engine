#!/usr/bin/env python3
"""Exercise the XAML example's popup menus with child-only Win32 input."""
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
from windows_retained_smoke import WM_MOUSEMOVE, WM_LBUTTONDOWN, WM_LBUTTONUP
from windows_smoke import SmokeFailure, WM_CLOSE
from windows_xaml_smoke import (FormSession, FormWindows, parse_state, VK_RETURN,
                                VK_ESCAPE, VK_RIGHT, VK_LEFT, VK_DOWN, VK_TAB)


POINTER_MOVED = re.compile(
    r"^edit-input=Input\(PointerMoved \{ position: Point \{ x: ([-+0-9.eE]+), y: ([-+0-9.eE]+) \}")


def latest_pointer(log: str) -> tuple[float, float] | None:
    """Read the last complete native pointer diagnostic, respecting leave."""
    for line in reversed(log.splitlines(keepends=True)):
        if not line.endswith("\n"):
            continue
        if line.startswith("edit-input=Input(PointerLeft)"):
            return None
        match = POINTER_MOVED.match(line)
        if match:
            return float(match[1]), float(match[2])
    return None


def latest_snapshot(path: Path) -> dict | None:
    latest = None
    for line in path.read_text(encoding="utf-8", errors="strict").splitlines(keepends=True):
        if line.startswith("popup-state ") and line.endswith("\n"):
            try:
                latest = json.loads(line.removeprefix("popup-state "))
            except json.JSONDecodeError as error:
                raise SmokeFailure(f"Invalid complete popup snapshot: {error}") from error
    return latest


class PopupWindows(FormWindows):
    def verify_capture(self, pixels, width, height, scale, state):
        if state["missing"] or len(pixels) != width * height * 4:
            raise SmokeFailure("Invalid popup capture or missing glyphs")
        return {"missing_glyphs": state["missing"], "scale": scale}


def popup_capture_worker(hwnd, pid, destination, result_queue, dark):
    try:
        result_queue.put(PopupWindows(dark).screenshot(hwnd, pid, destination))
    except Exception as error:
        result_queue.put({"status": "unavailable", "reason": str(error)})


class PopupSession(FormSession):
    def pointer(self, message, x, y, scale, held=False):
        if message != WM_MOUSEMOVE:
            return super().pointer(message, x, y, scale, held)
        self.alive()
        physical_x, physical_y = round(x * scale), round(y * scale)
        width, height = self.windows.client_size(self.hwnd)
        if not (0 <= physical_x < width and 0 <= physical_y < height):
            raise SmokeFailure("Pointer coordinate is outside the child client area")
        screen = wt.POINT(physical_x, physical_y)
        self.windows.require(self.windows.user.ClientToScreen(self.hwnd, ctypes.byref(screen)), "ClientToScreen")
        self.wait("child window exposed at pointer coordinates",
                  lambda: self.windows.point_is_owned(self.hwnd, self.process.pid, screen))
        self.windows.move_cursor(screen.x, screen.y)
        self.expected_cursor = (screen.x, screen.y)
        # SetCursorPos already queues a native move. Posting another WM_MOUSEMOVE
        # can race native leave tracking; button messages must also wait until
        # the application has consumed the new pointer position.
        expected = (physical_x / scale, physical_y / scale)

        def delivered():
            position = latest_pointer(self.log())
            return (position is not None
                    and all(abs(a - b) < 0.001 for a, b in zip(position, expected))
                    and self.windows.cursor_position() == self.expected_cursor)

        try:
            self.wait("native pointer position delivered", delivered)
        except SmokeFailure:
            self.report["pointer_delivery_failure"] = {
                "expected_screen": list(self.expected_cursor),
                "actual_screen": list(self.windows.cursor_position()),
                "expected_logical": list(expected),
                "last_logical": latest_pointer(self.log()),
                "expected_point_owned": self.windows.point_is_owned(self.hwnd, self.process.pid, screen),
            }
            raise

    def snapshot(self, description, predicate=lambda _: True):
        def matches():
            state = latest_snapshot(self.stdout_path)
            return state if state is not None and predicate(state) else None
        state = self.wait(description, matches)
        # Keep the report compact; full snapshots remain in stdout.log.
        self.report["checks"].append({"check": description, "menus": len(state["menus"]),
                                      "focused": state["focused"], "pin": state["pin_checked"]})
        return state

    def state(self):
        state = latest_snapshot(self.stdout_path)
        if state is None:
            raise SmokeFailure("No popup diagnostic")
        return state

    def item(self, label, *, menu=False, popup=None):
        matches = [item for item in self.state()["controls"]
                   if item["label"] == label and (item["role"] == "MenuItem") == menu
                   and (popup is None or item["has_popup"] == popup)]
        if len(matches) != 1:
            raise SmokeFailure(f"Expected unique control {label!r}, got {matches}")
        return matches[0]

    def click_item(self, item):
        x, y, width, height = item["bounds"]
        self.click_control(round(x + width / 2), round(y + height / 2), self.scale)

    def commands(self):
        return self.log().count("command=")

    def ensure_no_command(self, count, description):
        time.sleep(0.15)
        self.alive()
        if self.commands() != count:
            raise SmokeFailure(description + " unexpectedly invoked a command")
        self.report["checks"].append({"check": description + " invokes no command"})

    def capture_popup(self, output, name, tooltip=False):
        time.sleep(0.15)
        state = self.state()
        result = capture_client(self.hwnd, self.process.pid, output / f"{name}.bmp",
                                self.timeout, worker_target=popup_capture_worker,
                                worker_args=(self.windows.dark,))
        self.report["captures"][name] = result
        if result["status"] != "available":
            raise SmokeFailure(f"{name} capture unavailable: {result}")
        width, height = result["width"], result["height"]
        data = Path(result["bmp"]).read_bytes()
        offset = struct.unpack_from("<I", data, 10)[0]
        regions = [state["tooltip_bounds"]] if tooltip else state["menus"]
        if not regions or any(region is None for region in regions):
            raise SmokeFailure("Expected visible popup regions")
        ink_counts = []
        foreground = (245, 245, 245) if self.windows.dark else (27, 27, 27)
        for x, y, w, h in regions:
            if not (x >= 0 and y >= 0 and w > 0 and h > 0
                    and (x + w) * self.scale <= width + 1
                    and (y + h) * self.scale <= height + 1):
                raise SmokeFailure(f"Popup exceeds client bounds: {[x,y,w,h]}")
            count = 0
            for py in range(math.ceil((y + 4) * self.scale), math.floor((y + h - 4) * self.scale)):
                for px in range(math.ceil((x + 4) * self.scale), math.floor((x + w - 4) * self.scale)):
                    position = offset + ((height - 1 - py) * width + px) * 4
                    b, g, r = data[position:position + 3]
                    count += all(abs(a - b) <= 10 for a, b in zip((r, g, b), foreground, strict=True))
            if count < 20:
                raise SmokeFailure(f"Popup region lacks text/vector ink: {count}")
            ink_counts.append(count)
        result["popup_regions"] = regions
        result["ink_pixels"] = ink_counts

    def run(self, output):
        self.stdout_path = output / "stdout.log"
        self.hwnd = self.wait("visible form", lambda: self.windows.find_window(self.process.pid))
        self.report["window_handle"] = self.hwnd
        self.wait_state("first GPU frame", lambda s: s["frames"] > 0 and s["glyphs"] > 0)
        dpi = self.windows.user.GetDpiForWindow(self.hwnd)
        if dpi <= 0:
            raise SmokeFailure("Invalid DPI")
        self.report["dpi"] = dpi
        self.scale = dpi / 96.0
        self.windows.topmost_without_activation(self.hwnd, True)
        self.promoted = True
        self.focus_messages(True)
        self.wait("focus notification completes", lambda: "Input(WindowFocused(true))" in self.log())
        # winit may synthesize key releases during focus initialization. Let
        # that bounded message batch complete before starting an idle hover.
        time.sleep(0.2)
        self.snapshot("initial popup state", lambda s: not s["popup_open"])
        dropdown = self.item("Действия", popup=True)
        x, y, w, h = dropdown["bounds"]
        self.pointer(WM_MOUSEMOVE, 20, 100, self.scale)
        self.snapshot("neutral pointer before hover", lambda s: s["hovered"] != "Действия")
        self.pointer(WM_MOUSEMOVE, round(x + w / 2), round(y + h / 2), self.scale)
        self.snapshot("pointer enters dropdown and arms delay", lambda s: s["hovered"] == "Действия"
                      and s["tooltip_pending"])
        self.snapshot("hover deadline displays ToolTip", lambda s: s["tooltip_bounds"] is not None)
        self.capture_popup(output, "tooltip", tooltip=True)
        count = self.commands()
        self.click_item(dropdown)
        state = self.snapshot("DropDown opens root and hides ToolTip",
                              lambda s: len(s["menus"]) == 1 and s["tooltip_bounds"] is None)
        if state["menus"][0][1] >= y:
            raise SmokeFailure("Bottom-row menu did not flip above its anchor")
        self.capture_popup(output, "root-menu")
        self.ensure_no_command(count, "DropDown opening")
        self.click_item(self.item("Режим", menu=True))
        self.snapshot("pointer opens nested menu", lambda s: len(s["menus"]) == 2)
        self.capture_popup(output, "submenu")
        self.key(VK_LEFT)
        self.snapshot("Left returns to submenu owner",
                      lambda s: len(s["menus"]) == 1 and s["focused"] == "Режим")
        self.key(VK_RIGHT)
        self.snapshot("Right opens submenu", lambda s: len(s["menus"]) == 2)
        self.key(VK_ESCAPE)
        self.snapshot("Escape closes only nested level", lambda s: len(s["menus"]) == 1)
        self.key(VK_ESCAPE)
        self.snapshot("Escape restores DropDown focus",
                      lambda s: not s["popup_open"] and s["focused"] == "Действия")
        self.key(VK_RETURN)
        self.snapshot("Enter reopens DropDown", lambda s: len(s["menus"]) == 1)
        disabled = self.item("Недоступно", menu=True)
        if disabled["enabled"]:
            raise SmokeFailure("Disabled menu item is enabled")
        self.click_item(disabled)
        self.ensure_no_command(count, "Disabled item")
        if not self.state()["popup_open"]:
            raise SmokeFailure("Disabled menu item dismissed popup")
        self.click_item(self.item("Режим", menu=True))
        self.snapshot("submenu reopens", lambda s: len(s["menus"]) == 2)
        self.click_item(self.item("Закрепить", menu=True))
        self.snapshot("nested toggle invokes once and closes chain",
                      lambda s: not s["popup_open"] and s["pin_checked"])
        time.sleep(0.15)
        if self.commands() != count + 1:
            raise SmokeFailure("Nested toggle did not invoke exactly once")
        count = self.commands()
        self.click_item(dropdown)
        self.snapshot("root opens before outside click", lambda s: s["popup_open"])
        self.click_control(380, 206, self.scale)  # Existing disabled button, outside menu.
        self.snapshot("outside pointer dismisses popup", lambda s: not s["popup_open"])
        self.ensure_no_command(count, "Outside disabled control")
        self.click_item(dropdown)
        self.snapshot("root opens before window blur", lambda s: s["popup_open"])
        self.focus_messages(False)
        self.snapshot("window blur safely closes popup", lambda s: not s["popup_open"])
        self.focus_messages(True)
        # Split parts use independently focusable controls with distinct bounds.
        split_parts = [item for item in self.state()["controls"]
                       if item["bounds"][1] >= 430 and item["bounds"][0] >= 195
                       and item["role"] in ("Button", "ToggleButton")]
        if len(split_parts) != 2:
            raise SmokeFailure(f"Expected two SplitButton parts: {split_parts}")
        primary, arrow = sorted(split_parts, key=lambda item: item["bounds"][0])
        self.click_item(primary)
        self.wait("split primary invokes Confirm", lambda: self.commands() == count + 1)
        self.snapshot("split primary does not open popup", lambda s: not s["popup_open"])
        count = self.commands()
        self.click_item(arrow)
        self.snapshot("split arrow opens menu", lambda s: len(s["menus"]) == 1)
        self.ensure_no_command(count, "Split arrow")
        self.capture_popup(output, "split-menu")
        old_x = self.state()["menus"][0][0]
        target = (round(480 * self.scale), round(512 * self.scale))
        self.windows.resize_client(self.hwnd, *target)
        self.wait("popup client resize", lambda: self.windows.client_size(self.hwnd) == target)
        self.snapshot("open menu repositions after resize",
                      lambda s: len(s["menus"]) == 1 and s["menus"][0][0] < old_x)
        self.click_item(self.item("Режим", menu=True))
        narrow = self.snapshot("nested menu opens in resized client", lambda s: len(s["menus"]) == 2)
        if narrow["menus"][1][0] >= narrow["menus"][0][0]:
            raise SmokeFailure("Submenu did not flip left at the right edge")
        self.capture_popup(output, "submenu-left")
        self.key(VK_ESCAPE)
        self.snapshot("Escape closes left cascade only", lambda s: len(s["menus"]) == 1)
        self.key(VK_ESCAPE)
        self.snapshot("Escape restores split arrow focus",
                      lambda s: not s["popup_open"] and any(i["focused"] and i["has_popup"]
                                                           and i["bounds"][0] >= 195 for i in s["controls"]))
        self.key(VK_RETURN)
        self.snapshot("split keyboard opens menu", lambda s: s["popup_open"])
        self.key(VK_TAB)
        self.snapshot("Tab closes chain and returns to editor", lambda s: not s["popup_open"]
                      and s["focused"] == "Текст")
        self.windows.post(self.hwnd, WM_CLOSE)
        code = self.process.wait(timeout=self.timeout)
        if code:
            raise SmokeFailure(f"Unexpected child exit {code}")
        self.report["checks"].append({"check": "native close", "exit_code": code})


def self_test():
    moved = "edit-input=Input(PointerMoved { position: Point { x: 112.0, y: 450.66666 } }) focused=false\n"
    left = "edit-input=Input(PointerLeft) focused=false\n"
    assert latest_pointer("") is None
    assert latest_pointer(moved) == (112.0, 450.66666)
    assert latest_pointer(moved + left) is None
    assert latest_pointer(left + moved) == (112.0, 450.66666)
    assert latest_pointer(moved + "popup-state {}\n") == (112.0, 450.66666)
    assert latest_pointer(left + moved.rstrip("\n")) is None
    print("PASS: native pointer acknowledgement diagnostics")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--dark", action="store_true")
    parser.add_argument("--timeout", type=float, default=20)
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if args.exe is None or args.output_dir is None:
        parser.error("--exe and --output-dir are required for a window test")
    if sys.platform != "win32" or not math.isfinite(args.timeout) or not 0 < args.timeout <= 60:
        parser.error("Windows and a finite timeout in (0,60] are required")
    executable = args.exe.resolve()
    if not executable.is_file():
        parser.error("--exe must name a file")
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = {"status": "failed", "started_utc": datetime.now(timezone.utc).isoformat(),
              "executable": str(executable), "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
              "theme": "dark" if args.dark else "light", "backend_environment": os.environ.get("WGPU_BACKEND"),
              "checks": [], "captures": {}, "input_method": "Native cursor moves acknowledged before posted child-only clicks and keys",
              "limits": ["No physical keyboard, native IME or screen reader",
              "No multi-monitor DPI verification; captures require visual review"]}
    session = None
    try:
        windows = PopupWindows(args.dark)
        original = windows.cursor_position()
        with (output / "stdout.log").open("wb") as stdout, (output / "stderr.log").open("wb") as stderr:
            process = subprocess.Popen([str(executable), "--diagnostics"] + (["--dark"] if args.dark else []),
                                       stdout=stdout, stderr=stderr, creationflags=subprocess.CREATE_NO_WINDOW)
            report["process_id"] = process.pid
            session = PopupSession(windows, process, report, args.timeout, original)
            try:
                session.run(output)
                report["status"] = "passed"
            finally:
                session.cleanup()
    except (SmokeFailure, OSError, subprocess.SubprocessError) as error:
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

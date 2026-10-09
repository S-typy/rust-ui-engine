#!/usr/bin/env python3
"""Exercise the retained GPU example with posted Win32 pointer/key messages.

Only the child process window is used. Native handles, timeout handling and
cleanup are shared with windows_smoke; capture never reads the desktop DC.
The OS cursor temporarily follows test coordinates so native leave tracking is
consistent with posted events. Its original position is restored during cleanup.
"""

from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes as wt
from datetime import datetime, timezone
import json
import math
import multiprocessing
import os
from pathlib import Path
import queue
import re
import struct
import subprocess
import sys
import time

from windows_smoke import (
    BitmapInfo, BitmapInfoHeader, Session, SmokeFailure, Windows,
    WM_CLOSE, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    packed_point,
)


TITLE = re.compile(
    r"^Rust UI Engine \| mode=retained \| frame=(\d+) \| layout=(\d+) "
    r"\| paint=(\d+) \| focus=(none|a|b|c|scroll) "
    r"\| hover=(none|a|b|c|scroll|row|other) \| capture=(none|a|b|c|scroll) "
    r"\| activated=(\d+) \| removed=(\d+) \| nodes=(\d+) "
    r"\| scroll=(\d+) \| size=(\d+)x(\d+)$"
)
WM_KEYDOWN = 0x0100
WM_KEYUP = 0x0101
WM_SETFOCUS = 0x0007
WM_KILLFOCUS = 0x0008
WM_NCACTIVATE = 0x0086
VK_TAB = 0x09
VK_RETURN = 0x0D
VK_DELETE = 0x2E


def parse_state(title: str) -> dict | None:
    match = TITLE.fullmatch(title)
    if match is None:
        return None
    keys = ("frame", "layout", "paint", "focus", "hover", "capture",
            "activated", "removed", "nodes", "scroll", "width", "height")
    return {key: value if key in {"focus", "hover", "capture"} else int(value)
            for key, value in zip(keys, match.groups(), strict=True)}


def key_message(scan_code: int, released: bool) -> int:
    """Build repeat/scan/extended/transition bits from MAPVK_VK_TO_VSC_EX."""
    if scan_code <= 0 or scan_code > 0xFFFF:
        raise SmokeFailure(f"Invalid keyboard scan code: {scan_code}")
    value = 1 | ((scan_code & 0xFF) << 16)
    if (scan_code >> 8) in (0xE0, 0xE1):
        value |= 1 << 24
    if released:
        value |= (1 << 30) | (1 << 31)
    return value


def scene_capture_checkpoints(pixels: bytes, width: int, height: int, scale: float) -> dict:
    """Validate known retained primitives before saving an image.

    The final scene has C removed, no A/B focus or hover, and a scrolled body.
    Accept direct and sRGB-encoded channels, as the swapchain format can differ.
    """
    if not math.isfinite(scale) or scale <= 0 or len(pixels) != width * height * 4:
        raise SmokeFailure("Invalid retained capture dimensions or DPI")

    def rgb(x: int, y: int) -> tuple[int, int, int]:
        physical_x, physical_y = round(x * scale), round(y * scale)
        if not (0 <= physical_x < width and 0 <= physical_y < height):
            raise SmokeFailure("Capture is too small for retained scene checkpoints")
        offset = ((height - 1 - physical_y) * width + physical_x) * 4
        blue, green, red = pixels[offset:offset + 3]
        return red, green, blue

    def srgb(channel: int) -> int:
        linear = channel / 255.0
        encoded = 12.92 * linear if linear <= 0.0031308 else 1.055 * linear ** (1 / 2.4) - 0.055
        return round(encoded * 255)

    checks = {
        "primitive_a_rgb": (rgb(50, 40), (35, 90, 170)),
        "primitive_b_rgb": (rgb(220, 40), (22, 128, 110)),
        "navigation_rgb": (rgb(20, 100), (255, 255, 255)),
    }
    for name, (actual, direct) in checks.items():
        expected = (direct, tuple(srgb(channel) for channel in direct))
        if not any(all(abs(a - b) <= 4 for a, b in zip(actual, option, strict=True))
                   for option in expected):
            raise SmokeFailure(f"Unexpected {name}: {actual}; retained image not saved")
    return {name: actual for name, (actual, _expected) in checks.items()}


class RetainedWindows(Windows):
    def __init__(self) -> None:
        super().__init__()
        self.user.MapVirtualKeyW.argtypes = [wt.UINT, wt.UINT]
        self.user.MapVirtualKeyW.restype = wt.UINT
        self.user.GetCursorPos.argtypes = [ctypes.POINTER(wt.POINT)]
        self.user.GetCursorPos.restype = wt.BOOL
        self.user.SetCursorPos.argtypes = [ctypes.c_int, ctypes.c_int]
        self.user.SetCursorPos.restype = wt.BOOL
        self.user.WindowFromPoint.argtypes = [wt.POINT]
        self.user.WindowFromPoint.restype = wt.HWND
        self.user.IsChild.argtypes = [wt.HWND, wt.HWND]
        self.user.IsChild.restype = wt.BOOL

    def cursor_position(self) -> tuple[int, int]:
        point = wt.POINT()
        self.require(self.user.GetCursorPos(ctypes.byref(point)), "GetCursorPos")
        return point.x, point.y

    def move_cursor(self, x: int, y: int) -> None:
        self.require(self.user.SetCursorPos(x, y), "SetCursorPos")
        if self.cursor_position() != (x, y):
            raise SmokeFailure("OS cursor did not reach the requested point; desktop clipping may be active")

    def point_is_owned(self, hwnd: int, pid: int, point: wt.POINT) -> bool:
        target = self.user.WindowFromPoint(point)
        return bool(target and self.owns(target, pid)
                    and (target == hwnd or self.user.IsChild(hwnd, target)))

    def topmost_without_activation(self, hwnd: int, enabled: bool) -> None:
        # The child is temporary. Only its z-order changes; SWP_NOACTIVATE keeps
        # keyboard focus/foreground belonging to the currently active window.
        insert_after = wt.HWND(-1 if enabled else -2)  # HWND_TOPMOST / HWND_NOTOPMOST
        self.require(self.user.SetWindowPos(hwnd, insert_after, 0, 0, 0, 0,
                                           0x0001 | 0x0002 | 0x0010 | 0x4000),
                     "SetWindowPos(topmost without activation)")

    def screenshot(self, hwnd: int, pid: int, destination: Path) -> dict:
        if not self.owns(hwnd, pid) or parse_state(self.title(hwnd)) is None:
            raise SmokeFailure("Capture target is not the retained child window")
        width, height = self.client_size(hwnd)
        if width <= 0 or height <= 0 or width * height > 64_000_000:
            raise SmokeFailure(f"Invalid client capture size {width}x{height}")
        memory = bitmap = original = None
        try:
            memory = self.gdi.CreateCompatibleDC(None)
            self.require(memory, "CreateCompatibleDC")
            info = BitmapInfo()
            info.header.size = ctypes.sizeof(BitmapInfoHeader)
            info.header.width = width
            info.header.height = height
            info.header.planes = 1
            info.header.bit_count = 32
            info.header.image_size = width * height * 4
            bits = ctypes.c_void_p()
            bitmap = self.gdi.CreateDIBSection(memory, ctypes.byref(info), 0,
                                              ctypes.byref(bits), None, 0)
            self.require(bitmap and bits.value, "CreateDIBSection")
            original = self.gdi.SelectObject(memory, bitmap)
            self.require(original and original != ctypes.c_void_p(-1).value, "SelectObject")
            self.require(self.user.PrintWindow(hwnd, memory, 0x0001 | 0x0002), "PrintWindow(client)")
            self.require(self.gdi.GdiFlush(), "GdiFlush")
            pixels = ctypes.string_at(bits, info.header.image_size)
            if not self.owns(hwnd, pid) or parse_state(self.title(hwnd)) is None:
                raise SmokeFailure("Retained window changed during capture")
            checkpoints = scene_capture_checkpoints(
                pixels, width, height, self.user.GetDpiForWindow(hwnd) / 96.0)
            header = struct.pack("<2sIHHI", b"BM", 54 + len(pixels), 0, 0, 54)
            destination.write_bytes(header + bytes(info.header) + pixels)
        finally:
            if original and memory:
                self.gdi.SelectObject(memory, original)
            if bitmap:
                self.gdi.DeleteObject(bitmap)
            if memory:
                self.gdi.DeleteDC(memory)
        result = {"status": "available", "method": "PrintWindow client/full-content",
                  "bmp": str(destination), "width": width, "height": height,
                  "scene_checkpoints": checkpoints}
        try:
            from PIL import Image
        except ImportError:
            return result
        with Image.open(destination) as screenshot:
            png = destination.with_suffix(".png")
            screenshot.convert("RGB").save(png)
            result["png"] = str(png)
        return result


def capture_worker(hwnd: int, pid: int, destination: Path, result_queue) -> None:
    try:
        result_queue.put(RetainedWindows().screenshot(hwnd, pid, destination))
    except Exception as error:
        result_queue.put({"status": "unavailable", "reason": str(error)})


def capture_client(hwnd: int, pid: int, destination: Path, timeout: float) -> dict:
    context = multiprocessing.get_context("spawn")
    results = context.Queue()
    worker = context.Process(target=capture_worker, args=(hwnd, pid, destination, results))
    try:
        worker.start()
        worker.join(timeout=min(timeout, 5.0))
        if worker.is_alive():
            worker.terminate()
            worker.join(timeout=2)
            if worker.is_alive():
                worker.kill()
                worker.join(timeout=2)
            return {"status": "unavailable", "reason": "Retained PrintWindow capture timed out"}
        try:
            return results.get(timeout=1)
        except queue.Empty:
            return {"status": "unavailable", "reason": f"Capture worker exited with status {worker.exitcode}"}
    finally:
        results.close()
        results.join_thread()
        if worker.pid is not None and not worker.is_alive():
            worker.close()


class RetainedSession(Session):
    def __init__(self, windows: RetainedWindows, process: subprocess.Popen,
                 report: dict, timeout: float, original_cursor: tuple[int, int]):
        super().__init__(windows, process, report, timeout)
        self.original_cursor = original_cursor
        self.promoted = False
        self.report["cursor_positioning"] = {
            "method": "SetCursorPos follows posted test coordinates inside the child client area",
            "original_position": list(original_cursor), "restored": False,
        }

    def wait_state(self, description: str, predicate) -> dict:
        def matches():
            state = parse_state(self.windows.title(self.hwnd))
            return state if state is not None and predicate(state) else None
        state = self.wait(description, matches)
        self.report["checks"].append({"check": description, "state": state})
        return state

    def idle(self, description: str) -> dict:
        deadline = time.monotonic() + self.timeout
        previous = None
        stable_since = time.monotonic()
        while time.monotonic() < deadline:
            self.alive()
            state = parse_state(self.windows.title(self.hwnd))
            if state is None or state != previous:
                stable_since = time.monotonic()
                previous = state
            elif time.monotonic() - stable_since >= 0.3:
                break
            time.sleep(0.05)
        else:
            raise SmokeFailure(f"Frames never settled for {description}")
        end = time.monotonic() + 0.75
        while time.monotonic() < end:
            self.alive()
            current = parse_state(self.windows.title(self.hwnd))
            if current != previous:
                raise SmokeFailure(f"Unexpected retained work during {description}: {previous} -> {current}")
            time.sleep(0.05)
        self.report["checks"].append({"check": description, "idle_seconds": 0.75, "state": previous})
        return previous

    def pointer(self, message: int, x: int, y: int, scale: float, held: bool = False) -> None:
        self.alive()
        physical_x, physical_y = round(x * scale), round(y * scale)
        width, height = self.windows.client_size(self.hwnd)
        if not (0 <= physical_x < width and 0 <= physical_y < height):
            raise SmokeFailure("Synthetic pointer coordinate is outside the child client area")
        screen = wt.POINT(physical_x, physical_y)
        self.windows.require(self.windows.user.ClientToScreen(self.hwnd, ctypes.byref(screen)), "ClientToScreen")
        self.wait("child window exposed at pointer coordinates",
                  lambda: self.windows.point_is_owned(self.hwnd, self.process.pid, screen))
        # winit calls TrackMouseEvent(TME_LEAVE) after an entering WM_MOUSEMOVE.
        # A posted move alone leaves the real cursor elsewhere, causing Windows
        # to immediately emit WM_MOUSELEAVE and clear the retained hover state.
        self.windows.move_cursor(screen.x, screen.y)
        self.windows.post(self.hwnd, message, int(held),
                          packed_point(physical_x, physical_y))

    def key(self, virtual_key: int) -> None:
        self.alive()
        scan = self.windows.user.MapVirtualKeyW(virtual_key, 4)  # MAPVK_VK_TO_VSC_EX
        self.windows.post(self.hwnd, WM_KEYDOWN, virtual_key, key_message(scan, False))
        self.windows.post(self.hwnd, WM_KEYUP, virtual_key, key_message(scan, True))

    def focus_messages(self, focused: bool) -> None:
        self.alive()
        if focused:
            # winit reports focus only when its active AND focused flags are
            # true. These are fixture messages; no SetForegroundWindow/SetFocus
            # call changes the user's foreground application.
            self.windows.post(self.hwnd, WM_NCACTIVATE, 1)
            self.windows.post(self.hwnd, WM_SETFOCUS)
        else:
            self.windows.post(self.hwnd, WM_KILLFOCUS)
            self.windows.post(self.hwnd, WM_NCACTIVATE, 0)

    def run(self, output: Path) -> None:
        self.hwnd = self.wait("visible retained child window", lambda: self.windows.find_window(self.process.pid))
        self.report["window_handle"] = self.hwnd
        initial = self.wait_state("first retained GPU frame", lambda s: s["frame"] >= 1)
        if (initial["activated"], initial["removed"], initial["scroll"], initial["focus"], initial["capture"]) != (0, 0, 0, "none", "none"):
            raise SmokeFailure(f"Unexpected initial retained state: {initial}")
        if initial["layout"] < 1 or initial["paint"] < 1 or initial["nodes"] < 4:
            raise SmokeFailure(f"Initial retained frame did not perform layout/paint: {initial}")
        dpi = self.windows.user.GetDpiForWindow(self.hwnd)
        if dpi == 0:
            raise SmokeFailure("GetDpiForWindow returned zero")
        scale = dpi / 96.0
        self.report["dpi"] = dpi
        self.windows.topmost_without_activation(self.hwnd, True)
        self.promoted = True
        self.report["window_visibility"] = "Own child temporarily topmost with SWP_NOACTIVATE; pointer points checked by WindowFromPoint"
        self.report["focus_source"] = "Synthetic WM_NCACTIVATE and WM_SETFOCUS/WM_KILLFOCUS to the child HWND"
        self.focus_messages(True)
        # Establish a pointer position outside A even if the cursor happened to
        # be over A when the window opened.
        self.pointer(WM_MOUSEMOVE, 600, 300, scale)
        initial = self.idle("initial idle performs no frames, layout or paint")

        self.pointer(WM_MOUSEMOVE, 50, 40, scale)
        hovered = self.wait_state("hover A repaints without layout", lambda s: s["hover"] == "a"
                                  and s["frame"] > initial["frame"] and s["paint"] > initial["paint"]
                                  and s["layout"] == initial["layout"])
        self.pointer(WM_LBUTTONDOWN, 50, 40, scale, held=True)
        down = self.wait_state("A receives focus and pointer capture", lambda s: s["focus"] == "a"
                               and s["capture"] == "a" and s["activated"] == 1
                               and s["layout"] == hovered["layout"])
        self.pointer(WM_MOUSEMOVE, 600, 300, scale, held=True)
        moved = self.wait_state("pointer capture survives movement outside A", lambda s: s["capture"] == "a"
                                and s["focus"] == "a" and s["hover"] != "a"
                                and s["frame"] > down["frame"] and s["activated"] == 1
                                and s["layout"] == down["layout"])
        self.pointer(WM_LBUTTONUP, 600, 300, scale)
        released = self.wait_state("primary release clears pointer capture", lambda s: s["capture"] == "none"
                                   and s["focus"] == "a" and s["activated"] == 1
                                   and s["layout"] == moved["layout"])

        self.key(VK_TAB)
        focused_b = self.wait_state("Tab moves focus from A to B without layout", lambda s: s["focus"] == "b"
                                    and s["paint"] > released["paint"] and s["layout"] == released["layout"])
        self.key(VK_RETURN)
        activated = self.wait_state("Enter activates focused B", lambda s: s["activated"] == 2
                                    and s["focus"] == "b" and s["layout"] == focused_b["layout"])
        self.key(VK_TAB)
        focused_c = self.wait_state("Tab moves focus to C", lambda s: s["focus"] == "c"
                                    and s["layout"] == activated["layout"])
        self.key(VK_DELETE)
        removed = self.wait_state("Delete removes focused C and clears interaction state", lambda s:
                                  s["removed"] == 1 and s["nodes"] == initial["nodes"] - 1
                                  and s["focus"] == "none" and s["capture"] == "none"
                                  and s["layout"] > focused_c["layout"] and s["activated"] == 2)

        self.pointer(WM_MOUSEMOVE, 400, 200, scale)
        screen = wt.POINT(round(400 * scale), round(200 * scale))
        self.windows.require(self.windows.user.ClientToScreen(self.hwnd, ctypes.byref(screen)), "ClientToScreen")
        self.windows.post(self.hwnd, WM_MOUSEWHEEL, ((-120) & 0xFFFF) << 16,
                          packed_point(screen.x, screen.y))
        wheel = self.wait_state("wheel changes retained scroll without layout", lambda s: s["scroll"] > 0
                                and s["layout"] == removed["layout"] and s["paint"] > removed["paint"])
        self.key(VK_TAB)
        blur_target = self.wait_state("focus A before explicit native-message blur", lambda s: s["focus"] == "a"
                                      and s["layout"] == wheel["layout"])
        self.focus_messages(False)
        self.wait_state("posted blur clears focus and capture without layout", lambda s: s["focus"] == "none"
                        and s["capture"] == "none" and s["paint"] > blur_target["paint"]
                        and s["layout"] == wheel["layout"])
        self.focus_messages(True)
        width, height = round(900 * scale), round(600 * scale)
        self.windows.resize_client(self.hwnd, width, height)
        resized = self.wait_state("retained layout follows native resize", lambda s: s["width"] == width
                                  and s["height"] == height and s["frame"] > wheel["frame"]
                                  and s["layout"] > wheel["layout"])
        if self.windows.client_size(self.hwnd) != (width, height):
            raise SmokeFailure("Reported retained size differs from the native client area")
        self.windows.require(self.windows.user.ShowWindowAsync(self.hwnd, 6), "ShowWindowAsync(minimize)")
        self.wait("native minimize", lambda: self.windows.user.IsIconic(self.hwnd))
        minimized = self.idle("minimized retained window performs no continuous work")
        self.windows.require(self.windows.user.ShowWindowAsync(self.hwnd, 9), "ShowWindowAsync(restore)")
        self.wait("native restore", lambda: not self.windows.user.IsIconic(self.hwnd))
        self.wait_state("retained GPU frame after restore", lambda s: s["frame"] > minimized["frame"]
                        and s["width"] == width and s["height"] == height)
        final = self.idle("restored retained window returns to idle")
        if (final["activated"], final["removed"], final["nodes"], final["capture"]) != (2, 1, initial["nodes"] - 1, "none"):
            raise SmokeFailure(f"Retained state was lost during resize/minimize/restore: {final}")
        if final["scroll"] != resized["scroll"] or final["scroll"] > wheel["scroll"]:
            raise SmokeFailure(f"Retained scroll was not preserved or clamped on resize: {final}")
        self.report["screenshot"] = capture_client(self.hwnd, self.process.pid, output / "client.bmp", self.timeout)
        self.windows.post(self.hwnd, WM_CLOSE)
        try:
            code = self.process.wait(timeout=self.timeout)
        except subprocess.TimeoutExpired as error:
            raise SmokeFailure("Retained application did not exit after WM_CLOSE") from error
        self.report["exit_code"] = code
        if code != 0:
            raise SmokeFailure(f"Retained application exited with status {code} after WM_CLOSE")
        self.report["checks"].append({"check": "native close exits successfully", "exit_code": code})

    def cleanup(self) -> None:
        try:
            try:
                if self.promoted and self.hwnd and self.windows.owns(self.hwnd, self.process.pid):
                    self.windows.topmost_without_activation(self.hwnd, False)
            finally:
                super().cleanup()
        finally:
            self.windows.move_cursor(*self.original_cursor)
            self.report["cursor_positioning"]["restored"] = True


def self_test() -> None:
    title = ("Rust UI Engine | mode=retained | frame=12 | layout=2 | paint=7 "
             "| focus=b | hover=row | capture=none | activated=2 | removed=1 "
             "| nodes=24 | scroll=84 | size=1350x900")
    parsed = parse_state(title)
    assert parsed and parsed["focus"] == "b" and parsed["scroll"] == 84 and parsed["width"] == 1350
    assert parse_state(title + " extra") is None
    assert parse_state(title.replace("focus=b", "focus=invalid")) is None
    assert key_message(0x0F, False) == 0x000F0001
    assert key_message(0xE053, True) == 0xC1530001
    width, height = 900, 600
    pixels = bytearray(width * height * 4)
    for x, y, (red, green, blue) in [(50, 40, (35, 90, 170)), (220, 40, (22, 128, 110)),
                                  (20, 100, (255, 255, 255))]:
        offset = ((height - 1 - y) * width + x) * 4
        pixels[offset:offset + 4] = bytes((blue, green, red, 255))
    assert len(scene_capture_checkpoints(bytes(pixels), width, height, 1.0)) == 3
    try:
        scene_capture_checkpoints(bytes(width * height * 4), width, height, 1.0)
    except SmokeFailure:
        pass
    else:
        raise AssertionError("An empty capture was incorrectly accepted")

    class FakeWindows:
        def __init__(self):
            self.position = None

        def move_cursor(self, x, y):
            self.position = (x, y)

    class ExitedProcess:
        def poll(self):
            return 0

    class FailedCleanupProcess:
        def poll(self):
            raise SmokeFailure("simulated cleanup error")

    for process in (ExitedProcess(), FailedCleanupProcess()):
        windows = FakeWindows()
        report = {"checks": []}
        session = RetainedSession(windows, process, report, 1.0, (123, 456))
        try:
            session.cleanup()
        except SmokeFailure:
            assert isinstance(process, FailedCleanupProcess)
        assert windows.position == (123, 456)
        assert report["cursor_positioning"]["restored"]
    print("PASS: retained title, key-message, capture-validation and cursor-cleanup fixtures")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, help="Path to a built gpu-shell.exe")
    parser.add_argument("--output-dir", type=Path, help="Directory for logs, JSON and client capture")
    parser.add_argument("--timeout", type=float, default=20, help="Maximum seconds per operation (default: 20)")
    parser.add_argument("--self-test", action="store_true", help="Check parser/message/capture fixtures without starting a window")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if sys.platform != "win32":
        parser.error("This smoke test requires an interactive Windows desktop")
    if not args.exe or not args.output_dir:
        parser.error("--exe and --output-dir are required")
    if not math.isfinite(args.timeout) or args.timeout <= 0 or args.timeout > 60:
        parser.error("--timeout must be in the range (0, 60]")
    executable = args.exe.resolve()
    if not executable.is_file():
        parser.error("--exe must name a file")
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = {"status": "failed", "started_utc": datetime.now(timezone.utc).isoformat(),
              "executable": str(executable), "mode": "retained",
              "input_method": "Win32 PostMessageW synthetic pointer/key events with temporary SetCursorPos alignment",
              "backend_environment": os.environ.get("WGPU_BACKEND"), "checks": []}
    session = None
    try:
        windows = RetainedWindows()
        original_cursor = windows.cursor_position()
        with (output / "stdout.log").open("wb") as stdout, (output / "stderr.log").open("wb") as stderr:
            process = subprocess.Popen([str(executable)], stdout=stdout, stderr=stderr,
                                       creationflags=subprocess.CREATE_NO_WINDOW)
            report["process_id"] = process.pid
            session = RetainedSession(windows, process, report, args.timeout, original_cursor)
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

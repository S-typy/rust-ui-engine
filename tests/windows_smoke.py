#!/usr/bin/env python3
"""Exercise gpu-shell through posted Win32 events and capture its client area.

SAFETY: Every ctypes call has explicit pointer-sized signatures. Handles come
from Windows and are checked against the child process PID before use. Message
parameters contain integer coordinates, never borrowed pointers. GDI owns bitmap
storage until it is copied into Python bytes; selected objects are restored
before deletion. Cleanup targets only the application and capture helper we start.
"""

from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes as wt
from datetime import datetime, timezone
import json
import multiprocessing
import os
from pathlib import Path
import queue
import re
import struct
import subprocess
import sys
import time


TITLE = re.compile(
    r"^Rust UI Engine \| frame=(\d+) \| first_row=(\d+) "
    r"\| selected=(none|\d+) \| tab=(\d+) \| size=(\d+)x(\d+)$"
)
WM_CLOSE = 0x0010
WM_MOUSEMOVE = 0x0200
WM_LBUTTONDOWN = 0x0201
WM_LBUTTONUP = 0x0202
WM_MOUSEWHEEL = 0x020A


class SmokeFailure(RuntimeError):
    pass


class BitmapInfoHeader(ctypes.Structure):
    _fields_ = [
        ("size", wt.DWORD), ("width", wt.LONG), ("height", wt.LONG),
        ("planes", wt.WORD), ("bit_count", wt.WORD),
        ("compression", wt.DWORD), ("image_size", wt.DWORD),
        ("x_pixels_per_meter", wt.LONG), ("y_pixels_per_meter", wt.LONG),
        ("colors_used", wt.DWORD), ("colors_important", wt.DWORD),
    ]


class BitmapInfo(ctypes.Structure):
    _fields_ = [("header", BitmapInfoHeader), ("colors", wt.DWORD * 1)]


class Windows:
    def __init__(self) -> None:
        self.user = ctypes.WinDLL("user32", use_last_error=True)
        self.gdi = ctypes.WinDLL("gdi32", use_last_error=True)
        self.enum_callback = ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, ctypes.c_ssize_t)
        signatures = [
            (self.user, "EnumWindows", [self.enum_callback, ctypes.c_ssize_t], wt.BOOL),
            (self.user, "GetWindowThreadProcessId", [wt.HWND, ctypes.POINTER(wt.DWORD)], wt.DWORD),
            (self.user, "IsWindow", [wt.HWND], wt.BOOL),
            (self.user, "IsWindowVisible", [wt.HWND], wt.BOOL),
            (self.user, "IsIconic", [wt.HWND], wt.BOOL),
            (self.user, "GetWindowTextLengthW", [wt.HWND], ctypes.c_int),
            (self.user, "GetWindowTextW", [wt.HWND, wt.LPWSTR, ctypes.c_int], ctypes.c_int),
            (self.user, "GetDpiForWindow", [wt.HWND], wt.UINT),
            (self.user, "GetClientRect", [wt.HWND, ctypes.POINTER(wt.RECT)], wt.BOOL),
            (self.user, "GetWindowRect", [wt.HWND, ctypes.POINTER(wt.RECT)], wt.BOOL),
            (self.user, "ClientToScreen", [wt.HWND, ctypes.POINTER(wt.POINT)], wt.BOOL),
            (self.user, "PostMessageW", [wt.HWND, wt.UINT, ctypes.c_size_t, ctypes.c_ssize_t], wt.BOOL),
            (self.user, "SetWindowPos", [wt.HWND, wt.HWND, ctypes.c_int, ctypes.c_int,
                                        ctypes.c_int, ctypes.c_int, wt.UINT], wt.BOOL),
            (self.user, "ShowWindowAsync", [wt.HWND, ctypes.c_int], wt.BOOL),
            (self.user, "SetProcessDpiAwarenessContext", [wt.HANDLE], wt.BOOL),
            (self.user, "PrintWindow", [wt.HWND, wt.HDC, wt.UINT], wt.BOOL),
            (self.gdi, "CreateCompatibleDC", [wt.HDC], wt.HDC),
            (self.gdi, "CreateDIBSection", [wt.HDC, ctypes.POINTER(BitmapInfo), wt.UINT,
                                          ctypes.POINTER(ctypes.c_void_p), wt.HANDLE, wt.DWORD], wt.HBITMAP),
            (self.gdi, "SelectObject", [wt.HDC, wt.HANDLE], wt.HANDLE),
            (self.gdi, "GdiFlush", [], wt.BOOL),
            (self.gdi, "DeleteObject", [wt.HANDLE], wt.BOOL),
            (self.gdi, "DeleteDC", [wt.HDC], wt.BOOL),
        ]
        for library, name, arguments, result in signatures:
            function = getattr(library, name)
            function.argtypes = arguments
            function.restype = result
        # Match winit's per-monitor physical coordinate system. A prior setting
        # can make this return false; GetDpiForWindow remains authoritative.
        self.user.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))

    @staticmethod
    def require(result: object, operation: str) -> None:
        if not result:
            error = ctypes.get_last_error()
            raise SmokeFailure(f"{operation} failed ({error}): {ctypes.FormatError(error)}")

    def owns(self, hwnd: int, pid: int) -> bool:
        owner = wt.DWORD()
        return bool(self.user.IsWindow(hwnd)
                    and self.user.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
                    and owner.value == pid)

    def find_window(self, pid: int) -> int | None:
        found = []

        @self.enum_callback
        def collect(hwnd: int, _parameter: int) -> bool:
            if (self.owns(hwnd, pid) and self.user.IsWindowVisible(hwnd)
                    and self.title(hwnd).startswith("Rust UI Engine |")):
                found.append(hwnd)
            return True

        self.require(self.user.EnumWindows(collect, 0), "EnumWindows")
        return found[0] if found else None

    def title(self, hwnd: int) -> str:
        buffer = ctypes.create_unicode_buffer(self.user.GetWindowTextLengthW(hwnd) + 1)
        self.user.GetWindowTextW(hwnd, buffer, len(buffer))
        return buffer.value

    def client_size(self, hwnd: int) -> tuple[int, int]:
        rect = wt.RECT()
        self.require(self.user.GetClientRect(hwnd, ctypes.byref(rect)), "GetClientRect")
        return rect.right - rect.left, rect.bottom - rect.top

    def post(self, hwnd: int, message: int, wparam: int = 0, lparam: int = 0) -> None:
        self.require(self.user.PostMessageW(hwnd, message, wparam, lparam), "PostMessageW")

    def resize_client(self, hwnd: int, width: int, height: int) -> None:
        client_width, client_height = self.client_size(hwnd)
        outer = wt.RECT()
        self.require(self.user.GetWindowRect(hwnd, ctypes.byref(outer)), "GetWindowRect")
        outer_width = width + outer.right - outer.left - client_width
        outer_height = height + outer.bottom - outer.top - client_height
        # SWP_ASYNCWINDOWPOS keeps a hung child UI from blocking this test's
        # timeout/cleanup thread while Windows delivers the resize request.
        self.require(self.user.SetWindowPos(hwnd, None, 0, 0, outer_width, outer_height,
                                           0x0002 | 0x0004 | 0x0010 | 0x4000), "SetWindowPos")

    def screenshot(self, hwnd: int, pid: int, destination: Path) -> dict:
        if not self.owns(hwnd, pid) or parse_state(self.title(hwnd)) is None:
            raise SmokeFailure("Client capture target is not the expected demonstration window")
        width, height = self.client_size(hwnd)
        if width <= 0 or height <= 0 or width * height > 64_000_000:
            raise SmokeFailure(f"Invalid client capture size {width}x{height}")
        memory = bitmap = original = None
        try:
            # This DC contains a new memory bitmap. No screen or window DC is
            # read: PrintWindow asks the target window to render into our bitmap.
            memory = self.gdi.CreateCompatibleDC(None)
            self.require(memory, "CreateCompatibleDC")
            info = BitmapInfo()
            info.header.size = ctypes.sizeof(BitmapInfoHeader)
            info.header.width = width
            info.header.height = height  # Standard bottom-up, 32-bit BMP.
            info.header.planes = 1
            info.header.bit_count = 32
            info.header.image_size = width * height * 4
            bits = ctypes.c_void_p()
            bitmap = self.gdi.CreateDIBSection(memory, ctypes.byref(info), 0,
                                              ctypes.byref(bits), None, 0)
            self.require(bitmap and bits.value, "CreateDIBSection")
            original = self.gdi.SelectObject(memory, bitmap)
            self.require(original and original != ctypes.c_void_p(-1).value, "SelectObject")
            # PW_CLIENTONLY | PW_RENDERFULLCONTENT. This synchronous API runs
            # only in a disposable worker, so a hung target cannot hang the test.
            self.require(self.user.PrintWindow(hwnd, memory, 0x0001 | 0x0002), "PrintWindow(client)")
            self.require(self.gdi.GdiFlush(), "GdiFlush")
            pixels = ctypes.string_at(bits, info.header.image_size)
            if not self.owns(hwnd, pid) or parse_state(self.title(hwnd)) is None:
                raise SmokeFailure("Client capture target changed during PrintWindow")
            dpi = self.user.GetDpiForWindow(hwnd)
            checkpoints = scene_capture_checkpoints(pixels, width, height, dpi / 96.0)
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


def scene_capture_checkpoints(pixels: bytes, width: int, height: int, scale: float) -> dict:
    """Reject empty/incorrect captures before saving any image to disk.

    These points belong to the final test scene: tab 1 active, first row 3.
    Tolerances accept both linear and sRGB swapchain encodings. This guards the
    capture mechanism; it is not a substitute for a visual review of the image.
    """
    if scale <= 0:
        raise SmokeFailure("Cannot validate capture with zero DPI")

    def rgb(logical_x: int, logical_y: int) -> tuple[int, int, int]:
        x, y = round(logical_x * scale), round(logical_y * scale)
        if not (0 <= x < width and 0 <= y < height):
            raise SmokeFailure("Capture is too small for scene checkpoints")
        offset = ((height - 1 - y) * width + x) * 4
        blue, green, red = pixels[offset:offset + 3]
        return red, green, blue

    active = rgb(140, 20)
    accent = rgb(140, 38)
    marker = rgb(258, 198)
    if not (min(active) >= 245 and accent[2] > accent[1] + 20
            and accent[1] > accent[0] + 25 and max(marker) < 200
            and marker[2] > marker[1] > marker[0]):
        raise SmokeFailure("PrintWindow did not capture the expected GPU scene; image not saved")
    return {"active_tab_rgb": active, "blue_underline_rgb": accent, "row_marker_rgb": marker}


def capture_worker(hwnd: int, pid: int, destination: Path, result_queue) -> None:
    try:
        result_queue.put(Windows().screenshot(hwnd, pid, destination))
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
            return {"status": "unavailable", "reason": "PrintWindow capture timed out"}
        try:
            return results.get(timeout=1)
        except queue.Empty:
            return {"status": "unavailable", "reason": f"Capture worker exited with status {worker.exitcode}"}
    finally:
        results.close()
        results.join_thread()
        if worker.pid is not None and not worker.is_alive():
            worker.close()


def packed_point(x: int, y: int) -> int:
    if not (-32768 <= x <= 32767 and -32768 <= y <= 32767):
        raise SmokeFailure("Mouse coordinates exceed the Win32 message range")
    return (x & 0xFFFF) | ((y & 0xFFFF) << 16)


def parse_state(title: str) -> dict | None:
    match = TITLE.fullmatch(title)
    if match is None:
        return None
    frame, first, selected, tab, width, height = match.groups()
    return {"frame": int(frame), "first_row": int(first),
            "selected": None if selected == "none" else int(selected),
            "tab": int(tab), "width": int(width), "height": int(height)}


class Session:
    def __init__(self, windows: Windows, process: subprocess.Popen, report: dict, timeout: float):
        self.windows = windows
        self.process = process
        self.report = report
        self.timeout = timeout
        self.hwnd: int | None = None

    def alive(self) -> None:
        code = self.process.poll()
        if code is not None:
            raise SmokeFailure(f"Application exited unexpectedly with status {code}")
        if self.hwnd is not None and not self.windows.owns(self.hwnd, self.process.pid):
            raise SmokeFailure("The child process window disappeared")

    def wait(self, description: str, predicate):
        deadline = time.monotonic() + self.timeout
        while time.monotonic() < deadline:
            self.alive()
            result = predicate()
            if result:
                return result
            time.sleep(0.05)
        title = self.windows.title(self.hwnd) if self.hwnd else "no visible window"
        raise SmokeFailure(f"Timed out waiting for {description}; last title: {title!r}")

    def wait_state(self, description: str, predicate) -> dict:
        def matches():
            state = parse_state(self.windows.title(self.hwnd))
            return state if state is not None and predicate(state) else None
        state = self.wait(description, matches)
        self.report["checks"].append({"check": description, "state": state})
        return state

    def idle(self, description: str) -> dict:
        # Allow queued expose/resize events to settle, then require an unchanged
        # presented-frame counter throughout a separate idle observation window.
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
                raise SmokeFailure(f"Unexpected redraw/state change during {description}: {previous} -> {current}")
            time.sleep(0.05)
        self.report["checks"].append({"check": description, "idle_seconds": 0.75, "state": previous})
        return previous

    def click(self, logical_x: int, logical_y: int, scale: float) -> None:
        self.alive()
        point = packed_point(round(logical_x * scale), round(logical_y * scale))
        self.windows.post(self.hwnd, WM_MOUSEMOVE, 0, point)
        self.windows.post(self.hwnd, WM_LBUTTONDOWN, 1, point)
        self.windows.post(self.hwnd, WM_LBUTTONUP, 0, point)

    def run(self, output: Path) -> None:
        self.hwnd = self.wait("visible child window", lambda: self.windows.find_window(self.process.pid))
        self.report["window_handle"] = self.hwnd
        initial = self.wait_state("first presented GPU frame", lambda s: s["frame"] >= 1)
        if initial["first_row"] != 0 or initial["selected"] is not None or initial["tab"] != 0:
            raise SmokeFailure(f"Unexpected initial demonstration state: {initial}")
        dpi = self.windows.user.GetDpiForWindow(self.hwnd)
        if dpi == 0:
            raise SmokeFailure("GetDpiForWindow returned zero")
        scale = dpi / 96.0
        self.report["dpi"] = dpi
        self.idle("idle window does not continuously redraw")

        self.click(150, 20, scale)
        tab = self.wait_state("posted tab click", lambda s: s["tab"] == 1 and s["frame"] > initial["frame"])
        self.click(400, 242, scale)
        row = self.wait_state("posted row click", lambda s: s["selected"] == 2 and s["frame"] > tab["frame"])
        screen = wt.POINT(round(400 * scale), round(242 * scale))
        self.windows.require(self.windows.user.ClientToScreen(self.hwnd, ctypes.byref(screen)), "ClientToScreen")
        self.windows.post(self.hwnd, WM_MOUSEWHEEL, ((-120) & 0xFFFF) << 16,
                          packed_point(screen.x, screen.y))
        wheel = self.wait_state("posted wheel scroll", lambda s: s["first_row"] == 3
                                and s["selected"] == 2 and s["frame"] > row["frame"])

        width, height = round(900 * scale), round(600 * scale)
        self.windows.resize_client(self.hwnd, width, height)
        resized = self.wait_state("GPU frame after native resize", lambda s: s["width"] == width
                                  and s["height"] == height and s["frame"] > wheel["frame"])
        if self.windows.client_size(self.hwnd) != (width, height):
            raise SmokeFailure("Reported size does not match the native client area")
        self.windows.require(self.windows.user.ShowWindowAsync(self.hwnd, 6),
                             "ShowWindowAsync(minimize)")  # SW_MINIMIZE
        self.wait("native minimize", lambda: self.windows.user.IsIconic(self.hwnd))
        minimized = self.idle("minimized window does not continuously redraw")
        self.windows.require(self.windows.user.ShowWindowAsync(self.hwnd, 9),
                             "ShowWindowAsync(restore)")  # SW_RESTORE
        self.wait("native restore", lambda: not self.windows.user.IsIconic(self.hwnd))
        self.wait_state("GPU frame after native restore", lambda s: s["frame"] > minimized["frame"]
                        and s["width"] == resized["width"] and s["height"] == resized["height"])
        final = self.idle("restored window returns to idle")
        if (final["tab"], final["selected"], final["first_row"]) != (1, 2, 3):
            raise SmokeFailure(f"State was lost during resize/minimize/restore: {final}")
        self.report["screenshot"] = capture_client(self.hwnd, self.process.pid, output / "client.bmp", self.timeout)
        self.windows.post(self.hwnd, WM_CLOSE)
        try:
            code = self.process.wait(timeout=self.timeout)
        except subprocess.TimeoutExpired as error:
            raise SmokeFailure("Application did not exit after WM_CLOSE") from error
        self.report["exit_code"] = code
        if code != 0:
            raise SmokeFailure(f"Application exited with status {code} after WM_CLOSE")
        self.report["checks"].append({"check": "native close exits successfully", "exit_code": code})

    def cleanup(self) -> None:
        if self.process.poll() is not None:
            return
        if self.hwnd and self.windows.owns(self.hwnd, self.process.pid):
            self.windows.user.PostMessageW(self.hwnd, WM_CLOSE, 0, 0)
            try:
                self.process.wait(timeout=2)
                return
            except subprocess.TimeoutExpired:
                pass
        self.process.terminate()
        try:
            self.process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=2)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", required=True, type=Path, help="Path to a built gpu-shell.exe")
    parser.add_argument("--output-dir", required=True, type=Path, help="Directory for logs, JSON, and client screenshot")
    parser.add_argument("--timeout", type=float, default=20, help="Maximum seconds per operation (default: 20)")
    args = parser.parse_args()
    if sys.platform != "win32":
        parser.error("This smoke test requires an interactive Windows desktop")
    if args.timeout <= 0 or args.timeout > 60:
        parser.error("--timeout must be in the range (0, 60]")
    executable = args.exe.resolve()
    if not executable.is_file():
        parser.error("--exe must name a file")
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = {"status": "failed", "started_utc": datetime.now(timezone.utc).isoformat(),
              "executable": str(executable), "input_method": "Win32 PostMessageW synthetic events",
              "backend_environment": os.environ.get("WGPU_BACKEND"), "checks": []}
    session = None
    try:
        windows = Windows()
        with (output / "stdout.log").open("wb") as stdout, (output / "stderr.log").open("wb") as stderr:
            process = subprocess.Popen([str(executable)], stdout=stdout, stderr=stderr,
                                       creationflags=subprocess.CREATE_NO_WINDOW)
            report["process_id"] = process.pid
            session = Session(windows, process, report, args.timeout)
            try:
                session.run(output)
                report["status"] = "passed"
            finally:
                session.cleanup()
    except (SmokeFailure, OSError, subprocess.SubprocessError) as error:
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

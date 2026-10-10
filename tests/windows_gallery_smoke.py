#!/usr/bin/env python3
"""Exercise the gallery through posted input to its own Win32 child window.

Captures use PrintWindow client-only in a bounded worker. No desktop DC,
global keyboard input or user clipboard is read. The temporary cursor movement
needed for native leave tracking is restored by the retained smoke helpers.
"""
from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes as wt
from datetime import datetime, timezone
import hashlib
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

from windows_smoke import BitmapInfo, BitmapInfoHeader, SmokeFailure, WM_CLOSE
from windows_retained_smoke import (
    RetainedSession, RetainedWindows, WM_MOUSEMOVE, WM_LBUTTONDOWN, WM_LBUTTONUP,
)

TITLE = re.compile(
    r"^Rust UI Engine · (Библиотека|Компоненты|О проекте) · "
    r"(Светлая|Тёмная|Компактная) · (\d+) / (\d+) строк(?: · поиск…)? "
    r"\| frames=(\d+) glyphs=(\d+) missing=(\d+)$"
)


def parse_state(title: str) -> dict | None:
    match = TITLE.fullmatch(title)
    if match is None:
        return None
    page, theme, rows, source_rows, frames, glyphs, missing = match.groups()
    return {"page": page, "theme": theme, "rows": int(rows), "source_rows": int(source_rows),
            "frames": int(frames), "glyphs": int(glyphs), "missing": int(missing)}


def capture_checkpoints(pixels: bytes, width: int, height: int, scale: float, theme: str) -> dict:
    """Require the expected canvas and raster text pixels before saving a capture.

This verifies capture content, not textual correctness; screenshots still need
visual inspection. Direct and sRGB swapchain channel encodings are accepted.
"""
    if (not math.isfinite(scale) or scale <= 0 or len(pixels) != width * height * 4
            or width < 300 * scale or height < 240 * scale):
        raise SmokeFailure("Invalid gallery capture dimensions or DPI")

    def rgb(x: int, y: int) -> tuple[int, int, int]:
        offset = ((height - 1 - y) * width + x) * 4
        b, g, r = pixels[offset:offset + 3]
        return r, g, b

    def encoded(color):
        def channel(value):
            linear = value / 255
            return round(255 * (12.92 * linear if linear <= 0.0031308
                               else 1.055 * linear ** (1 / 2.4) - 0.055))
        return tuple(channel(value) for value in color)

    def matches(actual, expected, tolerance):
        return any(all(abs(a - b) <= tolerance for a, b in zip(actual, option, strict=True))
                   for option in (expected, encoded(expected)))

    dark = theme == "Тёмная"
    canvas = (25, 29, 37) if dark else (240, 244, 249)
    foreground = (236, 241, 250) if dark else (30, 43, 61)
    actual = rgb(round(3 * scale), round(3 * scale))
    if not matches(actual, canvas, 5):
        raise SmokeFailure(f"Unexpected gallery canvas {actual} for {theme}; image not saved")
    ink = 0
    shades = set()
    # QAT + Ribbon labels, excluding the outer canvas and TabControl below.
    for y in range(round(15 * scale), round(185 * scale)):
        for x in range(round(16 * scale), min(width, round(450 * scale))):
            color = rgb(x, y)
            if matches(color, foreground, 16):
                ink += 1
                shades.add(color)
    if ink < 40 or len(shades) < 8:
        raise SmokeFailure(f"No credible raster text in gallery capture: ink={ink}, shades={len(shades)}")
    return {"canvas_rgb": actual, "text_pixels": ink, "text_shades": len(shades),
            "sha256_pixels": hashlib.sha256(pixels).hexdigest()}


class GalleryWindows(RetainedWindows):
    def capture_state(self, hwnd: int) -> dict | None:
        return parse_state(self.title(hwnd))

    def verify_capture(self, pixels: bytes, width: int, height: int, scale: float, state: dict) -> dict:
        return capture_checkpoints(pixels, width, height, scale, state["theme"])

    def find_window(self, pid: int) -> int | None:
        found = []

        @self.enum_callback
        def collect(hwnd: int, _parameter: int) -> bool:
            if (self.owns(hwnd, pid) and self.user.IsWindowVisible(hwnd)
                    and self.title(hwnd).startswith("Rust UI Engine ·")):
                found.append(hwnd)
            return True

        self.require(self.user.EnumWindows(collect, 0), "EnumWindows")
        return found[0] if found else None

    def screenshot(self, hwnd: int, pid: int, destination: Path) -> dict:
        state = self.capture_state(hwnd)
        if not self.owns(hwnd, pid) or state is None:
            raise SmokeFailure("Capture target is not the gallery child window")
        width, height = self.client_size(hwnd)
        if width <= 0 or height <= 0 or width * height > 64_000_000:
            raise SmokeFailure(f"Invalid client size {width}x{height}")
        memory = bitmap = original = None
        try:
            memory = self.gdi.CreateCompatibleDC(None)
            self.require(memory, "CreateCompatibleDC")
            info = BitmapInfo()
            info.header.size = ctypes.sizeof(BitmapInfoHeader)
            info.header.width, info.header.height = width, height
            info.header.planes, info.header.bit_count = 1, 32
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
            after = self.capture_state(hwnd)
            if (not self.owns(hwnd, pid) or after is None
                    or after["page"] != state["page"] or after["theme"] != state["theme"]
                    or after["frames"] < state["frames"]):
                raise SmokeFailure("Gallery window changed during capture")
            checkpoints = self.verify_capture(pixels, width, height,
                self.user.GetDpiForWindow(hwnd) / 96.0, state)
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
                  "state_before": state, "state_after": after,
                  "captured_utc": datetime.now(timezone.utc).isoformat(), "checkpoints": checkpoints}
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
        result_queue.put(GalleryWindows().screenshot(hwnd, pid, destination))
    except Exception as error:
        result_queue.put({"status": "unavailable", "reason": str(error)})


def capture_client(hwnd: int, pid: int, destination: Path, timeout: float, *,
                   worker_target=capture_worker, worker_args=()) -> dict:
    context = multiprocessing.get_context("spawn")
    results = context.Queue()
    worker = context.Process(target=worker_target,
                             args=(hwnd, pid, destination, results, *worker_args))
    try:
        worker.start()
        worker.join(timeout=min(timeout, 5.0))
        if worker.is_alive():
            worker.terminate()
            worker.join(timeout=2)
            if worker.is_alive():
                worker.kill()
                worker.join(timeout=2)
            return {"status": "unavailable", "reason": "Gallery PrintWindow capture timed out"}
        try:
            return results.get(timeout=1)
        except queue.Empty:
            return {"status": "unavailable", "reason": f"Capture worker exited with status {worker.exitcode}"}
    finally:
        results.close()
        results.join_thread()
        if worker.pid is not None and not worker.is_alive():
            worker.close()


class GallerySession(RetainedSession):
    def wait_state(self, description: str, predicate) -> dict:
        def matches():
            state = parse_state(self.windows.title(self.hwnd))
            return state if state is not None and predicate(state) else None
        state = self.wait(description, matches)
        self.report["checks"].append({"check": description, "state": state})
        return state

    def click(self, logical_x: int, logical_y: int, scale: float) -> None:
        self.focus_messages(True)
        self.pointer(WM_MOUSEMOVE, logical_x, logical_y, scale)
        self.pointer(WM_LBUTTONDOWN, logical_x, logical_y, scale, held=True)
        self.pointer(WM_LBUTTONUP, logical_x, logical_y, scale)

    def settle(self) -> None:
        previous = None
        stable_since = time.monotonic()
        def stable():
            nonlocal previous, stable_since
            state = parse_state(self.windows.title(self.hwnd))
            if state is None or state != previous:
                previous, stable_since = state, time.monotonic()
                return False
            return time.monotonic() - stable_since >= 0.25
        self.wait("gallery frames settle before capture/input", stable)

    def capture(self, output: Path, name: str) -> dict:
        self.settle()
        result = capture_client(self.hwnd, self.process.pid, output / f"{name}.bmp", self.timeout)
        self.report["captures"][name] = result
        if result["status"] != "available":
            raise SmokeFailure(f"Required {name} capture unavailable: {result.get('reason')}")
        return result

    def run(self, output: Path) -> None:
        self.hwnd = self.wait("visible gallery child window", lambda: self.windows.find_window(self.process.pid))
        self.report["window_handle"] = self.hwnd
        initial = self.wait_state("first GPU frame contains glyphs", lambda s: s["frames"] >= 1 and s["glyphs"] > 100)
        if initial["page"] != "Библиотека" or initial["theme"] != "Светлая":
            raise SmokeFailure(f"Unexpected initial gallery state: {initial}")
        dpi = self.windows.user.GetDpiForWindow(self.hwnd)
        if dpi <= 0:
            raise SmokeFailure("GetDpiForWindow returned zero")
        scale = dpi / 96.0
        self.report["dpi"] = dpi
        self.windows.topmost_without_activation(self.hwnd, True)
        self.promoted = True
        self.focus_messages(True)
        width, height = self.windows.client_size(self.hwnd)
        self.pointer(WM_MOUSEMOVE, round(width / scale) - 5, round(height / scale) - 5, scale)
        self.capture(output, "library-light")

        # The TabControl spans the body width directly below the 196px Ribbon.
        self.click(round(width / scale / 2), 229, scale)
        controls = self.wait_state("posted Controls tab click", lambda s:
            s["page"] == "Компоненты" and s["frames"] > initial["frames"] and s["glyphs"] > 100)
        self.capture(output, "controls-light")

        # View Ribbon tab, then the small Dark command in its Theme group.
        # Posted pointer input is independent of the active keyboard language.
        self.click(230, 60, scale)
        view = self.wait_state("posted View Ribbon tab click", lambda s: s["frames"] > controls["frames"])
        self.settle()
        self.capture(output, "view-ribbon-light")
        self.click(155, 97, scale)
        dark = self.wait_state("posted Dark command changes theme", lambda s:
            s["theme"] == "Тёмная" and s["page"] == "Компоненты" and s["frames"] > view["frames"])
        self.capture(output, "controls-dark")

        target = (round(960 * scale), round(680 * scale))
        self.windows.resize_client(self.hwnd, *target)
        self.wait("native client resize", lambda: self.windows.client_size(self.hwnd) == target)
        self.wait_state("resized gallery redraw preserves page/theme", lambda s:
            s["frames"] > dark["frames"] and s["theme"] == "Тёмная" and s["page"] == "Компоненты")
        self.capture(output, "controls-dark-resized")
        self.windows.post(self.hwnd, WM_CLOSE)
        try:
            code = self.process.wait(timeout=self.timeout)
        except subprocess.TimeoutExpired as error:
            raise SmokeFailure("Gallery did not exit after WM_CLOSE") from error
        if code != 0:
            raise SmokeFailure(f"Gallery exited with status {code}")
        self.report["checks"].append({"check": "native close exits successfully", "exit_code": code})


def self_test() -> None:
    title = "Rust UI Engine · Компоненты · Тёмная · 100000 / 100000 строк | frames=12 glyphs=450 missing=0"
    assert parse_state(title)["page"] == "Компоненты"
    assert parse_state(title)["frames"] == 12
    assert parse_state(title + " extra") is None
    assert parse_state(title.replace("Тёмная", "unknown")) is None
    width, height = 600, 400
    pixels = bytearray(bytes((249, 244, 240, 255)) * width * height)
    for y in range(25, 45):
        for x in range(30, 80):
            offset = ((height - 1 - y) * width + x) * 4
            shade = x % 12
            pixels[offset:offset+4] = bytes((61+shade, 43+shade, 30+shade, 255))
    assert capture_checkpoints(pixels, width, height, 1.0, "Светлая")["text_pixels"] >= 40
    for invalid in [bytes(width*height*4), bytes((249,244,240,255))*width*height]:
        try:
            capture_checkpoints(invalid, width, height, 1.0, "Светлая")
        except SmokeFailure:
            pass
        else:
            raise AssertionError("Empty/incorrect capture must be rejected")
    print("PASS: gallery title parser and raster-content capture fixtures")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, help="Path to controls-gallery.exe")
    parser.add_argument("--output-dir", type=Path, help="Logs, report and client captures")
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
              "executable": str(executable), "mode": "gallery", "checks": [], "captures": {},
              "input_method": "Posted child-only Win32 pointer messages; temporary restored cursor alignment",
              "backend_environment": os.environ.get("WGPU_BACKEND"),
              "limits": ["No native IME or screen reader test", "Raster checkpoints require separate visual review"]}
    session = None
    try:
        windows = GalleryWindows()
        original_cursor = windows.cursor_position()
        with (output / "stdout.log").open("wb") as stdout, (output / "stderr.log").open("wb") as stderr:
            process = subprocess.Popen([str(executable), "--rows", "100000", "--state-file", str(output / "columns.txt")],
                                       stdout=stdout, stderr=stderr, creationflags=subprocess.CREATE_NO_WINDOW)
            report["process_id"] = process.pid
            session = GallerySession(windows, process, report, args.timeout, original_cursor)
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

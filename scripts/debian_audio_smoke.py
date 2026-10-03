#!/usr/bin/env python3
"""Test named microphone capture and indicator lifecycle with isolated audio.

Run under xvfb-run / dbus-run-session. Requires pulseaudio and the dependencies
of debian_smoke.py. No physical microphone or cloud credentials are used.
"""
import ctypes
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

from debian_smoke import Atspi, application, close_settings, find, visible_frames, wait_for


class TranscriptionStub(BaseHTTPRequestHandler):
    def do_POST(self):
        assert self.path == "/v1/audio/transcriptions", self.path
        body = self.rfile.read(int(self.headers["Content-Length"]))
        assert b"RIFF" in body, "No captured WAV in transcription request"
        self.server.request_received.set()
        time.sleep(2)  # Keep processing visible long enough to inspect the real UI.
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"text":""}')

    def log_message(self, *args):
        pass


def press_hotkey(down):
    x = ctypes.CDLL("libxdo.so.3")
    x.xdo_new.argtypes = [ctypes.c_char_p]
    x.xdo_new.restype = ctypes.c_void_p
    x.xdo_free.argtypes = [ctypes.c_void_p]
    function = getattr(x, "xdo_send_keysequence_window_" + ("down" if down else "up"))
    function.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_char_p, ctypes.c_ulong]
    context = x.xdo_new(None)
    try:
        assert function(context, 0, b"Alt+grave", 12000) == 0
    finally:
        x.xdo_free(context)


def nodes(node):
    if node is None:
        return
    yield node
    for index in range(node.get_child_count()):
        yield from nodes(node.get_child_at_index(index))


def status_text(text):
    for node in nodes(application()):
        if node.get_name() == text:
            return True
        try:
            if text in node.get_text_iface().get_text(0, -1):
                return True
        except (NotImplementedError, AttributeError):
            pass
    return False


def assert_rounded_window():
    """Inspect the native bounding shape, independent of CSS or a compositor."""
    class Rectangle(ctypes.Structure):
        _fields_ = [("x", ctypes.c_short), ("y", ctypes.c_short),
                    ("width", ctypes.c_ushort), ("height", ctypes.c_ushort)]

    x = ctypes.CDLL("libX11.so.6")
    shape = ctypes.CDLL("libXext.so.6")
    x.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x.XOpenDisplay.restype = ctypes.c_void_p
    x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
    x.XDefaultRootWindow.restype = ctypes.c_ulong
    x.XQueryTree.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong),
                            ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)),
                            ctypes.POINTER(ctypes.c_uint)]
    x.XFetchName.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_char_p)]
    x.XFree.argtypes = [ctypes.c_void_p]
    x.XCloseDisplay.argtypes = [ctypes.c_void_p]
    shape.XShapeGetRectangles.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int,
                                        ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_int)]
    shape.XShapeGetRectangles.restype = ctypes.POINTER(Rectangle)
    display = x.XOpenDisplay(None)
    assert display, "Cannot open test display"
    children = ctypes.POINTER(ctypes.c_ulong)()
    try:
        root, parent, count = ctypes.c_ulong(), ctypes.c_ulong(), ctypes.c_uint()
        assert x.XQueryTree(display, x.XDefaultRootWindow(display), ctypes.byref(root),
                            ctypes.byref(parent), ctypes.byref(children), ctypes.byref(count))
        for index in range(count.value):
            name = ctypes.c_char_p()
            x.XFetchName(display, children[index], ctypes.byref(name))
            title = name.value
            if name:
                x.XFree(name)
            if title != b"easySTT Recording":
                continue
            length, ordering = ctypes.c_int(), ctypes.c_int()
            rectangles = shape.XShapeGetRectangles(display, children[index], 0,
                                                    ctypes.byref(length), ctypes.byref(ordering))
            try:
                def contains(px, py):
                    return any(r.x <= px < r.x + r.width and r.y <= py < r.y + r.height
                               for r in rectangles[:length.value])
                assert contains(90, 22), "Indicator center was clipped"
                assert all(not contains(px, py) for px, py in [(0, 0), (179, 0), (0, 43), (179, 43)]), "Opaque square indicator corners"
                return True
            finally:
                x.XFree(rectangles)
        raise AssertionError("Native indicator window missing")
    finally:
        if children:
            x.XFree(children)
        x.XCloseDisplay(display)


def main():
    binary = str(Path(sys.argv[1]).resolve())
    with tempfile.TemporaryDirectory(prefix="easystt-audio-") as directory:
        root = Path(directory)
        runtime = root / "pulse"
        runtime.mkdir(mode=0o700)
        socket = root / "audio.sock"
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"),
                   XDG_DATA_HOME=str(root / "data"), GDK_BACKEND="x11",
                   PULSE_RUNTIME_PATH=str(runtime), PULSE_SERVER="unix:" + str(socket))
        env.pop("WAYLAND_DISPLAY", None)
        os.environ.update(XDG_CONFIG_HOME=env["XDG_CONFIG_HOME"], XDG_DATA_HOME=env["XDG_DATA_HOME"])
        server = ThreadingHTTPServer(("127.0.0.1", 0), TranscriptionStub)
        server.request_received = threading.Event()
        threading.Thread(target=server.serve_forever, daemon=True).start()
        config = root / "data/com.easystt.desktop/settings.json"
        config.parent.mkdir(parents=True)
        config.write_text(json.dumps({
            "cloudruApiKey": "local-test-token", "cloudruBaseUrl": f"http://127.0.0.1:{server.server_port}/v1",
            "micDeviceName": "pulse:easystt_mic", "hotkey": "Alt+`", "sttBackend": "cloudru",
        }))
        with (root / "pulse.log").open("w+") as pulse_log, (root / "app.log").open("w+") as app_log:
            pulse = subprocess.Popen([
                "pulseaudio", "-n", "--daemonize=no", "--exit-idle-time=-1", "--use-pid-file=no",
                "--load=module-native-protocol-unix socket=" + str(socket) + " auth-anonymous=1",
                "--load=module-null-sink sink_name=easystt_test",
                "--load=module-remap-source master=easystt_test.monitor source_name=easystt_mic source_properties=device.description=CI_Microphone",
            ], env=env, stdout=pulse_log, stderr=subprocess.STDOUT)
            process = None
            try:
                wait_for(socket.exists, "Private PulseAudio server did not start")
                process = subprocess.Popen([binary], env=env, stdout=app_log, stderr=subprocess.STDOUT)
                manifest = root / "config/agent-hub/agents/easystt.json"
                wait_for(manifest.exists, "App did not start")
                endpoint = json.loads(manifest.read_text())["endpoint"]
                time.sleep(2)
                assert not visible_frames(), "Unexpected startup window"
                urllib.request.urlopen(urllib.request.Request(endpoint + "/open-native-ui", method="POST")).close()
                wait_for(lambda: find(application(), "Обновить микрофоны"), "No microphone refresh button")
                wait_for(lambda: status_text("CI_Microphone"), "Named microphone missing")
                names = [node.get_name() for node in nodes(application())]
                assert not any(name.startswith("Monitor of") or name.startswith("plughw:") for name in names), names
                close_settings()
                wait_for(lambda: not visible_frames(), "Settings did not close")
                # Three sessions also exercise old completion timers against a new recording.
                for session in range(3):
                    server.request_received.clear()
                    press_hotkey(True)
                    try:
                        wait_for(lambda: visible_frames() == ["easySTT Recording"] and status_text("Запись…"), "No recording indicator")
                        indicator = next(node for node in nodes(application()) if node.get_name() == "easySTT Recording")
                        def bottom_center():
                            rect = indicator.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
                            return rect if (abs(rect.x + rect.width / 2 - 640) <= 3
                                            and rect.y >= 900 and rect.width == 180 and rect.height == 44) else None
                        # GTK applies resize/move asynchronously on its first realization.
                        # xvfb-run's default screen is 1280x1024.
                        extents = wait_for(bottom_center, "Indicator did not settle at bottom center", timeout=5)
                        assert_rounded_window()
                        # The null sink's first monitor block can take 2 seconds.
                        time.sleep(3.5)
                    finally:
                        press_hotkey(False)
                    wait_for(server.request_received.is_set, "Transcription did not receive microphone audio")
                    wait_for(lambda: status_text("Обработка…"), "No processing indicator")
                    wait_for(lambda: status_text("Ошибка обработки"), "Completion did not reach indicator")
                    if session == 2:
                        wait_for(lambda: not visible_frames(), "Completed indicator stayed visible")
                assert process.poll() is None
                print("PASS: named input, excluded monitors, shared capture, rounded bottom-center recording/processing and auto-hide")
            except Exception:
                print("Window geometry:", [(node.get_name(), tuple(getattr(node.get_component_iface().get_extents(Atspi.CoordType.SCREEN), axis) for axis in ("x", "y", "width", "height"))) for node in nodes(application()) if node.get_role_name() == "frame"], file=sys.stderr)
                print("Accessible UI:", [(node.get_role_name(), node.get_name(), node.get_state_set().contains(Atspi.StateType.SHOWING)) for node in nodes(application())], file=sys.stderr)
                pulse_log.flush()
                app_log.flush()
                print((root / "pulse.log").read_text(), file=sys.stderr)
                print((root / "app.log").read_text(), file=sys.stderr)
                raise
            finally:
                for child in (process, pulse):
                    if child is not None:
                        child.terminate()
                        try:
                            child.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            child.kill()
                            child.wait()
                server.shutdown()


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        if os.environ.get("GITHUB_ACTIONS") == "true":
            message = str(error).replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
            print(f"::error::{type(error).__name__}: {message}", flush=True)
        raise

#!/usr/bin/env python3
"""Exercise the packaged native UI on Debian under Xvfb and a private D-Bus session.

Usage: xvfb-run -a dbus-run-session -- python3 scripts/debian_smoke.py BINARY
Requires python3-gi, gir1.2-atspi-2.0, at-spi2-core, dbus, xvfb and xauth.
"""

import ctypes
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import urllib.request

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi


def wait_for(check, description, timeout=20):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(0.1)
    raise AssertionError(description)


def application():
    desktop = Atspi.get_desktop(0)
    for i in range(desktop.get_child_count()):
        app = desktop.get_child_at_index(i)
        if app.get_name().lower() == "easystt":
            return app
    return None


def find(node, name, role="push button"):
    if node is None:
        return None
    if node.get_name() == name and node.get_role_name() in (role, "button"):
        return node
    for i in range(node.get_child_count()):
        found = find(node.get_child_at_index(i), name, role)
        if found:
            return found
    return None


def visible_frames():
    app = application()
    if app is None:
        return []
    return [app.get_child_at_index(i).get_name()
            for i in range(app.get_child_count())
            if app.get_child_at_index(i).get_state_set().contains(Atspi.StateType.SHOWING)]


def close_settings():
    """Send the real WM close request; no window manager is needed in Xvfb."""
    x = ctypes.CDLL("libX11.so.6")
    display_t = ctypes.c_void_p
    window_t = ctypes.c_ulong
    x.XOpenDisplay.restype = display_t
    x.XDefaultRootWindow.argtypes = [display_t]
    x.XDefaultRootWindow.restype = window_t
    x.XQueryTree.argtypes = [display_t, window_t, ctypes.POINTER(window_t),
                            ctypes.POINTER(window_t), ctypes.POINTER(ctypes.POINTER(window_t)),
                            ctypes.POINTER(ctypes.c_uint)]
    x.XFetchName.argtypes = [display_t, window_t, ctypes.POINTER(ctypes.c_char_p)]
    x.XGetWindowProperty.argtypes = [display_t, window_t, window_t, ctypes.c_long,
                                    ctypes.c_long, ctypes.c_int, window_t,
                                    ctypes.POINTER(window_t), ctypes.POINTER(ctypes.c_int),
                                    ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_ulong),
                                    ctypes.POINTER(ctypes.c_void_p)]
    x.XInternAtom.argtypes = [display_t, ctypes.c_char_p, ctypes.c_int]
    x.XInternAtom.restype = window_t
    x.XSendEvent.argtypes = [display_t, window_t, ctypes.c_int, ctypes.c_long, ctypes.c_void_p]
    x.XFlush.argtypes = x.XCloseDisplay.argtypes = [display_t]
    x.XFree.argtypes = [ctypes.c_void_p]
    display = x.XOpenDisplay(None)
    assert display, "No X11 display"

    class Message(ctypes.Structure):
        _fields_ = [("type", ctypes.c_int), ("serial", ctypes.c_ulong),
                    ("send_event", ctypes.c_int), ("display", display_t),
                    ("window", window_t), ("message_type", window_t),
                    ("format", ctypes.c_int), ("data", ctypes.c_long * 5)]

    root, parent, count = window_t(), window_t(), ctypes.c_uint()
    children = ctypes.POINTER(window_t)()
    try:
        assert x.XQueryTree(display, x.XDefaultRootWindow(display), ctypes.byref(root),
                            ctypes.byref(parent), ctypes.byref(children), ctypes.byref(count))
        for i in range(count.value):
            title, actual_type = ctypes.c_void_p(), window_t()
            actual_format, length, remaining = ctypes.c_int(), ctypes.c_ulong(), ctypes.c_ulong()
            x.XGetWindowProperty(display, children[i], x.XInternAtom(display, b"_NET_WM_NAME", 0),
                                 0, 1024, 0, 0, ctypes.byref(actual_type), ctypes.byref(actual_format),
                                 ctypes.byref(length), ctypes.byref(remaining), ctypes.byref(title))
            if title.value:
                name = ctypes.string_at(title, length.value).decode("utf-8")
                x.XFree(title)
                if name == "easySTT — Настройки":
                    event = ctypes.create_string_buffer(192)
                    message = Message.from_buffer(event)
                    message.type, message.display, message.window, message.format = 33, display, children[i], 32
                    message.message_type = x.XInternAtom(display, b"WM_PROTOCOLS", 0)
                    message.data[0] = x.XInternAtom(display, b"WM_DELETE_WINDOW", 0)
                    assert x.XSendEvent(display, children[i], 0, 0, event)
                    x.XFlush(display)
                    return
        raise AssertionError("Settings X11 window not found")
    finally:
        if children:
            x.XFree(children)
        x.XCloseDisplay(display)


def main():
    binary = str(Path(sys.argv[1]).resolve())
    with tempfile.TemporaryDirectory(prefix="easystt-smoke-") as directory:
        env = dict(os.environ, XDG_CONFIG_HOME=directory + "/config",
                   XDG_DATA_HOME=directory + "/data", GDK_BACKEND="x11")
        for key in ("WAYLAND_DISPLAY", "WEBKIT_DISABLE_DMABUF_RENDERER", "WEBKIT_DISABLE_COMPOSITING_MODE"):
            env.pop(key, None)
        # Keep the accessibility registry on the same private bus as the app.
        os.environ.update(XDG_CONFIG_HOME=env["XDG_CONFIG_HOME"], XDG_DATA_HOME=env["XDG_DATA_HOME"])
        log_path = Path(directory) / "app.log"
        with log_path.open("w+") as log:
            process = subprocess.Popen([binary], env=env, stdout=log, stderr=subprocess.STDOUT)
            try:
                manifest = Path(env["XDG_CONFIG_HOME"]) / "agent-hub/agents/easystt.json"
                wait_for(manifest.exists, "App did not start")
                endpoint = json.loads(manifest.read_text())["endpoint"]
                time.sleep(2)  # Allow startup JavaScript and any delayed window callbacks to run.
                assert process.poll() is None and not visible_frames(), "Unexpected startup window"

                def open_settings():
                    with urllib.request.urlopen(urllib.request.Request(endpoint + "/open-native-ui", method="POST")) as response:
                        assert response.status == 200
                    wait_for(lambda: find(application(), "Основные"), "Settings did not render")

                open_settings()
                for tab in ("Основные", "Внешний вид", "Распознавание", "Горячие клавиши", "Плагины"):
                    button = wait_for(lambda: find(application(), tab), "Missing tab: " + tab)
                    assert button.get_action_iface().do_action(0)
                    time.sleep(0.2)
                wait_for(lambda: find(application(), "+ Добавить плагин"), "Plugins did not render")
                assert find(application(), "Сохранить").get_action_iface().do_action(0)
                wait_for(lambda: find(application(), "✓ Сохранено"), "Settings did not save")
                close_settings()
                wait_for(lambda: not visible_frames(), "Closing settings showed a floating window")
                assert process.poll() is None, "Closing settings terminated the app"
                open_settings()
                assert visible_frames() == ["easySTT — Настройки"], "Unexpected visible windows"
                print("PASS: tray-only startup, five settings tabs, save, close and reopen")
            except Exception:
                log.flush()
                print(log_path.read_text(), file=sys.stderr)
                raise
            finally:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        if os.environ.get("GITHUB_ACTIONS") == "true":
            message = str(error).replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
            print(f"::error::{type(error).__name__}: {message}", flush=True)
        raise

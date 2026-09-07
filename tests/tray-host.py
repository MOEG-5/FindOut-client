#!/usr/bin/python3
"""Start a client, then provide a delayed fake StatusNotifier watcher."""

import os
from pathlib import Path
import subprocess
import sys

# Always create a private display and a bus that cannot activate the real keychain.
if os.environ.get("FINDOUT_ISOLATED_TRAY_TEST") != "1":
    raise SystemExit(subprocess.call([
        "dbus-run-session", f"--config-file={Path(__file__).with_name('dbus-session.conf')}",
        "--", "xvfb-run", "-a", "env", "-u", "WAYLAND_DISPLAY", "-u", "WAYLAND_SOCKET",
        "FINDOUT_ISOLATED_TRAY_TEST=1", sys.executable, __file__, *sys.argv[1:],
    ]))

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib


if len(sys.argv) < 2:
    raise SystemExit(f"usage: {sys.argv[0]} CLIENT [STARTUP_DELAY_SECONDS]")

delay = float(sys.argv[2]) if len(sys.argv) > 2 else 1.0
child = subprocess.Popen([sys.argv[1]])
loop = GLib.MainLoop()
bus = None
owner_id = 0
object_id = 0
registered = []
success = False

WATCHER_XML = """
<node>
  <interface name="org.kde.StatusNotifierWatcher">
    <method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
    <method name="RegisterStatusNotifierHost"><arg type="s" direction="in"/></method>
    <property name="RegisteredStatusNotifierItems" type="as" access="read"/>
    <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
  </interface>
</node>
"""
watcher = Gio.DBusNodeInfo.new_for_xml(WATCHER_XML).interfaces[0]


def call(_bus, sender, _path, _interface, method, parameters, invocation):
    global success
    if method == "RegisterStatusNotifierItem":
        service = parameters.unpack()[0]
        registered.append((sender, service))
        print(f"registered item: sender={sender} service={service}", flush=True)
        success = True
    invocation.return_value(GLib.Variant("()", ()))
    if success:
        loop.quit()


def get_property(_bus, _sender, _path, _interface, name):
    if name == "RegisteredStatusNotifierItems":
        return GLib.Variant("as", [service for _sender, service in registered])
    if name == "IsStatusNotifierHostRegistered":
        return GLib.Variant("b", True)
    return None


def start_watcher():
    global bus, owner_id, object_id
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    object_id = bus.register_object_with_closures2(
        "/StatusNotifierWatcher", watcher, call, get_property, None
    )
    owner_id = Gio.bus_own_name_on_connection(
        bus,
        "org.kde.StatusNotifierWatcher",
        Gio.BusNameOwnerFlags.NONE,
        lambda *_: print("fake watcher ready", flush=True),
        lambda *_: loop.quit(),
    )
    return GLib.SOURCE_REMOVE


def timeout():
    print("timed out waiting for RegisterStatusNotifierItem", flush=True)
    loop.quit()
    return GLib.SOURCE_REMOVE


GLib.timeout_add(int(delay * 1000), start_watcher)
GLib.timeout_add(8000, timeout)
try:
    loop.run()
finally:
    if owner_id:
        Gio.bus_unown_name(owner_id)
    if bus and object_id:
        bus.unregister_object(object_id)
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=2)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()

raise SystemExit(0 if success else 1)

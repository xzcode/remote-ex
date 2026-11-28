#!/usr/bin/env python3
"""Sample nine pixels from a disposable CI Xvfb root window.

Independent of the Remote EX capture and decoder. No full screenshot,
filesystem output, physical screen access, or network connection.
"""
import ctypes as C
import json
import os
import re

assert os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted"
assert os.environ.get("XDG_SESSION_TYPE") == "x11"
assert re.fullmatch(r":[0-9]+", os.environ.get("DISPLAY", ""))
auth = os.environ.get("XAUTHORITY", "")
assert auth and os.path.isabs(auth) and os.path.isfile(auth)

class Color(C.Structure):
    _fields_ = [("pixel", C.c_ulong), ("red", C.c_ushort),
                ("green", C.c_ushort), ("blue", C.c_ushort),
                ("flags", C.c_char), ("pad", C.c_char)]

x11 = C.CDLL("libX11.so.6")
def bind(name, result, *args):
    f = getattr(x11, name)
    f.restype, f.argtypes = result, args
    return f

ptr, integer, ulong = C.c_void_p, C.c_int, C.c_ulong
open_display = bind("XOpenDisplay", ptr, C.c_char_p)
close_display = bind("XCloseDisplay", integer, ptr)
default_screen = bind("XDefaultScreen", integer, ptr)
root_window = bind("XRootWindow", ulong, ptr, integer)
display_width = bind("XDisplayWidth", integer, ptr, integer)
display_height = bind("XDisplayHeight", integer, ptr, integer)
get_image = bind("XGetImage", ptr, ptr, ulong, integer, integer,
                 C.c_uint, C.c_uint, ulong, integer)
get_pixel = bind("XGetPixel", ulong, ptr, integer, integer)
destroy_image = bind("XDestroyImage", integer, ptr)
colormap = bind("XDefaultColormap", ulong, ptr, integer)
query_color = bind("XQueryColor", integer, ptr, ulong, C.POINTER(Color))
display = open_display(None)
assert display, "Cannot open the isolated source display"
try:
    screen = default_screen(display)
    width, height = display_width(display, screen), display_height(display, screen)
    assert (width, height) == (1024, 768), "Unexpected source fixture dimensions"
    root, cmap = root_window(display, screen), colormap(display, screen)
    pixels = []
    for y in (.35, .5, .65):
        for x in (.4, .5, .6):
            image = get_image(display, root, int(width*x), int(height*y), 1, 1,
                              C.c_ulong(-1).value, 2)  # ZPixmap
            assert image, "Cannot sample source root"
            try:
                color = Color()
                color.pixel = get_pixel(image, 0, 0)
                assert query_color(display, cmap, C.byref(color)), "Cannot resolve source pixel"
                pixels.append([round(v/257) for v in (color.red, color.green, color.blue)])
            finally:
                destroy_image(image)
    print(json.dumps(pixels, separators=(",", ":")))
finally:
    close_display(display)

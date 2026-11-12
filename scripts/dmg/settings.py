# dmgbuild settings for DiskPilot.dmg. dmgbuild writes the window layout
# itself instead of asking Finder, so it works on CI runners.
#   dmgbuild -s scripts/dmg/settings.py -D app=path/to/DiskPilot.app \
#     -D background=scripts/dmg/background.tiff "DiskPilot" out.dmg
# background.tiff holds the 1x and 2x images (see background.py).
import os.path

app = defines["app"]  # noqa: F821 (provided by dmgbuild)
# dmgbuild runs this file without __file__; the background is passed in too.
background = defines["background"]  # noqa: F821

format = "UDZO"
filesystem = "HFS+"
files = [app]
symlinks = {"Applications": "/Applications"}
hide_extension = [os.path.basename(app)]

window_rect = ((200, 160), (600, 380))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
icon_size = 112
text_size = 13
icon_locations = {
    os.path.basename(app): (160, 175),
    "Applications": (440, 175),
}

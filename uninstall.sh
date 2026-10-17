#!/usr/bin/env bash
# Removes F75 Studio. Keeps your profiles in ~/.config/f75-studio unless you pass --purge.
# Settings already written to the keyboard stay on the keyboard.
set -euo pipefail
if [ "$(id -u)" = 0 ]; then
    echo "Run ./uninstall.sh as your normal user (not with sudo). It asks for the password when needed."
    exit 1
fi
export PATH="$HOME/.local/bin:$PATH"
command -v f75-studio >/dev/null && timeout 5 f75-studio quit </dev/null >/dev/null 2>&1 || true
rm -f ~/.local/bin/f75-studio \
      ~/.local/share/applications/f75-studio.desktop \
      ~/.local/share/icons/hicolor/256x256/apps/f75-studio.png
update-desktop-database ~/.local/share/applications >/dev/null 2>&1 || true
gtk-update-icon-cache -q ~/.local/share/icons/hicolor >/dev/null 2>&1 || true
echo "Removing the keyboard access rule (sudo)"
sudo rm -f /etc/udev/rules.d/70-aula-f75.rules /etc/udev/rules.d/99-aula-f75.rules
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw --action=change || true
if [ "${1:-}" = "--purge" ]; then
    rm -rf ~/.config/f75-studio
    echo "Profiles removed."
fi
echo "F75 Studio removed. (The GNOME AppIndicator extension was left installed.)"

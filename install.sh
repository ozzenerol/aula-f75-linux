#!/usr/bin/env bash
# F75 Studio installer for Linux (Fedora, Ubuntu/Debian, Arch). Safe to run again to update.
# Installs build tools, builds the app, adds it to your app menu and gives your
# user access to the keyboard. Asks for your password (sudo) for system steps.
set -euo pipefail
cd "$(dirname "$0")"

RUST_MIN=1.95   # oldest Rust that builds the GUI toolkit (eframe 0.36)
step() { printf '\n\033[1;32m==> %s\033[0m\n' "$*"; }
note() { printf '    %s\n' "$*"; }

if [ "$(id -u)" = 0 ]; then
    echo "Run ./install.sh as your normal user (not with sudo). It asks for the password when needed."
    exit 1
fi

need_path_note=0
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
    need_path_note=1
fi
export PATH="$HOME/.local/bin:$PATH"

PKG_MGR=""
if [ -r /etc/os-release ]; then
    # shellcheck source=/dev/null
    os_like=$(. /etc/os-release && printf '%s %s' "${ID:-}" "${ID_LIKE:-}")
    case " $os_like " in
        *" fedora "*|*" rhel "*) command -v dnf >/dev/null 2>&1 && PKG_MGR="dnf" ;;
        *" debian "*|*" ubuntu "*) command -v apt-get >/dev/null 2>&1 && PKG_MGR="apt" ;;
        *" arch "*) command -v pacman >/dev/null 2>&1 && PKG_MGR="pacman" ;;
    esac
fi
if [ -z "$PKG_MGR" ]; then
    if command -v dnf >/dev/null 2>&1; then
        PKG_MGR="dnf"
    elif command -v apt-get >/dev/null 2>&1; then
        PKG_MGR="apt"
    elif command -v pacman >/dev/null 2>&1; then
        PKG_MGR="pacman"
    else
        echo "Unsupported package manager. Please install dependencies manually (see README.md)."
        exit 1
    fi
fi

# A running copy (window or tray) holds the keyboard and the old binary.
if command -v f75-studio >/dev/null; then timeout 5 f75-studio quit </dev/null >/dev/null 2>&1 || true; fi

step "1/6 Build dependencies (sudo)"
case "$PKG_MGR" in
    dnf)
        sudo dnf install -y rust cargo gcc pkgconf-pkg-config systemd-devel hidapi-devel \
            libxkbcommon-devel wayland-devel mesa-libGL-devel libX11-devel pulseaudio-libs-devel curl
        ;;
    apt)
        sudo apt-get update
        sudo apt-get install -y build-essential pkg-config libudev-dev libhidapi-dev \
            libxkbcommon-dev libwayland-dev libgl1-mesa-dev libx11-dev libpulse-dev \
            curl ca-certificates
        ;;
    pacman)
        sudo pacman -S --needed --noconfirm base-devel hidapi libxkbcommon wayland \
            mesa libx11 libpulse curl
        ;;
esac

step "2/6 Rust toolchain"
# shellcheck source=/dev/null
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
have=$(rustc --version 2>/dev/null | awk '{print $2}' || true)
if printf '%s\n%s\n' "$RUST_MIN" "${have:-0}" | sort -V -C; then
    note "Rust $have is new enough"
elif command -v rustup >/dev/null; then
    note "Rust ${have:-none} is older than $RUST_MIN, updating with rustup"
    rustup default stable
    rustup update stable
else
    note "System Rust (${have:-none}) is older than $RUST_MIN."
    note "Installing current Rust with rustup into ~/.cargo (no sudo, official installer)."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    # shellcheck source=/dev/null
    . "$HOME/.cargo/env"
fi

step "3/6 Building F75 Studio (first build takes a few minutes)"
cargo build --release

step "4/6 Installing the app"
install -Dm755 target/release/f75-studio ~/.local/bin/f75-studio
install -Dm644 assets/icon.png ~/.local/share/icons/hicolor/256x256/apps/f75-studio.png
install -Dm644 packaging/f75-studio.desktop ~/.local/share/applications/f75-studio.desktop
sed -i "s|^Exec=f75-studio$|Exec=$HOME/.local/bin/f75-studio|" ~/.local/share/applications/f75-studio.desktop
update-desktop-database ~/.local/share/applications >/dev/null 2>&1 || true
gtk-update-icon-cache -q ~/.local/share/icons/hicolor >/dev/null 2>&1 || true
note "~/.local/bin/f75-studio and an \"F75 Studio\" app menu entry"

step "5/6 Keyboard access (sudo)"
# 70- so it runs before Fedora's 73-seat-late.rules, which applies the uaccess tag
sudo rm -f /etc/udev/rules.d/99-aula-f75.rules
sudo install -Dm644 packaging/70-aula-f75.rules /etc/udev/rules.d/70-aula-f75.rules
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw --action=change
sudo udevadm settle
found=0
for d in /sys/class/hidraw/hidraw*; do
    [ -e "$d/device/uevent" ] || continue
    id=$(grep -oE 'HID_ID=[0-9A-F:]+' "$d/device/uevent" | cut -d= -f2 || true)
    case "$id" in
        *:00003554:0000FA09) what="2.4G receiver" ;;
        *:00003554:0000FA07|*:00003554:0000FA08) what="Bluetooth" ;;
        *:0000258A:0000010C) what="USB-C cable" ;;
        *) continue ;;
    esac
    found=1
    node=/dev/$(basename "$d")
    if [ -r "$node" ] && [ -w "$node" ]; then note "ok       $node ($what)"; else note "BLOCKED  $node ($what): unplug and replug it once"; fi
done
[ "$found" = 1 ] || note "keyboard not connected right now (that's fine)"

step "6/6 Tray icon"
tray_logout=0
if [[ "${XDG_CURRENT_DESKTOP:-}" == *GNOME* ]]; then
    if [ -d /usr/share/gnome-shell/extensions/ubuntu-appindicators@ubuntu.com ]; then
        note "Ubuntu AppIndicator extension is already present"
    else
        # GNOME has no tray by default; ensure AppIndicator extension is installed.
        uuid=appindicatorsupport@rgcjonas.gmail.com
        case "$PKG_MGR" in
            dnf) sudo dnf install -y gnome-shell-extension-appindicator ;;
            apt) sudo apt-get install -y gnome-shell-extension-appindicator ;;
            pacman) sudo pacman -S --needed --noconfirm gnome-shell-extension-appindicator ;;
        esac
        if ! gnome-extensions enable "$uuid" 2>/dev/null; then
            cur=$(gsettings get org.gnome.shell enabled-extensions 2>/dev/null || echo "@as []")
            if [[ "$cur" != *"$uuid"* ]]; then
                # "@as []", "[]" or nothing means empty; otherwise append to "['a', 'b']"
                if [[ "$cur" != \[* || "$cur" == "[]" ]]; then new="['$uuid']"; else new="${cur%]}, '$uuid']"; fi
                gsettings set org.gnome.shell enabled-extensions "$new" || true
            fi
            tray_logout=1
        fi
        note "AppIndicator extension installed and enabled"
    fi
else
    note "${XDG_CURRENT_DESKTOP:-your desktop} shows tray icons already"
fi

echo
printf '\033[1;32mDone.\033[0m Open "F75 Studio" from your app menu, or run: f75-studio\n'
note "USB-C cable: everything, including live effects and audio sync."
note "2.4G receiver: everything except live effects, audio sync and macros."
[ "$need_path_note" = 1 ] && note "$HOME/.local/bin is not on your PATH; add it or log out and back in to run f75-studio from a terminal."
[ "$tray_logout" = 1 ] && note "Log out and back in once to see the tray icon."
note "Update later with: git pull && ./install.sh"

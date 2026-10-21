#!/usr/bin/env bash
# Starts F75 Studio in write mode: real keyboard (never dry run), with read/write access
# to its hidraw nodes, over the cable or 2.4G receiver (Bluetooth is view only).
set -euo pipefail
cd "$(dirname "$0")"

BIN=target/release/f75-studio
RULE=/etc/udev/rules.d/70-aula-f75.rules

[ -x "$BIN" ] || cargo build --release

# Write access needs the udev rule; install it once if missing or out of date.
if ! cmp -s packaging/70-aula-f75.rules "$RULE"; then
    echo "Installing udev rule (sudo)"
    sudo install -Dm644 packaging/70-aula-f75.rules "$RULE"
    sudo udevadm control --reload-rules
    sudo udevadm trigger --subsystem-match=hidraw --action=change
    sudo udevadm settle
fi

writable=""
for d in /sys/class/hidraw/hidraw*; do
    [ -e "$d/device/uevent" ] || continue
    id=$(grep -oE 'HID_ID=[0-9A-F:]+' "$d/device/uevent" | cut -d= -f2 || true)
    case "$id" in
        *:0000258A:0000010C) what="USB-C cable" ;;
        *:00003554:0000FA09) what="2.4G receiver" ;;
        *:00003554:0000FA07|*:00003554:0000FA08) what="Bluetooth" ;;
        *) continue ;;
    esac
    node=/dev/$(basename "$d")
    if [ -r "$node" ] && [ -w "$node" ]; then
        writable="$what"
    else
        echo "No write access to $node ($what): unplug and replug it, then run this again."
        exit 1
    fi
done
if [ -z "$writable" ]; then
    echo "No AULA F75 found over the cable or 2.4G receiver."
    exit 1
fi
echo "Write access OK ($writable)"

# Never start in the screenshot/dry-run mode, and replace a copy already running.
unset F75_STUDIO_DRY
"$BIN" quit >/dev/null 2>&1 || true
exec "$BIN" "$@"

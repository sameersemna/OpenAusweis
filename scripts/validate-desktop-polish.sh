#!/usr/bin/env bash

set -euo pipefail

desktop_env="${XDG_CURRENT_DESKTOP:-unknown}"
session_type="${XDG_SESSION_TYPE:-unknown}"
runtime_dir="${XDG_RUNTIME_DIR:-unset}"

desktop_family="unknown"
if [[ "$desktop_env" == *GNOME* ]]; then
  desktop_family="gnome"
elif [[ "$desktop_env" == *KDE* || "$desktop_env" == *Plasma* ]]; then
  desktop_family="kde"
fi

desktop_app_running="no"
daemon_running="no"

if pgrep -f "openausweis-desktop|target/.*/openausweis-desktop" >/dev/null 2>&1; then
  desktop_app_running="yes"
fi

if pgrep -f "openausweis-daemon|target/.*/openausweis-daemon" >/dev/null 2>&1; then
  daemon_running="yes"
fi

cat <<EOF
OpenAusweis Desktop Polish Validation

Environment
- Desktop: ${desktop_env}
- Desktop family: ${desktop_family}
- Session: ${session_type}
- XDG runtime dir: ${runtime_dir}
- Desktop app running: ${desktop_app_running}
- Daemon running: ${daemon_running}

Manual checklist
1. Launch the desktop app with no active session and verify Home stays calm and says it is waiting for browser sign-in.
2. Stop or disconnect the daemon briefly and verify Home shows a reconnecting state without transport or backend wording.
3. Start a browser sign-in and verify the desktop shows one clear next action for PIN entry or sign-in in progress.
4. Hide or close the desktop window before PIN entry, then trigger a PIN prompt and verify the window is shown once without repeated focus stealing.
5. Close the desktop window and verify OpenAusweis remains available in the tray; confirm the close-to-tray reminder appears only once per app run.
6. On GNOME/Wayland, verify tray limitations do not create repeated alerts and notifications are only used for important state changes.
7. Submit an invalid short PIN and verify the message is plain-language guidance with remaining attempts.
8. Exhaust invalid PIN attempts and verify the desktop tells the user to start again instead of exposing raw backend phrases.
9. Complete a sign-in and verify the desktop shows a calm return-to-browser cue.
10. After completion or recovery, verify the Home view settles back to a calm waiting state.

Suggested command sequence
- npm run dev:aa2
- npm run validate:desktop-polish
- cargo test -p openausweis-desktop
EOF
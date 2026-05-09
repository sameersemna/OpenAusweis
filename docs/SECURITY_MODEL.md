# Security Model

## Principles

- Least privilege
- Local-first architecture
- No unnecessary cloud dependency
- Strong IPC boundaries
- Browser treated as untrusted
- No sensitive logging

## Threat Model

Potential threats:
- malicious browser extensions
- local malware
- IPC spoofing
- privilege escalation
- reader spoofing

## Security Measures

- validate all IPC messages
- signed browser communication where possible
- sandbox desktop components
- strict permission boundaries
- avoid storing secrets
- enforce relying-party origin allowlists in extension and native host

## Browser Origin Policy

- `START_SESSION` messages are accepted only when the sender origin equals the declared `relying_party`.
- Extension background validates origin against local policy:
	- exact origins from `chrome.storage.local.allowedExactOrigins`
	- host suffixes from `chrome.storage.local.allowedSuffixes`
	- defaults: `http://localhost`, `https://localhost`, `.bundid.de`, `.bund.de`

## Native Host Origin Policy

- Native host re-validates `START_SESSION.relying_party` before forwarding to daemon.
- Native host reads policy from `~/.config/openausweis/origin-policy/current/` by default.
- The `current` symlink points to a versioned bundle directory containing `policy.json` and `policy.sha256`.
- Native host verifies the bundle checksum before accepting it.
- `OPENAUSWEIS_POLICY_DIR` can point to an alternate bundle root.
- `OPENAUSWEIS_POLICY_FILE` is accepted as a legacy compatibility read path.
- Policy is configurable by environment variables:
	- `OPENAUSWEIS_ALLOWED_EXACT_ORIGINS` (CSV exact origins)
	- `OPENAUSWEIS_ALLOWED_SUFFIXES` (CSV domain suffixes)
- Defaults mirror extension defaults for defense in depth.

## Authentication Philosophy

Use official German eID mechanisms where possible.

Do not implement custom cryptographic replacements for:
- PACE
- EAC
- certificate validation
- APDU security layers

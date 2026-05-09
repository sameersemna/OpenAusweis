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

## Authentication Philosophy

Use official German eID mechanisms where possible.

Do not implement custom cryptographic replacements for:
- PACE
- EAC
- certificate validation
- APDU security layers

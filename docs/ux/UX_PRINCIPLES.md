# OpenAusweis UX Principles

## Purpose

OpenAusweis should feel calm, secure, lightweight, respectful, Linux-native, privacy-oriented, trustworthy, and technically competent without exposing technical complexity.

This document defines non-negotiable UX rules for copy, hierarchy, and interaction design in the desktop app.

## Core Experience Attributes

1. Calm
- Use short, steady language.
- Avoid urgency unless there is an actionable risk.
- Prefer one clear next action over multiple competing actions.

2. Secure
- Make trust visible through stable cues: local processing, no cloud storage, official eID context.
- Keep security-critical actions explicit and unambiguous.

3. Lightweight
- Minimize on-screen density in Home view.
- Defer details, logs, and low-level state to Advanced.

4. Respectful
- Never blame users for failures.
- Explain what happened, what to do next, and what is safe.

5. Native to Linux
- Respect desktop runtime differences (Wayland/X11, tray availability) with practical guidance.
- Use Linux terminology only when it directly helps user action.

6. Privacy-oriented
- Make local-first behavior clear.
- Do not expose unnecessary identifiers or internal references in primary views.

7. Trustworthy
- Be transparent about readiness and outcomes.
- Do not overstate capability; avoid vague success language.

8. Technically competent without technical overload
- Implementation can be advanced; presentation should remain citizen-facing.
- Replace internal system terms with task language in Home view.

## Cognitive Load Rules

Home view must answer only four questions:
1. Is sign-in ready?
2. Is my card reader detected?
3. Is my card detected?
4. What do I do next?

Anything that does not help answer these questions belongs in Advanced.

## Language Policy

### Home view: required language style
- Use task-oriented phrases:
	- "Start sign-in"
	- "Enter your PIN"
	- "Keep your card inserted"
	- "Return to your browser"
- Use readiness language:
	- "Ready"
	- "Reconnecting"
	- "Not detected"

### Home view: avoid these terms
- daemon
- IPC
- session-stream
- metrics
- infrastructure
- request ID / handoff ID
- protocol/version terminology

If these terms are needed for troubleshooting, place them in Advanced only.

## Information Hierarchy

### Home (Primary)
Must include:
- Current readiness summary
- Reader/card detection status
- Current sign-in state
- One next-step action block
- Minimal onboarding guidance for first run

Must not include:
- Raw diagnostics
- Transport/protocol details
- Internal counters and technical metrics
- Infrastructure health internals beyond user-facing readiness

### Advanced
May include:
- Diagnostics output
- Developer mode and telemetry counters
- Runtime/environment details
- Troubleshooting playbooks
- Policy and technical configuration

## Interaction Rules

1. One primary action per state
- Each state must have a clear default action.

2. Errors must be recoverable in-place
- Error states should provide a direct "Start again" path in the same panel.

3. Progressive disclosure
- Technical details are collapsed by default and gated behind Advanced/Developer mode.

4. Accessibility
- State changes should be announced clearly (aria-live).
- Keyboard-first paths must work for all core sign-in actions.

## Copy Guidelines

Use:
- "Current sign-in" (not "current authentication request")
- "Sign-in progress" (not "browser handoff")
- "OpenAusweis is reconnecting" (not "secure service unavailable" unless truly unavailable to user action)

Avoid:
- Internal actor names in Home copy
- Raw backend error phrases when a plain-language equivalent exists

## Acceptance Checklist For Home UX Changes

Any PR that changes Home view should pass this checklist:

1. Home still answers readiness, reader, card, and next-action in one scan.
2. No daemon/IPC/session-stream/metrics terms are visible in Home.
3. Error state includes an immediate recovery action.
4. Diagnostics and telemetry remain in Advanced only.
5. Copy remains calm, concise, and respectful.
6. Accessibility announcements still match user-visible states.

## Scope Note

These principles constrain presentation only. They do not require backend architecture, daemon ownership, IPC contract, or state-machine changes.

# AI Agent Rules

## General Rules

- Never perform massive refactors without approval
- Never delete working code unless explicitly required
- Always explain architectural decisions
- Prefer incremental changes
- Keep commits logically grouped
- Preserve Linux compatibility
- Preserve Wayland compatibility

---

## Coding Style

- Production-quality code
- Clear naming
- Strong typing
- Minimal dependencies
- Security-first
- Async-first Rust design

---

## Rust Rules

- Prefer Tokio async runtime
- Prefer modular architecture
- Avoid global mutable state
- Use structured logging
- Use error handling properly
- Avoid unwrap() in production code

---

## Frontend Rules

- Minimal modern UI
- Native-feeling UX
- Responsive layout
- Accessibility support
- Dark mode support

---

## Security Rules

- Never log sensitive authentication data
- Never store card secrets
- Use secure IPC
- Validate all browser messages
- Treat browser extensions as untrusted

---

## Packaging Rules

- Ensure Ubuntu compatibility
- Ensure sandbox compatibility
- Keep installation simple

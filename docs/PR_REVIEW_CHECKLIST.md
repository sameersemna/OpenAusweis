# PR Review Checklist (Short)

1. Session start succeeds with daemon freshly started and with stale daemon restarted.
2. Session stream shows Connected + IDLE immediately after watcher subscribe.
3. System theme is applied on first load; manual Light/Dark override persists.
4. `scripts/dev-up.sh` recovers if port 1420 is occupied (fallback port selected).
5. Snap-injected env uses snap-safe path and does not hit GLIBC_PRIVATE crash.
6. Repeated identical PC/SC errors do not spam daemon warnings.
7. `cargo test -p openausweis-daemon` passes.
8. `cargo test -p openausweis-desktop` passes.
9. `npm --workspace @openausweis/desktop run typecheck` passes.
10. Rollback path confirmed by reverting commits `30d7843`, `14ada18`, `ec99266`, `4dff6cd`.

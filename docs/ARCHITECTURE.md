# Bridge architecture

## Product boundary
Bridge is a cross-device utility, not a cloud drive. The first product boundary is Windows + Android, sharing one React UI and a Rust native core through Tauri 2.

## Layers
1. **Presentation** — React + TypeScript. Navigation, native file selection (Tauri Dialog on Windows and Android; browser fallback on web), selected-file review, pairing screens, clipboard preferences, settings, FAQ and support.
2. **Native shell** — Tauri 2. Platform-specific windows, file access, Android packaging and OS integrations.
3. **Identity and pairing** — local device identity, short-lived pairing handshake, user confirmation, trusted-device registry and revocation.
4. **Transport** — iroh / QUIC peer connectivity. Direct path is preferred; relay-assisted connection establishment handles networks where direct connectivity fails. Relay is not file storage.
5. **Transfer engine** — chunked streaming, bounded queues, SHA-256 integrity checks, pause/resume, collision handling, cleanup and explicit error states.
6. **Persistence** — local metadata and encrypted local secrets. Transfer payloads should not be persisted by default after completion.

## Security requirements before real transfers
- Do not trust display names or pairing codes as identity.
- Bind each approval to an authenticated device public key.
- Keep pairing tokens short-lived, single-use and rate-limited.
- Validate filenames and never join untrusted paths without canonicalization.
- Use temporary files, verify the complete digest, then atomically move into the destination.
- Never overwrite an existing file silently.
- Apply per-file and total-queue limits with clear user-facing errors.
- Never log file content, clipboard content, pairing secrets or private keys.
- Clipboard sync must be opt-in, scoped to trusted devices and include a clear-history control.
- Test interrupted transfers, duplicate chunks, path traversal, revoked peers and digest mismatch.

## Current implementation status
The repository contains the UI foundation, local demo interactions, support message validation and a Tauri shell with a runtime capability declaration. The native transport, pairing, secure secret storage, real cross-device transfer, persistent settings and OS clipboard integration are **not implemented yet**. The UI must not describe demo entries as real transfers.

## Motion and accessibility
- Use short opacity/transform transitions for navigation and feedback.
- Respect the OS reduced-motion preference and in-app setting.
- Keep focus visible and controls keyboard-operable.
- Avoid animation of layout-heavy properties; prefer opacity and transform.
- Use restrained color; gradients are reserved for semantic emphasis.

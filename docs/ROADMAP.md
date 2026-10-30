# Bridge roadmap

## Foundation — current
- [x] Repository initialization and product principles
- [x] React + TypeScript + Vite UI shell
- [x] Desktop-first responsive navigation
- [x] Home, send queue, devices, clipboard, activity, settings and help screens
- [x] Motion preference, theme direction and 800-character support validation
- [x] Screenshot preview UI and support contact paths
- [x] Tauri 2 Rust shell for Windows / Android target
- [ ] CI verification on real Windows and Android runners

## Milestone 1 — local native experience
- [ ] Persist theme, motion and preferences safely
- [x] Native multi-file picker on Windows and Android, with browser fallback
- [ ] Safe destination picker and incoming-file destination handling
- [ ] Build and smoke-test Windows NSIS installer
- [ ] Initialize Android project and build debug APK
- [ ] Add device identity storage using platform secure storage
- [ ] Device discovery and QR / one-time code pairing

## Milestone 2 — trusted device connectivity
- [x] Integrate native iroh endpoint lifecycle (bind, start / stop / status, versioned ALPN)
- [ ] Persist peer identity in Windows DPAPI / Android Keystore-backed storage
- [x] Implement versioned encrypted peer hello / hello-ack connectivity probe
- [x] Implement user-approved six-digit, one-use pairing invitations with expiry and attempt limits
- [x] Session trusted-device list and revoke
- [ ] Persist trusted devices with the platform-protected endpoint identity
- [ ] Direct connection and relay fallback diagnostics
- [ ] Security review of handshake and threat model

## Milestone 3 — real transfers
- [ ] Chunked streaming and bounded backpressure
- [ ] SHA-256 integrity verification
- [ ] Pause / resume / cancel
- [ ] Atomic file finalization and collision resolution
- [ ] Large-file, interruption and network-switch tests
- [ ] Android background transfer behavior and notification UX

## Milestone 4 — quality and ecosystem
- [ ] Explicit opt-in clipboard sync with local history
- [ ] Share sheet and Android receiving intents
- [ ] Windows Explorer context-menu / share integration
- [ ] Notification and system tray options
- [ ] Internationalization and screen-reader audit
- [ ] Public beta, crash diagnostics with consent and release signing

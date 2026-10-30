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
- [ ] Native file picker and destination picker
- [ ] Build and smoke-test Windows NSIS installer
- [ ] Initialize Android project and build debug APK
- [ ] Add device identity storage using platform secure storage
- [ ] Device discovery and QR / one-time code pairing

## Milestone 2 — trusted device connectivity
- [ ] Integrate iroh endpoint lifecycle
- [ ] Implement authenticated pairing and key confirmation
- [ ] Trusted device list, revoke and re-pair
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

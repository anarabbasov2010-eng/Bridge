# Bridge

**Move things between your devices. Keep control of your data.**

Bridge is a free, privacy-first cross-device companion for Windows and Android. The project is designed around direct device-to-device transfer, explicit pairing, and local-first defaults.

## Product principles

- No advertising, subscription gates, or account requirement for local transfers.
- Explicit pairing and trusted-device revocation.
- End-to-end encrypted transport for device-to-device sessions.
- Clipboard sharing is opt-in and can be cleared at any time.
- Clear transfer states, integrity checks, and recoverable failures.
- Reduced-motion support and accessible keyboard navigation.
- Help requests support screenshots and messages up to 800 characters.

## Stack

- **Desktop + Android:** Tauri 2 with a Rust core.
- **UI:** React, TypeScript, Vite, Framer Motion, Lucide.
- **Native file selection:** Tauri Dialog plugin for Windows and Android, with a browser picker fallback.
- **Peer transport:** iroh / QUIC, with relay-assisted connection establishment when direct connectivity is unavailable.
- **Builds:** GitHub Actions for Windows and Android.

The app currently includes:
- React UI for Windows and Android, with responsive desktop and mobile layouts.
- Native iroh / QUIC endpoint, encrypted peer identity checks and one-time QR / six-digit pairing invitations.
- Native file selection on Windows and Android. Selected files remain local until the recipient accepts an incoming offer.
- An initial peer-to-peer file streaming path: receiver approval, bounded 128 KiB chunks, a 5 GiB per-file limit, progress events, temporary cache staging and SHA-256 verification before received data is copied to the selected destination.
- Incoming offer review, recipient selection, transfer progress and verified transfer history.
- FAQ and support UI with up to five screenshot previews and an 800-Unicode-character message limit.

**Current preview limitations:** this is an early implementation, not a public-release claim. Windows and Android CI builds and manual two-device tests must pass before treating it as installable. Endpoint identities and trusted-device records are still in memory and are cleared when the peer engine stops. Pairing uses a manually shared six-digit invite; automatic camera QR scanning is not implemented. File transfers currently run while both apps are open; pause/resume, background Android transfers, persistent identity, persistent trusted devices, robust cleanup of partial documents across every Android provider, and OS clipboard integration remain unfinished. Ordinary filesystem paths use atomic create-new semantics so existing files are not silently overwritten. Browser-selected files are preview-only; native peer transfer runs in the installed Windows / Android app.

## Development

```bash
npm install
npm run dev
```

For a Tauri desktop build:

```bash
npm run tauri:dev
npm run tauri:build
```

For Android, install the Tauri 2 Android prerequisites, initialize the Android project, then run:

```bash
npm run android:init
npm run android:dev
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [docs/ROADMAP.md](docs/ROADMAP.md).

## Support

- Telegram: [@vvpse](https://t.me/vvpse)
- Email: [linka.comp@gmail.com](mailto:linka.comp@gmail.com)

## License

The license will be selected before the first public release. Until then, all rights are reserved by the repository owner.
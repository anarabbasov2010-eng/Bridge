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
- **Peer transport:** iroh / QUIC, with relay-assisted connection establishment when direct connectivity is unavailable.
- **Builds:** GitHub Actions for Windows and Android.

The app includes the React UI foundation, local demo interactions, support validation, a Tauri shell, and a native iroh peer-endpoint lifecycle (start / stop / status / ephemeral peer ID). Authenticated pairing, persistent device identity, connection handlers, file transfer and clipboard OS integration are still separate milestones and are not represented as completed yet.

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
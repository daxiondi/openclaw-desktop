# openclaw-desktop

Default Chinese docs: [README.md](./README.md)

`openclaw-desktop` is a zero-friction desktop wrapper for OpenClaw.
The goal is simple: install once, use immediately.

## Why this project

- Zero setup feeling: users install one desktop app, no manual dependency chain.
- Offline-friendly: installer bundles an offline OpenClaw payload for weak/no-internet setups.
- Faster onboarding: OAuth login is built in, and existing local auth can be reused.
- China-friendly path: supports both OAuth and API-key routes for local providers/gateways.
- Official capability preserved: users can open the official local OpenClaw page directly.
- Cross-platform delivery: macOS, Windows, and Linux packages from one codebase.

## Quick Start

1. Download the package for your OS from GitHub Releases.
2. Install and launch `openclaw-desktop`.
3. Choose a login mode in onboarding:
   - OAuth (Codex / Claude / Gemini / Qwen Portal)
   - API Key (including OpenAI-compatible domestic endpoints)
   - Local Ollama
4. Start chatting and configuring models.

## Windows Offline Installation

Goal: keep first-run as close to one-click as possible for weak-network/offline users.

Two paths are now available on Windows:

1. Automatic path (recommended)
   - Install and launch `openclaw-desktop`.
   - On first bootstrap, the app tries bundled offline payload first.
   - If bundled payload is missing/incomplete, it auto-downloads and extracts `openclaw-desktop-windows-portable.zip`, then continues install.
2. Manual path (fallback)
   - Click `Install from Portable Zip` on the bootstrap page.
   - Pick your downloaded `openclaw-desktop-windows-portable.zip`.
   - The app extracts/installs the offline payload and continues bootstrap.

Why Windows can still fail to include payload inside setup:

- In the Windows packaging pipeline, resource embedding can be unstable (symptom: `openclaw-bundle` missing in installer output).
- To keep users unblocked, runtime auto-download fallback and manual portable selection are both supported, with no extra CLI setup required.

### Verify Windows portable install (dev/CI)

This script validates that the Windows portable zip can bootstrap OpenClaw offline:
download/extract payload → offline install → start gateway → verify local page reachable.

```bash
npm run test:windows-portable

# verify a local zip you already downloaded
npm run test:windows-portable -- C:\\path\\to\\openclaw-desktop-windows-portable.zip
```

## Community Group

Scan the QR code to join the `openclaw` community chat group:

![openclaw community group QR code](./src/assets/wechat.jpg)

## In-App Updates (auto-detect + one click)

The app now includes a built-in updater control in the header:

- It silently checks for updates after startup.
- When a newer version is found, users get an `Update & Relaunch` button.
- Clicking it downloads, installs, and relaunches the app without reinstalling a new package manually.

### One-time setup

1. Generate updater signing keys:

```bash
npx tauri signer generate -w .tmp/updater/tauri-updater.key
```

2. Put the generated public key into `src-tauri/tauri.conf.json` at `plugins.updater.pubkey`.
3. Configure GitHub repository secrets:
   - `TAURI_SIGNING_PRIVATE_KEY`: private key content
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: private key password (if set)
4. Publish a tag (for example `v0.2.0`). CI will automatically:
   - build installers
   - generate `latest.json`
   - upload all assets to GitHub Release (used by in-app update checks)

## Development

### Run frontend

```bash
npm install
npm run dev
```

### Run desktop app in dev mode

```bash
npm run tauri:dev
```

### Build installers (with offline payload)

```bash
npm run tauri:build
```

Skip offline payload preparation for faster local iteration:

```bash
OPENCLAW_DESKTOP_SKIP_BUNDLE_PREP=1 npm run tauri:build
```

### Offline smoke test (local Codex + official page)

```bash
npm run test:offline-local-codex-ui
```

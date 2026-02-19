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

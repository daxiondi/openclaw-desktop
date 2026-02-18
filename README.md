# openclaw-desktop (Scaffold)

This folder contains the native app shell implementation.

## Current scope

- Tauri 2 + React + TypeScript scaffold
- Onboarding with 3 entry modes: OAuth / API Key / local Ollama
- i18n base: `zh-CN` (default) and `en-US`
- Bridge layer that reads OAuth providers from upstream OpenClaw (`openclaw models status --json`)
- Local Codex auth detection (`~/.codex/auth.json`) wired into OAuth onboarding
- If local Codex auth is detected and provider is `openai-codex`, onboarding reuses it directly without re-login

## Run

```bash
npm install
npm run dev
```

## Build frontend

```bash
npm run build
```

## Run native app (Tauri)

Requires Rust/Cargo installed locally.

```bash
npm run tauri:dev
```

## Build installer (with offline OpenClaw payload)

`tauri:build` now prepares an offline OpenClaw bundle first, then packages it into
the installer resources (`src-tauri/bundle/resources/openclaw-bundle`).

```bash
npm run tauri:build
```

If you need to skip offline payload preparation (for fast local iteration):

```bash
OPENCLAW_DESKTOP_SKIP_BUNDLE_PREP=1 npm run tauri:build
```

## Offline smoke test (local Codex + official page)

Requires:

- A prepared offline bundle (`npm run prepare:openclaw-bundle`)
- Local Codex auth file at `~/.codex/auth.json`

Run:

```bash
npm run test:offline-local-codex-ui
```

## Notes

- `saveApiKey` is placeholder now. Next step is system secure storage integration.
- `startOAuthLogin` currently shells out to `openclaw models auth login --provider <id>`.
- Installer now includes an offline OpenClaw payload; production signing/icon pipeline still pending.

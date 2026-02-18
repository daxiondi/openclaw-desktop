# openclaw-desktop

默认中文文档。English docs: [README.en.md](./README.en.md)

`openclaw-desktop` 是面向普通用户的 OpenClaw 桌面版，目标是「安装即用、零门槛」。

## 这个项目的好处

- 零依赖体感：用户安装一个桌面包即可，不需要先手动装一堆 CLI 和环境。
- 离线友好：安装包内置 OpenClaw 离线载荷，弱网/无外网场景也能完成初始化。
- 登录更顺滑：支持 OAuth 登录流程，并可复用本机已有的登录状态（如 Codex）。
- 国内可用：同时支持 OAuth 路线和 API Key 路线（可接入国内模型/网关）。
- 官方能力不丢：可直接打开 OpenClaw 官方本地页面使用聊天与配置能力。
- 跨平台交付：统一产出 macOS / Windows / Linux 安装包。

## 用户快速开始

1. 打开 Releases 页面下载对应系统安装包。
2. 安装并启动 `openclaw-desktop`。
3. 在引导页选择登录方式：
   - OAuth（如 Codex / Claude / Gemini / Qwen Portal）
   - API Key（可对接国内兼容端点）
   - 本地 Ollama
4. 登录完成后即可进入聊天和模型配置。

## 开发环境

### 运行前端

```bash
npm install
npm run dev
```

### 运行桌面开发模式

```bash
npm run tauri:dev
```

### 构建安装包（含离线载荷）

```bash
npm run tauri:build
```

如果只想快速本地调试、跳过离线载荷准备：

```bash
OPENCLAW_DESKTOP_SKIP_BUNDLE_PREP=1 npm run tauri:build
```

### 离线冒烟测试（本地 Codex + 官方页面）

```bash
npm run test:offline-local-codex-ui
```

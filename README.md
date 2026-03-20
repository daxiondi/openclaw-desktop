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

## Windows 离线安装说明

目标：弱网/离线用户只需一次安装，尽量不依赖复杂外网步骤。

当前 Windows 提供两条路径：

1. 自动路径（推荐）
   - 正常安装并启动 `openclaw-desktop`。
   - 首次引导时，程序会优先尝试安装包内离线载荷。
   - 如果安装包内离线载荷缺失或不完整，会自动下载并提取 `openclaw-desktop-windows-portable.zip` 后继续安装。
2. 手动路径（兜底）
   - 在引导页点击 `选择 portable 包安装`。
   - 选择你已经下载好的 `openclaw-desktop-windows-portable.zip`。
   - 程序会自动提取并安装离线载荷，然后继续初始化。

为什么 Windows 还会出现“安装包内离线载荷不可用”：

- Windows 的打包链路里，资源注入偶发失败/路径不稳定（表现为构建产物里缺少 `openclaw-bundle`）。
- 为避免用户被阻塞，我们增加了运行时自动下载兜底 + 手动选择 portable 兜底，两条路径都不要求用户手动配环境。

### 验证 Windows portable 安装（dev/CI）

该脚本用于验证 Windows portable zip 是否能完成离线自举：下载/解压载荷 → 离线安装 → 启动 gateway → 验证本地页面可访问。

```bash
npm run test:windows-portable

# 验证你已经下载好的 zip
npm run test:windows-portable -- C:\\path\\to\\openclaw-desktop-windows-portable.zip
```

## 沟通交流群

扫码加入 `openclaw` 交流群：

![openclaw交流群二维码](./src/assets/wechat.jpg)

## 在线更新（自动检测 + 一键更新）

应用顶部已内置在线更新入口：

- 启动后自动静默检测新版本。
- 检测到新版本时，出现“更新并重启”按钮。
- 用户点击后自动下载、安装并重启应用，无需重新下载安装包。

### 首次配置（只做一次）

1. 生成 updater 签名密钥（私钥只保存在你手里）：

```bash
npx tauri signer generate -w .tmp/updater/tauri-updater.key
```

2. 把公钥内容填到 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`。
3. 在 GitHub 仓库 Secrets 配置：
   - `TAURI_SIGNING_PRIVATE_KEY`：私钥文件内容
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：私钥密码（如果你生成时设置了密码）
4. 打 tag 发布（例如 `v0.2.0`），CI 会自动：
   - 构建安装包
   - 生成 `latest.json`
   - 上传到 Release 资产（客户端据此检测更新）

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

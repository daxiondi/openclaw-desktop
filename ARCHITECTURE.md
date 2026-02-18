# openclaw-desktop 架构图（V1）

更新时间：2026-02-18

## 架构目标
- 一键安装，一次登录即可使用。
- 兼容主流 Windows、macOS、Linux 用户环境。
- 同时支持三种模型接入方式：OAuth（免 Key，覆盖 OpenClaw 全部可用 OAuth）、本地模型、API Key。
- 支持高频在线升级（OTA）与失败回滚。
- 默认中文 UI，可切换英文 UI。

## 总体架构图
```mermaid
flowchart TD
    A[用户安装器<br/>Windows NSIS / macOS dmg-pkg / Linux AppImage-deb-rpm] --> B[桌面客户端<br/>Tauri 2 + React]
    B --> B0[引导界面 Onboarding]
    B0 --> B01[方式1 OAuth 登录]
    B0 --> B02[方式2 API Key]
    B0 --> B03[方式3 本地 Ollama]
    B --> B1[i18n 层<br/>zh-CN 默认 + en-US]
    B --> C[运行时管理器 Runtime Manager]
    C --> D[OpenClaw Bridge Adapter]
    D --> D1[调用上游 openclaw CLI / RPC]

    B --> E[认证中心 Auth Hub]
    E --> E1[OAuth Provider Matrix<br/>由 OpenClaw Provider/Auth 注册表驱动]
    E1 --> E11[OpenAI Codex / Chutes]
    E1 --> E12[Google Antigravity / Gemini CLI]
    E1 --> E13[MiniMax / Qwen / GitHub Device Login]
    E --> E2[API Key 管理<br/>OpenAI/Anthropic/DeepSeek/Qwen Key]

    B --> F[模型路由 Model Router]
    F --> F1[云模型通道]
    F --> F2[本地模型通道<br/>Ollama/LM Studio/OpenAI-Compatible]

    D --> G[任务与提醒引擎]
    G --> G1[应用内 Cron/Heartbeat]
    G --> G2[系统守护层]
    G2 --> G21[Windows Task Scheduler]
    G2 --> G22[macOS LaunchAgent]
    G2 --> G23[Linux systemd --user]

    B --> H[更新与运维]
    H --> H1[自动更新 Updater<br/>stable/beta/dev + 灰度]
    H --> H2[日志/诊断包导出]
    H --> H3[失败自动回滚]
```

## 关键设计说明
- 不强制 WSL：Windows 走原生后台守护（Task Scheduler）与原生打包安装。
- 优先小包体：桌面壳采用 Tauri，减少安装包体积与冷启动成本。
- OAuth 优先：默认引导 OAuth 模式，并展示 OpenClaw 当前全部可用 OAuth 提供商。
- 本地模型内建：默认提供本地模型入口与连通性检测，降低新手配置难度。
- 统一控制面板：登录、模型选择、渠道绑定、任务提醒均在同一 UI 内完成。
- 引导界面固定三种使用方式：OAuth、API Key、本地 Ollama，并支持后续自由切换。
- OAuth Provider 动态化：OAuth 列表从 OpenClaw 上游注册表/插件系统读取，避免“只支持单一提供商”的文档或实现偏差。
- 上游最小侵入：业务逻辑放在 `openclaw-desktop/`，通过 Bridge Adapter 对接 `openclaw/`，避免长期维护 Fork。

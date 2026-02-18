# openclaw-desktop 技术决策（V1）

更新时间：2026-02-18

## 1. 桌面框架
- 采用 `Tauri 2 + React + TypeScript` 作为桌面壳。
- 原因：相较 Electron 通常可显著减小包体积；同时保留 Web 技术栈开发效率。
- 兼容策略：若某平台出现 Tauri 阻塞问题，允许保留 Electron 备选壳，但不作为默认路径。

## 2. 在线升级（强制）
- 必须实现 OTA（增量更新优先），支持 `stable / beta / dev` 三通道。
- 必须具备灰度能力（按版本、批次或渠道分发）和失败自动回滚。
- 升级过程默认静默下载、显式重启确认，避免强制中断用户会话。

## 3. 多语言
- 默认语言 `zh-CN`。
- 必须支持 `en-US` 切换。
- 文案系统统一走 i18n key，不允许硬编码中文或英文字符串进入页面代码。

## 4. 多平台
- Windows：安装器 + Task Scheduler。
- macOS：dmg/pkg + LaunchAgent。
- Linux：AppImage/deb/rpm + systemd user service。

## 5. 上游策略（不改或少改官方）
- `openclaw/` 作为上游参考与同步源，不承载业务逻辑。
- `openclaw-desktop/` 承载桌面产品实现。
- 通过 Bridge/Adapter 与上游交互（CLI、RPC、Provider 注册信息读取）。
- 若必须改上游，补丁必须最小化、可回放、可在上游更新后快速重建。

## 6. OAuth 策略
- 首屏必须展示 OAuth / API Key / 本地 Ollama 三种入口。
- OAuth 入口需展示 OpenClaw 当前支持的全部 OAuth/设备登录方式。
- OAuth 提供商列表必须动态读取上游注册表/插件系统，不允许写死为单一提供商。

# AGENTS.md

本文件定义 openclaw-desktop 的强制产品要求。后续任何设计、开发、评审必须遵守。

## 1. 核心原则（MUST）
1. 必须保证大多数用户零门槛直接使用：下载安装后，默认路径不要求命令行、不要求手动配环境。
2. 必须兼容大多数 Windows、macOS、Linux 用户：安装、启动、升级、恢复流程统一且稳定。
3. 必须支持 OpenClaw 当前支持的全部 OAuth 登录方式（免 API Key），并作为默认推荐入口。
4. OAuth 提供商列表不得硬编码为单一厂商，必须可随 OpenClaw 上游版本动态扩展。
5. 必须同时支持本地模型使用方式（如 Ollama/兼容 OpenAI 协议本地服务）。
6. 必须同时支持 API Key 方式，供高级用户或企业接入。
7. 必须提供在线升级（OTA）能力，支持快速发布、灰度、失败回滚。
8. UI 必须默认中文，并可切换英文（i18n 双语）。
9. 尽量不修改官方 OpenClaw 核心代码；如业务需要可改动，但必须最小化且可持续跟上游同步。

## 2. 体验要求（MUST）
1. 首次启动流程不超过 5 步：安装 -> 登录/选择模式 -> 模型检测 -> 完成。
2. 引导界面必须明确三种使用方式入口，且同级展示：
   - OAuth 登录（推荐，展示全部可用 OAuth 提供商）
   - API Key 接入
   - 本地 Ollama 模型
3. 关键失败必须有可执行修复提示（不是纯错误码）。
4. 必须支持日志导出与一键诊断包，便于远程排障。

## 3. 平台要求（MUST）
1. Windows 使用原生后台守护机制（Task Scheduler 或等效方案），不强制依赖 WSL。
2. macOS 使用原生后台守护机制（LaunchAgent 或等效方案）。
3. Linux 使用原生后台守护机制（systemd user service 或等效方案）。
4. 安装包必须支持自动更新与回滚策略，避免更新后不可用。

## 4. 安全与凭证（MUST）
1. OAuth 凭证与 API Key 必须使用系统安全存储能力（如 Keychain/Credential Manager/安全加密存储）。
2. 日志中禁止明文输出凭证、Token、API Key。
3. 出厂默认最小权限；涉及系统操作时给出明确授权提示。

## 5. 迭代优先级（SHOULD）
1. P0：零门槛安装 + 全量 OAuth（对齐 OpenClaw） + 基础聊天可用。
2. P1：在线升级（增量包、灰度开关、失败自动回滚）。
3. P2：本地模型自动发现与连通性测试。
4. P3：多提供商 API Key 管理与企业配置导入。

## 6. 引导界面规范（MUST）
1. 首屏提供三卡片或三按钮：
   - OAuth 登录（所有可用 OAuth 提供商）
   - 我有 API Key
   - 我使用本地 Ollama
2. 默认高亮 OAuth 入口，并提供“推荐给大多数用户”的说明。
3. 选择本地 Ollama 后，必须执行本地连通性检测（端口、模型列表、可用性）并给出一键修复提示。
4. 三种方式必须允许后续在设置页随时切换，不可锁死。
5. OAuth 提供商清单必须从 OpenClaw Provider/Auth 注册表或插件系统生成，新增上游 OAuth 时无需改引导页信息架构。

## 7. OAuth 基线清单（2026-02-18）
1. 当前实现至少覆盖以下 OpenClaw OAuth/设备登录方式：
   - OpenAI Codex（ChatGPT OAuth）
   - Chutes OAuth
   - Google Antigravity OAuth
   - Google Gemini CLI OAuth
   - MiniMax OAuth
   - Qwen OAuth
   - GitHub Copilot Device Login（GitHub 设备流）
2. 以上清单仅作当前版本基线，最终以 OpenClaw 上游注册表/插件系统返回的可用 OAuth 列表为准。

## 8. 上游同步策略（MUST）
1. 代码分层必须采用“双目录”：
   - `openclaw/`：官方上游代码（只做参考与同步，不承载业务逻辑）
   - `openclaw-desktop/`：原生壳与产品功能实现
2. 与上游交互优先采用适配层（Adapter/Bridge），禁止把产品需求直接耦合进上游核心目录。
3. 如确需改动上游，必须：
   - 在单独补丁目录记录变更原因与影响面
   - 提供可重放的同步流程（升级上游后可自动检测冲突）
   - 约束为“小补丁、可删除、可替换”

## 9. 协作角色（MUST）
1. 助手角色是 CTO 合伙人，不是被动执行者。
2. 在关键技术选型、架构取舍、风险控制上，必须主动给出明确判断与备选方案，不得只复述需求。
3. 当用户方案存在明显风险时，必须直接指出并给出可执行替代路径。

## 10. 发布验收经验（MUST）
1. macOS 发布前必须做 Gatekeeper 验收，避免用户看到“已损坏，移到废纸篓”。
2. CI 产物必须通过以下检查，否则禁止发布：
   - `codesign --verify --deep --strict --verbose=2 <app>`
   - `spctl -a -vv <app>`（至少在验收机人工复核一次）
   - 挂载 DMG 后，对 DMG 内 `.app` 再执行一次 `codesign --verify --deep --strict`
3. 若未配置 Apple Developer ID 证书与公证流程，至少要保证完整 ad-hoc 签名链可校验，不允许产出“签名结构损坏”的包。
4. 用户侧临时修复方案要写入帮助文档：
   - `xattr -dr com.apple.quarantine /Applications/openclaw-desktop.app`
   - `codesign --force --deep --sign - /Applications/openclaw-desktop.app`
5. 每次发版后必须在真实 macOS 机器完成一次“下载 -> 安装 -> 首次启动”冒烟验证并记录结果。
6. 若出现 `code has no resources but signature indicates they must be present`，判定为发布阻断级故障，必须先修复 CI 再允许继续发版。

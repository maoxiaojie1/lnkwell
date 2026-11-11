# 贡献指南

感谢愿意一起完善 Lnkwell。提 PR 前花几分钟读一下这份约定，能省掉来回沟通。

## 环境要求

| 工具 | 版本 | 说明 |
|---|---|---|
| Node.js | ≥ 20 | 前端构建 |
| Rust | ≥ 1.77.2 | 后端；`rustup target add aarch64-linux-android` 等按需添加 |
| Tauri CLI | 2.x | 随 `npm install` 安装，无需全局 |
| Android SDK | platforms 36 + NDK 26.1 | 仅 Android 构建需要 |

桌面端依赖 WebView2（Windows）、WebKitGTK（Linux）、系统 WebKit（macOS），详见 [Tauri 前置条件](https://tauri.app/start/prerequisites/)。

Android 还需要两个**不在仓库里**的本机文件，重建工程后要重新补：

- `src-tauri/.cargo/config.toml` —— 从 `config.toml.example` 复制并填入 NDK 路径
- `src-tauri/gen/android/` —— 由 `npx tauri android init --ci` 生成，签名与安全区代码见 [开发文档](docs/development.md)

## 常用命令

```bash
npm install          # 安装前端依赖
npm run dev          # 只跑前端（无 Tauri API，部分功能报错，用于调样式）
npm run tauri dev    # 完整桌面应用
npm run check        # svelte-check 类型检查（PR 必须通过）
npm run build        # 前端产物

cd src-tauri
cargo test           # 单测 + fixtures 渲染回归
cargo fmt --check    # 格式检查
cargo clippy --all-targets
```

## 代码风格

- 前端：2 空格缩进，Svelte 5 runes（`$state` / `$derived` / `$effect`），不引入新的状态库。
- Rust：`cargo fmt` 默认风格；错误统一走 `error.rs` 的 `AppError`（带 code），不要 `unwrap()` 处理用户输入。
- 注释写「为什么」，不写「是什么」。已有注释多为踩坑记录，改动相关代码时请一并更新，否则会误导后来者。
- 新增文案、注释用中文；标识符、提交信息遵循 Conventional Commits（`feat:` / `fix:` / `chore:` / `docs:` …）。

## 提 PR 前自检

1. `npm run check` 与 `cargo test` 均通过；
2. 涉及 Markdown 渲染的改动，在 `fixtures/cases/` 补一个样本，并在 `markdown.rs` 的测试里断言；
3. 涉及检索层 SQL 的改动，跑 `python scripts/verify-fts.py`（24 项断言，含老库迁移）；
4. 不要提交 `src-tauri/gen/`、`dist/`、`target/`、APK、构建日志、本机绝对路径；
5. 一个 PR 只做一件事，附带「为什么」而不只是「改了什么」。

## 分支与提交

- 从 `master` 切分支：`feat/xxx`、`fix/xxx`、`docs/xxx`；
- 提交粒度按「可独立回滚的功能」划分，不要一个大提交混多种改动；
- 合并使用 squash merge，PR 标题即最终 commit 标题。

## 报告问题

- Bug 用 [Bug 报告模板](.github/ISSUE_TEMPLATE/bug_report.yml)，附平台、版本、复现步骤；渲染问题请贴出最小 Markdown 片段。
- 安全问题不要开公开 issue，见 [SECURITY.md](SECURITY.md)。

## 许可

提交代码即表示同意以 [MIT](LICENSE) 协议发布。

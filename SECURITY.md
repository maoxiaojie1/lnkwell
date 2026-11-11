# 安全策略 / Security Policy

## 支持版本

只维护最新发布版本的安全修复。

| 版本 | 支持 |
|---|---|
| 0.1.x | ✅ |

## 报告漏洞

**请不要开公开 issue。** 通过以下任一渠道私下报告：

- GitHub Security Advisories：仓库 **Security → Report a vulnerability**
- 邮件：1324501764@qq.com

请附上：影响版本、平台（Windows / Android / Linux / macOS）、复现步骤、影响描述（能否读到任意文件、执行代码、绕过 XSS 消毒等）。

一般 3 个工作日内回复。修复发布后会在 Release Notes 中致谢（除非你要求匿名）。

## 威胁模型

Lnkwell 处理的是**不受信任的 Markdown**，因此以下面为安全边界：

- Markdown 渲染在 Rust 侧完成，HTML 经 `ammonia` 白名单消毒后才注入 WebView；
- 文件访问只允许在用户选定的文档库根目录内，禁止 `..` 越界与符号链接逃逸；
- 相对路径图片经 `md-rel://` 哨兵回到 Rust 解析，前端不拼接路径；
- WebView 开启了 CSP 与 `freezePrototype`；外链交给系统浏览器，应用内不加载远端内容。

以下**不在**范围内：恶意 Markdown 作者本身就是文件的所有者（例如你自己打开一个恶意仓库的 README）；已获得 root / 管理员权限的攻击者；Tauri 或系统 WebView 本身的 0day。

Please report vulnerabilities privately via GitHub Security Advisories or the email above, not in public issues.

# Lnkwell 测试文档

这是一个用于验证渲染效果的**测试文档**，覆盖 Markdown 主要语法。

## 文本格式

- **加粗文本**
- *斜体文本*
- ~~删除线~~
- `行内代码`
- [外部链接](https://tauri.app)

## 代码块

```rust
#[tauri::command]
async fn pick_vault(app: AppHandle) -> AppResult<Option<String>> {
    Ok(vault::pick_root(&app).await?.map(|v| v.0))
}
```

## 表格

| 平台 | 框架 | 状态 |
|---|---|---|
| Android | Tauri 2 | 可用 |
| Windows | Tauri 2 | 可用 |

## 任务列表

- [x] 选库（SAF）
- [x] 目录树
- [x] Markdown 渲染
- [ ] 全文搜索

## 引用

> 离线优先的 Markdown 阅读器
> 把本地文档渲染得和 GitHub 一样好看

## 数学公式

行内公式 $E = mc^2$，块级公式：

$$
\int_{0}^{\infty} e^{-x^2} dx = \frac{\sqrt{\pi}}{2}
$$

## 列表

1. 第一项
2. 第二项
3. 第三项

## 分隔线

---

文档结束。

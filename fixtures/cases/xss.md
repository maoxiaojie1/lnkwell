# XSS 样本

`<script>` 内联脚本必须被剥离。

<script>alert('xss')</script>

<img src=x onerror="alert('xss')">

[坏链接](javascript:alert('xss'))

<a href="javascript:alert(1)">anchor</a>

<iframe src="https://evil.example"></iframe>

正常段落 **加粗** 与 `代码`。

<details>
<summary>原生折叠块应保留</summary>

折叠内容。

</details>

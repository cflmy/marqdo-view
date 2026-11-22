# @knowledge 安全测试页

<script>alert('xss')</script>

<img src=x onerror="alert(1)">

<svg onload="alert(1)"></svg>

[危险链接](javascript:alert(2))

[数据链接](data:text/html;base64,PHNjcmlwdD5hbGVydCgzKTwvc2NyaXB0Pg==)

普通正文仍然必须可读：**加粗** 与 `代码`。

| 安全 | 说明 |
| --- | --- |
| 原始 HTML | 已忽略 |

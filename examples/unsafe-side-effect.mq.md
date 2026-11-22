# @knowledge 受控写入示例

本文件只有在显式提交 `project` 运行并授予项目内 `out/**` 写入权限时才会写文件。
打开、刷新、搜索、切换和静态导出都不会运行它。

## @exec main

**创建 = resource.request("fs.mkdir", ["out"])**

**写入 = fs.write("out/view-sentinel.txt", "explicit run only\n")**

*return 写入*

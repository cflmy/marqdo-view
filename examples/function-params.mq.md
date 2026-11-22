# @knowledge 计算示例

这是一个可读取、可独立调用的函数。未点击运行时不能发生计算副作用。

## @exec double

`params(n: Int)`

`return n * 2`

## @exec format_arguments

组合表单提交的文本和列表，用于展示常量参数经过 CLI 后返回的结果。

`params(label: Text, items: List)`

`return label + items[0] + " · " + 文本(items[1])`

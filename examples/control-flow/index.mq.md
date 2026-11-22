# @knowledge 控制流演示

下面的函数可以在 `pure` 模式显式运行并产生真实 Trace。示例把 ASCII 数字和换行写入标准输出资源。

# @exec 演示

## @exec main

`x = 0`

`buffer = memory.new(1, 1)`

`buffer[1] = 10`

`stdout = resource.open("stdio", "stdout", ["write"])`

**while x < 3**

**if x > 1**

`buffer[0] = 48 + x`

**else**

`buffer[0] = 48`

**end**

`resource.write(stdout, buffer, 0, 2)`

**x = x + 1**

**end**

`resource.close(stdout)`

`return 3`

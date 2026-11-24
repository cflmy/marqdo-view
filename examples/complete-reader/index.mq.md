---
import:
  notes: "./docs/explanation.mq.md"
---

# @knowledge 小型统计实验

样本的均值 $\bar{x}$ 可用下式计算：

$$
\bar{x} = \frac{1}{n} \sum_{i=1}^{n} x_i
$$

![样本数据图](media/flow.png)

| 样本 | 数值 |
| :--- | ---: |
| A | 3 |
| B | 5 |

- [x] 记录样本
- [ ] 审查结果

[误差说明](docs/explanation.mq.md#误差-1) · [本页公式](#计算均值)

# @exec 计算

## @exec main

`return 4`

## @exec 计算均值

计算两个样本的均值。

`params(a: Int, b: Int = 5)`

`return (a + b) / 2`

## @exec classify

`params(n: Int)`

**if n < 0**

`return -1`

**else if n == 0**

`return 0`

**else**

`return 1`

**end**

## @exec scan

`n = 0`

**while n < 10**

`n = n + 1`

**if n == 2**

`continue`

**else if n == 4**

`break`

**end**

**end**

`return n`

## @exec fail

`params(divisor: Int)`

`return 3 / divisor`

## @exec write_output

`buffer = memory.new(1, 1)`

`buffer[0] = 52`

`buffer[1] = 10`

`stdout = resource.open("stdio", "stdout", ["write"])`

`resource.write(stdout, buffer, 0, 2)`

`resource.close(stdout)`

`return 4`

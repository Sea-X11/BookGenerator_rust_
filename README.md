# book_generator（Rust 版）

用 Rust 重写的 Gemini API 章节内容自动生成器，功能与 Python 版完全对应。

## 环境要求

- Rust 1.75+（安装：https://rustup.rs）
- 有效的 Gemini API Key

## 快速开始

```bash
# 1. 复制并填写 API Key
cp .env.example .env
# 编辑 .env，填入你的 GEMINI_API_KEY

# 2. 编译
cargo build --release

# 3. 运行（交互模式）
./target/release/book_generator

# 4. 直接指定文件（批处理模式）
./target/release/book_generator chapters.txt

# 5. 更多选项
./target/release/book_generator chapters.txt -o my_output -q -y
```

## 命令行参数

| 参数 | 说明 | 默认值 |
|------|------|--------|
| `input_file` | 章节概述文件路径（可选，省略则交互输入）| 无 |
| `-o, --output` | 输出目录 | `generated_chapters` |
| `-q, --quiet` | 安静模式，不打印生成内容 | 否 |
| `-y, --yes` | 跳过确认提示 | 否 |

## 章节概述文件格式

每行一个章节概述，空行会被忽略，例如：

```
引言：人工智能的崛起与挑战
机器学习基础：从感知机到神经网络
深度学习的革命
```

## 代理设置

代码默认尝试使用 `http://127.0.0.1:7890` 代理（与原 Python 版一致）。
如果不需要代理，代理连接失败会自动降级为直连。

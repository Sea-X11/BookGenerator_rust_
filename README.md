# Book Generator 📖

这是一个用 Rust 编写的高效章节内容自动生成器。它可以根据你提供的章节大纲，结合上下文自动编写整本书的内容。
目前支持 **Google Gemini API** 和 **本地 Ollama** 模型。

## ✨ 主要特性

- **双引擎支持**：支持在线调用高质量的 Gemini API（如 `gemini-2.0-pro-exp`）或本地完全离线跑运行 Ollama 模型（如 `llama3`）。
- **SSE 流式输出**：在终端实时看到生成的内容，并同时将结果保存到文件中。
- **自动重试机制**：使用 Gemini API 时，如果遇到 429 频率限制（Rate Limit），程序会自动解析重试时间并等待后重新请求。
- **全书上下文感知**：在生成每个独立章节时，会自动拼接所有章节的概述作为全局背景上下文，确保全书内容逻辑连贯。
- **完善的参数配置**：支持通过 `.env` 环境变量文件进行配置，同时提供了丰富的命令行参数用于覆盖默认设置。
- **网络代理支持**：自动适配代理环境变量，并在代码层面专门为国内直连不畅情况设计了代理支持机制。

## ⚙️ 环境要求

- **Rust 1.75+**：[安装 Rust](https://rustup.rs/) (用于编译此项目)
- **Gemini 模式需求**：有效的 Google Gemini API Key
- **Ollama 模式需求**：本地或同局域网已部署 [Ollama](https://ollama.com/) 服务并下载了对应模型（如 `llama3`）

## 🚀 快速开始

### 1. 克隆与编译

```bash
git clone <your-repo-url>
cd book_generator

# 编译为 release 版本以获得最佳性能
cargo build --release
```

### 2. 配置文件设置

在项目根目录创建一个 `.env` 文件，并填入你的个人配置：

```env
# API 服务端类型: gemini 或 ollama
API_TYPE=gemini

# 若使用 Gemini，必须填写 API Key
GEMINI_API_KEY=your_gemini_api_key_here

# 代理设置 (可选，对本地 Ollama 自动无效以免本地连接被代理)
PROXY_URL=http://127.0.0.1:7890

# 可选高级设置覆盖 
# API_BASE_URL=https://generativelanguage.googleapis.com/v1beta/models
# MODEL=gemini-2.0-pro-exp-02-05
```

### 3. 准备大纲文件

准备一个 `chapters.txt` 文件，每行书写一章的概述内容（忽略空行）：

```text
第一章：人工智能的黎明与早期探索
第二章：从规则系统到机器学习的演变
第三章：深度学习的爆发式增长与革命
第四章：大语言模型架构及原理解析
```

### 4. 运行生成器

**常规交互式运行：**

```bash
./target/release/book_generator
```

**自动化批处理模式（静默执行）：**

```bash
./target/release/book_generator chapters.txt -o my_book_output -q -y
```

**临时切换为使用本地 Ollama（如断网时）：**

```bash
./target/release/book_generator chapters.txt --api-type ollama --model llama3 --api-base http://127.0.0.1:11434/api/generate
```

## 🛠 命令行参数详解

| 参数标志 | 说明 | 默认值 / 行为 |
|----------|------|---------------|
| `input_file`  | 包含章节概述的源文件路径（作为位置参数，如未指定将交互提示输入） | 无 |
| `-o, --output`| 生成的章节内容保存的目录 | `generated_chapters` |
| `-q, --quiet` | 安静模式：不在终端实时打印生成的文本流内容 | `false` |
| `-y, --yes`   | 自动化选项：跳过“是否开始生成”的用户确认提示 | `false` |
| `--api-type`  | 强制设定 API 类型（`gemini` 或 `ollama`），优先级高于 `.env` 配置 | 读取环境变量或设为 `gemini` |
| `--api-base`  | 自定义底层的 API 请求地址，优先级高于 `.env` | 由 API 类型决定默认地址 |
| `--model`     | 执行生成所用的模型名称，优先级高于 `.env` | Gemini 默认 `gemini-2.0-pro-exp-02-05`，Ollama 默认 `llama3` |

## 📝 输出效果

生成器会在命令指定的输出目录（默认 `./generated_chapters`）下自动构建带编号的 TXT 文件，并逐个完成流式写入：
- `第1章.txt`
- `第2章.txt`
- `第3章.txt`
- ...

如果生成途中进程因网络掉线或其他意外中断，你可以很方便地修改 `chapters.txt`（仅保留尚未生成的最后几章信息），然后重新执行即可恢复进度（无需覆盖已有内容）。

## 📜 许可证

本项目基于 [MIT License](LICENSE) 许可协议开源。

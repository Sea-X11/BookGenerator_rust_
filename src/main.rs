use clap::Parser;
use dotenvy::dotenv;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use tokio::time::{sleep, Duration};

// ─── CLI 参数 ────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "book_generator", about = "自动从章节概述生成书籍内容 (Gemini API)")]
struct Args {
    /// 包含章节概述的文件路径（每行一个章节）
    input_file: Option<String>,

    /// 输出目录
    #[arg(short, long, default_value = "generated_chapters")]
    output: String,

    /// 安静模式，不在终端打印生成内容
    #[arg(short, long)]
    quiet: bool,

    /// 跳过确认提示，自动开始
    #[arg(short, long)]
    yes: bool,

    /// 提供服务后端的 API 类型, 可选: gemini, ollama. 默认读取 .env 的 API_TYPE
    #[arg(long)]
    api_type: Option<String>,

    /// API 基准链接
    #[arg(long)]
    api_base: Option<String>,

    /// 使用的模型名称
    #[arg(long)]
    model: Option<String>,
}

// ─── Gemini API 数据结构 ──────────────────────────────────────────────────────

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "generationConfig")]
    generation_config: GenerationConfig,
}

#[derive(Serialize, Deserialize, Debug)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize, Deserialize, Debug)]
struct GeminiPart {
    text: String,
}

#[derive(Serialize)]
struct GenerationConfig {
    temperature: f32,
    #[serde(rename = "topP")]
    top_p: f32,
    #[serde(rename = "topK")]
    top_k: u32,
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: u32,
    #[serde(rename = "responseMimeType")]
    response_mime_type: String,
}

// SSE 流中的候选项
#[derive(Deserialize, Debug)]
struct StreamChunk {
    candidates: Option<Vec<Candidate>>,
}

#[derive(Deserialize, Debug)]
struct Candidate {
    content: Option<GeminiContent>,
}

// ─── Ollama API 数据结构 ──────────────────────────────────────────────────────

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize, Debug)]
struct OllamaResponse {
    response: String,
    done: bool,
}

// ─── 核心生成函数 ─────────────────────────────────────────────────────────────

/// 调用 Gemini streaming API，将结果打印并写入文件
async fn generate_content_gemini(
    client: &Client,
    api_base: &str,
    model: &str,
    api_key: &str,
    prompt: &str,
    output_file: Option<&Path>,
    verbose: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let url = format!(
        "{}/{}:streamGenerateContent?alt=sse&key={}",
        api_base.trim_end_matches('/'), model, api_key
    );

    let body = GeminiRequest {
        contents: vec![GeminiContent {
            role: "user".to_string(),
            parts: vec![GeminiPart {
                text: prompt.to_string(),
            }],
        }],
        generation_config: GenerationConfig {
            temperature: 1.0,
            top_p: 0.95,
            top_k: 64,
            max_output_tokens: 8192,
            response_mime_type: "text/plain".to_string(),
        },
    };

    let max_retries = 3u32;
    let mut attempt = 0u32;

    loop {
        attempt += 1;
        let response = client
            .post(&url)
            .json(&body)
            .send()
            .await?;

        if response.status() == 429 {
            let status = response.status();
            let text = response.text().await?;

            // 尝试从返回的 JSON 中解析 retryDelay
            let retry_secs: u64 = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v["error"]["details"].as_array().cloned())
                .and_then(|details| {
                    details.iter().find_map(|d| {
                        d["retryDelay"].as_str().and_then(|s| {
                            s.trim_end_matches('s').parse::<u64>().ok()
                        })
                    })
                })
                .unwrap_or(60);

            if attempt < max_retries {
                eprintln!("\n请求超出频率限制 (429)，{} 秒后自动重试... ({}/{})", retry_secs, attempt, max_retries - 1);
                sleep(Duration::from_secs(retry_secs + 2)).await;
                continue;
            } else {
                return Err(format!("API 错误 {}: {}", status, text).into());
            }
        }

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await?;
            return Err(format!("API 错误 {}: {}", status, text).into());
        }

        let mut full_response = String::new();
        let mut file_handle = output_file.map(|p| {
            fs::File::create(p).expect("无法创建输出文件")
        });

        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            let text = String::from_utf8_lossy(&chunk);

            // SSE 格式：每行以 "data: " 开头
            for line in text.lines() {
                if let Some(json_str) = line.strip_prefix("data: ") {
                    if json_str.trim() == "[DONE]" {
                        break;
                    }
                    if let Ok(parsed) = serde_json::from_str::<StreamChunk>(json_str) {
                        if let Some(candidates) = parsed.candidates {
                            for candidate in candidates {
                                if let Some(content) = candidate.content {
                                    for part in content.parts {
                                        if verbose {
                                            print!("{}", part.text);
                                            let _ = std::io::stdout().flush();
                                        }
                                        full_response.push_str(&part.text);
                                        if let Some(fh) = file_handle.as_mut() {
                                            fh.write_all(part.text.as_bytes())?;
                                            fh.flush()?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if verbose {
            println!(); // 换行
        }

        return Ok(full_response);
    } // end loop
}

/// 调用 Ollama streaming API，将结果打印并写入文件
async fn generate_content_ollama(
    client: &Client,
    api_base: &str,
    model: &str,
    prompt: &str,
    output_file: Option<&Path>,
    verbose: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let body = OllamaRequest {
        model: model.to_string(),
        prompt: prompt.to_string(),
        stream: true,
    };

    let response = client.post(api_base).json(&body).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await?;
        return Err(format!("Ollama API 错误 {}: {}", status, text).into());
    }

    let mut full_response = String::new();
    let mut file_handle = output_file.map(|p| {
        fs::File::create(p).expect("无法创建输出文件")
    });

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        while let Some(idx) = buffer.find('\n') {
            let line = buffer[..idx].trim().to_string();
            buffer = buffer[idx + 1..].to_string();

            if line.is_empty() {
                continue;
            }

            if let Ok(parsed) = serde_json::from_str::<OllamaResponse>(&line) {
                if verbose {
                    print!("{}", parsed.response);
                    let _ = std::io::stdout().flush();
                }
                full_response.push_str(&parsed.response);
                if let Some(fh) = file_handle.as_mut() {
                    fh.write_all(parsed.response.as_bytes())?;
                    fh.flush()?;
                }
                if parsed.done {
                    break;
                }
            }
        }
    }

    if verbose {
        println!(); // 换行
    }

    Ok(full_response)
}

// ─── 章节 Prompt 构造 ─────────────────────────────────────────────────────────

fn create_chapter_prompt(
    chapter_overview: &str,
    chapter_index: usize,
    all_overviews: &[String],
) -> String {
    let chapters_context = all_overviews
        .iter()
        .enumerate()
        .map(|(i, ov)| format!("第{}章: {}", i + 1, ov))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"我需要你帮我撰写一本书的内容。以下是整本书的所有章节概述:

{chapters_context}

请根据上述整体结构，帮我详细撰写第{chapter_num}章的内容。当前章节概述是："{chapter_overview}"。

请创作约2000-3000字的高质量内容，保持与整本书的连贯性，并根据章节概述展开叙述。请使用专业、流畅的语言风格。
"#,
        chapters_context = chapters_context,
        chapter_num = chapter_index + 1,
        chapter_overview = chapter_overview,
    )
}

// ─── 读取章节概述文件 ──────────────────────────────────────────────────────────

fn read_chapter_outlines(file_path: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(file_path)
        .map_err(|_| format!("找不到文件: '{}'", file_path))?;

    let outlines: Vec<String> = content
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    if outlines.is_empty() {
        return Err(format!("文件 '{}' 为空或不包含任何章节概述", file_path).into());
    }

    Ok(outlines)
}

// ─── 主函数 ───────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // 加载 .env
    dotenv().ok();

    let args = Args::parse();

    println!("{}", "=".repeat(60));
    println!("{:^50}", "章节内容自动生成器");
    println!("{}", "=".repeat(60));

    // 解析配置：API 类型，Base URL，Model
    let api_type = args.api_type
        .or_else(|| env::var("API_TYPE").ok())
        .unwrap_or_else(|| "gemini".to_string())
        .to_lowercase();
    let is_ollama = api_type == "ollama";

    let api_key = env::var("GEMINI_API_KEY").unwrap_or_default();
    if !is_ollama && api_key.is_empty() {
        eprintln!("错误：使用 Gemini 时，未找到 GEMINI_API_KEY，请在 .env 文件中设置");
        std::process::exit(1);
    }

    let api_base = args.api_base
        .or_else(|| env::var("API_BASE_URL").ok())
        .unwrap_or_else(|| {
            if is_ollama {
                "http://127.0.0.1:11434/api/generate".to_string()
            } else {
                "https://generativelanguage.googleapis.com/v1beta/models".to_string()
            }
        });

    let model_name = args.model
        .or_else(|| env::var("MODEL").ok())
        .unwrap_or_else(|| {
            if is_ollama {
                "llama3".to_string() // 可以换成你想在本地运行的模型名称
            } else {
                "gemini-2.0-pro-exp-02-05".to_string()
            }
        });

    println!("当前配置:");
    println!("API 类型: {}", if is_ollama { "Ollama" } else { "Gemini" });
    println!("API 链接: {}", api_base);
    println!("模型名称: {:?}", model_name);
    println!("{}", "=".repeat(60));

    // 确定输入文件
    let input_file = match args.input_file {
        Some(f) => f,
        None => {
            print!("请输入包含章节概述的文件路径: ");
            std::io::stdout().flush().unwrap();
            let mut buf = String::new();
            std::io::stdin().read_line(&mut buf).unwrap();
            buf.trim().to_string()
        }
    };

    // 创建输出目录
    fs::create_dir_all(&args.output).expect("无法创建输出目录");

    // 读取章节概述
    println!("\n正在从 '{}' 读取章节概述...", input_file);
    let chapter_overviews = match read_chapter_outlines(&input_file) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("发生错误: {}", e);
            std::process::exit(1);
        }
    };

    println!("\n共读取到 {} 个章节概述:", chapter_overviews.len());
    for (i, ov) in chapter_overviews.iter().enumerate() {
        println!("第{}章: {}", i + 1, ov);
    }

    // 确认
    if !args.yes {
        print!("\n是否开始生成章节内容? (y/n): ");
        std::io::stdout().flush().unwrap();
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        if input.trim().to_lowercase() != "y" {
            println!("操作已取消");
            return;
        }
    }

    // 设置代理（如需要）
    let client = if is_ollama {
        // 访问本地 Ollama 必须绕过所有代理（包括系统代理），否则 reqwest 会
        // 自动读取 macOS 系统代理设置，导致本地请求被代理转发后返回 502
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap_or_else(|_| Client::new())
    } else {
        // Gemini：如果 .env 中设置了 PROXY_URL 则使用，否则不设置代理（走系统默认）
        let builder = reqwest::Client::builder();
        match env::var("PROXY_URL").ok().filter(|s| !s.is_empty()) {
            Some(proxy_url) => {
                println!("使用代理: {}", proxy_url);
                match reqwest::Proxy::all(&proxy_url) {
                    Ok(proxy) => builder.proxy(proxy).build().unwrap_or_else(|_| Client::new()),
                    Err(e) => {
                        eprintln!("代理配置错误: {}，将不使用代理", e);
                        builder.build().unwrap_or_else(|_| Client::new())
                    }
                }
            }
            None => builder.build().unwrap_or_else(|_| Client::new()),
        }
    };

    // 生成每一章
    println!("\n开始生成章节...\n");
    let total = chapter_overviews.len();

    for (i, overview) in chapter_overviews.iter().enumerate() {
        let chapter_num = i + 1;
        let filename = format!("{}/第{}章.txt", args.output, chapter_num);
        let out_path = Path::new(&filename);

        println!("正在生成第{}章: {}", chapter_num, overview);
        println!("{}", "-".repeat(60));

        let prompt = create_chapter_prompt(overview, i, &chapter_overviews);

        let result = if is_ollama {
            generate_content_ollama(&client, &api_base, &model_name, &prompt, Some(out_path), !args.quiet).await
        } else {
            generate_content_gemini(&client, &api_base, &model_name, &api_key, &prompt, Some(out_path), !args.quiet).await
        };

        match result {
            Ok(_) => {
                println!("\n第{}章内容已保存到: {}", chapter_num, filename);
            }
            Err(e) => {
                eprintln!("生成第{}章时出错: {}", chapter_num, e);
                eprintln!("跳过此章节并继续");
            }
        }

        println!("{}\n", "-".repeat(60));

        // 避免触发 API 速率限制（免费套餐限制较严，等待 15 秒）
        if i < total - 1 {
            println!("等待 15 秒后继续生成下一章...");
            sleep(Duration::from_secs(15)).await;
        }
    }

    println!("\n所有章节生成完成!");
    println!("文件已保存在 '{}' 目录下", args.output);
}

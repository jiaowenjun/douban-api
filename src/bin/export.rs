//! douban-export CLI —— 免登录导出豆瓣用户的公开书影音标记（含短评）
//!
//! 仅供个人数据备份，请保持默认限速（--delay），尊重豆瓣服务。

use anyhow::{bail, Result};
use clap::Parser;
use douban_export::crawler::{
    self, Category, Format, Record, Status, export_records, parse_uid,
};
use rand::Rng;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

// --------------------------------------------------------------------------- //
// 网络（阻塞式，CLI 专用；server 用独立的异步实现）
// --------------------------------------------------------------------------- //

fn make_client() -> Result<reqwest::blocking::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    for (k, v) in crawler::BASE_HEADERS {
        headers.insert(k, v.parse().expect("static header"));
    }
    Ok(reqwest::blocking::Client::builder()
        .user_agent(crawler::UA)
        .default_headers(headers)
        .timeout(Duration::from_secs(30))
        .build()?)
}

/// 403/418/5xx 视为被限流，指数退避重试；404 快速失败。
fn fetch_html(client: &reqwest::blocking::Client, url: &str, referer: &str) -> Result<String> {
    const MAX_RETRY: u32 = 5;
    for attempt in 1..=MAX_RETRY {
        match client.get(url).header("Referer", referer).send() {
            Ok(resp) => {
                let code = resp.status().as_u16();
                if code == 404 {
                    bail!("页面不存在（404）：{url}");
                }
                if resp.status().is_success() {
                    let html = String::from_utf8_lossy(&resp.bytes()?).into_owned();
                    if html.len() > 5000 {
                        return Ok(html); // 正常页面 >30KB；防被插入软拦截页
                    }
                    println!("  ⚠ 响应过短({}B)疑似拦截，{}s 后重试", html.len(), attempt * 15);
                    sleep(Duration::from_secs(u64::from(attempt) * 15));
                } else {
                    let wait = 20.0 * attempt as f64 + rand::thread_rng().gen_range(0.0..5.0);
                    println!("  ⚠ HTTP {code}，{wait:.0}s 后重试 {attempt}/{MAX_RETRY}");
                    if attempt == MAX_RETRY {
                        bail!("抓取失败（HTTP {code}）：{url}");
                    }
                    sleep(Duration::from_secs_f64(wait));
                }
            }
            Err(e) => {
                if attempt == MAX_RETRY {
                    bail!("网络异常：{e}");
                }
                let wait = 10 * attempt;
                println!("  ⚠ 网络异常({e})，{wait}s 后重试 {attempt}/{MAX_RETRY}");
                sleep(Duration::from_secs(u64::from(wait)));
            }
        }
    }
    bail!("抓取失败: {url}")
}

// --------------------------------------------------------------------------- //
// 抓取主流程（文件断点续爬，CLI 专用；server 用 DB 队列）
// --------------------------------------------------------------------------- //

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Checkpoint {
    next_start: usize,
    total: Option<usize>,
    items: Vec<Record>,
}

fn checkpoint_path(out_dir: &Path, uid: &str, category: Category, status: Status) -> PathBuf {
    out_dir.join(format!(
        ".{}_{}_{}.ckpt.json",
        uid,
        category.key(),
        status.key()
    ))
}

fn load_checkpoint(path: &Path) -> Checkpoint {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_checkpoint(path: &Path, ckpt: &Checkpoint) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_string(ckpt)?)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn crawl(
    client: &reqwest::blocking::Client,
    uid: &str,
    category: Category,
    status: Status,
    out_dir: &Path,
    delay: f64,
    max_pages: Option<usize>,
) -> Result<Vec<Record>> {
    let ckpt_file = checkpoint_path(out_dir, uid, category, status);
    let mut ckpt = load_checkpoint(&ckpt_file);
    if !ckpt.items.is_empty() {
        println!(
            "  ↩ 断点续爬：已有 {} 条，从 start={} 继续",
            ckpt.items.len(),
            ckpt.next_start
        );
    }

    let referer = crawler::referer_for(uid);
    let mut start = ckpt.next_start;
    loop {
        if let Some(mp) = max_pages {
            if start >= mp * crawler::PAGE_SIZE {
                println!("  ⏹ 已达 --max-pages={mp} 上限，提前结束（进度已保存）");
                break;
            }
        }
        if let Some(t) = ckpt.total {
            if start >= t {
                break;
            }
        }

        let url = crawler::collection_url(category, uid, status, start);
        let html = fetch_html(client, &url, &referer)
            .map_err(|e| e.context(format!("{}·{}", category.key(), status.key())))?;

        if let Some(t) = crawler::parse_total(&html) {
            ckpt.total = Some(t);
        }
        let items = crawler::parse_items(&html, category, status);
        if items.is_empty() {
            break;
        }

        // 按 subject_id 去重（标记新增/翻页偏移可能造成重复）
        let existing: HashSet<String> =
            ckpt.items.iter().map(|r| r.subject_id.clone()).collect();
        let page_len = items.len();
        let dup = items
            .iter()
            .filter(|r| existing.contains(&r.subject_id))
            .count();
        ckpt.items
            .extend(items.into_iter().filter(|r| !existing.contains(&r.subject_id)));

        let got = ckpt.items.len();
        let real_total = ckpt.total.unwrap_or(start + page_len);
        let dup_note = if dup > 0 {
            format!("（重复 {dup} 条已跳过）")
        } else {
            String::new()
        };
        println!("  start={start:<5} 本页 {page_len:>2} 条，累计 {got}/{real_total}{dup_note}");

        ckpt.next_start = start + crawler::PAGE_SIZE;
        save_checkpoint(&ckpt_file, &ckpt)?;

        // 不以"本页不满 15 条"作为结束条件：页面可能因条目被删而少渲染，
        // 只有"空页"或"已达总数"才是可靠的终点。
        start += crawler::PAGE_SIZE;
        sleep(Duration::from_secs_f64(delay + rand::thread_rng().gen_range(0.0..1.0)));
    }
    Ok(ckpt.items)
}

// --------------------------------------------------------------------------- //
// CLI
// --------------------------------------------------------------------------- //

#[derive(Parser)]
#[command(
    name = "douban-export",
    about = "免登录导出豆瓣用户的公开书影音标记与短评（仅公开数据）",
    version
)]
struct Args {
    /// 豆瓣用户 ID 或个人主页 URL
    user: String,

    /// 类型，默认全部
    #[arg(long, value_enum, num_args = 1.., default_values_t = Vec::from(crawler::Category::ALL))]
    category: Vec<Category>,

    /// 标记状态，默认 collect（看过/读过/听过）
    #[arg(long, value_enum, num_args = 1.., default_values_t = vec![Status::Collect])]
    status: Vec<Status>,

    /// 导出格式，默认 csv json
    #[arg(long, value_enum, num_args = 1.., default_values_t = vec![Format::Csv, Format::Json])]
    format: Vec<Format>,

    /// 输出目录
    #[arg(long, default_value = "output")]
    out: PathBuf,

    /// 翻页间隔秒数（默认 2.0，请勿调小）
    #[arg(long, default_value_t = 2.0)]
    delay: f64,

    /// 每个类别最多抓取页数（测试用）
    #[arg(long)]
    max_pages: Option<usize>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let Some(uid) = parse_uid(&args.user) else {
        bail!(
            "无法从 {:?} 解析豆瓣用户 ID（应为 /people/<id>/ 或直接给 id）",
            args.user
        );
    };
    fs::create_dir_all(&args.out)?;

    println!("目标用户：{uid}");
    println!(
        "抓取计划：{} × {}，限速 {:.1}s/页\n",
        args.category
            .iter()
            .map(|c| c.key())
            .collect::<Vec<_>>()
            .join(" "),
        args.status
            .iter()
            .map(|s| s.key())
            .collect::<Vec<_>>()
            .join(" "),
        args.delay
    );

    let client = make_client()?;
    let mut all_results: Vec<Record> = Vec::new();

    for &category in &args.category {
        for &status in &args.status {
            println!("▶ {} · {}", category.kind_label(), category.status_label(status));
            let records = match crawl(&client, &uid, category, status, &args.out, args.delay, args.max_pages) {
                Ok(r) => r,
                Err(e) => {
                    println!("  ✗ 失败：{e:#}（进度已保存，可直接重跑续爬）\n");
                    continue;
                }
            };
            if records.is_empty() {
                println!("  （无公开数据或用户未标记）\n");
                continue;
            }

            // 按标记日期倒序
            let mut records = records;
            records.sort_by(|a, b| b.date.cmp(&a.date));

            let base = args.out.join(format!("{uid}_{}_{}", category.key(), status.key()));
            for &fmt in &args.format {
                let p = base.with_extension(fmt.key());
                let md_title = format!(
                    "{} 的{}记录·{}",
                    uid,
                    category.status_label(status),
                    category.kind_label()
                );
                export_records(fmt, &records, &p, &md_title)
                    .map_err(|e| anyhow::anyhow!("导出 {}: {e}", p.display()))?;
            }

            let n_comment = records.iter().filter(|r| !r.comment.is_empty()).count();
            println!(
                "  ✓ 完成 {} 条，其中含短评 {n_comment} 条 → {}.*\n",
                records.len(),
                base.display()
            );
            all_results.extend(records);
            let _ = fs::remove_file(checkpoint_path(&args.out, &uid, category, status));
        }
    }

    if !all_results.is_empty() {
        let mut counter = std::collections::BTreeMap::new();
        for r in &all_results {
            *counter.entry(format!("{}·{}", r.category, r.status)).or_insert(0) += 1;
        }
        let summary = serde_json::json!({
            "user": uid,
            "exported_at": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            "total": all_results.len(),
            "with_comment": all_results.iter().filter(|r| !r.comment.is_empty()).count(),
            "by_category_status": counter,
        });
        let p = args.out.join(format!("{uid}_summary.json"));
        fs::write(&p, serde_json::to_string_pretty(&summary)?)?;
        println!("全部完成：共 {} 条 → {}", all_results.len(), p.display());
    }
    Ok(())
}

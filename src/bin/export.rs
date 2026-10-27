//! douban-export —— 免登录导出豆瓣用户的公开书影音标记（含短评）
//!
//! 原理（调研报告《豆瓣API调研报告.md》中的"路径②：公开收藏页 HTML 抓取"）：
//! 电影/图书/音乐的收藏页是公开网页，无需登录 Cookie，直接抓取 HTML 解析：
//!   https://movie.douban.com/people/{uid}/{collect|do|wish}?start={0,15,30,...}
//!   https://book.douban.com/people/{uid}/...
//!   https://music.douban.com/people/{uid}/...
//! collect/do/wish 分别对应 看过·读过·听过 / 在看·在读·在听 / 想看·想读·想听。
//!
//! 页面结构（2026-10 实测）：
//!   电影/音乐为旧结构 div.item.comment-item（短评在 span.comment 或无 class 的 li）；
//!   图书为新结构 li.subject-item（短评在 p.comment.comment-item）；
//!   book.douban.com 反爬较严，必须携带浏览器级请求头 + Referer，否则 403。
//!
//! 注意：仅供个人数据备份，请保持默认限速（--delay），尊重豆瓣服务。

use anyhow::{bail, Context, Result};
use chrono::Local;
use clap::{Parser, ValueEnum};
use rand::Rng;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
const PAGE_SIZE: usize = 15; // 豆瓣收藏页固定每页 15 条

// --------------------------------------------------------------------------- //
// 类型定义
// --------------------------------------------------------------------------- //

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Category {
    Movie,
    Book,
    Music,
}

impl Category {
    fn key(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Book => "book",
            Self::Music => "music",
        }
    }
    fn domain(self) -> &'static str {
        match self {
            Self::Movie => "movie.douban.com",
            Self::Book => "book.douban.com",
            Self::Music => "music.douban.com",
        }
    }
    fn kind_label(self) -> &'static str {
        match self {
            Self::Movie => "电影·剧集",
            Self::Book => "图书",
            Self::Music => "音乐",
        }
    }
    fn status_label(self, status: Status) -> &'static str {
        match (self, status) {
            (Self::Movie, Status::Collect) => "看过",
            (Self::Movie, Status::Do) => "在看",
            (Self::Movie, Status::Wish) => "想看",
            (Self::Book, Status::Collect) => "读过",
            (Self::Book, Status::Do) => "在读",
            (Self::Book, Status::Wish) => "想读",
            (Self::Music, Status::Collect) => "听过",
            (Self::Music, Status::Do) => "在听",
            (Self::Music, Status::Wish) => "想听",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Status {
    Collect,
    Do,
    Wish,
}

impl Status {
    fn key(self) -> &'static str {
        match self {
            Self::Collect => "collect",
            Self::Do => "do",
            Self::Wish => "wish",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Csv,
    Json,
    Md,
    Xlsx,
}

/// 一条标记记录（声明顺序即 CSV 列顺序）。
#[derive(Clone, Default, Serialize, Deserialize)]
struct Record {
    category: String,
    status: String,
    title: String,
    alt_title: String,
    my_rating: Option<u8>,
    date: String,
    comment: String,
    subject_url: String,
    subject_id: String,
    cover: String,
    comment_id: String,
    intro: String,
}

const FIELDS: [&str; 12] = [
    "category", "status", "title", "alt_title", "my_rating", "date", "comment",
    "subject_url", "subject_id", "cover", "comment_id", "intro",
];

#[derive(Default, Serialize, Deserialize)]
struct Checkpoint {
    next_start: usize,
    total: Option<usize>,
    items: Vec<Record>,
}

// --------------------------------------------------------------------------- //
// 文本/解析辅助
// --------------------------------------------------------------------------- //

/// 压缩空白为单个空格并去首尾。
fn norm_space(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text_of(el: ElementRef) -> String {
    norm_space(&el.text().collect::<String>())
}

fn attr_or(el: ElementRef, name: &str) -> String {
    el.value().attr(name).unwrap_or("").to_string()
}

/// 从条目 URL 提取数字 ID：/subject/30208001/ -> 30208001
fn subject_id_from(url: &str) -> String {
    match url.find("/subject/") {
        Some(i) => url[i + 9..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect(),
        None => String::new(),
    }
}

/// rating5-t / rating3-t ... -> 5 / 3；无星级返回 None。
fn rating_from_class(el: ElementRef) -> Option<u8> {
    let cls = el.value().attr("class")?;
    let i = cls.find("rating")? + "rating".len();
    let digits: String = cls[i..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// 图书页日期 span 是 "2026-05-28 读过"，取第一个空白分隔的字段。
fn first_token(s: &str) -> String {
    s.split_whitespace().next().unwrap_or("").to_string()
}

/// 从 '1-15&nbsp;/&nbsp;208' 里取总条数。
fn parse_total(html: &str) -> Option<usize> {
    let i = html.find(r#"class="subject-num">"#)?;
    let rest = &html[i..];
    let j = rest.find("&nbsp;/&nbsp;")?;
    let after = rest[j + "&nbsp;/&nbsp;".len()..].trim_start();
    let digits: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

// --------------------------------------------------------------------------- //
// 页面条目解析（兼容两代结构）
// --------------------------------------------------------------------------- //

fn parse_items(html: &str, category: Category, status: Status) -> Vec<Record> {
    let doc = Html::parse_document(html);
    let mut records = Vec::new();

    let s_pic_a = Selector::parse("div.pic a").unwrap();
    let s_img = Selector::parse("img").unwrap();
    let s_rating = Selector::parse("span[class^=rating]").unwrap();
    let s_date = Selector::parse("span.date").unwrap();
    let s_span = Selector::parse("span").unwrap();

    // ---- 旧结构：电影 / 音乐 —— div.item.comment-item ----
    let s_item_old = Selector::parse("div.item.comment-item").unwrap();
    let s_title_em = Selector::parse("li.title em").unwrap();
    let s_intro = Selector::parse("li.intro").unwrap();
    let s_comment = Selector::parse("span.comment").unwrap();
    let s_info_li = Selector::parse("div.info ul > li").unwrap();

    for item in doc.select(&s_item_old) {
        let Some(pic_a) = item.select(&s_pic_a).next() else { continue };
        let pic_title = norm_space(&attr_or(pic_a, "title"));
        let subject_url = attr_or(pic_a, "href");
        let subject_id = subject_id_from(&subject_url);
        let cover = pic_a
            .select(&s_img)
            .next()
            .map(|im| attr_or(im, "src"))
            .unwrap_or_default();

        // 标题：优先取 li.title em 的主标题（简体），封面图 alt 兜底
        let (title, alt_title) = match item.select(&s_title_em).next() {
            Some(em) => {
                let line = text_of(em);
                match line.split_once(" / ") {
                    Some((main, alt)) => {
                        let main = norm_space(main);
                        (
                            if main.is_empty() { pic_title.clone() } else { main },
                            norm_space(alt),
                        )
                    }
                    None => (
                        if line.is_empty() { pic_title.clone() } else { line },
                        String::new(),
                    ),
                }
            }
            None => (pic_title.clone(), String::new()),
        };

        let intro = item
            .select(&s_intro)
            .next()
            .map(text_of)
            .unwrap_or_default();
        let my_rating = item.select(&s_rating).next().and_then(rating_from_class);
        let date = item
            .select(&s_date)
            .next()
            .map(|el| first_token(&text_of(el)))
            .unwrap_or_default();

        // 短评：电影在 span.comment；音乐在无 class 且无 span 子节点的 <li> 里
        let mut comment = item
            .select(&s_comment)
            .next()
            .map(text_of)
            .unwrap_or_default();
        if comment.is_empty() {
            for li in item.select(&s_info_li) {
                let has_class = li
                    .value()
                    .attr("class")
                    .is_some_and(|c| !c.trim().is_empty());
                if has_class || li.select(&s_span).next().is_some() {
                    continue; // 跳过 title/intro/评分日期行
                }
                let t = text_of(li);
                if !t.is_empty() {
                    comment = t;
                    break;
                }
            }
        }
        let comment_id = attr_or(item, "data-cid");

        records.push(Record {
            category: category.key().to_string(),
            status: category.status_label(status).to_string(),
            title,
            alt_title,
            my_rating,
            date,
            comment,
            subject_url,
            subject_id,
            cover,
            comment_id,
            intro,
        });
    }

    // ---- 新结构：图书 —— li.subject-item ----
    let s_item_new = Selector::parse("li.subject-item").unwrap();
    let s_h2a = Selector::parse("h2 a[title]").unwrap();
    let s_pub = Selector::parse("div.pub").unwrap();
    let s_pcomment = Selector::parse("p.comment").unwrap();

    for item in doc.select(&s_item_new) {
        let h2a = item.select(&s_h2a).next();
        let pic = item.select(&s_pic_a).next();
        let Some(a_el) = h2a.or(pic) else { continue };
        let subject_url = attr_or(a_el, "href");
        let subject_id = subject_id_from(&subject_url);
        let cover = pic
            .and_then(|p| p.select(&s_img).next())
            .map(|im| attr_or(im, "src"))
            .unwrap_or_default();

        // 标题：title 属性优先（不含副书名），否则取链接文本
        let title = {
            let t = norm_space(&attr_or(a_el, "title"));
            if t.is_empty() { text_of(a_el) } else { t }
        };
        // 副书名在 a 内的 <span> 里，形如 " : 人间三梦：古典文库系列3"
        let alt_title = a_el
            .select(&s_span)
            .next()
            .map(|sp| {
                text_of(sp)
                    .trim_start_matches([':', '：', ' '])
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();

        // 出版信息：作者 / 出版社 / 出版年 / 价格
        let intro = item.select(&s_pub).next().map(text_of).unwrap_or_default();
        let my_rating = item.select(&s_rating).next().and_then(rating_from_class);
        let date = item
            .select(&s_date)
            .next()
            .map(|el| first_token(&text_of(el)))
            .unwrap_or_default();

        let p = item.select(&s_pcomment).next();
        let comment = p.map(text_of).unwrap_or_default();
        let comment_id = p
            .and_then(|p| p.value().attr("data-cid"))
            .unwrap_or_else(|| item.value().attr("data-cid").unwrap_or(""))
            .to_string();

        records.push(Record {
            category: category.key().to_string(),
            status: category.status_label(status).to_string(),
            title,
            alt_title,
            my_rating,
            date,
            comment,
            subject_url,
            subject_id,
            cover,
            comment_id,
            intro,
        });
    }

    records
}

// --------------------------------------------------------------------------- //
// 网络（带退避重试）
// --------------------------------------------------------------------------- //

fn make_client() -> Result<reqwest::blocking::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "Accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8"
            .parse()
            .expect("static header"),
    );
    headers.insert(
        "Accept-Language",
        "zh-CN,zh;q=0.9,en;q=0.8".parse().expect("static header"),
    );
    Ok(reqwest::blocking::Client::builder()
        .user_agent(UA)
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
// 抓取主流程（带断点续爬）
// --------------------------------------------------------------------------- //

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

    let referer = format!("https://www.douban.com/people/{uid}/"); // book 域名缺它会 403
    let mut start = ckpt.next_start;
    loop {
        if let Some(mp) = max_pages {
            if start >= mp * PAGE_SIZE {
                println!("  ⏹ 已达 --max-pages={mp} 上限，提前结束（进度已保存）");
                break;
            }
        }
        if let Some(t) = ckpt.total {
            if start >= t {
                break;
            }
        }

        let status_key = status.key();
        let url = format!(
            "https://{}/people/{uid}/{status_key}?start={start}&sort=time",
            category.domain()
        );
        let html = fetch_html(client, &url, &referer)
            .with_context(|| format!("{}·{}", category.key(), status.key()))?;

        if let Some(t) = parse_total(&html) {
            ckpt.total = Some(t);
        }
        let items = parse_items(&html, category, status);
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

        ckpt.next_start = start + PAGE_SIZE;
        save_checkpoint(&ckpt_file, &ckpt)?;

        // 不以"本页不满 15 条"作为结束条件：页面可能因条目被删而少渲染，
        // 只有"空页"或"已达总数"才是可靠的终点。
        start += PAGE_SIZE;
        sleep(Duration::from_secs_f64(delay + rand::thread_rng().gen_range(0.0..1.0)));
    }
    Ok(ckpt.items)
}

// --------------------------------------------------------------------------- //
// 导出
// --------------------------------------------------------------------------- //

fn export_csv(records: &[Record], path: &Path) -> Result<()> {
    use std::io::Write;
    let mut f = fs::File::create(path)?;
    f.write_all("\u{FEFF}".as_bytes())?; // BOM，便于 Excel 直接打开
    // CRLF 行尾（csv 模块的通用约定，Excel 友好）；
    // serialize 结构体时 csv 库会自动以字段名写表头，无需手动 write_record
    let mut w = csv::WriterBuilder::new()
        .terminator(csv::Terminator::CRLF)
        .from_writer(f);
    for r in records {
        w.serialize(r)?;
    }
    w.flush()?;
    Ok(())
}

fn export_json(records: &[Record], path: &Path) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(records)?)?;
    Ok(())
}

fn md_escape(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn export_md(records: &[Record], path: &Path, title: &str) -> Result<()> {
    let mut lines = vec![
        format!("# {title}（{} 条）", records.len()),
        String::new(),
        "| 标题 | 我的评分 | 日期 | 短评 | 链接 |".to_string(),
        "|---|---|---|---|---|".to_string(),
    ];
    for r in records {
        let rating = match r.my_rating {
            Some(n) => "★".repeat(n as usize),
            None => "—".to_string(),
        };
        lines.push(format!(
            "| {} | {rating} | {} | {} | [条目]({}) |",
            md_escape(&r.title),
            r.date,
            md_escape(&r.comment),
            r.subject_url
        ));
    }
    fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

fn export_xlsx(records: &[Record], path: &Path) -> Result<()> {
    use rust_xlsxwriter as xw;
    let mut wb = xw::Workbook::new();
    let ws = wb.add_worksheet();
    ws.write_row(0, 0, FIELDS)?;
    for (i, r) in records.iter().enumerate() {
        let rating = r.my_rating.map(|n| n.to_string()).unwrap_or_default();
        let row: [&str; 12] = [
            &r.category, &r.status, &r.title, &r.alt_title, &rating, &r.date,
            &r.comment, &r.subject_url, &r.subject_id, &r.cover, &r.comment_id, &r.intro,
        ];
        ws.write_row((i + 1) as u32, 0, row)?;
    }
    wb.save(path)?;
    Ok(())
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
    #[arg(long, value_enum, num_args = 1.., default_values_t = vec![Category::Movie, Category::Book, Category::Music])]
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

fn parse_uid(raw: &str) -> Result<String> {
    if let Some(i) = raw.find("/people/") {
        let id = raw[i + 8..]
            .split(['/', '?', '#'])
            .next()
            .unwrap_or("");
        if is_valid_id(id) {
            return Ok(id.to_string());
        }
    }
    if is_valid_id(raw) {
        return Ok(raw.to_string());
    }
    bail!("无法从 {raw:?} 解析豆瓣用户 ID（应为 /people/<id>/ 或直接给 id）");
}

fn is_valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

fn main() -> Result<()> {
    let args = Args::parse();
    let uid = parse_uid(&args.user)?;
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

            let base = args.out.join(format!(
                "{uid}_{}_{}",
                category.key(),
                status.key()
            ));
            for &fmt in &args.format {
                let p = base.with_extension(fmt_key(fmt));
                let res = match fmt {
                    Format::Csv => export_csv(&records, &p),
                    Format::Json => export_json(&records, &p),
                    Format::Md => export_md(
                        &records,
                        &p,
                        &format!("{} 的{}记录·{}", uid, category.status_label(status), category.kind_label()),
                    ),
                    Format::Xlsx => export_xlsx(&records, &p),
                };
                res.with_context(|| format!("导出 {}", p.display()))?;
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
        let mut counter: BTreeMap<String, usize> = BTreeMap::new();
        for r in &all_results {
            *counter.entry(format!("{}·{}", r.category, r.status)).or_insert(0) += 1;
        }
        let summary = serde_json::json!({
            "user": uid,
            "exported_at": Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
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

fn fmt_key(fmt: Format) -> &'static str {
    match fmt {
        Format::Csv => "csv",
        Format::Json => "json",
        Format::Md => "md",
        Format::Xlsx => "xlsx",
    }
}

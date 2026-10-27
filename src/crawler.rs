//! 抓取核心库——CLI 与 server 共用的解析、导出与模型逻辑。
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

use clap::ValueEnum;
use scraper::{ElementRef, Html, Selector, node::Node};
use serde::{Deserialize, Serialize};

pub const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                      (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
pub const PAGE_SIZE: usize = 15; // 豆瓣收藏页固定每页 15 条

/// 常用浏览器级请求头（除 Referer 外的部分）
pub const BASE_HEADERS: [(&str, &str); 2] = [
    (
        "Accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
    ),
    ("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8"),
];

// --------------------------------------------------------------------------- //
// 类型定义
// --------------------------------------------------------------------------- //

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Movie,
    Book,
    Music,
}

impl Category {
    pub const ALL: [Category; 3] = [Category::Movie, Category::Book, Category::Music];

    pub fn key(self) -> &'static str {
        match self {
            Category::Movie => "movie",
            Category::Book => "book",
            Category::Music => "music",
        }
    }

    pub fn domain(self) -> &'static str {
        match self {
            Category::Movie => "movie.douban.com",
            Category::Book => "book.douban.com",
            Category::Music => "music.douban.com",
        }
    }

    pub fn kind_label(self) -> &'static str {
        match self {
            Category::Movie => "电影·剧集",
            Category::Book => "图书",
            Category::Music => "音乐",
        }
    }

    pub fn status_label(self, status: Status) -> &'static str {
        match (self, status) {
            (Category::Movie, Status::Collect) => "看过",
            (Category::Movie, Status::Do) => "在看",
            (Category::Movie, Status::Wish) => "想看",
            (Category::Book, Status::Collect) => "读过",
            (Category::Book, Status::Do) => "在读",
            (Category::Book, Status::Wish) => "想读",
            (Category::Music, Status::Collect) => "听过",
            (Category::Music, Status::Do) => "在听",
            (Category::Music, Status::Wish) => "想听",
        }
    }

    pub fn from_key(key: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|c| c.key() == key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Collect,
    Do,
    Wish,
}

impl Status {
    pub const ALL: [Status; 3] = [Status::Collect, Status::Do, Status::Wish];

    pub fn key(self) -> &'static str {
        match self {
            Status::Collect => "collect",
            Status::Do => "do",
            Status::Wish => "wish",
        }
    }

    pub fn from_key(key: &str) -> Option<Status> {
        Status::ALL.into_iter().find(|s| s.key() == key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Csv,
    Json,
    Md,
    Xlsx,
}

impl Format {
    pub fn key(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Json => "json",
            Format::Md => "md",
            Format::Xlsx => "xlsx",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Format::Csv => "text/csv; charset=utf-8",
            Format::Json => "application/json; charset=utf-8",
            Format::Md => "text/markdown; charset=utf-8",
            Format::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }
    }
}

/// 一条标记记录（声明顺序即 CSV 列顺序）。
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Record {
    pub category: String,
    pub status: String,
    pub title: String,
    pub alt_title: String,
    pub my_rating: Option<u8>,
    pub date: String,
    pub comment: String,
    pub subject_url: String,
    pub subject_id: String,
    pub cover: String,
    pub comment_id: String,
    pub intro: String,
}

pub const FIELDS: [&str; 12] = [
    "category", "status", "title", "alt_title", "my_rating", "date", "comment",
    "subject_url", "subject_id", "cover", "comment_id", "intro",
];

/// 收藏页 URL（sort=time，从 start 开始）
pub fn collection_url(category: Category, uid: &str, status: Status, start: usize) -> String {
    let status_key = status.key();
    format!(
        "https://{}/people/{uid}/{status_key}?start={start}&sort=time",
        category.domain()
    )
}

pub fn referer_for(uid: &str) -> String {
    format!("https://www.douban.com/people/{uid}/") // book 域名缺它会 403
}

// --------------------------------------------------------------------------- //
// 文本/解析辅助
// --------------------------------------------------------------------------- //

/// 压缩空白为单个空格并去首尾。
pub fn norm_space(s: &str) -> String {
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
pub fn parse_total(html: &str) -> Option<usize> {
    let i = html.find(r#"class="subject-num">"#)?;
    let rest = &html[i..];
    let j = rest.find("&nbsp;/&nbsp;")?;
    let after = rest[j + "&nbsp;/&nbsp;".len()..].trim_start();
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

// --------------------------------------------------------------------------- //
// 页面条目解析（兼容两代结构）
// --------------------------------------------------------------------------- //

pub fn parse_items(html: &str, category: Category, status: Status) -> Vec<Record> {
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

        let intro = item.select(&s_intro).next().map(text_of).unwrap_or_default();
        let my_rating = item.select(&s_rating).next().and_then(rating_from_class);
        let date = item
            .select(&s_date)
            .next()
            .map(|el| first_token(&text_of(el)))
            .unwrap_or_default();

        // 短评：电影在 span.comment；音乐在无 class 的 <li> 里。
        // 被点过「有用」的短评 li 内会多出 <span class="color_gray">N 有用</span>，
        // 因此只取 li 的直接文本子节点：星级/日期与「N 有用」都在 span 里，
        // 取出为空即自然跳过
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
                if has_class {
                    continue; // 跳过 title/intro 行
                }
                let t = norm_space(
                    &li.children()
                        .filter_map(|n| match n.value() {
                            Node::Text(tx) => Some(tx.to_string()),
                            _ => None,
                        })
                        .collect::<String>(),
                );
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
// RSS 动态解析（订阅增量同步的数据源）
// --------------------------------------------------------------------------- //

/// 个人书影音动态 RSS（2026-10 实测：免登录、无反爬、仅最新约 10 条）。
/// 条目 guid 尾段数字与收藏页 data-cid 是同一 ID 空间，可作去重键。
pub fn feed_url(uid: &str) -> String {
    format!("https://www.douban.com/feed/people/{uid}/interests")
}

/// feed 条目：解析后的记录 + 去重 guid
pub struct FeedItem {
    pub record: Record,
    pub guid: String,
    /// 维度状态键（collect/do/wish）；record.status 存中文标签与页面解析一致
    pub status_key: String,
}

/// title 前缀（"看过生日快乐"）给出状态，剩余部分即标题
const FEED_PREFIXES: [(&str, Status); 9] = [
    ("想看", Status::Wish),
    ("在看", Status::Do),
    ("看过", Status::Collect),
    ("想读", Status::Wish),
    ("在读", Status::Do),
    ("读过", Status::Collect),
    ("想听", Status::Wish),
    ("在听", Status::Do),
    ("听过", Status::Collect),
];

fn category_from_link(url: &str) -> Option<Category> {
    Category::ALL
        .into_iter()
        .find(|c| url.starts_with(&format!("https://{}/", c.domain())))
}

/// description 里 "推荐: 力荐" 的文字档 → 星级
fn rating_from_word(word: &str) -> Option<u8> {
    match word.trim() {
        "力荐" => Some(5),
        "推荐" => Some(4),
        "还行" => Some(3),
        "较差" => Some(2),
        "很差" => Some(1),
        _ => None,
    }
}

/// 解析 description（CDATA 内的 HTML 片段）：封面 / 副标题 / 评分 / 短评
fn parse_feed_description(html: &str) -> (String, String, Option<u8>, String) {
    let doc = Html::parse_fragment(html);
    let s_img = Selector::parse("img").unwrap();
    let s_a = Selector::parse("a[title]").unwrap();
    let s_p = Selector::parse("p").unwrap();

    let cover = doc
        .select(&s_img)
        .next()
        .map(|im| attr_or(im, "src"))
        .unwrap_or_default();
    let alt_title = doc
        .select(&s_a)
        .next()
        .map(|a| norm_space(&attr_or(a, "title")))
        .unwrap_or_default();

    let mut rating = None;
    let mut comment = String::new();
    for p in doc.select(&s_p) {
        let t = text_of(p);
        // 分隔符半角/全角冒号都容忍（实测为 "推荐: 力荐"）
        if let Some(rest) = t.strip_prefix("推荐") {
            rating = rating_from_word(rest.trim_start_matches([':', '：', ' ']));
        } else if let Some(rest) = t.strip_prefix("备注") {
            comment = rest.trim_start_matches([':', '：', ' ']).trim().to_string();
        }
    }
    (cover, alt_title, rating, comment)
}

fn feed_field<'a>(fields: &'a [(Vec<u8>, String)], name: &[u8]) -> &'a str {
    fields
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
        .unwrap_or("")
}

fn build_feed_item(fields: &[(Vec<u8>, String)]) -> Option<FeedItem> {
    let title_raw = feed_field(fields, b"title");
    let link = feed_field(fields, b"link");
    let guid_raw = feed_field(fields, b"guid");

    let category = category_from_link(link)?;
    let (status, title) = FEED_PREFIXES.iter().find_map(|(p, st)| {
        title_raw
            .strip_prefix(p)
            .map(|rest| (*st, rest.to_string()))
    })?;
    let (cover, alt_title, my_rating, comment) =
        parse_feed_description(feed_field(fields, b"description"));
    // 豆瓣页面日期按北京时间显示，pubDate 是 GMT，转 +08:00 后取日期保持一致
    let date = chrono::DateTime::parse_from_rfc2822(feed_field(fields, b"pubDate"))
        .map(|d| {
            d.with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_default();
    let comment_id: String = guid_raw
        .rsplit('/')
        .next()
        .unwrap_or("")
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();

    Some(FeedItem {
        record: Record {
            category: category.key().to_string(),
            status: category.status_label(status).to_string(),
            title,
            alt_title,
            my_rating,
            date,
            comment,
            subject_url: link.to_string(),
            subject_id: subject_id_from(link),
            cover,
            comment_id: comment_id.clone(),
            intro: String::new(), // feed 里没有出版/导演信息
        },
        guid: comment_id,
        status_key: status.key().to_string(),
    })
}

/// 解析动态 RSS。畸形/截断的 XML 返回已成功解析的前缀部分，不报错——
/// RSS 是可失败的增强层，最坏退化为纯全量同步。
pub fn parse_feed(xml: &str) -> Vec<FeedItem> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut items = Vec::new();
    let mut in_item = false;
    let mut cur_tag: Vec<u8> = Vec::new();
    let mut buf = String::new();
    let mut fields: Vec<(Vec<u8>, String)> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_vec();
                if !in_item {
                    if name == b"item" {
                        in_item = true;
                        fields.clear();
                    }
                } else if matches!(
                    name.as_slice(),
                    b"title" | b"link" | b"description" | b"pubDate" | b"guid"
                ) {
                    cur_tag = name;
                    buf.clear();
                }
            }
            Ok(Event::Text(t)) if in_item && !cur_tag.is_empty() => {
                buf.push_str(&t.unescape().unwrap_or_default());
            }
            Ok(Event::CData(t)) if in_item && !cur_tag.is_empty() => {
                buf.push_str(&String::from_utf8_lossy(t.as_ref()));
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_vec();
                if in_item && name == cur_tag.as_slice() {
                    fields.push((std::mem::take(&mut cur_tag), std::mem::take(&mut buf)));
                } else if name == b"item" {
                    in_item = false;
                    if let Some(item) = build_feed_item(&fields) {
                        items.push(item);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    items
}

// --------------------------------------------------------------------------- //
// 导出（多格式）
// --------------------------------------------------------------------------- //

pub fn export_csv(records: &[Record], path: &std::path::Path) -> anyhow::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
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

pub fn export_json(records: &[Record], path: &std::path::Path) -> anyhow::Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(records)?)?;
    Ok(())
}

fn md_escape(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

pub fn export_md(
    records: &[Record],
    path: &std::path::Path,
    title: &str,
) -> anyhow::Result<()> {
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
    std::fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

pub fn export_xlsx(records: &[Record], path: &std::path::Path) -> anyhow::Result<()> {
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

pub fn export_records(
    fmt: Format,
    records: &[Record],
    path: &std::path::Path,
    md_title: &str,
) -> anyhow::Result<()> {
    match fmt {
        Format::Csv => export_csv(records, path),
        Format::Json => export_json(records, path),
        Format::Md => export_md(records, path, md_title),
        Format::Xlsx => export_xlsx(records, path),
    }
}

// --------------------------------------------------------------------------- //
// 用户 ID 解析
// --------------------------------------------------------------------------- //

pub fn parse_uid(raw: &str) -> Option<String> {
    if let Some(i) = raw.find("/people/") {
        let id = raw[i + 8..].split(['/', '?', '#']).next().unwrap_or("");
        if is_valid_id(id) {
            return Some(id.to_string());
        }
    }
    if is_valid_id(raw) {
        return Some(raw.to_string());
    }
    None
}

pub fn is_valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

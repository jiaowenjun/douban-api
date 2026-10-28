//! 豆瓣收藏列表页 intro 元数据行解析器（三类）——intro汇总/intro_parser.py 的忠实移植。
//!
//! 从 records.intro 解析结构化字段：
//! - [`parse_movie`]：dates/years/places/countries/genres/langs/duration/director_list/people…
//! - [`parse_book`]：authors/publisher/pubdate/pubyear/price_val/currency…
//! - [`parse_music`]：artist/date/year/mtype/medium/genre…
//!
//! 字段顺序无标注，靠词表 + 位置启发式归类（导演回扫规则见 parse_movie 注释）。
//! 黄金用例：tests/intro_tests.rs 逐例比对 intro汇总/wenjun30-*intro测试用例.json（291 例，
//! 与 Python 版共享同一基准）；运行 `cargo test --test intro_tests`。
//! 后端接入：subject.rs 读侧兜底（subjects 未富集时按 intro 合成视图，source="intro"）。

use regex::Regex;
use std::sync::OnceLock;

// ---------------- 词表（与 intro_parser.py 逐一对应） ----------------

pub const GENRES: &[&str] = &[
    "剧情", "喜剧", "爱情", "动作", "科幻", "动画", "悬疑", "惊悚", "恐怖", "犯罪", "同性", "音乐",
    "歌舞", "传记", "历史", "战争", "西部", "奇幻", "冒险", "灾难", "武侠", "情色", "家庭", "古装",
    "运动", "短片", "黑色电影", "音乐剧", "舞台艺术", "纪录片", "脱口秀", "真人秀", "青春", "儿童",
    "谍战", "枪战", "荒诞", "情节", "戏曲",
];

pub const COUNTRIES: &[&str] = &[
    "中国大陆", "中国香港", "中国台湾", "中国澳门", "中国", "美国", "日本", "韩国", "英国", "法国",
    "德国", "意大利", "西班牙", "葡萄牙", "荷兰", "比利时", "瑞典", "丹麦", "挪威", "芬兰", "冰岛",
    "波兰", "俄罗斯", "乌克兰", "土耳其", "印度", "泰国", "越南", "新加坡", "马来西亚", "印度尼西亚",
    "菲律宾", "埃及", "南非", "澳大利亚", "新西兰", "加拿大", "墨西哥", "巴西", "阿根廷", "智利",
    "哥伦比亚", "秘鲁", "古巴", "伊朗", "以色列", "沙特阿拉伯", "卡塔尔", "奥地利", "瑞士", "希腊",
    "匈牙利", "捷克", "罗马尼亚", "爱尔兰", "塞尔维亚", "克罗地亚", "保加利亚", "摩洛哥", "突尼斯",
    "阿尔及利亚", "尼日利亚", "肯尼亚", "蒙古", "柬埔寨", "缅甸", "尼泊尔", "巴基斯坦", "孟加拉国",
    "斯里兰卡", "卢森堡", "立陶宛", "拉脱维亚", "爱沙尼亚", "塞浦路斯", "苏联", "西德", "东德",
    "捷克斯洛伐克", "南斯拉夫", "波黑", "斯洛文尼亚", "北马其顿", "阿联酋", "巴林", "黎巴嫩",
    "叙利亚", "约旦", "伊拉克", "阿富汗", "乌兹别克斯坦", "哈萨克斯坦",
];

pub const LANGS: &[&str] = &[
    "汉语普通话", "粤语", "闽南语", "上海话", "四川话", "四川方言", "汉语", "手语", "无对白", "默片",
    "英语", "日语", "韩语", "法语", "德语", "西班牙语", "意大利语", "葡萄牙语", "俄语", "阿拉伯语",
    "希伯来语", "波斯语", "印地语", "泰语", "越南语", "蒙古语", "土耳其语", "荷兰语", "瑞典语",
    "挪威语", "丹麦语", "芬兰语", "波兰语", "捷克语", "希腊语", "匈牙利语", "冰岛语", "拉丁语",
    "印尼语", "菲律宾语", "他加禄语", "旁遮普语", "泰米尔语", "乌尔都语", "孟加拉语", "客家话",
    "温州话", "南京话", "武汉话", "陕西话", "山西话", "唐山话", "东北话", "粤语字", "广东话",
    "塞尔维亚语", "克罗地亚语", "乌克兰语", "罗马尼亚语", "保加利亚语",
];

const MEDIA_MUSIC: &[&str] = &[
    "CD", "Audio CD", "黑胶", "磁带", "卡带", "DVD", "Blu-ray", "数字", "数字专辑", "数字(Digital)",
    "Digital", "Import", "Vinyl", "LP", "胶片", "盒式磁带", "DSD", "SACD", "DVD-Audio",
];

const TYPE_MUSIC: &[&str] = &[
    "专辑", "单曲", "EP", "迷你专辑", "合辑", "精选集", "选集", "现场专辑", "原声带", "合集",
    "录音室专辑", "Live专辑", "Live", "现场",
];

fn in_list(list: &[&str], f: &str) -> bool {
    list.contains(&f)
}

fn is_cjk(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c)
}

fn has_cjk(s: &str) -> bool {
    s.chars().any(is_cjk)
}

// ---------------- 共享正则（OnceLock 惰性编译） ----------------

struct IntroRegex {
    /// 尾部 (地点/影展) 注记
    paren_tail: Regex,
    /// 尾部时间（2025-07-16 20:50）
    time_tail: Regex,
    /// 日期主体 YYYY[-/]M[-/]D
    date_body: Regex,
    /// 片长 N分钟[ (版本注记)]
    dur: Regex,
    /// 定价（币种前缀 + 数值 + 单位）
    price: Regex,
    /// 人名归一：行尾拉丁段剥离
    latin_tail: Regex,
    /// 语言自动识别：X语/X话（1–6 个 CJK 前缀）、XX方言
    lang_yu: Regex,
    lang_dialect: Regex,
    /// 日期/字段内的首个括号注记（取首映地点）
    first_paren: Regex,
    /// 官网等杂质字段
    junk: Regex,
    /// 艺人括号注记（科尔托 (Alfred Cortot)）
    artist_paren: Regex,
    /// 艺人行尾拉丁/数字段（王菲 Faye Wong → 王菲）
    artist_latin: Regex,
}

fn rx() -> &'static IntroRegex {
    static RX: OnceLock<IntroRegex> = OnceLock::new();
    RX.get_or_init(|| IntroRegex {
        paren_tail: Regex::new(r"[（(][^()（）]*[)）]$").unwrap(),
        time_tail: Regex::new(r"\s+\d{1,2}:\d{2}(:\d{2})?$").unwrap(),
        date_body: Regex::new(r"^(\d{4})(?:[-/](\d{1,2}))?(?:[-/](\d{1,2}))?$").unwrap(),
        dur: Regex::new(r"^(\d+)\s*分钟(?:[（(][^)）]*[)）])?$").unwrap(),
        price: Regex::new(
            r"^(?:CNY|RMB|NT\$|USD|US\$|HK\$|€|£|￥|¥)?\s*([\d,.]+)\s*(元|块|美元|港币|台币|日元|英镑|欧元|卢布|NT\$|USD|US\$|HK\$|￥|¥|€|£)?$",
        )
        .unwrap(),
        latin_tail: Regex::new(r"^(.*?)[\s]+([A-Za-z][A-Za-zÀ-ž'.\- ]*)$").unwrap(),
        lang_yu: Regex::new(r"^[\u{4e00}-\u{9fff}]{1,6}(语|话)$").unwrap(),
        lang_dialect: Regex::new(r"^[\u{4e00}-\u{9fff}]{2,5}方言$").unwrap(),
        first_paren: Regex::new(r"[（(](.+?)[)）]").unwrap(),
        junk: Regex::new(r"www\.|http|\.(com|net|org|cn|jp|info)(/|$)").unwrap(),
        artist_paren: Regex::new(r"\s*[（(][^()（）]*[)）]\s*$").unwrap(),
        artist_latin: Regex::new(r"[A-Za-zÀ-ž0-9.'\- ]+$").unwrap(),
    })
}

/// 全角数字归一（１９８０ → 1980）
fn normalize_fw(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '０'..='９' => char::from(b'0' + (c as u32 - '０' as u32) as u8),
            other => other,
        })
        .collect()
}

/// 识别 YYYY / YYYY-M / YYYY-M-D / YYYY年M月[日]（含尾部 (注记) 与时间），返回年份。
pub fn date_year(f: &str) -> Option<i32> {
    let r = rx();
    let s = normalize_fw(f.trim());
    let s = r.paren_tail.replace(&s, "");
    let s = r.time_tail.replace(&s, "");
    let s = s.replace('年', "-").replace('月', "-").replace('日', "");
    let s = s.trim_end_matches('-');
    let m = r.date_body.captures(s)?;
    let month = m.get(2).and_then(|x| x.as_str().parse::<u32>().ok());
    let day = m.get(3).and_then(|x| x.as_str().parse::<u32>().ok());
    if month.is_some_and(|v| v > 12) || day.is_some_and(|v| v > 31) {
        return None;
    }
    m.get(1)?.as_str().parse::<i32>().ok()
}

/// 人名字段归一键：取 CJK 部分（含 ·）；无 CJK 则原样。
pub fn cjk_key(field: &str) -> String {
    let m = rx().latin_tail.captures(field);
    if let Some(m) = &m {
        let head = m.get(1).map(|x| x.as_str()).unwrap_or_default();
        if head.chars().any(|c| is_cjk(c) || c == '·') {
            return head.trim().to_string();
        }
    }
    field.trim().to_string()
}

fn split_fields(intro: &str) -> Vec<&str> {
    intro.split(" / ").map(str::trim).filter(|x| !x.is_empty()).collect()
}

// ---------------- 电影 ----------------

#[derive(Debug, Default, PartialEq)]
pub struct MovieIntro {
    pub dates: Vec<String>,
    pub years: Vec<i32>,
    pub places: Vec<String>,
    pub people: Vec<String>,
    pub countries: Vec<String>,
    pub langs: Vec<String>,
    pub genres: Vec<String>,
    /// 首个片长的分钟数（无片长为 None）
    pub duration: Option<i32>,
    /// 启发式识别的导演（联合执导全收）
    pub director_list: Vec<String>,
    pub people_keys: Vec<String>,
}

pub fn parse_movie(intro: &str, title: &str, alt: &str) -> MovieIntro {
    let r = rx();
    let fields = split_fields(intro);
    let own_names: Vec<&str> = [title.trim(), alt.trim()]
        .into_iter()
        .filter(|t| !t.is_empty())
        .collect();
    let is_own_name = |x: &str| {
        own_names.contains(&x)
            || (x.chars().count() >= 4 && own_names.iter().any(|t| t.starts_with(x)))
    };

    let mut out = MovieIntro::default();
    let mut genre_idx: Option<usize> = None;
    let mut dur_idx: Option<usize> = None;
    for (i, f) in fields.iter().enumerate() {
        if date_year(f).is_some() {
            out.dates.push(f.to_string());
            out.years.push(date_year(f).unwrap());
            if let Some(m) = r.first_paren.captures(f) {
                out.places.push(m.get(1).map(|x| x.as_str()).unwrap_or_default().to_string());
            }
        } else if r.dur.is_match(f) {
            if dur_idx.is_none() {
                out.duration = r
                    .dur
                    .captures(f)
                    .and_then(|m| m.get(1))
                    .and_then(|x| x.as_str().parse().ok());
                dur_idx = Some(i);
            }
        } else if in_list(COUNTRIES, f) {
            out.countries.push(f.to_string());
        } else if in_list(LANGS, f) || r.lang_yu.is_match(f) || r.lang_dialect.is_match(f) {
            out.langs.push(f.to_string());
        } else if in_list(GENRES, f) {
            out.genres.push(f.to_string());
            if genre_idx.is_none() {
                genre_idx = Some(i);
            }
        } else if is_own_name(f) || r.junk.is_match(f) {
            // 原名/又名（含前缀变体）或官网等杂质字段，不计入人名
        } else if f.chars().any(|c| c.is_ascii_alphabetic() || is_cjk(c)) {
            out.people.push(f.to_string());
        }
    }

    // 导演启发式：从首个片长（无片长则首个类型）向前取连续人名段，越过其余片长字段；
    // 段内多人视为联合执导；遇到国家/类型/语言即停；日期/原名变体/官网——
    // 已取到人名则停（视为到达头部区域），否则跳过（导演段内偶夹裸日期，如“2026-07-15”）。
    let anchor = dur_idx.or(genre_idx);
    if let Some(anchor) = anchor {
        for f in fields[..anchor].iter().rev() {
            if in_list(COUNTRIES, f) || in_list(LANGS, f) || in_list(GENRES, f) {
                break;
            }
            if r.dur.is_match(f) {
                continue;
            }
            if date_year(f).is_some() || is_own_name(f) || r.junk.is_match(f) {
                if !out.director_list.is_empty() {
                    break;
                }
                continue;
            }
            out.director_list.push(cjk_key(f));
        }
        out.director_list.reverse();
    }
    out.people_keys = out.people.iter().map(|p| cjk_key(p)).collect();
    out
}

// ---------------- 图书 ----------------

#[derive(Debug, Default, PartialEq)]
pub struct BookIntro {
    pub authors: Vec<String>,
    pub publisher: String,
    pub pubdate: String,
    pub pubyear: Option<i32>,
    pub price_raw: String,
    pub price_val: Option<f64>,
    /// 空 = 人民币（无单位价格）；NT$/USD/HK$ 等为外币
    pub currency: String,
}

pub fn parse_book(intro: &str) -> BookIntro {
    let r = rx();
    let fields = split_fields(intro);
    let mut out = BookIntro::default();
    let date_i = fields.iter().position(|f| date_year(f).is_some());
    let Some(date_i) = date_i else {
        return out; // 无日期字段则整体不可解析（黄金用例中不存在此情况）
    };
    out.pubdate = fields[date_i].to_string();
    out.pubyear = date_year(fields[date_i]);
    if date_i >= 1 {
        out.publisher = fields[date_i - 1].to_string();
        out.authors = fields[..date_i - 1].iter().map(|s| s.to_string()).collect();
    }
    if fields.len() - 1 > date_i {
        let raw = fields[fields.len() - 1];
        out.price_raw = raw.to_string();
        if let Some(m) = r.price.captures(raw) {
            let digits = m.get(1).map(|x| x.as_str()).unwrap_or_default();
            if !digits.is_empty() {
                let val = digits.replace(',', "");
                let val = val.strip_suffix('.').unwrap_or(&val);
                if let Ok(v) = val.parse::<f64>() {
                    out.price_val = Some(v);
                    let unit = m.get(2).map(|x| x.as_str()).unwrap_or("");
                    let cur = if !unit.is_empty() {
                        unit.to_string()
                    } else {
                        let t = raw.trim_start();
                        ["NT$", "US", "HK$", "€", "£"]
                            .iter()
                            .find(|p| t.starts_with(*p))
                            .map(|_| t.chars().take(3).collect())
                            .unwrap_or_default()
                    };
                    out.currency = if cur.is_empty() && raw.contains("NT$") {
                        "NT$".to_string()
                    } else {
                        cur
                    };
                }
            }
        }
    }
    out
}

// ---------------- 音乐 ----------------

#[derive(Debug, Default, PartialEq)]
pub struct MusicIntro {
    pub artist: String,
    pub artist_raw: String,
    pub date: String,
    pub year: Option<i32>,
    pub mtype: String,
    /// 多介质以 + 相连（Audio CD→CD、数字(Digital)→数字 归一）
    pub medium: String,
    /// 多流派以 + 相连
    pub genre: String,
}

pub fn parse_music(intro: &str) -> MusicIntro {
    let r = rx();
    let fields = split_fields(intro);
    let mut out = MusicIntro::default();
    let date_i = fields.iter().position(|f| date_year(f).is_some());
    let Some(date_i) = date_i else {
        return out;
    };
    out.date = fields[date_i].to_string();
    out.year = date_year(fields[date_i]);
    out.artist_raw = fields[..date_i].join(" / ");
    let a = fields[0].split(',').next().unwrap_or_default().trim();
    let a = r.artist_paren.replace(a, "");
    let cjk = r.artist_latin.replace(&a, "").trim().to_string();
    out.artist = if has_cjk(&cjk) { cjk } else { a.trim().to_string() };
    fn media_norm(f: &str) -> &str {
        match f {
            "Audio CD" => "CD",
            "数字(Digital)" => "数字",
            other => other,
        }
    }
    for f in &fields[date_i + 1..] {
        if in_list(MEDIA_MUSIC, f) {
            out.medium = if out.medium.is_empty() {
                media_norm(f).to_string()
            } else {
                format!("{}+{}", out.medium, media_norm(f))
            };
        } else if in_list(TYPE_MUSIC, f) {
            out.mtype = f.to_string();
        } else {
            out.genre = if out.genre.is_empty() {
                f.to_string()
            } else {
                format!("{}+{}", out.genre, f)
            };
        }
    }
    out
}

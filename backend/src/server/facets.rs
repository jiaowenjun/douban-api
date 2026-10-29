//! 分面导航（分面树数据源）——条目元信息视图的维度值聚合与过滤。
//! 维度与 MarkCard 元信息同源：subjects 已富集用 rexxar meta，未富集回退
//! intro 启发式合成，树计数与 marks 过滤共用同一条视图管线（load_subject_views），
//! 保证两侧口径一致。演员维度不同值数千级，不入分面。

use anyhow::Result;
use serde_json::Value;
use sqlx::Row;

use crate::crawler::{Category, Record, Status};

use super::covers::cover_proxy_url;
use super::error::{AppError, AppResult};
use super::state::AppState;
use super::subject;

/// 单个分面过滤条件（marks 端点 `facet=dim=value`）
pub struct FacetFilter {
    pub dim: String,
    pub value: String,
}

/// 每类别的分面维度表：(维度键, 对应 meta 数组字段)。
/// decade 不在表内：全类别通用，由视图 year 派生（"1990s"）。
fn dimension_fields(category: Category) -> &'static [(&'static str, &'static str)] {
    match category {
        Category::Movie => &[
            ("genre", "genres"),
            ("country", "countries"),
            ("director", "directors"),
        ],
        Category::Book => &[
            ("press", "press"),
            ("author", "authors"),
            ("translator", "translators"),
        ],
        Category::Music => &[("genre", "genres"), ("singer", "singers")],
    }
}

/// 该类别全部合法维度键（含 decade，顺序即聚合输出顺序）
pub fn dimension_keys(category: Category) -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = dimension_fields(category).iter().map(|(d, _)| *d).collect();
    keys.push("decade");
    keys
}

/// 视图 → 某维度的值集合：meta 数组去重保序取值，decade 由 year 派生。
/// 无值/年份非法返回空集合（该条目不计入此维度）。
fn values_for(category: Category, dim: &str, view: &Value) -> Vec<String> {
    if dim == "decade" {
        return match view.get("year").and_then(Value::as_i64) {
            Some(y) if (1900..2100).contains(&y) => vec![format!("{}s", y / 10 * 10)],
            _ => vec![],
        };
    }
    let Some((_, field)) = dimension_fields(category).iter().find(|(d, _)| *d == dim) else {
        return vec![];
    };
    let values = view
        .pointer(&format!("/{}/{field}", category.key()))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // 单条目内同值去重（intro 合成的 '+' 拆分可能重复），保序保留首见
    let mut out: Vec<String> = Vec::with_capacity(values.len());
    for v in values {
        if !out.contains(&v) {
            out.push(v);
        }
    }
    out
}

/// 长尾合并的保留字面量：聚合输出中计数为 1 的值合并进它（前端显示
/// 「其他X」），marks 过滤对它做「值是 singleton」的特判匹配——真实
/// 豆瓣元数据值不会与之撞名。与前端 facet-model 的 OTHER_FACET_VALUE 对齐
pub const OTHER_VALUE: &str = "__other__";

/// 长尾合并按维度开关：仅树上展示且长尾明显的维度开启——电影类型、
/// 图书出版社；音乐流派不合并（值数少，singleton 保留原样更直观）；
/// 非展示维度不合并，聚合输出与过滤保持朴素口径
fn merge_enabled(category: Category, dim: &str) -> bool {
    matches!(
        (category, dim),
        (Category::Movie, "genre") | (Category::Book, "press")
    )
}

/// 过滤谓词：同维度多次出现 = 任一命中（OR），跨维度 = 全部命中（AND）。
/// 值为 OTHER_VALUE 时匹配「该维度存在 singleton 值」——singleton 依据
/// 全量视图聚合（singletons 由调用方预计算）
pub fn view_matches(
    category: Category,
    view: &Value,
    filters: &[FacetFilter],
    singletons: &std::collections::HashMap<String, std::collections::HashSet<String>>,
) -> bool {
    let mut dims: Vec<&str> = Vec::new();
    for f in filters {
        if !dims.contains(&f.dim.as_str()) {
            dims.push(&f.dim);
        }
    }
    dims.into_iter().all(|dim| {
        let have = values_for(category, dim, view);
        filters
            .iter()
            .filter(|f| f.dim == dim)
            .any(|f| {
                if f.value == OTHER_VALUE {
                    let Some(set) = singletons.get(&f.dim) else {
                        return false;
                    };
                    have.iter().any(|v| set.contains(v))
                } else {
                    have.iter().any(|v| *v == f.value)
                }
            })
    })
}

/// 全量视图集合各维度的 singleton 值集（计数恰为 1 的值）——「其他」
/// 聚合与过滤匹配的共同依据
fn singleton_sets(
    category: Category,
    views: &[Value],
) -> std::collections::HashMap<String, std::collections::HashSet<String>> {
    let mut out = std::collections::HashMap::new();
    for dim in dimension_keys(category) {
        let mut counts: std::collections::BTreeMap<String, i64> = Default::default();
        for view in views {
            for value in values_for(category, dim, view) {
                *counts.entry(value).or_insert(0) += 1;
            }
        }
        out.insert(
            dim.to_string(),
            counts
                .into_iter()
                .filter(|(_, c)| *c == 1)
                .map(|(v, _)| v)
                .collect(),
        );
    }
    out
}

/// 解析 marks 端点可重复的 facet 参数（dim=value）；维度按类别白名单校验
pub fn parse_filters(category: Category, raw: &[String]) -> AppResult<Vec<FacetFilter>> {
    let valid = dimension_keys(category);
    raw.iter()
        .map(|s| {
            let (dim, value) = s.split_once('=').ok_or_else(|| {
                AppError::InvalidArgument(format!("非法 facet 参数: {s}（格式 dim=value）"))
            })?;
            if !valid.contains(&dim) {
                return Err(AppError::InvalidArgument(format!(
                    "非法 facet 维度: {dim}（允许值: {}）",
                    valid.join("/")
                )));
            }
            if value.is_empty() {
                return Err(AppError::InvalidArgument(format!("facet 值不能为空: {s}")));
            }
            Ok(FacetFilter {
                dim: dim.to_string(),
                value: value.to_string(),
            })
        })
        .collect()
}

/// 纯聚合：视图集合 → 各维度 (值, 计数) 列表，计数为含该值的标记条数，
/// 排序计数降序、同计数按值升序（与 facets 端点输出一致）
fn aggregate_counts(
    category: Category,
    views: &[Value],
) -> Vec<(&'static str, Vec<(String, i64)>)> {
    dimension_keys(category)
        .into_iter()
        .map(|dim| {
            let mut counts: std::collections::BTreeMap<String, i64> = Default::default();
            for view in views {
                for value in values_for(category, dim, view) {
                    *counts.entry(value).or_insert(0) += 1;
                }
            }
            let mut list: Vec<(String, i64)> = counts.into_iter().collect();
            list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            // 长尾合并（按维度开关）：singleton 值（计数恰 1）≥2 个时收进
            // __other__ 钉在末尾；仅 1 个不合并（原值展示，合并反而多一个
            // 节点）。__other__ 计数口径与过滤一致：含 singleton 值的条目数
            // （一个条目可能带两个 singleton 值，只计一次）
            if merge_enabled(category, dim) {
                let singletons: std::collections::HashSet<&String> = list
                    .iter()
                    .filter(|(_, c)| *c == 1)
                    .map(|(v, _)| v)
                    .collect();
                if singletons.len() >= 2 {
                    let other_count = views
                        .iter()
                        .filter(|view| {
                            values_for(category, dim, view)
                                .iter()
                                .any(|v| singletons.contains(v))
                        })
                        .count() as i64;
                    list.retain(|(_, c)| *c > 1);
                    list.push((OTHER_VALUE.to_string(), other_count));
                }
            }
            (dim, list)
        })
        .collect()
}

/// 读该维度全部标记行，与 read_marks 同序（date DESC, rowid），
/// 保证过滤后的分页次序与未过滤路径一致
async fn load_all_records(
    state: &AppState,
    uid: &str,
    category: Category,
    status: Status,
) -> Result<Vec<Record>> {
    let rows = sqlx::query(
        "SELECT subject_id, title, alt_title, my_rating, date, comment,
                subject_url, cover, comment_id, intro
         FROM records
         WHERE uid = ? AND category = ? AND dim_status = ?
         ORDER BY date DESC, rowid",
    )
    .bind(uid)
    .bind(category.key())
    .bind(status.key())
    .fetch_all(&state.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Record {
            category: category.key().to_string(),
            status: status.key().to_string(),
            title: r.get("title"),
            alt_title: r.get("alt_title"),
            my_rating: r.get::<Option<i64>, _>("my_rating").map(|v| v as u8),
            date: r.get("date"),
            comment: r.get("comment"),
            subject_url: r.get("subject_url"),
            subject_id: r.get("subject_id"),
            cover: cover_proxy_url(&r.get::<String, _>("cover")),
            comment_id: r.get("comment_id"),
            intro: r.get("intro"),
        })
        .collect())
}

/// 全量条目视图：subjects 批查（与 marks?expand=subject 同管线，未富集回退
/// intro 合成），按 400 一段分批以避开 SQLite 绑定参数上限
async fn load_views(
    state: &AppState,
    category: Category,
    records: &[Record],
) -> Result<Vec<Value>> {
    let mut views = Vec::with_capacity(records.len());
    for chunk in records.chunks(400) {
        views.extend(subject::load_subject_views(state, category.key(), chunk).await?);
    }
    Ok(views)
}

/// GET /users/{uid}/facets：全量聚合各维度值计数（分面树数据源）。
/// 维度未同步与 marks 同语义返回 DIMENSION_NOT_CACHED。
pub async fn facet_counts(
    state: &AppState,
    uid: &str,
    category: Category,
    status: Status,
) -> AppResult<Value> {
    super::service::ensure_dimension_cached(state, uid, category, status).await?;
    let records = load_all_records(state, uid, category, status).await?;
    let views = load_views(state, category, &records).await?;
    let facets: serde_json::Map<String, Value> = aggregate_counts(category, &views)
        .into_iter()
        .map(|(dim, list)| {
            let values: Vec<Value> = list
                .into_iter()
                .map(|(value, count)| serde_json::json!({"value": value, "count": count}))
                .collect();
            (dim.to_string(), Value::Array(values))
        })
        .collect();
    Ok(serde_json::json!({
        "user": uid,
        "category": category.key(),
        "status": status.key(),
        "total": records.len(),
        "facets": Value::Object(facets),
    }))
}

/// marks 分面过滤：返回 (过滤后全量标记, 命中总数)，分页由调用方切片。
/// singleton 集按全量视图预计算（「其他」保留值的匹配依据，与聚合口径一致）
pub async fn filter_marks(
    state: &AppState,
    uid: &str,
    category: Category,
    status: Status,
    filters: &[FacetFilter],
) -> Result<(Vec<Record>, usize)> {
    let records = load_all_records(state, uid, category, status).await?;
    let views = load_views(state, category, &records).await?;
    let singletons = singleton_sets(category, &views);
    let filtered: Vec<Record> = records
        .into_iter()
        .zip(views)
        .filter(|(_, view)| view_matches(category, view, filters, &singletons))
        .map(|(record, _)| record)
        .collect();
    let total = filtered.len();
    Ok((filtered, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn movie_view(year: i64, genres: &[&str], countries: &[&str], directors: &[&str]) -> Value {
        json!({
            "year": year,
            "movie": {
                "genres": genres,
                "countries": countries,
                "directors": directors,
            }
        })
    }

    #[test]
    fn values_for_movie_dimensions() {
        let view = movie_view(1994, &["剧情", "犯罪"], &["美国"], &["弗兰克·德拉邦特"]);
        let cat = Category::Movie;
        assert_eq!(values_for(cat, "genre", &view), vec!["剧情", "犯罪"]);
        assert_eq!(values_for(cat, "country", &view), vec!["美国"]);
        assert_eq!(values_for(cat, "director", &view), vec!["弗兰克·德拉邦特"]);
        assert_eq!(values_for(cat, "decade", &view), vec!["1990s"]);
        // 电影维度表外的维度键（book 的 press）不出值
        assert!(values_for(cat, "press", &view).is_empty());
    }

    #[test]
    fn values_for_dedup_and_invalid_year() {
        // intro 合成的流派可能重复，单条目内去重
        let view = json!({
            "year": 2012,
            "music": {"genres": ["民谣", "民谣"], "singers": ["张玮玮", "郭龙"]}
        });
        let cat = Category::Music;
        assert_eq!(values_for(cat, "genre", &view), vec!["民谣"]);
        assert_eq!(values_for(cat, "singer", &view).len(), 2);
        // 年份缺失 / 越界 → 不出 decade
        assert_eq!(
            values_for(cat, "decade", &json!({"year": null})),
            Vec::<String>::new()
        );
        assert_eq!(
            values_for(cat, "decade", &json!({"year": 1899})),
            Vec::<String>::new()
        );
    }

    #[test]
    fn view_matches_or_within_dim_and_across_dims() {
        let view = movie_view(1994, &["剧情", "犯罪"], &["美国"], &["弗兰克·德拉邦特"]);
        let cat = Category::Movie;
        let f = |dim: &str, value: &str| FacetFilter {
            dim: dim.to_string(),
            value: value.to_string(),
        };
        // 空_singleton 集：普通值匹配不受影响
        let empty: std::collections::HashMap<String, std::collections::HashSet<String>> =
            Default::default();
        // 同维度两值任一命中
        let or = vec![f("genre", "喜剧"), f("genre", "犯罪")];
        assert!(view_matches(cat, &view, &or, &empty));
        // 跨维度需全部命中
        let and_ok = vec![
            f("genre", "剧情"),
            f("country", "美国"),
            f("decade", "1990s"),
        ];
        assert!(view_matches(cat, &view, &and_ok, &empty));
        let and_fail = vec![f("genre", "剧情"), f("country", "日本")];
        assert!(!view_matches(cat, &view, &and_fail, &empty));
        // 空过滤恒真
        assert!(view_matches(cat, &view, &[], &empty));
        // __other__：视图含 singleton 值（恐怖在此聚合里仅出现 1 次）才命中
        let views = vec![
            movie_view(1994, &["剧情", "犯罪"], &["美国"], &["弗兰克·德拉邦特"]),
            movie_view(2019, &["剧情", "恐怖"], &["中国大陆"], &["娄烨"]),
        ];
        let singletons = singleton_sets(cat, &views);
        // 视图一含 singleton 值「犯罪」→ 命中；不含 singleton 的过滤值不命中
        assert!(view_matches(cat, &view, &[f("genre", OTHER_VALUE)], &singletons));
        // 普通非 singleton 值过滤行为不变
        assert!(view_matches(cat, &view, &[f("genre", "剧情")], &singletons));
    }

    #[test]
    fn parse_filters_validates() {
        let cat = Category::Movie;
        let ok =
            parse_filters(cat, &["genre=剧情".to_string(), "country=美国".to_string()]).unwrap();
        assert_eq!(ok.len(), 2);
        assert_eq!(ok[0].value, "剧情");
        // __other__ 保留值是合法过滤值（空值校验天然放行）
        assert!(parse_filters(cat, &[format!("genre={OTHER_VALUE}")]).is_ok());
        // 非法维度 / 缺 = / 空值
        assert!(parse_filters(cat, &["actor=梁朝伟".to_string()]).is_err());
        assert!(parse_filters(cat, &["genre剧情".to_string()]).is_err());
        assert!(parse_filters(cat, &["genre=".to_string()]).is_err());
        // book 的 press 对 movie 非法
        assert!(parse_filters(cat, &["press=中华书局".to_string()]).is_err());
    }

    #[test]
    fn aggregate_counts_sorting_and_multi_membership() {
        let views = vec![
            movie_view(1994, &["剧情", "犯罪"], &["美国"], &["弗兰克·德拉邦特"]),
            movie_view(2019, &["剧情"], &["中国大陆", "法国"], &["娄烨"]),
            movie_view(1989, &["剧情"], &["中国台湾"], &["侯孝贤"]),
        ];
        let agg = aggregate_counts(Category::Movie, &views);
        let by_dim: std::collections::HashMap<_, _> = agg.into_iter().collect();
        // genre（合并开启）：犯罪是唯一 singleton（1 个）→ 不合并原值保留
        assert_eq!(
            by_dim["genre"],
            vec![("剧情".to_string(), 3), ("犯罪".to_string(), 1)]
        );
        // country（合并未开启）：4 个 singleton 值全部原样保留，
        // 同计数按值升序（台 U+53F0 < 大 U+5927）
        let countries: Vec<String> = by_dim["country"].iter().map(|(v, _)| v.clone()).collect();
        assert_eq!(countries.len(), 4);
        assert!(countries.contains(&"中国大陆".to_string()));
        assert!(countries.contains(&"法国".to_string()));
        assert_eq!(countries[0], "中国台湾");
        // decade（合并未开启）计数降序
        let decades: Vec<(String, i64)> = by_dim["decade"].clone();
        assert_eq!(decades.len(), 3);
        assert!(decades[0].1 >= decades[2].1);
    }

    #[test]
    fn aggregate_counts_merges_singletons_only_where_enabled() {
        // movie genre（开启）：4 个 singleton ≥2 → 合并进 __other__ 钉末尾，
        // 计数为含 singleton 值的条目数（3 部——第二部带两个 singleton 类型）
        let views = vec![
            movie_view(1994, &["剧情"], &["美国"], &["A"]),
            movie_view(2019, &["剧情", "同性", "恐怖"], &["中国大陆"], &["B"]),
            movie_view(1989, &["灾难"], &["中国台湾"], &["C"]),
        ];
        let agg = aggregate_counts(Category::Movie, &views);
        let by_dim: std::collections::HashMap<_, _> = agg.into_iter().collect();
        assert_eq!(
            by_dim["genre"],
            vec![
                ("剧情".to_string(), 2),
                (OTHER_VALUE.to_string(), 2),
            ]
        );
        // 同数据下 music genre（未开启）：singleton 全部原样保留
        let music_views: Vec<Value> = views
            .iter()
            .map(|v| {
                json!({
                    "year": v["year"],
                    "music": {"genres": v["movie"]["genres"].clone(), "singers": []},
                })
            })
            .collect();
        let music_agg = aggregate_counts(Category::Music, &music_views);
        let music_by_dim: std::collections::HashMap<_, _> = music_agg.into_iter().collect();
        let genres: Vec<String> = music_by_dim["genre"]
            .iter()
            .map(|(v, _)| v.clone())
            .collect();
        assert!(genres.contains(&"同性".to_string()));
        assert!(genres.contains(&"恐怖".to_string()));
        assert!(genres.contains(&"灾难".to_string()));
        assert!(!genres.contains(&OTHER_VALUE.to_string()));
    }
}

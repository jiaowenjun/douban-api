//! intro 解析器黄金用例——与 intro汇总/intro_parser.py 共享同一基准：
//! 逐例读 wenjun30-*intro测试用例.json（291 例，expected 为人工复查过的
//! Python 版解析结果），Rust 移植版输出必须与之深度相等。

use serde_json::{Value, json};
use weimo_douban::intro::{parse_book, parse_movie, parse_music};

fn load_cases(name: &str) -> Vec<Value> {
    let path = format!("../intro汇总/{name}");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读取 {path} 失败（需在仓库内运行）：{e}"));
    serde_json::from_str::<Value>(&text).expect("测试用例 JSON 解析失败")
        .get("cases").expect("缺少 cases 数组").as_array().unwrap().clone()
}

fn movie_value(c: &Value) -> Value {
    let e = c.get("expected").unwrap();
    let r = parse_movie(
        c.get("intro").and_then(Value::as_str).unwrap_or_default(),
        c.get("title").and_then(Value::as_str).unwrap_or_default(),
        c.get("alt_title").and_then(Value::as_str).unwrap_or_default(),
    );
    json!({
        "dates": r.dates, "years": r.years, "places": r.places,
        "countries": r.countries, "genres": r.genres, "langs": r.langs,
        "duration": r.duration, "director_list": r.director_list,
        "people": r.people, "people_keys": r.people_keys,
    })
}

fn book_value(c: &Value) -> Value {
    let r = parse_book(c.get("intro").and_then(Value::as_str).unwrap_or_default());
    json!({
        "authors": r.authors, "publisher": r.publisher, "pubdate": r.pubdate,
        "pubyear": r.pubyear, "price_raw": r.price_raw,
        "price_val": r.price_val, "currency": r.currency,
    })
}

fn music_value(c: &Value) -> Value {
    let r = parse_music(c.get("intro").and_then(Value::as_str).unwrap_or_default());
    json!({
        "artist": r.artist, "artist_raw": r.artist_raw, "date": r.date,
        "year": r.year, "mtype": r.mtype, "medium": r.medium, "genre": r.genre,
    })
}

fn diff(want: &Value, got: &Value, path: &str, out: &mut Vec<String>) {
    match (want, got) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in a {
                diff(v, b.get(k).unwrap_or(&Value::Null), &format!("{path}.{k}"), out);
            }
        }
        _ if want != got => out.push(format!("{path}: want {want} / got {got}")),
        _ => {}
    }
}

fn run(file: &str, kind: &str, build: fn(&Value) -> Value) {
    let cases = load_cases(file);
    let mut failed = 0;
    for c in &cases {
        let id = c.get("id").and_then(Value::as_str).unwrap_or("?");
        let title = c.get("title").and_then(Value::as_str).unwrap_or("");
        let want = c.get("expected").unwrap();
        let got = build(c);
        let mut diffs = Vec::new();
        diff(want, &got, "", &mut diffs);
        if !diffs.is_empty() {
            failed += 1;
            eprintln!("FAIL {id} {title}\n  {}", diffs.join("\n  "));
        }
    }
    println!("{kind}: {} cases, {} failed", cases.len(), failed);
    assert_eq!(failed, 0, "{kind} 存在与 Python 基准不一致的用例");
}

#[test]
fn golden_movie_intro() {
    run("wenjun30-电影intro测试用例.json", "movie", movie_value);
}

#[test]
fn golden_book_intro() {
    run("wenjun30-图书intro测试用例.json", "book", book_value);
}

#[test]
fn golden_music_intro() {
    run("wenjun30-音乐intro测试用例.json", "music", music_value);
}

#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
douban_export.py —— 免登录导出豆瓣用户的公开书影音标记（含短评）

原理（调研报告中的"路径②：公开收藏页 HTML 抓取"）：
  电影/图书/音乐的收藏页是公开网页，无需登录 Cookie，直接抓取 HTML 解析：
    https://movie.douban.com/people/{uid}/{collect|do|wish}?start={0,15,30,...}
    https://book.douban.com/people/{uid}/...
    https://music.douban.com/people/{uid}/...
  collect/do/wish 分别对应 看过·读过·听过 / 在看·在读·在听 / 想看·想读·想听。

页面结构（2026-10 实测）：
  电影/音乐为旧结构 div.item.comment-item（短评在 span.comment 或无 class 的 li）；
  图书为新结构 li.subject-item（短评在 p.comment.comment-item）；
  book.douban.com 反爬较严，必须携带浏览器级请求头 + Referer，否则 403。

用法：
  python3 douban_export.py wenjun30                       # 全部类型，看过/读过/听过
  python3 douban_export.py https://www.douban.com/people/wenjun30/ \
      --category movie book --status collect wish --format csv md --out output

注意：仅供个人数据备份，请保持默认限速（--delay），尊重豆瓣服务。
"""

from __future__ import annotations  # 兼容 Python 3.9 的 int | None 注解

import argparse
import csv
import json
import random
import re
import sys
import time
from collections import Counter
from pathlib import Path

import requests
from bs4 import BeautifulSoup

UA = ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
      "(KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36")

# category -> (子域名, {status: 中文标签}, 条目统称)
CATEGORIES = {
    "movie": ("movie.douban.com", {"collect": "看过", "do": "在看", "wish": "想看"}, "电影·剧集"),
    "book":  ("book.douban.com",  {"collect": "读过", "do": "在读", "wish": "想读"}, "图书"),
    "music": ("music.douban.com", {"collect": "听过", "do": "在听", "wish": "想听"}, "音乐"),
}

PAGE_SIZE = 15  # 豆瓣收藏页固定每页 15 条

FIELDS = ["category", "status", "title", "alt_title", "my_rating", "date",
          "comment", "subject_url", "subject_id", "cover", "comment_id", "intro"]


# --------------------------------------------------------------------------- #
# 网络
# --------------------------------------------------------------------------- #

def make_session(uid: str) -> requests.Session:
    s = requests.Session()
    s.headers.update({
        "User-Agent": UA,
        "Accept": ("text/html,application/xhtml+xml,application/xml;q=0.9,"
                   "image/avif,image/webp,*/*;q=0.8"),
        "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",
        "Referer": f"https://www.douban.com/people/{uid}/",  # book 域名缺它会 403
        "Upgrade-Insecure-Requests": "1",
    })
    return s


def fetch_html(session: requests.Session, url: str, max_retry: int = 5,
               on_wait=None) -> str:
    """带退避重试的抓取：403/418/5xx 视为被限流，等待后重试。"""
    for attempt in range(1, max_retry + 1):
        try:
            resp = session.get(url, timeout=30)
        except requests.RequestException as e:
            if attempt == max_retry:
                raise
            wait = 10 * attempt
            if on_wait:
                on_wait(f"网络异常({e})，{wait}s 后重试 {attempt}/{max_retry}")
            time.sleep(wait)
            continue

        if resp.status_code == 200:
            resp.encoding = "utf-8"
            html = resp.text
            if len(html) > 5000:  # 正常页面 >30KB；防被插入软拦截页
                return html
            if on_wait:
                on_wait(f"响应过短({len(html)}B)疑似拦截，{attempt * 15}s 后重试")
            time.sleep(attempt * 15)
            continue

        if resp.status_code == 404:
            raise RuntimeError(f"页面不存在（404）：{url}")  # 不重试，快速失败

        # 403/418/5xx：限流或临时故障，指数退避
        wait = 20 * attempt + random.uniform(0, 5)
        if on_wait:
            on_wait(f"HTTP {resp.status_code}，{wait:.0f}s 后重试 {attempt}/{max_retry}")
        if attempt == max_retry:
            resp.raise_for_status()
            raise RuntimeError(f"抓取失败: {url}")
        time.sleep(wait)
    raise RuntimeError(f"抓取失败: {url}")


# --------------------------------------------------------------------------- #
# 解析
# --------------------------------------------------------------------------- #

SUBJECT_ID_RE = re.compile(r"/subject/(\d+)/?")


def _rating_from_span(span) -> int | None:
    """rating5-t / rating3-t ... -> 5 / 3；无星级返回 None。"""
    if span is None:
        return None
    for cls in span.get("class", []):
        m = re.fullmatch(r"rating(\d)-t", cls)
        if m:
            return int(m.group(1))
    return None


def _norm_space(text: str) -> str:
    return re.sub(r"\s+", " ", (text or "").strip())


def _clean_date(raw: str) -> str:
    m = re.search(r"(\d{4}-\d{2}-\d{2})", raw or "")
    return m.group(1) if m else _norm_space(raw)


def _base_record(category: str, status: str) -> dict:
    labels = CATEGORIES[category][1]
    return {"category": category, "status": labels[status]}


def parse_items(html: str, category: str, status: str) -> list[dict]:
    """兼容两种页面结构，返回条目列表。"""
    soup = BeautifulSoup(html, "html.parser")
    records = []

    # 旧结构：电影 / 音乐 —— div.item.comment-item
    for item in soup.select("div.item.comment-item"):
        rec = _base_record(category, status)
        pic_a = item.select_one("div.pic a.nbg") or item.select_one("div.pic a")
        if pic_a is None:
            continue
        rec["subject_url"] = pic_a.get("href", "")
        m = SUBJECT_ID_RE.search(rec["subject_url"])
        rec["subject_id"] = m.group(1) if m else ""
        img = pic_a.find("img")
        rec["cover"] = img.get("src", "") if img else ""
        rec["title"] = _norm_space(pic_a.get("title", ""))

        em = item.select_one("li.title em")
        title_line = _norm_space(em.get_text()) if em else rec["title"]
        if " / " in title_line:
            main, alt = title_line.split(" / ", 1)
            rec["title"] = _norm_space(main) or rec["title"]
            rec["alt_title"] = _norm_space(alt)
        else:
            rec["title"] = title_line or rec["title"]
            rec["alt_title"] = ""

        intro = item.select_one("li.intro")
        rec["intro"] = _norm_space(intro.get_text()) if intro else ""

        rec["my_rating"] = _rating_from_span(item.select_one("span[class^=rating]"))
        date_span = item.select_one("span.date")
        rec["date"] = _clean_date(date_span.get_text()) if date_span else ""

        # 短评：电影在 span.comment；音乐在无 class 且无 span 子节点的 <li> 里
        comment = item.select_one("span.comment")
        if comment is None:
            for li in item.select("div.info ul > li"):
                if li.get("class") or li.find("span"):
                    continue
                text = _norm_space(li.get_text())
                if text:
                    comment = li
                    break
        rec["comment"] = _norm_space(comment.get_text()) if comment else ""
        rec["comment_id"] = item.get("data-cid", "")
        records.append({k: rec.get(k, "") for k in FIELDS})

    # 新结构：图书 —— li.subject-item
    for item in soup.select("li.subject-item"):
        rec = _base_record(category, status)
        pic_a = item.select_one("div.pic a.nbg") or item.select_one("div.pic a")
        h2a = item.select_one("h2 a[title]")
        if pic_a is None and h2a is None:
            continue
        a = h2a or pic_a
        rec["subject_url"] = a.get("href", "")
        m = SUBJECT_ID_RE.search(rec["subject_url"])
        rec["subject_id"] = m.group(1) if m else ""
        img = pic_a.find("img") if pic_a else None
        rec["cover"] = img.get("src", "") if img else ""
        rec["title"] = _norm_space(a.get("title", "") or a.get_text())
        sub = a.find("span")
        rec["alt_title"] = _norm_space(sub.get_text()).lstrip(": ").strip() if sub else ""

        pub = item.select_one("div.pub")
        rec["intro"] = _norm_space(pub.get_text()) if pub else ""  # 作者/出版社/出版年/价格

        rec["my_rating"] = _rating_from_span(item.select_one("span[class^=rating]"))
        date_span = item.select_one("span.date")
        rec["date"] = _clean_date(date_span.get_text()) if date_span else ""

        p = item.select_one("p.comment, p.comment-item")
        rec["comment"] = _norm_space(p.get_text()) if p else ""
        rec["comment_id"] = (p.get("data-cid", "") if p else "") or item.get("data-cid", "")
        records.append({k: rec.get(k, "") for k in FIELDS})

    return records


def parse_total(html: str) -> int | None:
    """从 '1-15 / 208' 里取总条数。"""
    m = re.search(r'class="subject-num">\s*[\d-]+\s*&nbsp;/&nbsp;\s*(\d+)', html)
    return int(m.group(1)) if m else None


# --------------------------------------------------------------------------- #
# 抓取主流程（带断点续爬）
# --------------------------------------------------------------------------- #

def checkpoint_path(out_dir: Path, uid: str, category: str, status: str) -> Path:
    return out_dir / f".{uid}_{category}_{status}.ckpt.json"


def load_checkpoint(path: Path) -> dict:
    if path.exists():
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            pass
    return {"next_start": 0, "total": None, "items": []}


def save_checkpoint(path: Path, ckpt: dict) -> None:
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(ckpt, ensure_ascii=False), encoding="utf-8")
    tmp.replace(path)


def crawl(session: requests.Session, uid: str, category: str, status: str,
          out_dir: Path, delay: float, max_pages: int | None) -> list[dict]:
    domain, labels, _ = CATEGORIES[category]
    ckpt_file = checkpoint_path(out_dir, uid, category, status)
    ckpt = load_checkpoint(ckpt_file)
    if ckpt["items"]:
        print(f"  ↩ 断点续爬：已有 {len(ckpt['items'])} 条，从 start={ckpt['next_start']} 继续")

    start = ckpt["next_start"]
    while True:
        if max_pages is not None and start >= max_pages * PAGE_SIZE:
            print(f"  ⏹ 已达 --max-pages={max_pages} 上限，提前结束（进度已保存）")
            break
        if ckpt["total"] and start >= ckpt["total"]:
            break

        url = f"https://{domain}/people/{uid}/{status}?start={start}&sort=time"
        html = fetch_html(session, url, on_wait=lambda msg: print(f"  ⚠ {msg}"))

        total = parse_total(html)
        if total is not None:
            ckpt["total"] = total
        items = parse_items(html, category, status)
        if not items:
            break

        # 按 subject_id 去重（标记新增/翻页偏移可能造成重复）
        existing = {r["subject_id"] for r in ckpt["items"]}
        new_items = [r for r in items if r["subject_id"] not in existing]
        ckpt["items"].extend(new_items)

        got = len(ckpt["items"])
        real_total = ckpt["total"] or (start + len(items))
        dup_note = f"（重复 {len(items) - len(new_items)} 条已跳过）" \
            if len(new_items) < len(items) else ""
        print(f"  start={start:<5d} 本页 {len(items):>2d} 条，"
              f"累计 {got}/{real_total}{dup_note}")

        ckpt["next_start"] = start + PAGE_SIZE
        save_checkpoint(ckpt_file, ckpt)

        # 不以"本页不满 15 条"作为结束条件：页面可能因条目被删而少渲染，
        # 只有"空页"或"已达总数"才是可靠的终点。
        if not items:
            break
        start += PAGE_SIZE
        time.sleep(delay + random.uniform(0, 1.0))

    return ckpt["items"]


# --------------------------------------------------------------------------- #
# 导出
# --------------------------------------------------------------------------- #

def export_csv(records: list[dict], path: Path) -> None:
    with open(path, "w", newline="", encoding="utf-8-sig") as f:  # BOM 便于 Excel 打开
        w = csv.DictWriter(f, fieldnames=FIELDS)
        w.writeheader()
        w.writerows(records)


def export_json(records: list[dict], path: Path) -> None:
    path.write_text(json.dumps(records, ensure_ascii=False, indent=2), encoding="utf-8")


def _md_escape(text: str) -> str:
    return (text or "").replace("|", "\\|").replace("\n", " ")


def export_md(records: list[dict], path: Path, title: str) -> None:
    lines = [f"# {title}（{len(records)} 条）", "",
             "| 标题 | 我的评分 | 日期 | 短评 | 链接 |",
             "|---|---|---|---|---|"]
    for r in records:
        rating = "★" * r["my_rating"] if r["my_rating"] else "—"
        lines.append("| {} | {} | {} | {} | [条目]({}) |".format(
            _md_escape(r["title"]), rating, r["date"],
            _md_escape(r["comment"]), r["subject_url"]))
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def export_xlsx(records: list[dict], path: Path) -> bool:
    try:
        from openpyxl import Workbook
    except ImportError:
        return False
    wb = Workbook()
    ws = wb.active
    ws.append(FIELDS)
    for r in records:
        ws.append([r[k] for k in FIELDS])
    wb.save(path)
    return True


# --------------------------------------------------------------------------- #
# CLI
# --------------------------------------------------------------------------- #

def parse_uid(raw: str) -> str:
    m = re.search(r"/people/([^/?#]+)/?", raw)
    if m:
        return m.group(1)
    if re.fullmatch(r"[\w.\-]+", raw):
        return raw
    sys.exit(f"无法从 {raw!r} 解析豆瓣用户 ID（应为 /people/<id>/ 或直接给 id）")


def main() -> None:
    ap = argparse.ArgumentParser(
        description="免登录导出豆瓣用户的公开书影音标记与短评（仅公开数据）")
    ap.add_argument("user", help="豆瓣用户 ID 或个人主页 URL")
    ap.add_argument("--category", nargs="+", choices=list(CATEGORIES),
                    default=list(CATEGORIES), help="类型，默认全部")
    ap.add_argument("--status", nargs="+", default=["collect"],
                    choices=["collect", "do", "wish"],
                    help="标记状态，默认 collect（看过/读过/听过）")
    ap.add_argument("--format", nargs="+", default=["csv", "json"],
                    choices=["csv", "json", "md", "xlsx"], dest="formats")
    ap.add_argument("--out", default="output", help="输出目录（默认 ./output）")
    ap.add_argument("--delay", type=float, default=2.0,
                    help="翻页间隔秒数（默认 2.0，请勿调小）")
    ap.add_argument("--max-pages", type=int, default=None,
                    help="每个类别最多抓取页数（测试用）")
    args = ap.parse_args()

    uid = parse_uid(args.user)
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    print(f"目标用户：{uid}")
    print(f"抓取计划：{' '.join(args.category)} × {' '.join(args.status)}，"
          f"限速 {args.delay}s/页\n")

    session = make_session(uid)
    all_results: list[dict] = []

    for category in args.category:
        for status in args.status:
            label = CATEGORIES[category][1][status]
            print(f"▶ {CATEGORIES[category][2]} · {label}"
                  f"（{category}/{status}）")
            try:
                records = crawl(session, uid, category, status, out_dir,
                                args.delay, args.max_pages)
            except Exception as e:
                print(f"  ✗ 失败：{e}（进度已保存，可直接重跑续爬）\n")
                continue

            if not records:
                print("  （无公开数据或用户未标记）\n")
                continue

            records.sort(key=lambda r: r["date"], reverse=True)
            base = out_dir / f"{uid}_{category}_{status}"
            if "csv" in args.formats:
                export_csv(records, base.with_suffix(".csv"))
            if "json" in args.formats:
                export_json(records, base.with_suffix(".json"))
            if "md" in args.formats:
                export_md(records, base.with_suffix(".md"),
                          f"{uid} 的{label}记录·{CATEGORIES[category][2]}")
            if "xlsx" in args.formats and not export_xlsx(records, base.with_suffix(".xlsx")):
                print("  （未安装 openpyxl，跳过 xlsx）")

            n_comment = sum(1 for r in records if r["comment"])
            print(f"  ✓ 完成 {len(records)} 条，其中含短评 {n_comment} 条 →"
                  f" {base.with_suffix('.*').name}\n")
            all_results.extend(records)
            checkpoint_path(out_dir, uid, category, status).unlink(missing_ok=True)

    if all_results:
        counter = Counter(f"{r['category']}·{r['status']}" for r in all_results)
        summary = out_dir / f"{uid}_summary.json"
        summary.write_text(json.dumps({
            "user": uid, "exported_at": time.strftime("%Y-%m-%d %H:%M:%S"),
            "total": len(all_results),
            "with_comment": sum(1 for r in all_results if r["comment"]),
            "by_category_status": dict(counter),
        }, ensure_ascii=False, indent=2), encoding="utf-8")
        print(f"全部完成：共 {len(all_results)} 条 → {summary}")


if __name__ == "__main__":
    main()

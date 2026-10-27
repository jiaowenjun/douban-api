# douban-export

免登录导出豆瓣用户的**公开书影音标记与短评**（电影/图书/音乐 × 看过·读过·听过/在看·在读·在听/想看·想读·想听）。

原理与背景见 [豆瓣API调研报告.md](豆瓣API调研报告.md) 中的"路径②：公开收藏页 HTML 抓取"——豆瓣官方 API 已停止服务，本工具直接抓取公开收藏页（`movie/book/music.douban.com/people/{uid}/...`）并解析，**无需登录、无需 Cookie**。

Rust 实现（reqwest + scraper + clap），编译为单二进制，无需运行时环境。

## 使用

```bash
cargo build --release
./target/release/douban-export wenjun30                      # 全部类型，默认导出 csv json
./target/release/douban-export https://www.douban.com/people/wenjun30/ \
    --category movie book --status collect wish \
    --format csv json md xlsx --out output
```

## 参数

| 参数 | 默认 | 说明 |
|---|---|---|
| `user` | 必填 | 豆瓣用户 ID 或个人主页 URL |
| `--category` | movie book music | 类型 |
| `--status` | collect | collect=看过·读过·听过，do=在，wish=想 |
| `--format` | csv json | csv / json / md / xlsx |
| `--out` | output | 输出目录 |
| `--delay` | 2.0 | 翻页间隔秒数（**请勿调小**） |
| `--max-pages` | 无 | 每类别最多页数（测试用） |

## 特性

- 兼容两代页面结构：电影/音乐（`div.item.comment-item`）与图书（`li.subject-item`）
- `book.douban.com` 反爬较严，已自动携带浏览器级请求头 + Referer
- 403/418/5xx 自动指数退避重试；404 快速失败
- **断点续爬**：进度存于 `output/.{user}_{cat}_{status}.ckpt.json`，中断后重跑自动续传；完成后自动清理
- 按 `subject_id` 去重；导出 CSV 带 BOM（Excel 直接打开不乱码）

## 导出字段

`category, status, title, alt_title, my_rating, date, comment, subject_url, subject_id, cover, comment_id, intro`

## 已知限制

- 个别条目豆瓣已删除/不再渲染，总数可能差 1~2 条（如测试账号电影 207/208），无法通过任何页面模式取回
- 仅能获取用户**公开**的标记；设置了隐私的账号拿不到
- 页面结构变更时解析器需要更新（2026-10 实测有效）

## 仅供个人数据备份

请保持默认限速，尊重豆瓣服务条款；请勿用于大规模抓取。

# -*- coding: utf-8 -*-
"""豆瓣收藏列表页 intro 元数据行解析器（三类）。

从 backend/data/douban_export.db 的 records.intro 解析结构化字段：
- parse_movie(intro, title, alt) → dates/years/places/countries/genres/langs/duration/director_list/people
- parse_book(intro)               → authors/publisher/pubdate/pubyear/price_raw/price_val/currency
- parse_music(intro)              → artist/date/year/mtype/medium/genre

字段顺序无标注，靠词表（GENRES/COUNTRIES/LANGS…）+ 位置启发式归类；
导演回扫规则见 parse_movie 内注释。测试用例见 wenjun30-*intro测试用例.json，
校验脚本 run_intro_tests.py。由 analyze.py 解析段抽取，与汇总文档同源。
"""

import json, re, statistics, datetime
from collections import Counter, defaultdict

BASE = "/tmp/intro_analysis"
OUT = "/Users/jwj/workspace/weimo-douban/intro汇总"

def load(name):
    with open(f"{BASE}/{name}.json", encoding="utf-8") as f:
        return json.load(f)

# ---------------- 词表 ----------------
GENRES = set("""剧情 喜剧 爱情 动作 科幻 动画 悬疑 惊悚 恐怖 犯罪 同性 音乐 歌舞 传记
历史 战争 西部 奇幻 冒险 灾难 武侠 情色 家庭 古装 运动 短片 黑色电影 音乐剧 舞台艺术
纪录片 脱口秀 真人秀 青春 儿童 谍战 枪战 荒诞 情节 戏曲""".split())

COUNTRIES = set("""中国大陆 中国香港 中国台湾 中国澳门 中国 美国 日本 韩国 英国 法国 德国
意大利 西班牙 葡萄牙 荷兰 比利时 瑞典 丹麦 挪威 芬兰 冰岛 波兰 俄罗斯 乌克兰 土耳其
印度 泰国 越南 新加坡 马来西亚 印度尼西亚 菲律宾 埃及 南非 澳大利亚 新西兰 加拿大
墨西哥 巴西 阿根廷 智利 哥伦比亚 秘鲁 古巴 伊朗 以色列 沙特阿拉伯 卡塔尔 奥地利
瑞士 希腊 匈牙利 捷克 罗马尼亚 爱尔兰 塞尔维亚 克罗地亚 保加利亚 摩洛哥 突尼斯
阿尔及利亚 尼日利亚 肯尼亚 蒙古 柬埔寨 缅甸 尼泊尔 巴基斯坦 孟加拉国 斯里兰卡
卢森堡 立陶宛 拉脱维亚 爱沙尼亚 塞浦路斯 苏联 西德 东德 捷克斯洛伐克 南斯拉夫 波黑 斯洛文尼亚 北马其顿
阿联酋 巴林 黎巴嫩 叙利亚 约旦 伊拉克 阿富汗 乌兹别克斯坦 哈萨克斯坦""".split())

LANGS = set("""汉语普通话 粤语 闽南语 上海话 四川话 四川方言 汉语 手语 无对白 默片
英语 日语 韩语 法语 德语 西班牙语 意大利语 葡萄牙语 俄语 阿拉伯语 希伯来语 波斯语
印地语 泰语 越南语 蒙古语 土耳其语 荷兰语 瑞典语 挪威语 丹麦语 芬兰语 波兰语 捷克语
希腊语 匈牙利语 冰岛语 拉丁语 印尼语 菲律宾语 他加禄语 旁遮普语 泰米尔语 乌尔都语
孟加拉语 粤语 客家话 温州话 南京话 武汉话 陕西话 山西话 唐山话 东北话
粤语字 广东话 塞尔维亚语 克罗地亚语 乌克兰语 罗马尼亚语 保加利亚语""".split())

MEDIA_MUSIC = {"CD", "Audio CD", "黑胶", "磁带", "卡带", "DVD", "Blu-ray", "数字", "数字专辑", "数字(Digital)", "Digital", "Import", "Vinyl", "LP", "胶片", "盒式磁带", "DSD", "SACD", "DVD-Audio"}
TYPE_MUSIC = {"专辑", "单曲", "EP", "迷你专辑", "合辑", "精选集", "选集", "现场专辑", "原声带", "合集", "录音室专辑", "Live专辑", "Live", "现场"}

FW = str.maketrans("０１２３４５６７８９", "0123456789")
DATE_RE = re.compile(r"^(\d{4})[-/](\d{1,2})(?:[-/](\d{1,2}))?")
def date_year(f):
    """识别 YYYY / YYYY-M / YYYY-M-D / YYYY年M月[日]，返回年份 int 或 None。"""
    s = f.translate(FW).strip()
    s = re.sub(r"[（(][^()（）]*[)）]$", "", s)  # 去掉日期尾部的 (地点/影展) 注记
    s = re.sub(r"\s+\d{1,2}:\d{2}(:\d{2})?$", "", s)  # 去掉尾部时间（2025-07-16 20:50）
    s = s.replace("年", "-").replace("月", "-").replace("日", "").rstrip("-")
    m = re.fullmatch(r"(\d{4})(?:[-/](\d{1,2}))?(?:[-/](\d{1,2}))?", s)
    if not m or (m.group(2) and int(m.group(2)) > 12) or (m.group(3) and int(m.group(3)) > 31):
        return None
    return int(m.group(1))
DUR_RE = re.compile(r"^(\d+)\s*分钟(?:[（(][^)）]*[)）])?$")
PRICE_RE = re.compile(r"^(?:CNY|RMB|NT\$|USD|US\$|HK\$|€|£|￥|¥)?\s*([\d,.]+)\s*(元|块|美元|港币|台币|日元|英镑|欧元|卢布|NT\$|USD|US\$|HK\$|￥|¥|€|£)?$")

def cjk_key(field):
    """人名字段归一键：取 CJK 部分；无 CJK 则原样。"""
    m = re.match(r"^(.*?)[\s]+([A-Za-z][A-Za-zÀ-ž'.\- ]*)$", field)
    if m and re.search(r"[\u4e00-\u9fff·]", m.group(1)):
        return m.group(1).strip()
    return field.strip()

# ---------------- 电影 ----------------
def parse_movie(intro, title="", alt=""):
    fields = [x.strip() for x in intro.split(" / ") if x.strip()]
    own_names = {title.strip(), alt.strip()} - {""}
    junk_re = re.compile(r"www\.|http|\.(com|net|org|cn|jp|info)(/|$)")
    def is_own_name(x):
        if x in own_names:
            return True
        return len(x) >= 4 and any(t.startswith(x) for t in own_names)
    r = {"dates": [], "years": [], "places": [], "people": [], "countries": [],
         "langs": [], "genres": [], "duration": None, "other": []}
    genre_idx = dur_idx = None
    for i, f in enumerate(fields):
        if date_year(f):
            r["dates"].append(f); r["years"].append(date_year(f))
            pm = re.search(r"[（(](.+?)[)）]", f)
            if pm: r["places"].append(pm.group(1))
        elif DUR_RE.match(f):
            if dur_idx is None:
                r["duration"] = int(DUR_RE.match(f).group(1)); dur_idx = i
        elif f in COUNTRIES: r["countries"].append(f)
        elif f in LANGS or re.fullmatch(r"[\u4e00-\u9fff]{1,6}(语|话)", f) or re.fullmatch(r"[\u4e00-\u9fff]{2,5}方言", f): r["langs"].append(f)
        elif f in GENRES: r["genres"].append(f); genre_idx = i if genre_idx is None else genre_idx
        elif is_own_name(f) or junk_re.search(f):
            pass  # 原名/又名（含前缀变体）或官网等杂质字段，不计入人名
        else:
            if re.search(r"[A-Za-z\u4e00-\u9fff]", f): r["people"].append(f)
            else: r["other"].append(f)
    # 导演启发式：从首个片长（无片长则首个类型）向前取连续人名段，越过其余片长字段；
    # 段内多人视为联合执导；遇到国家/日期/类型/语言/条目名即停。
    anchor = dur_idx if dur_idx is not None else genre_idx
    dirs = []
    if anchor is not None:
        for j in range(anchor - 1, -1, -1):
            f = fields[j]
            if f in COUNTRIES or f in LANGS or f in GENRES:
                break
            if DUR_RE.match(f):
                continue
            if date_year(f) or is_own_name(f) or junk_re.search(f):
                # 导演与片长之间偶夹裸日期/原名变体/官网；已取到人名则停，否则跳过
                if dirs:
                    break
                continue
            dirs.append(cjk_key(f))
        dirs.reverse()
    r["director_list"] = dirs
    r["director"] = "、".join(dirs)
    r["people_keys"] = [cjk_key(p) for p in r["people"]]
    return r

# ---------------- 图书 ----------------
def parse_book(intro):
    fields = [x.strip() for x in intro.split(" / ") if x.strip()]
    r = {"authors": [], "publisher": "", "pubdate": "", "pubyear": None, "price_raw": "", "price_val": None, "currency": ""}
    date_i = None
    for i, f in enumerate(fields):
        if date_year(f) and date_i is None:
            date_i = i; r["pubdate"] = f; r["pubyear"] = date_year(f)
    if date_i is None:
        r["unparsed"] = fields; return r
    r["authors"] = fields[:date_i - 1] if date_i >= 1 else []
    r["publisher"] = fields[date_i - 1] if date_i >= 1 else ""
    if len(fields) - 1 > date_i:
        r["price_raw"] = fields[-1]
        pm = PRICE_RE.match(fields[-1])
        if pm and pm.group(1):
            val = pm.group(1).replace(",", "")
            try:
                r["price_val"] = float(val.rstrip("."))
                cur = pm.group(2) or ""
                if not cur and fields[-1].lstrip().startswith(("NT$", "US", "HK$", "€", "£")):
                    cur = fields[-1].lstrip()[:3]
                r["currency"] = cur or ("NT$" if "NT$" in fields[-1] else cur)
                if not r["currency"] and "NT$" in fields[-1]: r["currency"] = "NT$"
            except ValueError:
                pass
    return r

# ---------------- 音乐 ----------------
def parse_music(intro):
    fields = [x.strip() for x in intro.split(" / ") if x.strip()]
    r = {"artist_raw": "", "artist": "", "date": "", "year": None, "mtype": "", "medium": "", "genre": "", "other": []}
    date_i = None
    for i, f in enumerate(fields):
        if date_year(f) and date_i is None:
            date_i = i; r["date"] = f; r["year"] = date_year(f)
    if date_i is None:
        r["other"] = fields; return r
    r["artist_raw"] = " / ".join(fields[:date_i])
    a = fields[0].split(",")[0].strip()
    a = re.sub(r"\s*[（(][^()（）]*[)）]\s*$", "", a)  # 科尔托 (Alfred Cortot) → 科尔托
    cjk = re.sub(r"[A-Za-zÀ-ž0-9.'\-\s]+$", "", a).strip()  # 王菲 Faye Wong → 王菲
    r["artist"] = cjk if re.search(r"[\u4e00-\u9fff]", cjk) else a
    MEDIA_NORM = {"Audio CD": "CD", "数字(Digital)": "数字"}
    for f in fields[date_i + 1:]:
        if f in MEDIA_MUSIC: r["medium"] = (r["medium"] + "+" + MEDIA_NORM.get(f, f)) if r["medium"] else MEDIA_NORM.get(f, f)
        elif f in TYPE_MUSIC: r["mtype"] = f
        else: r["genre"] = (r["genre"] + "+" + f) if r["genre"] else f
    return r


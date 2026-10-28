# -*- coding: utf-8 -*-
"""intro 测试用例校验脚本：逐条重解析 wenjun30-*intro测试用例.json 的 intro，
与 expected 深度比较。用法：python3 run_intro_tests.py [测试用例目录]"""
import json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from intro_parser import parse_movie, parse_book, parse_music

PARSERS = {
    "movie": lambda c: parse_movie(c["intro"], c["title"], ""),
    "book": lambda c: parse_book(c["intro"]),
    "music": lambda c: parse_music(c["intro"]),
}
FIELDS = {
    "movie": ["dates", "years", "places", "countries", "genres", "langs", "duration",
              "director_list", "people", "people_keys"],
    "book": ["authors", "publisher", "pubdate", "pubyear", "price_raw", "price_val", "currency"],
    "music": ["artist", "artist_raw", "date", "year", "mtype", "medium", "genre"],
}

def main(dirpath=HERE):
    total = failed = 0
    for cat, fname in [("movie", "wenjun30-电影intro测试用例.json"),
                       ("book", "wenjun30-图书intro测试用例.json"),
                       ("music", "wenjun30-音乐intro测试用例.json")]:
        with open(os.path.join(dirpath, fname), encoding="utf-8") as f:
            doc = json.load(f)
        for c in doc["cases"]:
            total += 1
            got = {k: PARSERS[cat](c)[k] for k in FIELDS[cat]}
            want = {k: c["expected"][k] for k in FIELDS[cat]}
            if got != want:
                failed += 1
                print(f"FAIL {c['id']} {c['title']}")
                for k in FIELDS[cat]:
                    if got[k] != want[k]:
                        print(f"  {k}: got {got[k]!r} != want {want[k]!r}")
        print(f"{cat}: {doc['count']} cases checked")
    print(f"\n{total - failed}/{total} passed" + (f", {failed} FAILED" if failed else ""))
    return 1 if failed else 0

if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else HERE))

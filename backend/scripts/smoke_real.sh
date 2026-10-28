#!/usr/bin/env bash
# 真机冒烟：起隔离实例走真实豆瓣，验证收藏页解析链路与 rexxar 富集链路。
#
# 用途：fixture 与单元测试锁的是"解析器实现"，检测豆瓣源漂移（改版/登录墙/
# 接口变化）只能靠真实请求（《豆瓣源依赖性与脆弱性分析.md》§6 第 5 条）。
# 哨兵（PARSE_ERROR）会把结构性漂移显式化为任务 failed——本脚本同时验证
# "正常链路可用"与"哨兵未误伤"两件事。
#
# 用法：backend/scripts/smoke_real.sh [uid]     # 默认 wenjun30，走 book 维度（4 页）
# 前置：本机可访问豆瓣；流量礼貌（4 页收藏页 + 若干 rexxar，遵守服务端 2s+/3s+ 限速）。
#
# 定期执行示例（crontab 每日 09:30，异常时退出码非 0）：
#   30 9 * * * cd /path/to/weimo-douban/backend && scripts/smoke_real.sh >> /tmp/weimo-smoke.log 2>&1
set -euo pipefail

SMOKE_UID=${1:-wenjun30}
PORT=$((20000 + RANDOM % 20000))
WORK=$(mktemp -d "${TMPDIR:-/tmp}/weimo-smoke.XXXXXX")
BIN=target/release/weimo-douban-server

cd "$(dirname "$0")/.."

cleanup() {
  [ -n "${SRV_PID:-}" ] && kill "$SRV_PID" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

jsonget() { # jsonget <json-string> <python-expr over d>
  printf '%s' "$1" | python3 -c "import json,sys; d=json.load(sys.stdin); print($2)"
}

echo "[smoke] 构建 release（增量）..."
cargo build --release --bin weimo-douban-server

echo "[smoke] 启动隔离实例 127.0.0.1:${PORT}（临时数据目录，不触碰开发库）"
SYNC_SERVER_BIND="127.0.0.1:$PORT" SYNC_SERVER_DATA_DIR="$WORK" \
  "$BIN" >"$WORK/server.log" 2>&1 &
SRV_PID=$!
disown # 摘出作业表，结束时 kill 不触发 shell 的 Terminated 噪音

for _ in $(seq 1 40); do
  curl -fsS "http://127.0.0.1:$PORT/healthz" >/dev/null 2>&1 && break
  sleep 0.5
done
curl -fsS "http://127.0.0.1:$PORT/healthz" >/dev/null # 仍不通则失败退出

echo "[smoke] 创建 book 同步任务（uid=${SMOKE_UID}）..."
TASK=$(curl -fsS -X POST "http://127.0.0.1:$PORT/api/v1/syncs" \
  -H 'Content-Type: application/json' \
  -d "{\"user\":\"$SMOKE_UID\",\"categories\":[\"book\"],\"statuses\":[\"collect\"]}")
TASK_ID=$(jsonget "$TASK" 'd["task_id"]')
echo "[smoke] task=${TASK_ID}（4 页 × ≥2s 限速，预计 ~15s）"

STATUS=running
for _ in $(seq 1 150); do # 上限 5 分钟
  VIEW=$(curl -fsS "http://127.0.0.1:$PORT/api/v1/syncs/$TASK_ID")
  STATUS=$(jsonget "$VIEW" 'd["status"]')
  case "$STATUS" in
    completed) break ;;
    failed|partial)
      echo "[smoke] FAIL: 任务终态 ${STATUS}——豆瓣源疑似漂移（含哨兵 PARSE_ERROR 命中）"
      jsonget "$VIEW" 'json.dumps(d.get("error"), ensure_ascii=False, indent=2)'
      exit 1 ;;
  esac
  sleep 2
done
if [ "$STATUS" != completed ]; then
  echo "[smoke] FAIL: 超时未完成（status=${STATUS})，查 $WORK/server.log"
  exit 1
fi

MARKS=$(curl -fsS "http://127.0.0.1:$PORT/api/v1/users/$SMOKE_UID/marks?category=book&status=collect&page_size=1")
TOTAL=$(jsonget "$MARKS" 'd["total"]')
SID=$(jsonget "$MARKS" 'd["items"][0]["subject_id"]')
echo "[smoke] marks total=${TOTAL}（首条 subject_id=${SID})"
if [ "${TOTAL:-0}" -le 0 ]; then
  echo "[smoke] FAIL: 任务 completed 但解析 0 条——存在哨兵未覆盖的静默形态，请人工排查"
  exit 1
fi

echo "[smoke] 等待 rexxar 富集产出（worker 空闲时段逐条，3s+/条，上限 3 分钟）..."
ENRICHED=0
for _ in $(seq 1 90); do
  CODE=$(curl -s -o /dev/null -w '%{http_code}' \
    "http://127.0.0.1:$PORT/api/v1/subjects/book/$SID")
  if [ "$CODE" = 200 ]; then ENRICHED=1; break; fi
  sleep 2
done
if [ "$ENRICHED" != 1 ]; then
  echo "[smoke] FAIL: 条目 ${SID} 富集无产出——疑似 rexxar 接口变化，查 $WORK/server.log"
  exit 1
fi

echo "[smoke] PASS：收藏页解析 + rexxar 富集两链路真机可用"

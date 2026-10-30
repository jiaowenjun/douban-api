/** 卡片日期栏三级相对格式（与 weimo-biji skyline 的
 *  components/memo-card/date-display.ts 逐字对齐，改名 memo→mark）：
 *  今天且带时分 → H:mm（小时不补零）；今年 → M/D（不补零）；往年 →
 *  Y/M/D。输入不匹配 YYYY-MM-DD[ HH:mm] 正则时原样透传，空串返回空。
 *  豆瓣标记数据 date 为纯日期无时分，今天即落到 M/D 分支——与
 *  skyline 对无时分输入的行为一致。React 每次渲染重算，无 skyline
 *  的跨午夜缓存不刷新问题 */
const MARK_TIMESTAMP_RE = /^(\d{4})-(\d{2})-(\d{2})(?:\s+(\d{2}):(\d{2}))?$/

function getLocalDateParts(date: Date): { year: number; month: number; day: number } | null {
  if (Number.isNaN(date.getTime())) return null
  return {
    year: date.getFullYear(),
    month: date.getMonth() + 1,
    day: date.getDate(),
  }
}

export function formatMarkCardDateBarText(
  dateText: string,
  now: Date = new Date(),
): string {
  const text = String(dateText || '').trim()
  if (!text) return ''

  const match = MARK_TIMESTAMP_RE.exec(text)
  if (!match) return text

  const year = Number(match[1])
  const month = Number(match[2])
  const day = Number(match[3])
  const hourText = match[4] || ''
  const minuteText = match[5] || ''
  const today = getLocalDateParts(now)

  if (
    today &&
    year === today.year &&
    month === today.month &&
    day === today.day &&
    hourText &&
    minuteText
  ) {
    return `${Number(hourText)}:${minuteText}`
  }

  if (today && year === today.year) {
    return `${month}/${day}`
  }

  return `${year}/${month}/${day}`
}

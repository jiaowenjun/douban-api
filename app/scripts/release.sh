#!/usr/bin/env bash
# 发布桌面 App：构建（含 updater 签名产物）→ 生成 latest.json → gh release 上传。
# 已装旧版的 App 经启动自检/托盘「检查更新」走 GitHub Releases latest-download
# 重定向自动升级（默认端点见 src/main.rs UPDATER_ENDPOINT）。
#
# 前置：
#   1. tauri.conf.json 的 version 已 bump 到目标版本
#   2. TAURI_SIGNING_PRIVATE_KEY 指向私钥（首次生成：
#      pnpm --dir app tauri signer generate -w ~/.tauri-keys/weimo-douban-updater.key --password "" --ci
#      私钥丢失则老版本无法升级到新版本，务必备份）
#   3. gh 已登录且有仓库发布权限
# 用法：TAURI_SIGNING_PRIVATE_KEY=~/.tauri-keys/weimo-douban-updater.key ./scripts/release.sh
set -euo pipefail
cd "$(dirname "$0")/.."

: "${TAURI_SIGNING_PRIVATE_KEY:?需要设置 TAURI_SIGNING_PRIVATE_KEY（私钥路径或内容），密钥丢失将无法推送更新}"
REPO="jiaowenjun/weimo-douban"
VERSION=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' tauri.conf.json | head -1)
BUNDLE=target/release/bundle
# 产物名随 productName（微墨·豆瓣）；latest.json 的 URL 保持未编码 UTF-8，
# updater 侧 reqwest/url 解析时会自动百分号编码请求路径
APP_TGZ="$BUNDLE/macos/微墨·豆瓣.app.tar.gz"

echo "==> 构建 v$VERSION"
pnpm tauri build

DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
SIG=$(cat "$APP_TGZ.sig")
ASSET_URL="https://github.com/$REPO/releases/download/v$VERSION/微墨·豆瓣.app.tar.gz"
python3 - "$ASSET_URL" "$VERSION" "$DATE" "$SIG" << 'EOF'
import json, sys
url, version, date, sig = sys.argv[1:5]
json.dump({
    "version": version,
    "notes": f"微墨·豆瓣桌面 App v{version}",
    "pub_date": date,
    "platforms": {"darwin-aarch64": {"signature": sig, "url": url}},
}, open("latest.json", "w"), indent=2)
EOF

echo "==> 创建 GitHub Release v$VERSION"
gh release create "v$VERSION" \
  "$BUNDLE/dmg/微墨·豆瓣_${VERSION}_aarch64.dmg" \
  "$APP_TGZ" "$APP_TGZ.sig" latest.json \
  --title "微墨·豆瓣 App v$VERSION" \
  --notes "微墨·豆瓣桌面 App v$VERSION（darwin-aarch64）。已装旧版者经托盘「检查更新」或启动自检自动升级；新安装请下载 dmg。"
echo "==> 发布完成：v$VERSION（latest.json 已随 release 上传，默认端点即可发现新版本）"

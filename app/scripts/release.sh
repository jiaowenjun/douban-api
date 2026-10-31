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
# 本地产物名随 productName（微墨·豆瓣，含 CJK）；上传前统一复制为 ASCII 名——
# gh release 上传会剥掉资产名里的非 ASCII 前缀（实测 CJK dmg 变成
# "_0.4.0_aarch64.dmg"），latest/download 直链与 updater URL 全部落空，
# 故 dmg/tgz/sig 三件全用 weimo-douban 前缀的稳定 ASCII 名
APP_TGZ="$BUNDLE/macos/微墨·豆瓣.app.tar.gz"
DMG_ASCII="$BUNDLE/dmg/weimo-douban_${VERSION}_aarch64.dmg"
TGZ_ASCII="$BUNDLE/macos/weimo-douban.app.tar.gz"

echo "==> 构建 v$VERSION"
pnpm tauri build

echo "==> 复制产物为 ASCII 资产名"
cp "$BUNDLE/dmg/微墨·豆瓣_${VERSION}_aarch64.dmg" "$DMG_ASCII"
cp "$APP_TGZ" "$TGZ_ASCII"
cp "$APP_TGZ.sig" "$TGZ_ASCII.sig"

DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
SIG=$(cat "$APP_TGZ.sig")
ASSET_URL="https://github.com/$REPO/releases/download/v$VERSION/weimo-douban.app.tar.gz"
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
# 注意：紧邻全角字符的版本变量必须写成 ${VERSION}——macOS 自带 bash 3.2
# 的多字节解析 bug 会把后续 CJK 字符的字节吞进变量名（unbound variable）
gh release create "v$VERSION" \
  "$DMG_ASCII" \
  "$TGZ_ASCII" "$TGZ_ASCII.sig" latest.json \
  --title "微墨·豆瓣 App v$VERSION" \
  --notes "微墨·豆瓣桌面 App v${VERSION}（darwin-aarch64）。已装旧版者经托盘「检查更新」或启动自检自动升级；新安装请下载 dmg。"
echo "==> 发布完成：v${VERSION}（latest.json 已随 release 上传，默认端点即可发现新版本）"

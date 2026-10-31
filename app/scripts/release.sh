#!/usr/bin/env bash
# 发布桌面 App：构建（含 updater 签名产物）→ 拉取 Windows CI 产物 →
# 生成 latest.json → gh release 上传。已装旧版的 App 经启动自检/托盘
# 「检查更新」走 GitHub Releases latest-download 重定向自动升级（默认
# 端点见 src/main.rs UPDATER_ENDPOINT）。
#
# 前置：
#   1. tauri.conf.json 的 version 已 bump 到目标版本
#   2. TAURI_SIGNING_PRIVATE_KEY 指向私钥（首次生成：
#      pnpm --dir app tauri signer generate -w ~/.tauri-keys/weimo-douban-updater.key --password "" --ci
#      私钥丢失则老版本无法升级到新版本，务必备份）
#   3. gh 已登录且有仓库发布权限
#   4.（可选，发 Windows 版）目标版本已 push 到 main 且 app-windows
#      工作流（.github/workflows/app-windows.yml）已手动 dispatch 成功
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

# ---- Windows 产物：mac 无法交叉编译，由 app-windows 工作流出
# windows-latest 构建并存为 run artifact（exe 安装包 + exe.sig 更新
# 签名 + version.txt）。拉最新成功运行，版本一致才并入本次发布；
# 缺失或版本不符则仅发 macOS（Windows 条目留待下次发布补上）。
WIN_DIR="$BUNDLE/windows-ci"
WIN_PRESENT=0
RUN_ID=$(gh run list --repo "$REPO" --workflow app-windows.yml --status success --limit 1 --json databaseId --jq '.[0].databaseId // empty' 2>/dev/null || true)
if [ -n "$RUN_ID" ]; then
  rm -rf "$WIN_DIR" && mkdir -p "$WIN_DIR"
  gh run download "$RUN_ID" --repo "$REPO" --name windows-app --dir "$WIN_DIR" \
    || { echo "  !! 拉取 Windows 产物失败（run $RUN_ID）"; exit 1; }
  CI_VERSION=$(cat "$WIN_DIR/version.txt")
  if [ "$CI_VERSION" = "$VERSION" ]; then
    WIN_PRESENT=1
    echo "==> Windows 产物已就绪（run ${RUN_ID}，v${CI_VERSION}）"
  else
    echo "==> 跳过 Windows 产物：CI 版本 v$CI_VERSION 与本地 v$VERSION 不一致（push 后请重新 dispatch app-windows 工作流）"
  fi
else
  echo "==> 未找到 app-windows 成功运行，本次仅发布 macOS 产物"
fi

DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
SIG=$(cat "$APP_TGZ.sig")
ASSET_URL="https://github.com/$REPO/releases/download/v$VERSION/weimo-douban.app.tar.gz"
WIN_SIG=""
WIN_URL=""
if [ "$WIN_PRESENT" = 1 ]; then
  WIN_SIG=$(cat "$WIN_DIR/weimo-douban_x64-setup.exe.sig")
  # Tauri v2 NSIS updater 直接指向 setup.exe 本体（对其 minisign 签名，
  # 无 v1 时代的 .nsis.zip 包装），URL 用 ASCII 资产名
  WIN_URL="https://github.com/$REPO/releases/download/v$VERSION/weimo-douban_${VERSION}_x64-setup.exe"
fi
python3 - "$ASSET_URL" "$VERSION" "$DATE" "$SIG" "$WIN_SIG" "$WIN_URL" << 'EOF'
import json, sys
url, version, date, sig, win_sig, win_url = sys.argv[1:7]
platforms = {"darwin-aarch64": {"signature": sig, "url": url}}
if win_sig:
    platforms["windows-x86_64"] = {"signature": win_sig, "url": win_url}
json.dump({
    "version": version,
    "notes": f"微墨·豆瓣桌面 App v{version}",
    "pub_date": date,
    "platforms": platforms,
}, open("latest.json", "w"), indent=2)
EOF

echo "==> 创建 GitHub Release v$VERSION"
ASSETS=("$DMG_ASCII" "$TGZ_ASCII" "$TGZ_ASCII.sig" latest.json)
if [ "$WIN_PRESENT" = 1 ]; then
  ASSETS+=(
    "$WIN_DIR/weimo-douban_${VERSION}_x64-setup.exe"
    "$WIN_DIR/weimo-douban_x64-setup.exe.sig"
  )
fi
# 注意：紧邻全角字符的版本变量必须写成 ${VERSION}——macOS 自带 bash 3.2
# 的多字节解析 bug 会把后续 CJK 字符的字节吞进变量名（unbound variable）
PLATFORMS="darwin-aarch64"
[ "$WIN_PRESENT" = 1 ] && PLATFORMS="$PLATFORMS / windows-x64"
gh release create "v$VERSION" "${ASSETS[@]}" \
  --title "微墨·豆瓣 App v$VERSION" \
  --notes "微墨·豆瓣桌面 App v${VERSION}（${PLATFORMS}）。已装旧版者经托盘「检查更新」或启动自检自动升级；新安装请下载 dmg（macOS）或 x64-setup.exe（Windows）。"
echo "==> 发布完成：v${VERSION}（latest.json 已随 release 上传，默认端点即可发现新版本）"

// 从 tauri-action 生成的 latest.json 派生各加速镜像的变体清单。
// 变体之间仅资产 URL 前缀不同——归档字节与 minisign 签名完全一致，
// 因此客户端无论从哪个变体下载，验签都通过（见 src-tauri/src/updater.rs）。
//
// 用法：node scripts/gen-update-manifests.mjs [latest.json 路径]
// 产出：latest-m1.json ... latest-m5.json（与后端 MIRROR_PREFIXES[1..] 一一对应）
import { readFileSync, writeFileSync } from "node:fs";

const REPO_RELEASE_DL = "https://github.com/Ljinzhou/oii_sticker/releases/download/";
// ⚠️ 必须与 src-tauri/src/updater.rs 的 MIRROR_PREFIXES[1..] 保持同序
const MIRRORS = [
  "https://gh-proxy.org/",
  "https://v4.gh-proxy.org/",
  "https://v6.gh-proxy.org/",
  "https://cdn.gh-proxy.org/",
  "https://axisnow.gh-proxy.org/",
];

const file = process.argv[2] ?? "latest.json";
const manifest = JSON.parse(readFileSync(file, "utf8"));

if (!manifest.platforms || !Object.keys(manifest.platforms).length) {
  console.error("latest.json 缺少 platforms 字段，请确认 tauri-action 开启了 includeUpdaterJson");
  process.exit(1);
}

let rewritten = 0;
for (const [, mirror] of MIRRORS.entries()) {
  const variant = structuredClone(manifest);
  for (const platform of Object.values(variant.platforms)) {
    if (typeof platform.url === "string" && platform.url.startsWith(REPO_RELEASE_DL)) {
      platform.url = mirror + platform.url;
      rewritten++;
    }
  }
  const out = `latest-m${MIRRORS.indexOf(mirror) + 1}.json`;
  writeFileSync(out, JSON.stringify(variant));
  console.log("generated:", out);
}
if (rewritten === 0) {
  console.error("警告：没有任何资产 URL 被改写——检查 latest.json 的 url 是否为 GitHub releases/download 直链");
  process.exit(1);
}

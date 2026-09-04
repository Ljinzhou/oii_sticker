// 版本号单一来源：package.json。
// 用法（在仓库根运行）：
//   node scripts/bump-version.mjs 0.3.0   # 升级版本：写 package.json，并同步 Cargo.toml / Cargo.lock
//   node scripts/bump-version.mjs sync    # 仅做一次同步：把 package.json 的当前版本写入 Cargo 侧
//   node scripts/bump-version.mjs check   # 校验 package.json / Cargo.toml / tauri.conf.json 派生的版本一致（CI 用）
//
// 原理：
//   - tauri.conf.json 的 "version" 指向 "../package.json"，Tauri 自动读取根 package.json 的 version，无需改动。
//   - Cargo.toml 的 version 是 Rust 编译期静态值，无法引用外部文件，因此这里用脚本在改动 package.json 时同步过去。
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const thisDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(thisDir, "..");

const packageJsonPath = join(root, "package.json");
const tauriConfPath = join(root, "src-tauri", "tauri.conf.json");
const cargoTomlPath = join(root, "src-tauri", "Cargo.toml");
const cargoLockPath = join(root, "src-tauri", "Cargo.lock");

const readJson = (p) => JSON.parse(readFileSync(p, "utf8"));

/** 从 Cargo.toml 的 [package] 段读取 version。 */
function getCargoVersion() {
  const text = readFileSync(cargoTomlPath, "utf8");
  const m = text.match(/^\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m);
  if (!m) throw new Error("Cargo.toml 中未找到 [package].version");
  return m[1];
}

/** 解析 tauri.conf.json 最终生效的版本（可能直接是版本号，也可能是指向 package.json 的相对路径）。 */
function getTauriVersion() {
  const conf = readJson(tauriConfPath);
  const v = conf.version;
  if (typeof v !== "string" || v === "") {
    throw new Error("tauri.conf.json 中缺少 version 字段");
  }
  // 形如 "../package.json" 表示引用外部 package.json 的 version 字段
  if (v.endsWith(".json")) {
    const target = resolve(dirname(tauriConfPath), v);
    if (!existsSync(target)) throw new Error(`tauri.conf.json 引用的文件不存在: ${target}`);
    const nested = readJson(target);
    if (typeof nested.version !== "string") {
      throw new Error(`tauri.conf.json 引用的 ${target} 中没有 version 字段`);
    }
    return nested.version;
  }
  return v;
}

/** 把 version 写入 Cargo.toml 的 [package] 段。 */
function writeCargoVersion(version) {
  let text = readFileSync(cargoTomlPath, "utf8");
  const replaced = text.replace(
    /^\[package\][\s\S]*?^version\s*=\s*"[^"]+"/m,
    (m) => m.replace(/version\s*=\s*"[^"]+"/, `version = "${version}"`)
  );
  if (replaced === text) throw new Error("Cargo.toml 同步失败：未命中 [package].version");
  writeFileSync(cargoTomlPath, replaced, "utf8");
}

/** 把 version 写入 Cargo.lock 中 name = "oii_sticker" 所在块的 version。 */
function writeCargoLockVersion(version) {
  const lines = readFileSync(cargoLockPath, "utf8").split("\n");
  let found = false;
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].trim() === 'name = "oii_sticker"') {
      for (let j = i + 1; j < lines.length && j <= i + 6; j++) {
        if (/^\s*version\s*=\s*"/.test(lines[j])) {
          lines[j] = lines[j].replace(/version\s*=\s*"[^"]+"/, `version = "${version}"`);
          found = true;
          break;
        }
        if (lines[j].trim() === "[[package]]" || j === i + 6) break;
      }
      break;
    }
  }
  if (!found) throw new Error('Cargo.lock 同步失败：未找到 name = "oii_sticker" 的版本行');
  writeFileSync(cargoLockPath, lines.join("\n"), "utf8");
}

function check() {
  const pkg = readJson(packageJsonPath);
  const fromPkg = pkg.version;
  const fromCargo = getCargoVersion();
  const fromTauri = getTauriVersion();
  const ok = fromPkg === fromCargo && fromPkg === fromTauri;
  console.log(`package.json      : ${fromPkg}`);
  console.log(`Cargo.toml        : ${fromCargo}`);
  console.log(`tauri.conf.json   : ${fromTauri}`);
  if (ok) {
    console.log("✅ 三处版本一致");
    return true;
  }
  console.error("❌ 版本不一致，请编辑 package.json 后运行 `node scripts/bump-version.mjs sync` 同步。");
  return false;
}

const arg = process.argv[2];

if (arg === "check") {
  if (!check()) process.exit(1);
} else if (arg === "sync") {
  const v = readJson(packageJsonPath).version;
  writeCargoVersion(v);
  writeCargoLockVersion(v);
  console.log(`✅ 已将 Cargo.toml / Cargo.lock 同步为 ${v}`);
} else if (arg) {
  // 直接给出版本号：先写 package.json，再同步 Cargo
  const newVersion = String(arg).startsWith("v") ? String(arg).slice(1) : String(arg);
  const pkg = readJson(packageJsonPath);
  pkg.version = newVersion;
  writeFileSync(packageJsonPath, JSON.stringify(pkg, null, 2) + "\n", "utf8");
  writeCargoVersion(newVersion);
  writeCargoLockVersion(newVersion);
  console.log(`✅ 版本已升级为 ${newVersion}（package.json / Cargo.toml / Cargo.lock）`);
  console.log("   tauri.conf.json 通过引用 package.json 自动跟随，无需改动。");
} else {
  console.error("用法:");
  console.error("  node scripts/bump-version.mjs 0.3.0   升级到指定版本并同步");
  console.error("  node scripts/bump-version.mjs sync    把 package.json 版本同步到 Cargo");
  console.error("  node scripts/bump-version.mjs check   校验三处版本是否一致");
  process.exit(1);
}
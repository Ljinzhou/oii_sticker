import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import "./styles/global.css";
// remixicon 矢量图标库（所有界面图标统一使用）
import "remixicon/fonts/remixicon.css";
import { showBootFailure } from "./utils/boot-failure";
// highlight.js 浅色主题（便签背景为浅色系；代码块底色在 MarkdownView 中置透明）
import "highlight.js/styles/github.css";

// 全局禁用 WebView2 默认右键菜单（返回/刷新/检查等浏览器菜单）。
// display 模式的右键双击唤醒由 Rust 侧全局鼠标钩子处理，不受影响。
document.addEventListener("contextmenu", (e) => e.preventDefault());

// 全局禁用所有「浏览器/WebView 功能」快捷键（下载页、新标签、查看源码、打印、
// 历史、地址栏、开发者工具等）。保留应用自身快捷键：Ctrl+S 保存、
// Ctrl+B / Ctrl+I 加粗斜体、Ctrl+Shift+X 删除线、Ctrl+Z / X / C / V / A 编辑，
// 以及方向键。用 capture 阶段拦截，先于页面内 keydown 处理器执行。
// 说明：WebView2 的自带浏览器加速键（如 Ctrl+J 下载页）在部分版本由内核层直接处理，
// 页面 keydown 可能无法完全抑制；此处已尽力覆盖所有可达 DOM 的浏览器组合键。
const BLOCKED_BROWSER_LETTERS = new Set([
  "j", // 下载页面
  "t", // 新标签页
  "n", // 新窗口
  "w", // 关闭窗口
  "u", // 查看源码
  "p", // 打印
  "h", // 历史
  "d", // 收藏书签
  "l", // 地址栏
  "f", // 查找
  "o", // 打开文件
  "r", // 刷新
  "g", // 查找下一个
  "e", // 地址栏（部分内核）
  "k", // 搜索栏
]);
// 编辑/应用快捷键，不拦截：s(保存) b/i(格式) z/x/c/v/a(撤销剪切复制粘贴全选) y/Shift-z(重做)
const KEEP_EDIT_LETTERS = new Set(["s", "b", "i", "z", "x", "c", "v", "a", "y"]);
function blockBrowserShortcut(e: KeyboardEvent) {
  // F12：开发者工具（无论修饰键）
  if (e.key === "F12") {
    e.preventDefault();
    e.stopImmediatePropagation();
    return;
  }
  // Alt+左/右：前进/后退
  if (e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
    e.preventDefault();
    e.stopImmediatePropagation();
    return;
  }
  // 无 Ctrl/Meta：普通按键交给编辑器
  if (!e.ctrlKey && !e.metaKey) return;
  // Ctrl+Alt：非浏览器功能，保留
  if (e.altKey) return;
  const key = e.key.toLowerCase();
  // Ctrl+Shift 组合：先判断应用自身的 Shift 编辑快捷键放行（删除线/重做/粘贴纯文本），
  // 其余 Ctrl+Shift 组合视为浏览器功能（开发者工具/无痕窗口/重开标签/清除数据/强制刷新）拦截。
  if (e.shiftKey) {
    // 保留：Ctrl+Shift+X 删除线、Ctrl+Shift+Z 重做、Ctrl+Shift+V 粘贴纯文本
    if (["x", "z", "v"].includes(key)) return;
    // 拦截：Ctrl+Shift+J/C 开发者工具/审查元素、T 重开标签、N 无痕、Delete 清除数据、R 强制刷新（I 已放行以支持调试）
    if (["t", "n", "j", "c", "delete", "r"].includes(key)) {
      e.preventDefault();
      e.stopImmediatePropagation();
    }
    return;
  }
  // 非 Shift：应用自身的编辑快捷键放行（保存/加粗斜体/撤销剪切复制粘贴全选/重做）
  if (key.length === 1 && KEEP_EDIT_LETTERS.has(key)) return;
  // 单字母浏览器快捷键大礼包
  if (key.length === 1 && BLOCKED_BROWSER_LETTERS.has(key)) {
    e.preventDefault();
    e.stopImmediatePropagation();
  }
}
document.addEventListener("keydown", blockBrowserShortcut, true);

window.addEventListener("error", (event) => showBootFailure(event.error ?? event.message));
window.addEventListener("unhandledrejection", (event) => showBootFailure(event.reason));

try {
  const app = createApp(App);
  app.config.errorHandler = (error) => showBootFailure(error);
  app.use(createPinia()).mount("#app");
} catch (error) {
  showBootFailure(error);
}

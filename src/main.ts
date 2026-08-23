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

window.addEventListener("error", (event) => showBootFailure(event.error ?? event.message));
window.addEventListener("unhandledrejection", (event) => showBootFailure(event.reason));

try {
  const app = createApp(App);
  app.config.errorHandler = (error) => showBootFailure(error);
  app.use(createPinia()).mount("#app");
} catch (error) {
  showBootFailure(error);
}

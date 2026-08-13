import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import "./styles/global.css";

// 全局禁用 WebView2 默认右键菜单（返回/刷新/检查等浏览器菜单）。
// display 模式的右键双击唤醒由 Rust 侧全局鼠标钩子处理，不受影响。
document.addEventListener("contextmenu", (e) => e.preventDefault());

createApp(App).use(createPinia()).mount("#app");

import { defineConfig } from "vitest/config";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath, URL } from "node:url";

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      // 测试环境没有 public 静态服务器：把模板中的绝对 public 路径映射到真实文件，
      // 避免 Vue 插件尝试把 /thanks-deepseek.png 当模块导入时报
      // "The argument 'filename' must be ... Received 'file:///thanks-deepseek.png'"。
      "/thanks-deepseek.png": fileURLToPath(
        new URL("./public/thanks-deepseek.png", import.meta.url),
      ),
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
  },
});

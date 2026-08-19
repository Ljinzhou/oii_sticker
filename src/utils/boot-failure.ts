function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** 在 Vue 入口无法完成挂载时保留可读错误，避免独立窗口只显示空白页。 */
export function showBootFailure(error: unknown): void {
  const app = document.getElementById("app");
  if (!app) return;
  let fallback = document.getElementById("boot-fallback");
  if (!fallback) {
    fallback = document.createElement("p");
    fallback.id = "boot-fallback";
    app.replaceChildren(fallback);
  }
  fallback.textContent = `窗口初始化失败：${messageOf(error)}`;
  fallback.setAttribute("role", "alert");
}

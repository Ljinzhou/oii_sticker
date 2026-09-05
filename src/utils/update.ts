// 更新进度展示辅助：整体进度条宽度与阶段文案（纯函数，便于单测）。
//
// 进度条 = 整体流程百分比（与下载字节共用前端进度条）：
//   检查/探测阶段     → 不确定（null，前端显示流动动画）
//   下载阶段          → downloaded/total 映射到 0–90%
//   安装阶段          → 95%（on_before_exit 后进程即将退出）
//   重启阶段          → 100%
//   已最新/失败/待机   → null（不显示百分比）

export type UpdateStagePhase =
  | "idle"
  | "checking"
  | "up_to_date"
  | "available"
  | "downloading"
  | "installing"
  | "restarting"
  | "failed";

/** 整体进度百分比（0–100）；null = 不确定/不显示。 */
export function updateBarWidth(
  phase: string,
  downloaded: number | null,
  total: number | null,
): number | null {
  switch (phase) {
    case "downloading": {
      if (total === null || total <= 0 || downloaded === null) return null;
      return Math.min(90, Math.round((downloaded / total) * 90));
    }
    case "installing":
      return 95;
    case "restarting":
      return 100;
    default:
      return null;
  }
}

/** 阶段提示文案（进度条下方）。 */
export function updateStageText(
  phase: string,
  pct: number | null,
  retrying: boolean,
): string {
  switch (phase) {
    case "checking":
      return "正在检查更新…";
    case "downloading":
      return retrying
        ? `下载中（已切换镜像重试）${pct !== null ? ` ${pct}%` : ""}`
        : `正在下载更新包…${pct !== null ? ` ${pct}%` : ""}`;
    case "installing":
      return "正在安装…";
    case "restarting":
      return "安装完成，应用即将重启…";
    default:
      return "";
  }
}

/** 下载进度的独立百分比（0–100，用于按钮内显示）。 */
export function downloadPercent(downloaded: number | null, total: number | null): number | null {
  if (downloaded === null || total === null || total <= 0) return null;
  return Math.min(99, Math.round((downloaded / total) * 100));
}
import { StateEffect } from "@codemirror/state";

/** 通知 Live Preview decoration 重新读取异步渲染器（例如 MathJax）。 */
export const refreshLivePreview = StateEffect.define<null>();

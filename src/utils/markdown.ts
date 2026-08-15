// markdown-it 封装：渲染 + todo 源行映射 + 数学公式（mathjax 离线 SVG）
import MarkdownIt from "markdown-it";
import { ref } from "vue";
import { createMathjaxInstance, mathjax } from "@mdit/plugin-mathjax";

// ═══════════════ SVG 动态字体预加载 ═══════════════
// mdit-mathjax 的 asyncLoad 用动态 import(e) 加载字体模块（如
// '@mathjax/mathjax-newcm-font/js/svg/dynamic/latin.js'），vite 无法静态分析
// 该变量路径，运行时浏览器解析裸说明符失败 → 公式渲染中断。
// 修复：静态导入全部 SVG 动态字体（副作用 dynamicSetup 注册字形数据），
// 并在 mdit 初始化完成后覆盖 mathjax 单例的 asyncLoad 直接返回已解析。
// 与 mdit 插件解析到同一文件（mjs/mathjax.js 单例），此处经 "./*" 通配直通路径
import { mathjax as mathjaxCore } from "@mathjax/src/mjs/mathjax.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/accents.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/accents-b-i.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/arabic.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/arrows.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/braille.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/braille-d.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/calligraphic.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/cherokee.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/cyrillic.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/cyrillic-ss.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/devanagari.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/double-struck.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/fraktur.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/greek.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/greek-ss.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/hebrew.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/latin.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/latin-b.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/latin-bi.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/latin-i.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/marrows.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/math.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/monospace.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/monospace-ex.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/monospace-l.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/mshapes.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/phonetics.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/phonetics-ss.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/PUA.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/sans-serif.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/sans-serif-b.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/sans-serif-bi.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/sans-serif-ex.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/sans-serif-i.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/sans-serif-r.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/script.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/shapes.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/symbols.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/symbols-b-i.js";
import "@mathjax/mathjax-newcm-font/js/svg/dynamic/variants.js";

// ═══════════════ 数学公式（@mdit/plugin-mathjax，离线 SVG 输出） ═══════════════
// createMathjaxInstance 为异步（sync 入口是 Node 专用，浏览器构建必须用异步版）。
// 初始化完成前 $..$ 按普通文本输出，就绪后 mathVersion+1 触发视图重渲染。
type MathjaxInstance = Awaited<ReturnType<typeof createMathjaxInstance>>;

let mathInst: MathjaxInstance | null = null;
/** mathjax 就绪版本号：就绪后 +1，渲染视图据此重渲染公式。 */
export const mathVersion = ref(0);

/** 转义 HTML 属性（data-tex 内嵌 tex 源）。 */
export function escapeAttr(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/"/g, "&quot;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

// markdown-it 渲染规则的宽松签名（其官方类型带 Env，与自定义包裹互操作不便）
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type RendererRule = (tokens: any[], idx: number, options: any, env: any, self: any) => string;

/**
 * 包裹 math 渲染规则：外层加 span/div（class=math-inline|math-block、
 * contenteditable=false、data-tex 保留 tex 源），供即时预览编辑与 turndown 回写。
 */
export function wrapMathRule(
  rule: RendererRule | undefined,
  className: string,
  tag: string,
) {
  return function (tokens: any[], idx: number, options: any, env: any, self: any): string {
    const inner = rule
      ? rule(tokens, idx, options, env, self)
      : self.renderToken(tokens, idx, options);
    const tex = tokens[idx].content;
    return `<${tag} class="${className}" contenteditable="false" data-tex="${escapeAttr(tex)}">${inner}</${tag}>`;
  };
}

// ── 渲染实例（StickerViewer / 展示 / 交互模式） ──
const md = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: false,
});

// 自写任务清单渲染：检测 `- [ ]` / `- [x]` / `- [X]` 标记，
// 输出带 data-line（源行号）的 checkbox；点击由组件事件委托处理。
const defaultListItemOpen = md.renderer.rules.list_item_open;
md.renderer.rules.list_item_open = function (tokens, idx, options, env, self) {
  const html = defaultListItemOpen
    ? defaultListItemOpen(tokens, idx, options, env, self)
    : self.renderToken(tokens, idx, options);

  // 向后找 inline token（list_item 内可能夹 paragraph_open 等）
  const inline = tokens
    .slice(idx + 1)
    .find((t) => t.type === "inline" && t.children?.length);
  if (!inline) {
    return html;
  }
  const first = inline.children![0];
  if (first.type !== "text") {
    return html;
  }
  const m = /^(\[[ xX]\])\s*/.exec(first.content);
  if (!m) {
    return html;
  }
  // 剥离任务标记文本（由 checkbox 视觉替代）
  first.content = first.content.slice(m[0].length);
  const line = tokens[idx].map ? tokens[idx].map[0] : -1;
  const checked = m[1] === "[x]" || m[1] === "[X]";
  const checkbox = `<input type="checkbox" class="task-checkbox" data-line="${line}"${
    checked ? " checked" : ""
  }>`;
  return html.replace("<li", `<li class="task-item"`) + checkbox;
};

/** 复合编号行（编辑层标记）→ 标准嵌套列表语法（渲染视图专用，不改源码）：
 *  `1.1 文本` → `  1. 文本`（嵌套深度 = 链长-1），供 markdown-it 识别为嵌套列表，
 *  编号层级由 CSS counter（counters(item, ".")）显示为 1. / 1.1 / 1.1.1。 */
export function normalizeCompoundLists(md: string): string {
  let inFence = false;
  let fenceChar = "";
  return md
    .split("\n")
    .map((line) => {
      const t = line.trimStart();
      // 围栏开关跟踪：``` / ~~~
      if (!inFence && (t.startsWith("```") || t.startsWith("~~~"))) {
        inFence = true;
        fenceChar = t.startsWith("```") ? "`" : "~";
        return line;
      }
      if (inFence && ((fenceChar === "`" && t.startsWith("```")) || (fenceChar === "~" && t.startsWith("~~~")))) {
        inFence = false;
        return line;
      }
      if (inFence) return line; // 代码块内不转换
      const m = /^(\s*)(\d+(?:\.\d+)+)\.?(\s.*)$/.exec(line);
      if (!m) return line;
      const depth = m[2].split(".").length - 1;
      return `${m[1]}${"  ".repeat(depth)}1.${m[3]}`;
    })
    .join("\n");
}

/** 渲染 markdown → HTML（含 todo checkbox 的 data-line 与 mathjax SVG；
 *  复合编号先归一化为嵌套列表语法）。 */
export function renderMarkdown(content: string): string {
  return md.render(normalizeCompoundLists(content));
}

/** 收集 mathjax 渲染产生的 SVG CSS 并清空缓存（渲染后调用，注入全局 style）。 */
export async function collectMathStyle(): Promise<string> {
  if (!mathInst) return "";
  const css = await mathInst.outputStyle();
  mathInst.clearStyle();
  return css;
}

/** 异步初始化 mathjax：完成后注册进 md，并 bump mathVersion 触发重渲染。
 *  解析为实例（或 null 表示失败），供编辑实例复用注册。 */
export const mathInstancePromise = createMathjaxInstance({
  output: "svg",
  delimiters: "dollars",
})
  .then((inst) => {
    if (!inst) {
      console.error("[math] mathjax 实例创建失败");
      return null;
    }
    mathInst = inst;
    md.use(mathjax, inst);
    // 字体已全部静态预加载（dynamicSetup 已注册字形数据），
    // 覆盖 mdit 设置的 asyncLoad（动态 import 在 vite 打包下会失败）
    mathjaxCore.asyncLoad = (name: string) => {
      console.debug(`[math] 字体已预加载，跳过动态加载：${name}`);
      return Promise.resolve({});
    };
    // 公式规则包裹（须在插件注册后覆盖，保留 data-tex）
    md.renderer.rules.math_inline = wrapMathRule(
      md.renderer.rules.math_inline,
      "math-inline",
      "span",
    );
    md.renderer.rules.math_block = wrapMathRule(
      md.renderer.rules.math_block,
      "math-block",
      "div",
    );
    mathVersion.value++;
    return inst;
  })
  .catch((e) => {
    console.error("[math] mathjax 初始化失败：", e);
    return null;
  });

/** 从 CSS 变量生成 rgba（前端偏好面板用）。 */
export function hexToRgba(hex: string, alpha: number): string {
  const m = /^#?([0-9a-fA-F]{6})$/.exec(hex.trim());
  if (!m) return `rgba(255, 255, 255, ${alpha})`;
  const n = parseInt(m[1], 16);
  const r = (n >> 16) & 0xff;
  const g = (n >> 8) & 0xff;
  const b = n & 0xff;
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

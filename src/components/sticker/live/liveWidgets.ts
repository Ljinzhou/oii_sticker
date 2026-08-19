import { EditorView, WidgetType } from "@codemirror/view";
import { renderMarkdownEditable } from "../../../utils/markdown-editable";

/** 渲染 markdown-it 片段并提取行内 HTML。 */
export function renderFragment(src: string): string {
  const html = renderMarkdownEditable(src);
  const match = /<p>(.*?)<\/p>\s*$/s.exec(html);
  return match ? match[1] : html;
}

/** 将完整 fenced 代码块渲染为语义化代码块。 */
export class CodeBlockWidget extends WidgetType {
  constructor(readonly code: string, readonly language?: string) {
    super();
  }

  eq(other: CodeBlockWidget) {
    return other.code === this.code && other.language === this.language;
  }

  toDOM() {
    const wrapper = document.createElement("div");
    const pre = document.createElement("pre");
    const code = document.createElement("code");
    wrapper.className = "live-code-block";
    if (this.language) code.className = `language-${this.language}`;
    code.textContent = this.code;
    pre.append(code);
    wrapper.append(pre);
    return wrapper;
  }

  ignoreEvent() {
    return false;
  }
}

/** 将行内 Markdown 片段替换为渲染 DOM。 */
export class InlineRenderWidget extends WidgetType {
  constructor(readonly html: string, readonly cls: string) {
    super();
  }

  eq(other: InlineRenderWidget) {
    return other.html === this.html && other.cls === this.cls;
  }

  toDOM() {
    const span = document.createElement("span");
    span.className = `live-render ${this.cls}`;
    span.innerHTML = this.html;
    return span;
  }

  ignoreEvent() {
    return false;
  }
}

/** 任务 checkbox，持有稳定的 Markdown 源范围。 */
export class TaskCheckboxWidget extends WidgetType {
  constructor(
    readonly checked: boolean,
    readonly view: EditorView,
    readonly from: number,
    readonly to: number,
  ) {
    super();
  }

  eq(other: TaskCheckboxWidget) {
    return other.checked === this.checked && other.from === this.from && other.to === this.to;
  }

  toDOM() {
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.className = "live-task-checkbox";
    checkbox.checked = this.checked;
    checkbox.setAttribute("aria-label", this.checked ? "取消任务" : "完成任务");
    checkbox.addEventListener("click", (event) => {
      event.preventDefault();
      this.view.dispatch({
        changes: {
          from: this.from,
          to: this.to,
          insert: this.checked ? "[ ]" : "[x]",
        },
      });
      this.view.focus();
    });
    return checkbox;
  }

  ignoreEvent() {
    return false;
  }
}

/** 列表 marker widget。 */
export class ListMarkWidget extends WidgetType {
  constructor(readonly text: string) {
    super();
  }

  eq(other: ListMarkWidget) {
    return other.text === this.text;
  }

  toDOM() {
    const span = document.createElement("span");
    span.className = "live-listmark";
    span.textContent = this.text;
    return span;
  }

  ignoreEvent() {
    return false;
  }
}

/** 分隔线 widget。 */
export class HrWidget extends WidgetType {
  eq() {
    return true;
  }

  toDOM() {
    const div = document.createElement("div");
    div.className = "live-hr";
    return div;
  }

  ignoreEvent() {
    return false;
  }
}

/** 标题 marker widget。 */
export class HeadingMarkWidget extends WidgetType {
  eq() {
    return true;
  }

  toDOM() {
    const span = document.createElement("span");
    span.className = "live-heading-mark";
    return span;
  }

  ignoreEvent() {
    return true;
  }
}

/** 引用 marker widget。 */
export class QuoteMarkWidget extends WidgetType {
  eq() {
    return true;
  }

  toDOM() {
    const span = document.createElement("span");
    span.className = "live-quote-mark";
    span.textContent = " ";
    return span;
  }

  ignoreEvent() {
    return true;
  }
}

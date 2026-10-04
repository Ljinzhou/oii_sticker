import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises } from "@vue/test-utils";
import {
  attachImagePaste,
  bytesToBase64,
  handleImagePaste,
  imageExt,
  imageFilesFrom,
  imageMarkdown,
  standaloneInsertion,
} from "./paste-image";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("../composables/useTauri", () => ({
  invoke: (...args: unknown[]) => mocks.invoke(...args),
  listen: vi.fn(),
}));

/** 最小 EditorView 替身：只需 state.doc / state.selection / dispatch / focus。 */
function fakeView(anchor = 3, docText = "") {
  const dispatched: Array<Record<string, unknown>> = [];
  const view = {
    state: {
      doc: { toString: () => docText, length: docText.length },
      selection: { main: { from: anchor } },
    },
    dispatch: (spec: Record<string, unknown>) => dispatched.push(spec),
    focus: vi.fn(),
    dom: document.createElement("div"),
  };
  return { view: view as never, dispatched, dom: view.dom };
}

function pasteEvent(files: File[], items?: Array<{ kind: string; type: string; getAsFile: () => File | null }>) {
  const preventDefault = vi.fn();
  const clipboardData = {
    files,
    items: items ?? files.map((file) => ({ kind: "file", type: file.type, getAsFile: () => file })),
  };
  return { event: { clipboardData, preventDefault } as unknown as ClipboardEvent, preventDefault };
}

function pngFile(bytes = [1, 2, 3]) {
  return new File([new Uint8Array(bytes)], "shot.png", { type: "image/png" });
}

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.invoke.mockResolvedValue("assets/7/粘贴-20261002-154500.png");
});

describe("剪贴板图片识别", () => {
  it("优先用 files，只有图片文件才算", () => {
    const img = pngFile();
    const txt = new File(["hi"], "a.txt", { type: "text/plain" });
    expect(imageFilesFrom({ files: [img, txt], items: [] } as unknown as DataTransfer)).toEqual([img]);
  });

  it("files 为空时回落到 items（Windows 截图粘贴的典型形态）", () => {
    const img = pngFile();
    const data = {
      files: [],
      items: [{ kind: "file", type: "image/png", getAsFile: () => img }],
    } as unknown as DataTransfer;
    expect(imageFilesFrom(data)).toEqual([img]);
  });

  it("纯文本/无数据返回空数组", () => {
    expect(imageFilesFrom(null)).toEqual([]);
    expect(imageFilesFrom({ files: [], items: [] } as unknown as DataTransfer)).toEqual([]);
  });
});

describe("扩展名与 base64", () => {
  it("MIME 优先，其次文件名，最后兜底 png", () => {
    expect(imageExt(new File([], "a", { type: "image/jpeg" }))).toBe("jpg");
    expect(imageExt(new File([], "a.webp", { type: "" }))).toBe("webp");
    expect(imageExt(new File([], "a.unknown", { type: "" }))).toBe("png");
  });

  it("字节 → base64（分块编码，长数组也不炸栈）", () => {
    expect(bytesToBase64(new Uint8Array([104, 105]))).toBe("aGk=");
    const big = new Uint8Array(70000).fill(65);
    expect(bytesToBase64(big).length).toBe(Math.ceil(70000 / 3) * 4);
  });

  it("图片 Markdown 用固定 alt 文本", () => {
    expect(imageMarkdown("assets/7/a.png")).toBe("![粘贴图片](assets/7/a.png)");
  });

  // 图片行必须独占一行：否则会和相邻文字粘在一起（`![..](..)# 标题` 会让标题失效、标题推导变脏）
  it("standaloneInsertion：按上下文补换行，光标落在插入内容之后", () => {
    const img = "![粘贴图片](assets/7/a.png)";
    // 文档开头 + 后面有内容 → 只补尾换行
    expect(standaloneInsertion("# 标题\n正文", 0, img)).toEqual({ insert: `${img}\n`, anchor: img.length + 1 });
    // 行中间 → 前后都补
    expect(standaloneInsertion("abc\ndef", 1, img)).toEqual({ insert: `\n${img}\n`, anchor: 1 + img.length + 2 });
    // 行首（前面是换行）→ 不补前缀
    expect(standaloneInsertion("abc\ndef", 4, img)).toEqual({ insert: `${img}\n`, anchor: 4 + img.length + 1 });
    // 文末且前面是换行 → 原样
    expect(standaloneInsertion("abc\n", 4, img)).toEqual({ insert: img, anchor: 4 + img.length });
    // 空文档 → 原样
    expect(standaloneInsertion("", 0, img)).toEqual({ insert: img, anchor: img.length });
  });
});

describe("粘贴处理", () => {
  it("有图片：落盘并把 Markdown 插到光标处（光标落在插入文本之后）", async () => {
    const { view, dispatched } = fakeView(3);
    const { event, preventDefault } = pasteEvent([pngFile([137, 80, 78, 71])]);

    const inserted = await handleImagePaste(event, view, 7);

    expect(preventDefault).toHaveBeenCalled();
    expect(mocks.invoke).toHaveBeenCalledWith("save_sticker_image_cmd", {
      stickerId: 7,
      ext: "png",
      dataBase64: "iVBORw==",
    });
    expect(inserted).toBe("![粘贴图片](assets/7/粘贴-20261002-154500.png)");
    expect(dispatched[0]).toMatchObject({
      changes: { from: 3, insert: "![粘贴图片](assets/7/粘贴-20261002-154500.png)" },
    });
  });

  it("没有图片：不拦截、不落盘、不插入（纯文本/链接粘贴交给编辑器默认行为）", async () => {
    const { view, dispatched } = fakeView();
    const { event, preventDefault } = pasteEvent([new File(["hi"], "a.txt", { type: "text/plain" })]);

    expect(await handleImagePaste(event, view, 7)).toBeNull();
    expect(preventDefault).not.toHaveBeenCalled();
    expect(mocks.invoke).not.toHaveBeenCalled();
    expect(dispatched).toHaveLength(0);
  });

  it("多张图片按顺序逐张插入", async () => {
    const { view, dispatched } = fakeView(0);
    const { event } = pasteEvent([pngFile(), pngFile()]);
    await handleImagePaste(event, view, 7);
    expect(mocks.invoke).toHaveBeenCalledTimes(2);
    expect(dispatched).toHaveLength(2);
  });

  it("attachImagePaste：把 paste 监听挂到编辑器 DOM 上，失败时回报错误文案", async () => {
    const { view, dom, dispatched } = fakeView();
    const onError = vi.fn();
    attachImagePaste(view, 7, onError);

    const { event } = pasteEvent([pngFile()]);
    dom.dispatchEvent(Object.assign(new Event("paste"), { clipboardData: (event as unknown as { clipboardData: unknown }).clipboardData }));
    await flushPromises();
    expect(dispatched).toHaveLength(1);

    mocks.invoke.mockRejectedValueOnce("磁盘已满");
    dom.dispatchEvent(Object.assign(new Event("paste"), { clipboardData: (event as unknown as { clipboardData: unknown }).clipboardData }));
    await flushPromises();
    expect(onError).toHaveBeenCalledWith("粘贴图片失败：磁盘已满");
  });
});

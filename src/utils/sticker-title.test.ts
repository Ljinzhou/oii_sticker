import { describe, expect, it } from "vitest";
import { extractTitleFromContent, setTitleInContent } from "./sticker-title";

describe("extractTitleFromContent", () => {
  it("第一行是一级标题时取 # 后的内容", () => {
    expect(extractTitleFromContent("# 我的标题\n\n正文")).toBe("我的标题");
  });

  it("第一行不是一级标题、第二行才是一级标题时，取第二行（# 后的内容）", () => {
    expect(extractTitleFromContent("开头正文\n# 便签标题\n\n更多内容")).toBe("便签标题");
  });

  it("多行扫描只认首个一级标题，二级标题 ## 不算", () => {
    expect(extractTitleFromContent(" ## 二级标题\n### 三级\n# 一级标题\n正文")).toBe("一级标题");
  });

  it("没有一级标题时退回首个非空行并截断 30 字符", () => {
    expect(extractTitleFromContent("\n普通开头没有井号标题\n更多")).toBe("普通开头没有井号标题");
    expect(extractTitleFromContent("a".repeat(50))).toHaveLength(30);
  });
});

describe("setTitleInContent", () => {
  it("替换第一个一级标题行", () => {
    expect(setTitleInContent("# 旧标题\n\n正文", "新标题")).toBe("# 新标题\n\n正文");
  });

  it("一级标题在第二行时替换第二行，保留第一行", () => {
    expect(setTitleInContent("开头正文\n# 旧标题\n\n更多", "新标题")).toBe("开头正文\n# 新标题\n\n更多");
  });

  it("内容没有一级标题时在开头插入 # 标题", () => {
    expect(setTitleInContent("普通正文", "新标题")).toBe("# 新标题\n\n普通正文");
  });

  it("只替换第一个一级标题，保留后面的一级标题行", () => {
    expect(setTitleInContent("# 标题一\n\n# 标题二", "换名")).toBe("# 换名\n\n# 标题二");
  });
});

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

  // 粘贴图片会往正文里插 ![](assets/..)：这种行不能当标题（否则会推导出图片路径做标题/文件名）
  it("跳过纯图片行：有标题时仍取标题", () => {
    expect(extractTitleFromContent("![粘贴图片](assets/7/a.png)\n# 我的标题\n正文")).toBe("我的标题");
  });

  it("跳过纯图片行：没有标题时取其后第一行正文", () => {
    expect(extractTitleFromContent("![粘贴图片](assets/7/a.png)\n\n随手记的内容")).toBe("随手记的内容");
    // 整篇只有图片 → 退回空串（调用方会显示「（无标题）」）
    expect(extractTitleFromContent("![粘贴图片](assets/7/a.png)")).toBe("");
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

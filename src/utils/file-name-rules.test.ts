// 名称规则镜像实现的用例：与 src-tauri/src/workspace/naming.rs 的测试一一对应，
// 保证前端提示与后端拒绝口径一致（文案也一致）。
import { describe, expect, it } from "vitest";
import {
  GROUP_LABEL,
  MAX_COMPONENT_UNITS,
  STICKER_LABEL,
  isReservedName,
  nameError,
  sanitizeLegacyName,
  stickerTitleError,
  uniqueName,
  validateName,
} from "./file-name-rules";

describe("validateName", () => {
  it("拒绝空名与控制字符", () => {
    expect(nameError(STICKER_LABEL, "")).toBe("便签名称不能为空");
    expect(nameError(STICKER_LABEL, "   ")).toBe("便签名称不能为空");
    expect(nameError(GROUP_LABEL, "a\nb")).toBe("分组名称不能包含控制字符");
    expect(nameError(GROUP_LABEL, "a\tb")).toBe("分组名称不能包含控制字符");
  });

  it("拒绝系统禁用字符（与后端同一句文案）", () => {
    for (const bad of ["a/b", "a\\b", "a:b", "a*b", "a?b", 'a"b', "a<b", "a>b", "a|b"]) {
      expect(nameError(STICKER_LABEL, bad)).toBe('便签名称不能包含 < > : " / \\ | ? * 之一');
    }
  });

  it("拒绝以句点结尾与保留设备名", () => {
    expect(nameError(STICKER_LABEL, "名字.")).toBe("便签名称不能以句点结尾");
    for (const bad of ["CON", "con", "Con.md", "nul", "LPT1", "com9.txt", "AUX "]) {
      expect(nameError(GROUP_LABEL, bad)).toBe(`「${bad.trim()}」是系统保留名称，不能用作分组名称`);
    }
    expect(nameError(GROUP_LABEL, "console")).toBeNull();
    expect(nameError(GROUP_LABEL, "COM10")).toBeNull();
  });

  it("长度上限 255 个 UTF-16 码元", () => {
    const long = "字".repeat(MAX_COMPONENT_UNITS + 1);
    expect(nameError(STICKER_LABEL, long)).toBe("便签名称过长（最多 255 个字符）");
    expect(nameError(STICKER_LABEL, "字".repeat(MAX_COMPONENT_UNITS))).toBeNull();
  });

  it("去掉首尾空白后返回名称，中文与句点照常", () => {
    expect(validateName(STICKER_LABEL, "  随手记  ")).toEqual({ ok: true, name: "随手记" });
    expect(validateName(STICKER_LABEL, "带.句点.的")).toEqual({ ok: true, name: "带.句点.的" });
  });

  it("便签标题还要给 .md 留 3 个字符", () => {
    expect(stickerTitleError("字".repeat(252))).toBeNull();
    expect(stickerTitleError("字".repeat(253))).toBe("便签名称过长（最多 252 个字符）");
  });
});

describe("uniqueName", () => {
  it("撞名时追加序号，且忽略大小写", () => {
    expect(uniqueName([], "新建便签")).toBe("新建便签");
    expect(uniqueName(["新建便签"], "新建便签")).toBe("新建便签 2");
    expect(uniqueName(["新建便签", "新建便签 2"], "新建便签")).toBe("新建便签 3");
    expect(uniqueName(["Note"], "note")).toBe("note 2");
  });
});

describe("sanitizeLegacyName", () => {
  it("只替换系统真正非法的字符", () => {
    expect(sanitizeLegacyName("我的 笔记/啊？")).toBe("我的 笔记-啊？");
    expect(sanitizeLegacyName("..")).toBe("未命名");
    expect(sanitizeLegacyName("  ")).toBe("未命名");
    expect(sanitizeLegacyName("名字.")).toBe("名字");
    expect(sanitizeLegacyName("CON")).toBe("CON_");
    expect(sanitizeLegacyName("a\u0000b")).toBe("a-b");
  });
});

describe("isReservedName", () => {
  it("只看首个句点前的段，忽略大小写与尾随空格/句点", () => {
    expect(isReservedName("con")).toBe(true);
    expect(isReservedName("CON.md")).toBe(true);
    expect(isReservedName("AUX ")).toBe(true);
    expect(isReservedName("console")).toBe(false);
    expect(isReservedName("COM10")).toBe(false);
  });
});

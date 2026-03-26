import { describe, expect, it } from "vitest";
import { extractShellCommand, findComposerTrigger } from "./chatComposerTriggers";

describe("findComposerTrigger", () => {
  it("detects slash commands only on the first line", () => {
    expect(findComposerTrigger("/review", "/review".length)).toEqual({
      kind: "slash",
      trigger: "/",
      query: "review",
      replaceFrom: 0,
      replaceTo: 7,
      anchorOffset: 0,
    });
    expect(findComposerTrigger("hello /review", "hello /review".length)).toBeNull();
    expect(findComposerTrigger("\n/review", "\n/review".length)).toBeNull();
  });

  it("detects $ references under the caret", () => {
    const value = "Use $docs today";
    expect(findComposerTrigger(value, value.indexOf("$docs") + 3)).toEqual({
      kind: "reference",
      trigger: "$",
      query: "docs",
      replaceFrom: 4,
      replaceTo: 9,
      anchorOffset: 4,
    });
  });

  it("detects @ file queries under the caret", () => {
    const value = "Open @src/components/chat";
    expect(findComposerTrigger(value, value.length)).toEqual({
      kind: "file",
      trigger: "@",
      query: "src/components/chat",
      replaceFrom: 5,
      replaceTo: value.length,
      anchorOffset: 5,
    });
  });
});

describe("extractShellCommand", () => {
  it("extracts leading shell commands", () => {
    expect(extractShellCommand("!ls -la")).toBe("ls -la");
    expect(extractShellCommand("  !pwd  ")).toBe("pwd");
  });

  it("ignores non-shell input", () => {
    expect(extractShellCommand("hello")).toBeNull();
    expect(extractShellCommand("!   ")).toBeNull();
  });
});

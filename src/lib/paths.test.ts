import { describe, expect, it } from "vitest";

import {
  basename,
  isInside,
  isWindowsPath,
  joinPath,
  parentOf,
  relativeParts,
  samePath,
  separatorOf,
  splitPath,
  stripVerbatim,
  tildify,
} from "./paths";

describe("paths on macOS", () => {
  it("splits and joins", () => {
    expect(splitPath("/Users/a/Library")).toEqual({ root: "/", parts: ["Users", "a", "Library"] });
    expect(joinPath("/Users/a", "Downloads")).toBe("/Users/a/Downloads");
    expect(joinPath("/", "Users")).toBe("/Users");
    expect(separatorOf("/Users/a")).toBe("/");
    expect(isWindowsPath("/Users/a")).toBe(false);
  });

  it("finds names and parents", () => {
    expect(basename("/a/b/c.txt")).toBe("c.txt");
    expect(basename("/a/b/")).toBe("b");
    expect(basename("/")).toBe("/");
    expect(parentOf("/a/b/c.txt")).toBe("/a/b");
    expect(parentOf("/a")).toBe("/");
    expect(parentOf("/")).toBe("/");
  });

  it("checks containment name by name and with case", () => {
    expect(isInside("/a/b", "/a")).toBe(true);
    expect(isInside("/a/b", "/a/")).toBe(true);
    expect(isInside("/ab", "/a")).toBe(false);
    expect(isInside("/a", "/a")).toBe(false);
    expect(isInside("/A/b", "/a")).toBe(false);
    expect(isInside("/a", "/")).toBe(true);
    expect(relativeParts("/Users/a/x/y", "/Users/a")).toEqual(["x", "y"]);
    expect(relativeParts("/Users/b", "/Users/a")).toBeNull();
  });

  it("tildifies the home folder only", () => {
    expect(tildify("/Users/a/Library", "/Users/a")).toBe("~/Library");
    expect(tildify("/Users/a", "/Users/a")).toBe("~");
    expect(tildify("/Users/ab/x", "/Users/a")).toBe("/Users/ab/x");
    expect(tildify("/tmp", undefined)).toBe("/tmp");
  });
});

describe("paths on Windows", () => {
  const home = "C:\\Users\\Dev";

  it("recognises drive, UNC and verbatim paths", () => {
    expect(isWindowsPath("C:\\Users")).toBe(true);
    expect(isWindowsPath("d:/data")).toBe(true);
    expect(isWindowsPath("C:")).toBe(true);
    expect(isWindowsPath("\\\\server\\share")).toBe(true);
    expect(separatorOf("\\\\?\\C:\\Users")).toBe("\\");
    expect(stripVerbatim("\\\\?\\C:\\Very\\Long")).toBe("C:\\Very\\Long");
    expect(stripVerbatim("\\\\?\\UNC\\server\\share\\x")).toBe("\\\\server\\share\\x");
  });

  it("splits a drive root and a share root", () => {
    expect(splitPath("C:\\")).toEqual({ root: "C:\\", parts: [] });
    expect(splitPath("c:\\Users\\Dev")).toEqual({ root: "C:\\", parts: ["Users", "Dev"] });
    expect(splitPath("\\\\?\\D:\\Data\\x")).toEqual({ root: "D:\\", parts: ["Data", "x"] });
    expect(splitPath("\\\\nas\\media\\Movies")).toEqual({
      root: "\\\\nas\\media\\",
      parts: ["Movies"],
    });
  });

  it("joins without doubling the separator after a drive root", () => {
    expect(joinPath("C:\\", "Users")).toBe("C:\\Users");
    expect(joinPath("C:\\Users", "Dev")).toBe("C:\\Users\\Dev");
  });

  it("finds names and parents", () => {
    expect(basename("C:\\Users\\Dev\\file.txt")).toBe("file.txt");
    expect(basename("D:\\")).toBe("D:\\");
    expect(parentOf("C:\\Users\\Dev\\file.txt")).toBe("C:\\Users\\Dev");
    expect(parentOf("C:\\Users")).toBe("C:\\");
    expect(parentOf("C:\\")).toBe("C:\\");
    expect(parentOf("\\\\?\\C:\\a\\b")).toBe("C:\\a");
  });

  it("compares without case and separator style", () => {
    expect(samePath("C:\\Users\\Dev", "c:/users/dev/")).toBe(true);
    expect(samePath("C:\\", "c:\\")).toBe(true);
    expect(isInside("C:\\Users\\Dev\\x", "c:\\users\\dev")).toBe(true);
    expect(isInside("C:\\Users\\Devon", home)).toBe(false);
    expect(isInside("C:\\Users", "C:\\")).toBe(true);
    expect(isInside("D:\\Users", "C:\\")).toBe(false);
    expect(isInside("\\\\?\\C:\\Users\\Dev\\x", home)).toBe(true);
  });

  it("gives names below a drive root", () => {
    expect(relativeParts("C:\\Users\\Dev", "C:\\")).toEqual(["Users", "Dev"]);
    expect(relativeParts("C:\\", "C:\\")).toEqual([]);
    expect(relativeParts("D:\\x", "C:\\")).toBeNull();
  });

  it("tildifies the user folder with a backslash", () => {
    expect(tildify("C:\\Users\\Dev\\Downloads\\a.zip", home)).toBe("~\\Downloads\\a.zip");
    expect(tildify("c:\\users\\dev", home)).toBe("~");
    expect(tildify("\\\\?\\C:\\Users\\Dev\\AppData", home)).toBe("~\\AppData");
    expect(tildify("D:\\Games", home)).toBe("D:\\Games");
    expect(tildify("\\\\?\\D:\\Games", home)).toBe("D:\\Games");
  });
});

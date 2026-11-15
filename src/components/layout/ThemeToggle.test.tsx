import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { useApp } from "@/lib/store";

import { ThemeToggle } from "./ThemeToggle";

describe("Theme button", () => {
  it("switches between dark and light", () => {
    useApp.getState().setTheme("dark");
    render(<ThemeToggle />);
    fireEvent.click(screen.getByRole("button", { name: "Switch to light mode" }));
    expect(useApp.getState().theme).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Switch to dark mode" }));
    expect(useApp.getState().theme).toBe("dark");
  });
});

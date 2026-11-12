import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

import { api } from "@/lib/api";

import { SignupScreen } from "./SignupScreen";
import { looksLikeEmail } from "./signup";

const typeEmail = (v: string) =>
  fireEvent.change(screen.getByLabelText("Email"), { target: { value: v } });
const submit = () => fireEvent.click(screen.getByRole("button", { name: "Continue" }));

beforeEach(() => vi.restoreAllMocks());

describe("SignupScreen", () => {
  it("says exactly what is sent", () => {
    render(<SignupScreen onDone={() => {}} />);
    expect(screen.getByText(/sends your email, its version, your macOS version/)).toBeVisible();
    expect(screen.getByText(/Nothing about your files, folders or scans is sent/)).toBeVisible();
  });

  it("asks again for something that is not an email, without sending it", () => {
    const send = vi.spyOn(api, "submitSignup");
    render(<SignupScreen onDone={() => {}} />);
    typeEmail("not an email");
    submit();
    expect(screen.getByRole("alert")).toHaveTextContent("valid email");
    expect(send).not.toHaveBeenCalled();
  });

  it("signs up and opens the app", async () => {
    const signup = { email: "me@example.com", signedUpAtMs: 1, sent: true };
    const send = vi.spyOn(api, "submitSignup").mockResolvedValue(signup);
    const onDone = vi.fn();
    render(<SignupScreen onDone={onDone} />);
    typeEmail(" me@example.com ");
    submit();
    await waitFor(() => expect(onDone).toHaveBeenCalledWith(signup));
    expect(send).toHaveBeenCalledWith("me@example.com");
  });

  it("shows the engine's error and lets the person try again", async () => {
    vi.spyOn(api, "submitSignup").mockRejectedValue("Could not save the signup: disk full");
    render(<SignupScreen onDone={() => {}} />);
    typeEmail("me@example.com");
    submit();
    expect(await screen.findByRole("alert")).toHaveTextContent("disk full");
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
  });
});

describe("looksLikeEmail", () => {
  it("accepts addresses and rejects the rest", () => {
    expect(looksLikeEmail("a@b.co")).toBe(true);
    expect(looksLikeEmail("a@b")).toBe(false);
    expect(looksLikeEmail("a b@c.com")).toBe(false);
  });
});

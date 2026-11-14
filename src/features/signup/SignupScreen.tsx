import { openUrl } from "@tauri-apps/plugin-opener";
import { Loader2 } from "lucide-react";
import { type FormEvent, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api, errorMessage } from "@/lib/api";
import type { Signup } from "@/lib/types";

import { CONTACT_EMAIL, looksLikeEmail, PRIVACY_URL } from "./signup";

/**
 * Shown on first launch, before the app. DiskPilot needs an email to start,
 * so its maker can reach people about updates and feedback. What is sent is
 * spelled out here and on diskpilot.live/privacy.
 */
export function SignupScreen({ onDone }: { onDone: (s: Signup) => void }) {
  const [email, setEmail] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!looksLikeEmail(email)) {
      setError("Enter a valid email address, like name@gmail.com.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      onDone(await api.submitSignup(email));
    } catch (err) {
      setError(errorMessage(err));
      setBusy(false);
    }
  };

  return (
    <div className="flex h-full items-center justify-center bg-background p-6">
      <form
        onSubmit={(e) => void submit(e)}
        noValidate
        className="animate-in w-full max-w-[400px]"
        aria-labelledby="signup-title"
      >
        <img src="/logo.svg" alt="" className="mb-5 size-14" draggable={false} />
        <h1 id="signup-title" className="text-lg font-semibold text-foreground">
          Welcome to DiskPilot
        </h1>
        <p className="mt-2 text-[13px] leading-relaxed text-muted-foreground">
          Enter your email to get started. It's how you'll hear about new versions and fixes, and
          how I can ask what would make DiskPilot better. A few emails a year, no more.
        </p>

        <label htmlFor="signup-email" className="mt-5 block text-xs font-medium text-foreground">
          Email
        </label>
        <Input
          id="signup-email"
          type="email"
          autoComplete="email"
          autoFocus
          placeholder="you@gmail.com"
          value={email}
          onChange={(e) => {
            setEmail(e.target.value);
            setError(null);
          }}
          aria-invalid={!!error}
          aria-describedby={error ? "signup-error" : "signup-note"}
          className="mt-1.5 h-9"
          disabled={busy}
        />
        {error ? (
          <p id="signup-error" role="alert" className="mt-1.5 text-xs text-critical">
            {error}
          </p>
        ) : null}

        <Button type="submit" className="mt-4 h-9 w-full" disabled={busy}>
          {busy ? <Loader2 className="size-4 animate-spin" aria-hidden /> : null}
          {busy ? "Signing up" : "Continue"}
        </Button>

        <p id="signup-note" className="mt-4 text-xs leading-relaxed text-subtle-foreground">
          DiskPilot sends your email, its version, your macOS version and whether this Mac has Apple
          silicon or Intel to diskpilot.live. Nothing about your files, folders or scans is sent. To
          change or delete your email, write to {CONTACT_EMAIL}.{" "}
          <button
            type="button"
            className="underline underline-offset-2 hover:text-foreground"
            onClick={() => void openUrl(PRIVACY_URL)}
          >
            Privacy details
          </button>
        </p>
      </form>
    </div>
  );
}

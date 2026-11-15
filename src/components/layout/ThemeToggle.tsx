import { Moon, Sun } from "lucide-react";

import { Button } from "@/components/ui/button";
import { useApp } from "@/lib/store";

/**
 * Switches between light and dark from any page. "System" stays available in
 * Settings; a click here picks the opposite of what is showing now.
 */
export function ThemeToggle() {
  const setTheme = useApp((s) => s.setTheme);
  // Re-render when the preference changes; the class says what is showing.
  useApp((s) => s.theme);
  const dark = document.documentElement.classList.contains("dark");
  const label = dark ? "Switch to light mode" : "Switch to dark mode";

  return (
    <Button
      variant="ghost"
      size="icon"
      aria-label={label}
      title={label}
      onClick={() => setTheme(dark ? "light" : "dark")}
    >
      {dark ? <Sun /> : <Moon />}
    </Button>
  );
}

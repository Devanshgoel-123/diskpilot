/** Where the signup and what it sends are explained. */
export const PRIVACY_URL = "https://diskpilot.live/privacy";
/** Where people ask to change or delete their email. */
export const CONTACT_EMAIL = "devanshgoel.dev@gmail.com";

/** Light check before asking the engine, which checks again. */
export function looksLikeEmail(s: string): boolean {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(s.trim());
}

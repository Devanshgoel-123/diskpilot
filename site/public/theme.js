// The site is dark by default. Apply a saved light theme before the first
// paint to avoid a flash. Loaded as a file rather than inline so the
// Content-Security-Policy needs no exceptions.
try {
  if (localStorage.getItem("diskpilot-theme") === "light") {
    document.documentElement.dataset.theme = "light";
  }
} catch {
  /* storage unavailable: stay dark */
}

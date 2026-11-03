// Apply a saved theme before the first paint to avoid a flash. Loaded as a
// file rather than inline so the Content-Security-Policy needs no exceptions.
try {
  const t = localStorage.getItem("diskpilot-theme");
  if (t === "light" || t === "dark") document.documentElement.dataset.theme = t;
} catch {
  /* storage unavailable: follow the system theme */
}

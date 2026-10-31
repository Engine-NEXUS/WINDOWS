/* Sets the theme before the first paint, so a window never flashes the wrong colours while its bundle loads.
   Same rule as src/sidebar/theme.ts: the saved choice is "light", "dark" or "system" (follow Windows); anything else is dark.
   An external file (not inline) so the Content-Security-Policy needs no exception. Keep it tiny and dependency-free. */
(function () {
  try {
    var saved = localStorage.getItem("nexus-theme-mode");
    var osDark = !window.matchMedia || window.matchMedia("(prefers-color-scheme: dark)").matches;
    var mode = saved === "light" ? "light" : saved === "system" ? (osDark ? "dark" : "light") : "dark";
    var root = document.documentElement;
    root.setAttribute("data-theme", mode);
    root.style.colorScheme = mode;
  } catch (e) {
    /* storage blocked: the stylesheet default (dark) applies */
  }
})();

// Apply the saved choice before the stylesheet loads. Monolith starts in light mode.
(() => {
  const key = "findout.theme.v1";
  let light = true;
  try { light = localStorage.getItem(key) !== "dark"; } catch { /* Use the desktop default. */ }

  function applyTheme() {
    document.documentElement.dataset.theme = light ? "light" : "dark";
    document.querySelector('meta[name="theme-color"]').content = light ? "#ffffff" : "#1a1a1a";
    document.querySelector('meta[name="color-scheme"]').content = light ? "light" : "dark";
    const button = document.querySelector("#themeButton");
    if (button) {
      button.setAttribute("aria-pressed", String(light));
      button.lastElementChild.textContent = light ? "✓" : "○";
    }
  }

  applyTheme();
  document.addEventListener("DOMContentLoaded", () => {
    applyTheme();
    document.querySelector("#themeButton")?.addEventListener("click", () => {
      light = !light;
      applyTheme();
      try { localStorage.setItem(key, light ? "light" : "dark"); } catch { /* Keep the session choice. */ }
    });
  });
})();

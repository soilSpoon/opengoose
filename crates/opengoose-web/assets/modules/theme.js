const themeButtonSelector = "[data-theme-toggle]";
const themeColorMetaSelector = 'meta[name="theme-color"]';

const syncThemeButton = (button) => {
  if (!button) return;

  const isDark = document.documentElement.dataset.theme === "dark";
  const nextLabel = isDark ? "light" : "dark";
  const currentLabel = isDark ? "Dark" : "Light";

  button.setAttribute("aria-pressed", isDark ? "true" : "false");
  button.setAttribute("aria-label", `Switch to ${nextLabel} theme`);
  button.title = `Switch to ${nextLabel} theme`;
  button.dataset.theme = isDark ? "dark" : "light";
  button.textContent = currentLabel;
};

const applyTheme = (theme) => {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
  document
    .querySelector(themeColorMetaSelector)
    ?.setAttribute("content", theme === "dark" ? "#111418" : "#f4efe4");
  localStorage.setItem("opengoose-theme", theme);
};

export const initTheme = (root = document) => {
  const button = root.querySelector(themeButtonSelector);
  if (!button || button.dataset.enhanced === "true") {
    syncThemeButton(button);
    return;
  }

  button.dataset.enhanced = "true";
  button.addEventListener("click", () => {
    const next =
      document.documentElement.dataset.theme === "dark" ? "light" : "dark";
    applyTheme(next);
    syncThemeButton(button);
  });

  syncThemeButton(button);
};

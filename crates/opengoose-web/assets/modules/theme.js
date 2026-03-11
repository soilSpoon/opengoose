const themeButtonSelector = "[data-theme-toggle]";

const syncThemeButton = (button) => {
  if (!button) return;

  const isDark = document.documentElement.dataset.theme === "dark";
  button.setAttribute("aria-pressed", isDark ? "true" : "false");
  button.textContent = isDark ? "Theme: Dark" : "Theme: Light";
};

const applyTheme = (theme) => {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
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

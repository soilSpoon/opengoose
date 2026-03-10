(() => {
  const savedTheme = localStorage.getItem("opengoose-theme");
  const theme =
    savedTheme ||
    (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");

  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
})();

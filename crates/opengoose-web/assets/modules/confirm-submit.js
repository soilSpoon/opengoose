export const initConfirmSubmits = (root = document) => {
  root.addEventListener("submit", (event) => {
    const form = event.target;
    if (!(form instanceof HTMLFormElement)) return;

    const message = form.dataset.confirm;
    if (!message) return;

    if (!window.confirm(message)) {
      event.preventDefault();
    }
  });
};

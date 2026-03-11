export function initWorkflowTriggers(root = document) {
  const forms = root.querySelectorAll("[data-workflow-trigger]");

  forms.forEach((form) => {
    if (form.dataset.workflowTriggerBound === "true") {
      return;
    }
    form.dataset.workflowTriggerBound = "true";

    const status = form.querySelector("[data-trigger-status]");
    const button = form.querySelector("[data-trigger-submit]");
    const input = form.querySelector("textarea[name='input']");

    form.addEventListener("submit", async (event) => {
      event.preventDefault();

      if (button) {
        button.disabled = true;
      }
      if (status) {
        status.textContent = "Submitting manual run request…";
      }

      try {
        const response = await fetch(form.action, {
          method: "POST",
          headers: {
            "content-type": "application/json",
            accept: "application/json",
          },
          body: JSON.stringify({
            input: input?.value ?? "",
          }),
        });

        const payload = await response.json().catch(() => ({}));
        if (!response.ok) {
          throw new Error(payload.error ?? "Workflow trigger failed.");
        }

        if (status) {
          status.textContent = `${payload.workflow ?? "Workflow"} queued. Check Runs for live progress.`;
        }
      } catch (error) {
        if (status) {
          status.textContent =
            error instanceof Error ? error.message : "Workflow trigger failed.";
        }
      } finally {
        if (button) {
          button.disabled = false;
        }
      }
    });
  });
}

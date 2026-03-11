export function initRemoteAgentActions(root = document) {
  const buttons = root.querySelectorAll("[data-remote-agent-disconnect]");
  const status =
    document.querySelector("[data-remote-agents-status]") ||
    root.querySelector("[data-remote-agents-status]");

  buttons.forEach((button) => {
    if (button.dataset.remoteAgentBound === "true") {
      return;
    }
    button.dataset.remoteAgentBound = "true";

    button.addEventListener("click", async () => {
      const agentName = button.dataset.agentName || "agent";
      const url = button.dataset.disconnectUrl;
      if (!url) {
        if (status) {
          status.textContent = "Disconnect URL missing for the selected agent.";
        }
        return;
      }

      button.disabled = true;
      if (status) {
        status.textContent = `Disconnecting ${agentName}…`;
      }

      try {
        const response = await fetch(url, {
          method: "DELETE",
          headers: {
            accept: "text/plain",
          },
        });
        const message = await response.text();

        if (!response.ok) {
          throw new Error(message || `Disconnect failed for ${agentName}.`);
        }

        if (status) {
          status.textContent = message || `${agentName} disconnected. Refreshing…`;
        }
        window.location.reload();
      } catch (error) {
        if (status) {
          status.textContent =
            error instanceof Error
              ? error.message
              : `Disconnect failed for ${agentName}.`;
        }
        button.disabled = false;
      }
    });
  });
}

const LOGIN_TRIGGER_SELECTOR = "[data-openausweis-login]";

function setupLoginBridge() {
  const trigger = document.querySelector(LOGIN_TRIGGER_SELECTOR);
  if (!trigger) {
    return;
  }

  trigger.addEventListener("click", async () => {
    // Background validates this origin against local policy before forwarding.
    const response = await chrome.runtime.sendMessage({
      type: "START_SESSION",
      relying_party: window.location.origin,
    });

    if (!response?.ok) {
      console.warn("OpenAusweis bridge error", response?.error);
      return;
    }

    if (response.response?.type === "ERROR") {
      console.warn("OpenAusweis daemon error", response.response?.data?.message);
    }
  });
}

setupLoginBridge();

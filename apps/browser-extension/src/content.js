const LOGIN_TRIGGER_SELECTOR = "[data-openausweis-login]";

function getExtensionApi() {
  if (typeof chrome !== "undefined" && chrome?.runtime) {
    return chrome;
  }

  if (typeof browser !== "undefined" && browser?.runtime) {
    return browser;
  }

  throw new Error("Browser extension runtime API is not available");
}

const EXT_API = getExtensionApi();

function setupLoginBridge() {
  const trigger = document.querySelector(LOGIN_TRIGGER_SELECTOR);
  if (!trigger) {
    return;
  }

  trigger.addEventListener("click", async () => {
    let response;
    try {
      // Background validates this origin against local policy before forwarding.
      response = await EXT_API.runtime.sendMessage({
        type: "START_SESSION",
        relying_party: window.location.origin,
      });
    } catch (error) {
      response = { ok: false, error: String(error) };
    }

    // Dispatch a CustomEvent so demo pages (and relying parties) can observe the outcome.
    window.dispatchEvent(new CustomEvent("openausweis:response", { detail: response }));

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

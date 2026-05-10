(async () => {
  const root = typeof browser !== "undefined" ? browser : chrome;
  await import(root.runtime.getURL("src/background.js"));
})();

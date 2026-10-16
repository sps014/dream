// Cmd+K / Ctrl+K opens search, matching most documentation sites. Material already binds "/" and "s".
document.addEventListener("keydown", (event) => {
  if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "k") {
    return;
  }
  const toggle = document.querySelector('[data-md-toggle="search"]');
  if (!toggle) {
    return;
  }
  event.preventDefault();
  toggle.checked = true;
  toggle.dispatchEvent(new Event("change"));
  document.querySelector("[data-md-component=search-query]")?.focus();
});

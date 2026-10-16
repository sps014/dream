// Builds the status / target / text filter above a grouped directory. Rows carry data-status and
// data-targets from front matter, so new values only need a label here to get a button.
const FILTERS = {
  status: { label: "Status", values: { stable: "Stable", experimental: "Experimental", planned: "Planned" } },
  targets: { label: "Runs on", values: { native: "Native", web: "Browser", node: "Node" } },
};

function setupFilter(root) {
  const article = root.closest("article") || document;
  const rows = [...article.querySelectorAll(".dream-index__item")];
  if (!rows.length) {
    return;
  }
  const state = { status: "", targets: "", text: "" };

  const search = document.createElement("input");
  search.type = "search";
  search.className = "dream-filter__search";
  search.placeholder = "Filter features…";
  search.setAttribute("aria-label", "Filter features");
  root.append(search);

  for (const [key, filter] of Object.entries(FILTERS)) {
    const present = new Set(rows.flatMap((row) => (row.dataset[key] || "").split(" ").filter(Boolean)));
    const values = Object.entries(filter.values).filter(([value]) => present.has(value));
    if (values.length < 2) {
      continue;
    }
    const group = document.createElement("div");
    group.className = "dream-filter__group";
    group.setAttribute("role", "group");
    group.setAttribute("aria-label", filter.label);
    const label = document.createElement("span");
    label.className = "dream-filter__label";
    label.textContent = filter.label;
    group.append(label);
    for (const [value, text] of [["", "All"], ...values]) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "dream-filter__chip";
      button.textContent = text;
      button.setAttribute("aria-pressed", String(value === ""));
      button.addEventListener("click", () => {
        state[key] = value;
        for (const sibling of group.querySelectorAll("button")) {
          sibling.setAttribute("aria-pressed", String(sibling === button));
        }
        apply();
      });
      group.append(button);
    }
    root.append(group);
  }

  const empty = document.createElement("p");
  empty.className = "dream-filter__empty";
  empty.textContent = "No features match this filter.";
  empty.hidden = true;
  root.after(empty);

  function apply() {
    let shown = 0;
    for (const row of rows) {
      const matches =
        (!state.status || row.dataset.status === state.status) &&
        (!state.targets || (row.dataset.targets || "").split(" ").includes(state.targets)) &&
        (!state.text || row.textContent.toLowerCase().includes(state.text));
      row.hidden = !matches;
      shown += matches ? 1 : 0;
    }
    for (const group of article.querySelectorAll(".dream-directory__group")) {
      group.hidden = ![...group.querySelectorAll(".dream-index__item")].some((row) => !row.hidden);
    }
    empty.hidden = shown > 0;
  }

  search.addEventListener("input", () => {
    state.text = search.value.trim().toLowerCase();
    apply();
  });
}

document$.subscribe(() => {
  for (const root of document.querySelectorAll("[data-dream-filter]")) {
    if (!root.childElementCount) {
      setupFilter(root);
    }
  }
});

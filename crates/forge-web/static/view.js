// The result lists (addons, packs): tiles or a list, remembered in this browser, and a message
// when a search cannot reload them. Loaded in the head, so the saved view is set on <html>
// before the page is drawn and survives htmx swaps.
(function () {
  var key = "forge-view";
  function apply(view) {
    view = view === "list" ? "list" : "tiles";
    document.documentElement.setAttribute("data-view", view);
    mark();
  }
  // The pressed state for screen readers; the buttons come again with every htmx swap.
  function mark() {
    var view = document.documentElement.getAttribute("data-view");
    document.querySelectorAll("[data-set-view]").forEach(function (button) {
      button.setAttribute("aria-pressed", String(button.getAttribute("data-set-view") === view));
    });
  }
  document.addEventListener("DOMContentLoaded", mark);
  document.addEventListener("htmx:afterSwap", mark);
  try {
    apply(localStorage.getItem(key));
  } catch (e) {
    apply("tiles");
  }
  document.addEventListener("click", function (event) {
    var button = event.target.closest("[data-set-view]");
    if (!button) return;
    var view = button.getAttribute("data-set-view");
    apply(view);
    try {
      localStorage.setItem(key, view);
    } catch (e) {}
  });

  // Copy buttons next to read-only fields (the pack link). Hidden until this script runs.
  document.addEventListener("DOMContentLoaded", function () {
    document.querySelectorAll("[data-copy]").forEach(function (button) {
      button.hidden = false;
    });
  });
  document.addEventListener("click", function (event) {
    var button = event.target.closest("[data-copy]");
    if (!button) return;
    var id = button.getAttribute("data-copy");
    var field = document.getElementById(id);
    var status = document.querySelector('[data-copied="' + id + '"]');
    function say(text) {
      if (status) status.textContent = text;
    }
    function manual() {
      field.select();
      say("Press Ctrl+C to copy the selected link.");
    }
    if (!navigator.clipboard) return manual();
    navigator.clipboard.writeText(field.value).then(function () {
      say("Link copied.");
    }, manual);
  });

  // "Install with the launcher" opens survivorsforge://. A browser cannot tell whether that worked,
  // but an opened app takes the focus; when it stays here, point out where to get the launcher.
  document.addEventListener("click", function (event) {
    var link = event.target.closest("[data-launcher]");
    var note = document.querySelector("[data-launcher-missing]");
    if (!link || !note) return;
    var left = false;
    function away() {
      left = true;
    }
    window.addEventListener("blur", away, { once: true });
    setTimeout(function () {
      window.removeEventListener("blur", away);
      if (left || document.hidden) return;
      note.setAttribute("role", "status");
      note.className = "notice warn";
      note.firstChild.textContent = "Nothing opened? The launcher is probably not installed yet. ";
    }, 2000);
  });

  // htmx keeps the old results when a reload fails; say so above them.
  function failed(event) {
    var target = event.detail && event.detail.target;
    if (!target || target.id !== "results") return;
    var note = target.querySelector(".load-error");
    if (!note) {
      note = document.createElement("div");
      note.className = "notice bad load-error";
      note.setAttribute("role", "alert");
      target.prepend(note);
    }
    note.textContent = "Could not load the results. Check your connection and try again.";
  }
  document.addEventListener("htmx:responseError", failed);
  document.addEventListener("htmx:sendError", failed);
})();

/* Issues V2: filtering and disclosure without inline scripts (CSP safe). */
document.addEventListener("DOMContentLoaded", function () {
  var list = document.getElementById("addon-issues-list");
  if (!list) return;
  var rows = Array.from(list.querySelectorAll(".issue-row"));
  var tabs = Array.from(document.querySelectorAll("[data-inline-filter]"));
  var search = document.getElementById("addon-issues-search");
  var empty = document.getElementById("addon-issues-empty");
  var filter = "open";
  function refresh() {
    var query = (search && search.value || "").trim().toLowerCase();
    var visible = 0;
    rows.forEach(function (row) {
      var status = row.dataset.status;
      var matched = filter === "all" || (filter === "open" ? status === "open" || status === "in_progress" : status === "closed" || status === "resolved");
      var show = matched && (row.dataset.search || "").toLowerCase().includes(query);
      row.hidden = !show;
      if (show) visible++;
    });
    if (empty) empty.hidden = visible > 0 || rows.length === 0;
    tabs.forEach(function (tab) {
      var active = tab.dataset.inlineFilter === filter;
      tab.classList.toggle("active", active);
      tab.setAttribute("aria-pressed", String(active));
    });
  }
  tabs.forEach(function (tab) { tab.addEventListener("click", function () { filter = tab.dataset.inlineFilter; refresh(); }); });
  if (search) search.addEventListener("input", refresh);
  refresh();
  document.querySelectorAll("[data-open-new-issue]").forEach(function (link) {
    link.addEventListener("click", function () {
      var details = document.getElementById("new-issue");
      if (details) details.open = true;
    });
  });
});

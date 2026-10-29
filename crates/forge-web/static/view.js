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

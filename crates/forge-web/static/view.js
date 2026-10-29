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

  // Screenshots in the editor: dragged by their handle, the new order is saved at once. The arrow
  // buttons stay for the keyboard and without script.
  var dragged = null;
  document.addEventListener("mousedown", function (event) {
    var handle = event.target.closest("[data-drag-handle]");
    if (handle) handle.closest("[data-image-id]").draggable = true;
  });
  document.addEventListener("dragstart", function (event) {
    var figure = event.target.closest && event.target.closest("[data-image-id]");
    if (!figure || !figure.draggable) return;
    dragged = figure;
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", figure.getAttribute("data-image-id"));
    figure.classList.add("dragging");
  });
  document.addEventListener("dragover", function (event) {
    if (!dragged) return;
    event.preventDefault();
    var over = event.target.closest("[data-image-id]");
    if (!over || over === dragged || over.parentNode !== dragged.parentNode) return;
    var box = over.getBoundingClientRect();
    over.parentNode.insertBefore(dragged, event.clientX > box.left + box.width / 2 ? over.nextSibling : over);
  });
  document.addEventListener("drop", function (event) {
    if (dragged) event.preventDefault();
  });
  document.addEventListener("dragend", function () {
    if (!dragged) return;
    var figure = dragged;
    dragged = null;
    figure.classList.remove("dragging");
    figure.draggable = false;
    var list = figure.parentNode;
    var ids = Array.prototype.map.call(list.querySelectorAll("[data-image-id]"), function (f) {
      return f.getAttribute("data-image-id");
    });
    var body = new URLSearchParams({ order: ids.join(","), csrf: document.querySelector('input[name="csrf"]').value });
    fetch(list.getAttribute("data-order-url"), { method: "POST", body: body }).then(function () {
      location.reload();
    });
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

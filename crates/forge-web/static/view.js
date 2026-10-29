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

/* Dynamic issue navigation: progressively enhances ordinary links and forms.
   The server still owns authorization, validation and CSRF checks. */
(function () {
  var busy = false;
  function panel() { return document.querySelector("[data-dynamic-issues]"); }
  async function navigate(url, options, push) {
    if (busy) return;
    busy = true;
    var root = panel();
    if (root) root.setAttribute("aria-busy", "true");
    try {
      var response = await fetch(url, Object.assign({credentials: "same-origin", headers: {"Accept": "text/html"}}, options || {}));
      if (!response.ok) throw new Error("Request failed (" + response.status + ")");
      if (options && options.method === "POST" && !response.redirected) throw new Error("The update was not confirmed by the server");
      var doc = new DOMParser().parseFromString(await response.text(), "text/html");
      var next = doc.querySelector("[data-dynamic-issues]");
      if (!next) {
        window.location.assign(response.url);
        return;
      }
      var current = panel();
      if (!current) return;
      current.replaceWith(next);
      if (push) history.pushState({issuePanel: true}, "", response.url);
      setup();
      var detail = document.getElementById("issue-detail");
      if (detail) detail.scrollIntoView({block: "nearest"});
    } catch (error) {
      if (root) {
        root.removeAttribute("aria-busy");
        var message = root.querySelector("[data-issue-error]");
        if (!message) {
          message = document.createElement("p");
          message.className = "notice bad";
          message.setAttribute("role", "alert");
          message.dataset.issueError = "";
          root.prepend(message);
        }
        message.textContent = error.message + ". Please retry.";
      }
    } finally { busy = false; }
  }
  function setup() {
    var root = panel();
    if (!root) return;
    var rows = Array.from(root.querySelectorAll(".issue-row"));
    var tabs = Array.from(root.querySelectorAll("[data-inline-filter]"));
    var search = root.querySelector("#addon-issues-search");
    var empty = root.querySelector("#addon-issues-empty");
    var filter = "open";
    function refresh() {
      var query = (search && search.value || "").trim().toLowerCase(), count = 0;
      rows.forEach(function (row) {
        var status = row.dataset.status;
        var match = filter === "all" || (filter === "open" ? status === "open" || status === "acknowledged" || status === "in_progress" : status === "closed" || status === "resolved");
        row.hidden = !(match && (row.dataset.search || "").toLowerCase().includes(query));
        if (!row.hidden) count++;
      });
      if (empty) empty.hidden = count > 0 || rows.length === 0;
      tabs.forEach(function (tab) {
        var active = tab.dataset.inlineFilter === filter;
        tab.classList.toggle("active", active);
        tab.setAttribute("aria-pressed", String(active));
      });
    }
    tabs.forEach(function (tab) { tab.addEventListener("click", function () { filter = tab.dataset.inlineFilter; refresh(); }); });
    if (search) search.addEventListener("input", refresh);
    root.querySelectorAll("[data-open-new-issue]").forEach(function (link) {
      link.addEventListener("click", function () {
        var details = root.querySelector("#new-issue");
        if (details) details.open = true;
      });
    });
    refresh();
  }
  document.addEventListener("DOMContentLoaded", setup);
  document.addEventListener("click", function (event) {
    var link = event.target.closest("[data-dynamic-issues] a[href]");
    if (!link || event.defaultPrevented || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey || event.button !== 0) return;
    var url = new URL(link.href);
    if (url.origin !== location.origin || url.pathname !== location.pathname || url.searchParams.get("tab") !== "issues") return;
    event.preventDefault();
    navigate(url.href, null, true);
  });
  document.addEventListener("submit", function (event) {
    var form = event.target.closest("[data-dynamic-issues] form");
    if (form && form.dataset.confirmDelete && !window.confirm(form.dataset.confirmDelete)) { event.preventDefault(); return; }
    if (!form || event.defaultPrevented || !form.matches('form[action*="/issues"]')) return;
    event.preventDefault();
    var submit = form.querySelector('[type="submit"]');
    if (submit) submit.disabled = true;
    navigate(form.action, {method: "POST", body: new URLSearchParams(new FormData(form))}, true).finally(function () {
      if (submit && submit.isConnected) submit.disabled = false;
    });
  });
  window.addEventListener("popstate", function () {
    if (panel() && new URL(location.href).searchParams.get("tab") === "issues") navigate(location.href, null, false);
  });
})();

/* Accessible screenshot lightbox. Links still open the image without JS. */
document.addEventListener("DOMContentLoaded", function () {
  var gallery = document.querySelector("[data-gallery]");
  var dialog = document.getElementById("gallery-lightbox");
  if (!gallery || !dialog || !dialog.showModal) return;
  var links = Array.from(gallery.querySelectorAll("[data-gallery-link]"));
  var image = dialog.querySelector("[data-gallery-image]");
  var caption = dialog.querySelector("[data-gallery-caption]");
  var count = dialog.querySelector("[data-gallery-count]");
  var index = 0;
  function display(n) {
    index = (n + links.length) % links.length;
    var link = links[index], img = link.querySelector("img");
    image.src = link.href;
    image.alt = img ? img.alt : "Screenshot";
    caption.textContent = link.closest("figure").querySelector("figcaption")?.textContent || "";
    count.textContent = (index + 1) + " / " + links.length;
  }
  links.forEach(function (link, i) {
    link.addEventListener("click", function (event) {
      if (event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
      event.preventDefault();
      display(i);
      dialog.showModal();
    });
  });
  dialog.querySelector("[data-gallery-prev]").addEventListener("click", function () { display(index - 1); });
  dialog.querySelector("[data-gallery-next]").addEventListener("click", function () { display(index + 1); });
  dialog.querySelector("[data-gallery-close]").addEventListener("click", function () { dialog.close(); });
  dialog.addEventListener("click", function (event) { if (event.target === dialog) dialog.close(); });
  dialog.addEventListener("keydown", function (event) {
    if (event.key === "ArrowLeft") { event.preventDefault(); display(index - 1); }
    if (event.key === "ArrowRight") { event.preventDefault(); display(index + 1); }
  });
});

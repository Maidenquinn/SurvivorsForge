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
      if (status) status.setAttribute("data-i18n", "js_copy_manual");
      say(window.ForgeI18n ? window.ForgeI18n.t("js_copy_manual") : "Press Ctrl+C to copy the selected link.");
    }
    if (!navigator.clipboard) return manual();
    navigator.clipboard.writeText(field.value).then(function () {
      if (status) status.setAttribute("data-i18n", "js_link_copied");
      say(window.ForgeI18n ? window.ForgeI18n.t("js_link_copied") : "Link copied.");
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
      var intro = note.querySelector("[data-i18n=addon_launcher_explain]") || note.firstChild;
      if (intro && intro.nodeType === 1) intro.setAttribute("data-i18n", "js_launcher_missing");
      if (intro) intro.textContent = window.ForgeI18n ? window.ForgeI18n.t("js_launcher_missing") : "Nothing opened? The launcher may not be installed yet.";
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
    saved(fetch(list.getAttribute("data-order-url"), { method: "POST", body: body }), list);
  });

  // The arrow buttons move the screenshot in place too, so the page keeps its scroll position.
  document.addEventListener("submit", function (event) {
    var form = event.target.closest("form[data-move]");
    if (!form) return;
    event.preventDefault();
    var figure = form.closest("[data-image-id]");
    var list = figure.parentNode;
    if (new FormData(form).get("direction") === "up") list.insertBefore(figure, figure.previousElementSibling);
    else list.insertBefore(figure, figure.nextElementSibling && figure.nextElementSibling.nextElementSibling);
    saved(fetch(form.action, { method: "POST", body: new URLSearchParams(new FormData(form)) }), list);
  });

  // After a move is saved the first and last arrows switch off; a failed save shows the real order.
  function saved(request, list) {
    request
      .then(function (response) {
        if (!response.ok) throw new Error(response.status);
        var figures = list.querySelectorAll("[data-image-id]");
        figures.forEach(function (f, i) {
          var buttons = f.querySelectorAll("form[data-move] button");
          buttons[0].disabled = i === 0;
          buttons[1].disabled = i === figures.length - 1;
        });
      })
      .catch(function () {
        location.reload();
      });
  }

  // Screenshots on an addon page open in a viewer instead of a new tab: wheel, buttons, + and - or a
  // double click zoom around the pointer, dragging moves the zoomed image, the arrows step through.
  // ponytail: no pinch zoom on touch screens yet; the buttons and a double tap cover it.
  var view = { shots: [], index: 0, scale: 1, x: 0, y: 0, drag: null };
  function viewer() {
    return document.querySelector("dialog.lightbox");
  }
  function place() {
    var img = viewer().querySelector(".lightbox-stage img");
    img.style.transform = "translate(" + view.x + "px, " + view.y + "px) scale(" + view.scale + ")";
    img.style.cursor = view.scale > 1 ? (view.drag ? "grabbing" : "grab") : "zoom-in";
  }
  // Zooms by `factor` around the point (cx, cy), measured from the middle of the stage.
  function zoom(factor, cx, cy) {
    var scale = Math.min(6, Math.max(1, view.scale * factor));
    view.x = cx - ((cx - view.x) * scale) / view.scale;
    view.y = cy - ((cy - view.y) * scale) / view.scale;
    view.scale = scale;
    if (scale === 1) view.x = view.y = 0;
    place();
  }
  function fromMiddle(event) {
    var box = viewer().querySelector(".lightbox-stage").getBoundingClientRect();
    return [event.clientX - box.left - box.width / 2, event.clientY - box.top - box.height / 2];
  }
  function show(index) {
    var box = viewer();
    view.index = (index + view.shots.length) % view.shots.length;
    var link = view.shots[view.index];
    var img = box.querySelector(".lightbox-stage img");
    var caption = link.closest("figure").querySelector("figcaption");
    img.src = link.href;
    img.alt = link.querySelector("img").alt;
    box.querySelector(".lightbox-caption").textContent = caption ? caption.textContent : "";
    box.querySelector(".lightbox-count").textContent = view.shots.length > 1 ? view.index + 1 + " / " + view.shots.length : "";
    box.querySelectorAll("[data-step]").forEach(function (b) {
      b.hidden = view.shots.length < 2;
    });
    view.scale = 1;
    view.x = view.y = 0;
    place();
  }
  document.addEventListener("click", function (event) {
    var link = event.target.closest("[data-lightbox]");
    var box = viewer();
    if (link && box) {
      event.preventDefault();
      view.shots = Array.prototype.slice.call(link.closest(".shots").querySelectorAll("[data-lightbox]"));
      box.showModal();
      show(view.shots.indexOf(link));
      return;
    }
    if (!box || !box.open) return;
    if (event.target === box || event.target.classList.contains("lightbox-stage")) return box.close();
    var step = event.target.closest("[data-step]");
    if (step) return show(view.index + Number(step.getAttribute("data-step")));
    var button = event.target.closest("[data-zoom]");
    if (!button) return;
    var how = button.getAttribute("data-zoom");
    if (how === "fit") zoom(1 / view.scale, 0, 0);
    else zoom(how === "in" ? 1.5 : 1 / 1.5, 0, 0);
  });
  document.addEventListener("keydown", function (event) {
    var box = viewer();
    if (!box || !box.open || event.target.closest("button")) return;
    if (event.key === "ArrowRight") show(view.index + 1);
    else if (event.key === "ArrowLeft") show(view.index - 1);
    else if (event.key === "+" || event.key === "=") zoom(1.5, 0, 0);
    else if (event.key === "-") zoom(1 / 1.5, 0, 0);
    else if (event.key === "0") zoom(1 / view.scale, 0, 0);
  });
  document.addEventListener("DOMContentLoaded", function () {
    var box = viewer();
    if (!box) return;
    var stage = box.querySelector(".lightbox-stage");
    var img = stage.querySelector("img");
    stage.addEventListener(
      "wheel",
      function (event) {
        event.preventDefault();
        var at = fromMiddle(event);
        zoom(event.deltaY < 0 ? 1.2 : 1 / 1.2, at[0], at[1]);
      },
      { passive: false }
    );
    img.addEventListener("dblclick", function (event) {
      var at = fromMiddle(event);
      zoom(view.scale > 1 ? 1 / view.scale : 2.5, at[0], at[1]);
    });
    img.addEventListener("pointerdown", function (event) {
      if (view.scale === 1) return;
      view.drag = { px: event.clientX, py: event.clientY, x: view.x, y: view.y };
      img.setPointerCapture(event.pointerId);
      place();
    });
    img.addEventListener("pointermove", function (event) {
      if (!view.drag) return;
      view.x = view.drag.x + event.clientX - view.drag.px;
      view.y = view.drag.y + event.clientY - view.drag.py;
      place();
    });
    img.addEventListener("pointerup", function () {
      view.drag = null;
      place();
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
    note.setAttribute("data-i18n", "js_load_error");
    note.textContent = window.ForgeI18n ? window.ForgeI18n.t("js_load_error") : "Could not load the results. Check your connection and try again.";
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
      if (!response.ok) { var failure = new Error("Request failed (" + response.status + ")"); failure.status = response.status; throw failure; }
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
      document.dispatchEvent(new Event("htmx:afterSwap"));
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
          (root.querySelector("#new-issue") || root).append(message);
        }
        var duplicate = error.status === 409;
        message.textContent = duplicate ? (window.ForgeI18n ? window.ForgeI18n.t("duplicate_issue") : "This issue has already been submitted.") : (window.ForgeI18n ? window.ForgeI18n.t("issue_submit_error") : "Could not submit the issue. Please retry.");
        message.scrollIntoView({block: "center", behavior: "smooth"});
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
      link.addEventListener("click", function (event) {
        event.preventDefault();
        var details = root.querySelector("#new-issue");
        if (details) { details.open = true; details.scrollIntoView({behavior:"smooth",block:"center"}); var input=details.querySelector('input[name="title"]'); if(input) input.focus({preventScroll:true}); }
      });
    });
    refresh();
  }
  document.addEventListener("DOMContentLoaded", setup);
  document.addEventListener("click", function(event) {
    var toggle=event.target.closest("[data-edit-toggle], [data-edit-cancel]");
    if(!toggle) return;
    var container=toggle.closest("[data-inline-editor]");
    if(!container) return;
    var form=container.querySelector("[data-edit-form]"), view=container.querySelector("[data-edit-view]");
    if(!form || !view) return;
    var editing=toggle.hasAttribute("data-edit-toggle");
    form.hidden=!editing; view.hidden=editing;
    if(editing) {var field=form.querySelector("textarea"); if(field)field.focus();}
  });
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
    image.alt = img ? img.alt : (window.ForgeI18n ? window.ForgeI18n.t("js_gallery_screenshot") : "Screenshot");
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
  var fullscreenButton = dialog.querySelector("[data-gallery-fullscreen]");
  function syncFullscreenButton() {
    var active = dialog.classList.contains("gallery-expanded");
    fullscreenButton.textContent = active ? "↙" : "⛶";
    var ariaKey = active ? "js_exit_fullscreen" : "js_enter_fullscreen";
    var titleKey = active ? "js_exit_fullscreen" : "js_fullscreen";
    fullscreenButton.setAttribute("data-i18n-aria-label", ariaKey);
    fullscreenButton.setAttribute("data-i18n-title", titleKey);
    fullscreenButton.setAttribute("aria-label", window.ForgeI18n ? window.ForgeI18n.t(ariaKey) : (active ? "Exit fullscreen" : "Enter fullscreen"));
    fullscreenButton.title = window.ForgeI18n ? window.ForgeI18n.t(titleKey) : (active ? "Exit fullscreen" : "Fullscreen");
  }
  function leaveExpanded() {
    dialog.classList.remove("gallery-expanded");
    if (document.fullscreenElement === dialog && document.exitFullscreen) {
      document.exitFullscreen().catch(function () {});
    }
    syncFullscreenButton();
  }
  fullscreenButton.addEventListener("click", function () {
    if (dialog.classList.contains("gallery-expanded")) {
      leaveExpanded();
    } else {
      // Expand immediately, even if requestFullscreen is unavailable or rejected.
      dialog.classList.add("gallery-expanded");
      syncFullscreenButton();
      if (dialog.requestFullscreen) dialog.requestFullscreen().catch(function () {});
    }
  });
  document.addEventListener("fullscreenchange", function () {
    if (!document.fullscreenElement && dialog.classList.contains("gallery-expanded")) {
      dialog.classList.remove("gallery-expanded");
    }
    syncFullscreenButton();
  });
  dialog.addEventListener("close", leaveExpanded);
  dialog.addEventListener("keydown", function (event) {
    if (event.key === "Escape" && dialog.classList.contains("gallery-expanded")) {
      event.preventDefault();
      event.stopPropagation();
      leaveExpanded();
    }
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

/* Progressive enhancement for addon discussions. Forms work without JavaScript. */
(function () {
  "use strict";
  var busy = false;
  var controller = null;
  var sequence = 0;
  function root() { return document.querySelector("[data-dynamic-comments]"); }
  function t(key, fallback) {
    return window.ForgeI18n ? window.ForgeI18n.t(key) : fallback;
  }
  function errorMessage(container, key, fallback) {
    var notice = container.querySelector("[data-comment-error]");
    if (!notice) {
      notice = document.createElement("p");
      notice.className = "notice bad";
      notice.setAttribute("role", "alert");
      notice.dataset.commentError = "";
      container.prepend(notice);
    }
    notice.textContent = t(key, fallback);
  }
  async function load(url, options, push) {
    var old = root();
    if (!old) return;
    var inline = old.classList.contains('addon-tab-section');
    // A browser Back action may supersede an in-flight request.
    if (busy && !options && !controller?.mutation) {
      if (controller) controller.abort();
      busy = false;
    }
    if (busy) return;
    var requestId = ++sequence;
    busy = true;
    controller = new AbortController();
    var current = controller;
    current.mutation = !!options;
    old.setAttribute("aria-busy", "true");
    try {
      var response = await fetch(url, Object.assign({
        credentials: "same-origin",
        redirect: "follow",
        headers: {"Accept": "text/html"},
        signal: current.signal
      }, options || {}));
      // A redirected login/error page is not a successful comment update.
      if (!response.ok || new URL(response.url).pathname === "/login") {
        if (new URL(response.url).pathname === "/login") {
          window.location.assign("/login");
          return;
        }
        throw new Error("Comment request rejected");
      }
      // After a successful write, fetch the addon tab instead of replacing it with
      // the standalone comments page returned by the server redirect.
      if (inline && options) {
        response = await fetch(location.pathname + "?tab=comments", {
          credentials: "same-origin", headers: {"Accept": "text/html"}, signal: current.signal
        });
        if (!response.ok) throw new Error("Could not refresh comments");
      }
      var html = new DOMParser().parseFromString(await response.text(), "text/html");
      var next = html.querySelector("[data-dynamic-comments]");
      if (!next) throw new Error("Unexpected response");
      if (requestId !== sequence || old !== root()) return;
      old.replaceWith(next);
      // POST redirects to the canonical feed. Do not add mutation URLs to history.
      if (push && !options) history.pushState({comments: true}, "", response.url);
      else if (options && inline) history.replaceState({comments: true}, "", location.pathname + "?tab=comments");
      else if (options && new URL(response.url).origin === location.origin) history.replaceState({comments: true}, "", response.url);
      var heading = next.querySelector("h1, h2");
      if (heading) {
        heading.setAttribute("tabindex", "-1");
        heading.focus({preventScroll: true});
      }
      document.dispatchEvent(new Event("htmx:afterSwap"));
    } catch (err) {
      if (requestId === sequence && err.name !== "AbortError" && old.isConnected) {
        errorMessage(old, "comment_update_error", "Could not update comments. Check your connection and try again.");
      }
    } finally {
      if (requestId === sequence) {
        old.removeAttribute("aria-busy");
        var active = root();
        if (active) active.setAttribute("aria-busy", "false");
      }
      if (controller === current) {
        busy = false;
        controller = null;
      }
    }
  }
  document.addEventListener("submit", function (event) {
    var form = event.target.closest("[data-dynamic-comments] form");
    if (!form) return;
    event.preventDefault();
    if (busy) return;
    if (form.hasAttribute("data-confirm-delete") && !window.confirm(form.getAttribute("data-confirm-delete"))) return;
    if (!form.reportValidity()) return;
    var submit = form.querySelector('button[type="submit"]');
    if (submit) submit.disabled = true;
    load(form.action, {method: "POST", body: new URLSearchParams(new FormData(form)), headers: {"Accept": "text/html", "Content-Type": "application/x-www-form-urlencoded;charset=UTF-8"}}, false).finally(function () {
      if (submit && submit.isConnected) submit.disabled = false;
    });
  });
  document.addEventListener("click", function (event) {
    var cancel = event.target.closest("[data-dynamic-comments] [data-edit-cancel]");
    if (cancel) {
      var form = cancel.closest("form");
      if (form) form.reset();
      return;
    }
    var link = event.target.closest("[data-dynamic-comments] a[href]");
    if (link && !new URL(link.href).searchParams.has("before")) link = null;
    if (!link || event.defaultPrevented || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey || event.button !== 0) return;
    if (new URL(link.href).origin !== location.origin) return;
    event.preventDefault();
    load(link.href, null, true);
  });
  window.addEventListener("popstate", function () {
    if (root() && (location.pathname.endsWith("/comments") || new URL(location.href).searchParams.get("tab") === "comments")) load(location.href, null, false);
  });
})();

/* Progressive enhancement: swap addon tab content without reloading the document.
   Real links remain functional when JavaScript is disabled. */
(function () {
  "use strict";
  var controller = null;
  function tabs() { return document.querySelector("nav.addon-tabs"); }
  function section() { return tabs() && tabs().nextElementSibling; }
  async function navigate(url, push) {
    var nav = tabs(), old = section();
    if (!nav || !old) { location.assign(url); return; }
    if (controller) controller.abort();
    controller = new AbortController();
    var current = controller;
    nav.setAttribute("aria-busy", "true");
    try {
      var response = await fetch(url, {credentials: "same-origin", headers: {"Accept": "text/html"}, signal: current.signal});
      if (!response.ok || new URL(response.url).pathname !== location.pathname) throw new Error("Invalid addon tab response");
      var doc = new DOMParser().parseFromString(await response.text(), "text/html");
      var nextNav = doc.querySelector("nav.addon-tabs");
      var next = nextNav && nextNav.nextElementSibling;
      if (!nextNav || !next || !next.classList.contains("addon-tab-section")) throw new Error("Missing addon tab");
      if (current !== controller || !nav.isConnected) return;
      nav.replaceWith(nextNav);
      old.replaceWith(next);
      if (push) history.pushState({addonTab: true}, "", url);
      document.title = doc.title;
      if (window.ForgeI18n && typeof window.ForgeI18n.apply === "function") window.ForgeI18n.apply();
      document.dispatchEvent(new Event("htmx:afterSwap"));
    } catch (err) {
      if (err.name !== "AbortError") location.assign(url);
    } finally {
      if (controller === current) { controller = null; var active = tabs(); if (active) active.removeAttribute("aria-busy"); }
    }
  }
  document.addEventListener("click", function (event) {
    var link = event.target.closest("nav.addon-tabs a[href]");
    if (!link || event.defaultPrevented || event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    var url = new URL(link.href);
    if (url.origin !== location.origin || url.pathname !== location.pathname) return;
    event.preventDefault();
    navigate(url.href, true);
  });
  window.addEventListener("popstate", function () {
    if (tabs()) navigate(location.href, false);
  });
})();


/* Addon comment threads: progressive enhancement of ordinary POST forms. */
(function () {
  function init() {
    var root = document.querySelector(".addon-comments");
    if (!root) return;
    var list = root.querySelector(".issue-discussion");
    if (!list) return;
    var articles = Array.from(list.querySelectorAll(".threaded-comment"));
    var byId = new Map(articles.map(function (el) { return [el.dataset.commentId, el]; }));
    articles.forEach(function (el) {
      var parent = byId.get(el.dataset.parentId);
      if (parent && parent !== el) {
        var depth = 0, cursor = parent, visited = new Set([el.dataset.commentId]);
        while (cursor && depth < 20 && !visited.has(cursor.dataset.commentId)) {
          visited.add(cursor.dataset.commentId);
          depth++;
          cursor = byId.get(cursor.dataset.parentId);
        }
        if (!cursor || !visited.has(cursor.dataset.commentId)) {
          el.style.setProperty("--comment-depth", Math.min(depth, 3));
          parent.after(el);
        }
      }
    });
  }
  document.addEventListener("DOMContentLoaded", init);
  document.addEventListener("click", function (event) {
    var button = event.target.closest("[data-comment-reply],[data-comment-quote]");
    if (!button) return;
    var form = document.querySelector(".addon-comments .issue-reply");
    if (!form) return;
    var id = button.dataset.commentReply || button.dataset.commentQuote;
    form.querySelector("#comment-parent-id").value = id;
    form.querySelector("#comment-quote-id").value = button.hasAttribute("data-comment-quote") ? id : "";
    var context = form.querySelector("#comment-context");
    context.hidden = false;
    context.textContent = (button.hasAttribute("data-comment-quote") ? "↳ " : "↪ ") + button.dataset.commentAuthor;
    form.querySelector("#comment-clear-context").hidden = false;
    form.scrollIntoView({behavior:"smooth",block:"center"});
    form.querySelector("textarea[name=body]").focus();
  });
  document.addEventListener("click", function (event) {
    if (!event.target.closest("#comment-clear-context")) return;
    var form = document.querySelector(".addon-comments .issue-reply");
    form.querySelector("#comment-parent-id").value = "";
    form.querySelector("#comment-quote-id").value = "";
    form.querySelector("#comment-context").hidden = true;
    form.querySelector("#comment-clear-context").hidden = true;
  });
})();

// Tiles or a list for the card grids (addons, packs), remembered in this browser. Loaded in the
// head, so the saved view is set on <html> before the page is drawn and survives htmx swaps.
(function () {
  var key = "forge-view";
  function apply(view) {
    document.documentElement.setAttribute("data-view", view === "list" ? "list" : "tiles");
  }
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
})();

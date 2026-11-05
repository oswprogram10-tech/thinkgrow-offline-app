/* ThinkGrow offline theme: light by default, stored per installed app. */
(function () {
  "use strict";

  var KEY = "thinkgrow-theme";
  var root = document.documentElement;

  function current() {
    try {
      return localStorage.getItem(KEY) === "dark" ? "dark" : "light";
    } catch (error) {
      return "light";
    }
  }

  function apply(theme) {
    var next = theme === "dark" ? "dark" : "light";
    root.dataset.theme = next;
    try {
      localStorage.setItem(KEY, next);
    } catch (error) {
      /* storage may be unavailable; the active theme still applies */
    }
    var toggle = document.getElementById("theme-toggle");
    if (toggle) {
      toggle.setAttribute("aria-pressed", String(next === "dark"));
      toggle.textContent = next === "dark" ? "밝은 화면" : "어두운 화면";
    }
    return next;
  }

  root.dataset.theme = current();

  function bind() {
    var toggle = document.getElementById("theme-toggle");
    if (!toggle) {
      return;
    }
    toggle.addEventListener("click", function () {
      apply(root.dataset.theme === "dark" ? "light" : "dark");
    });
    toggle.setAttribute("aria-pressed", String(root.dataset.theme === "dark"));
    toggle.textContent = root.dataset.theme === "dark" ? "밝은 화면" : "어두운 화면";
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", bind);
  } else {
    bind();
  }
})();

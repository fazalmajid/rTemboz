// vanilla-JS replacement for the old jQuery + jQuery UI + jQuery Form +
// jQuery Hotkeys bundle. temboz.js is just this file run through jsmin.

// ---- text selection (used to pre-fill the "add filter rule" popup) ----

// On Safari iOS, tapping a UI element like the thumbs-up/down arrows
// clears the current selection, so we save it as soon as it's made.
var saved_selection = "";

function raw_selection() {
  var s = window.getSelection ? String(window.getSelection()) : "";
  return s.trim();
}
function save_selection() {
  saved_selection = raw_selection();
}
function get_selection() {
  var s = raw_selection();
  return s === "" ? saved_selection : s;
}

function capitalize(s) {
  return s.charAt(0).toUpperCase() + s.substring(1);
}

// ---- popups (the "..." callout menus) ----

function place_popup() {
  this.popup = document.getElementById("popup_" + this.id);
}
function hide_popups() {
  document.removeEventListener("click", hide_popups);
  document.querySelectorAll("div.popup").forEach(function (p) {
    p.style.display = "none";
  });
}
function show_popup(event) {
  event.stopPropagation();
  var rect = this.getBoundingClientRect();
  var containerRect = this.offsetParent
    ? this.offsetParent.getBoundingClientRect()
    : { left: 0, top: 0 };
  var wasOpen = this.popup.style.display === "block";
  this.popup.style.left = rect.left - containerRect.left + "px";
  this.popup.style.top =
    rect.top - containerRect.top + this.offsetHeight + 3 + "px";
  this.popup.style.display = wasOpen ? "none" : "block";
  document.addEventListener("click", hide_popups);
}
function init_popups() {
  document.querySelectorAll("a.callout").forEach(function (a) {
    place_popup.call(a);
    a.addEventListener("click", show_popup);
  });
}

function rand() {
  return Math.random().toString().substring(8);
}

// ---- tiny AJAX helpers (replace $.get / .ajaxSubmit) ----

function getRequest(url, params, callback) {
  if (typeof params === "function") {
    callback = params;
    params = null;
  }
  if (params) {
    url +=
      (url.indexOf("?") === -1 ? "?" : "&") +
      new URLSearchParams(params).toString();
  }
  var p = fetch(url);
  if (callback) {
    p = p.then(function (r) {
      return r.text();
    }).then(callback);
  }
  return p;
}

function submitForm(form) {
  // application/x-www-form-urlencoded, not multipart/form-data: the
  // Rust side extracts these with actix-web's web::Form<T>, which
  // only accepts the urlencoded content type.
  return fetch(form.action, {
    method: form.method || "GET",
    body: new URLSearchParams(new FormData(form)),
  });
}

// ---- hotkeys (replace the jQuery Hotkeys plugin) ----

// combos look like "ctrl+z" / "meta+z" / "ctrl+shift+l"; keys are
// matched case-insensitively against event.key
function matches_hotkey(event, combo) {
  var parts = combo.toLowerCase().split("+");
  var key = parts.pop();
  var need = { ctrl: false, meta: false, shift: false, alt: false };
  parts.forEach(function (m) {
    need[m] = true;
  });
  return (
    event.ctrlKey === need.ctrl &&
    event.metaKey === need.meta &&
    event.shiftKey === need.shift &&
    event.altKey === need.alt &&
    event.key.toLowerCase() === key
  );
}

// key-accepting form fields don't get hotkey treatment -- typing "l"
// in the search box shouldn't toggle collapse
function is_typing_target(el) {
  if (!el) return false;
  if (el.isContentEditable) return true;
  var tag = el.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

function bind_hotkeys(map) {
  console.log("[hotkeys] bind_hotkeys called, registering listener for:", Object.keys(map));
  document.addEventListener("keydown", function (event) {
    console.log("[hotkeys] keydown", {
      key: event.key,
      repeat: event.repeat,
      ctrlKey: event.ctrlKey,
      shiftKey: event.shiftKey,
      metaKey: event.metaKey,
      altKey: event.altKey,
      target: event.target.tagName
    });
    if (is_typing_target(event.target)) {
      console.log("[hotkeys] ignored, typing target");
      return;
    }
    for (var combo in map) {
      if (matches_hotkey(event, combo)) {
        console.log("[hotkeys] matched", combo);
        map[combo](event);
        return;
      }
    }
    console.log("[hotkeys] no match");
  });
}

// ---- tabs (replace the jQuery UI Tabs widget) ----
//
// Applies the same classes temboz.css already styles (ui-tabs-nav /
// ui-tabs-active / ui-tabs-panel / ui-tabs-hide), so no CSS changes
// are needed. The initially active tab is whichever panel doesn't
// already carry ui-tabs-hide in the server-rendered markup.
function init_tabs(container) {
  if (typeof container === "string") {
    container = document.querySelector(container);
  }
  if (!container) return;
  var nav = container.querySelector("ul");
  if (!nav) return;
  nav.classList.add("ui-tabs-nav");
  var tabs = Array.prototype.slice.call(nav.querySelectorAll('a[href^="#"]'));
  var panels = tabs.map(function (a) {
    return document.getElementById(a.getAttribute("href").substring(1));
  });
  panels.forEach(function (panel) {
    if (panel) panel.classList.add("ui-tabs-panel");
  });
  function activate(index) {
    tabs.forEach(function (a, i) {
      a.parentElement.classList.toggle("ui-tabs-active", i === index);
      if (panels[i]) panels[i].classList.toggle("ui-tabs-hide", i !== index);
    });
  }
  var initial = panels.findIndex(function (p) {
    return p && !p.classList.contains("ui-tabs-hide");
  });
  activate(initial === -1 ? 0 : initial);
  tabs.forEach(function (a, i) {
    a.addEventListener("click", function (event) {
      event.preventDefault();
      activate(i);
    });
  });
}

// ---- page init ----

document.addEventListener("DOMContentLoaded", function () {
  var search = document.getElementById("search");
  if (search) {
    search.addEventListener("focus", function () {
      document.querySelectorAll("select.hidden").forEach(function (s) {
        s.classList.remove("hidden");
      });
    });
  }
  document.body.addEventListener("touchend", save_selection, false);
  if (
    document.cookie.indexOf("collapse=true") !== -1 &&
    typeof collapse_all === "function"
  ) {
    collapse_all();
  }
});

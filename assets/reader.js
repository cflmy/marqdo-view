/* Marqdo View — the single reader interaction script.
 *
 * Boundaries: this file only manipulates DOM state, layout and accessibility.
 * It never parses @exec/@knowledge, never invents Analysis/Trace facts and
 * never executes anything. All content is rendered by the one server-side
 * ReaderRenderer (RFC-VIEW-0001 §7.1, §14.3). */
(function () {
  "use strict";

  var reader = document.querySelector(".mq-reader");
  if (!reader) return;

  function $(selector, root) {
    return (root || document).querySelector(selector);
  }
  function $$(selector, root) {
    return Array.prototype.slice.call((root || document).querySelectorAll(selector));
  }

  var toastTimer = null;
  function toast(message) {
    var el = $("#mq-toast");
    if (!el) return;
    el.textContent = message;
    el.hidden = false;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () {
      el.hidden = true;
    }, 3800);
  }

  function openDialog(id) {
    var dialog = document.getElementById(id);
    if (!dialog) return null;
    if (typeof dialog.showModal === "function") {
      if (!dialog.open) dialog.showModal();
    } else {
      dialog.setAttribute("open", "");
    }
    var focusTarget = dialog.querySelector("[autofocus], input, button");
    if (focusTarget) focusTarget.focus();
    return dialog;
  }

  function closeDialog(dialog) {
    if (!dialog) return;
    if (typeof dialog.close === "function" && dialog.open) dialog.close();
    else dialog.removeAttribute("open");
  }

  function closeAllDialogs() {
    $$("dialog[open]").forEach(closeDialog);
  }

  function setMode(mode) {
    reader.dataset.mode = mode === "source" ? "source" : "document";
    $$("[data-action='mode-document'], [data-action='mode-source']").forEach(function (button) {
      var active = button.dataset.action === "mode-" + reader.dataset.mode;
      button.setAttribute("aria-current", active ? "page" : "false");
    });
    if (reader.dataset.mode === "source") {
      var source = $(".mq-source");
      if (source) source.focus({ preventScroll: true });
    }
    try {
      var url = new URL(window.location.href);
      if (reader.dataset.mode === "document") url.searchParams.delete("mode");
      else url.searchParams.set("mode", "source");
      window.history.replaceState(null, "", url.toString());
    } catch (error) {
      /* history is optional */
    }
  }

  function gotoAnchor(target) {
    if (!target) return;
    var heading = document.getElementById(target);
    if (heading) {
      closeAllDialogs();
      heading.scrollIntoView({ block: "start" });
      heading.classList.add("is-focus");
      setTimeout(function () {
        heading.classList.remove("is-focus");
      }, 1600);
      return true;
    }
    return false;
  }

  function gotoSourceLine(line) {
    var row = document.querySelector(".mq-source-lines li[data-line='" + line + "']");
    if (!row) return false;
    closeAllDialogs();
    setMode("source");
    row.scrollIntoView({ block: "center" });
    row.classList.add("is-focus");
    setTimeout(function () {
      row.classList.remove("is-focus");
    }, 1600);
    return true;
  }

  function showFunctions() {
    var items = $$(".mq-functions li");
    var body = $("#mq-functions-body");
    if (!body) return;
    body.textContent = "";
    if (!items.length) {
      var empty = document.createElement("p");
      empty.textContent = "本文没有 Analysis 可见的 @exec 函数。";
      body.appendChild(empty);
    } else {
      var list = document.createElement("ul");
      list.className = "mq-annotation-list";
      items.forEach(function (item) {
        var clone = item.cloneNode(true);
        list.appendChild(clone);
      });
      body.appendChild(list);
    }
    openDialog("mq-dialog-functions");
  }

  function showSearch() {
    openDialog("mq-dialog-search");
    var input = $("#mq-search-input");
    if (input) {
      input.value = "";
      input.focus();
      renderSearch("");
    }
  }

  function collectSearchNodes() {
    var nodes = [];
    var walker = document.createTreeWalker(
      $(".mq-document") || reader,
      NodeFilter.SHOW_TEXT,
      null
    );
    while (walker.nextNode()) {
      var text = walker.currentNode;
      if (!text.nodeValue || !text.nodeValue.trim()) continue;
      if (text.parentElement && text.parentElement.closest("script, style")) continue;
      nodes.push(text);
    }
    return nodes;
  }

  var searchNodes = null;
  function renderSearch(query) {
    var results = $("#mq-search-results");
    if (!results) return;
    results.textContent = "";
    var needle = query.trim().toLowerCase();
    if (needle.length < 2) {
      var hint = document.createElement("li");
      hint.textContent = "输入至少两个字符以搜索当前文档正文。";
      hint.className = "mq-search-hint";
      results.appendChild(hint);
      return;
    }
    if (!searchNodes) searchNodes = collectSearchNodes();
    var shown = 0;
    searchNodes.forEach(function (node) {
      if (shown >= 40) return;
      var value = node.nodeValue;
      var index = value.toLowerCase().indexOf(needle);
      if (index < 0) return;
      var start = Math.max(0, index - 28);
      var end = Math.min(value.length, index + needle.length + 34);
      var li = document.createElement("li");
      var button = document.createElement("button");
      button.type = "button";
      var before = document.createTextNode(
        (start > 0 ? "…" : "") + value.slice(start, index)
      );
      var mark = document.createElement("mark");
      mark.textContent = value.slice(index, index + needle.length);
      var after = document.createTextNode(
        value.slice(index + needle.length, end) + (end < value.length ? "…" : "")
      );
      button.appendChild(before);
      button.appendChild(mark);
      button.appendChild(after);
      button.addEventListener("click", function () {
        closeAllDialogs();
        var el = node.parentElement;
        if (el) {
          el.scrollIntoView({ block: "center" });
          el.classList.add("is-focus");
          setTimeout(function () {
            el.classList.remove("is-focus");
          }, 1600);
        }
      });
      li.appendChild(button);
      results.appendChild(li);
      shown += 1;
    });
    if (!shown) {
      var none = document.createElement("li");
      none.textContent = "没有匹配的正文文本。";
      none.className = "mq-search-hint";
      results.appendChild(none);
    }
  }

  function toggleAnnotationVisibility() {
    var off = reader.dataset.annotations === "off";
    reader.dataset.annotations = off ? "on" : "off";
    toast(off ? "已显示语义标注" : "已隐藏语义标注");
    closeAllDialogs();
  }

  function cycleTextSize() {
    var order = ["normal", "large", "small"];
    var current = reader.dataset.size || "normal";
    var next = order[(order.indexOf(current) + 1) % order.length];
    reader.dataset.size = next;
    toast("正文字号：" + (next === "large" ? "大" : next === "small" ? "小" : "标准"));
  }

  function showUnsupported(what, why) {
    var body = $("#mq-unsupported-body");
    if (body) {
      body.textContent = "";
      var title = document.createElement("p");
      title.textContent = what + "：当前版本不支持。";
      var reason = document.createElement("p");
      reason.className = "mq-colophon";
      reason.textContent = why;
      body.appendChild(title);
      body.appendChild(reason);
    }
    openDialog("mq-dialog-unsupported");
  }

  function handleAction(action) {
    switch (action) {
      case "mode-document":
        setMode("document");
        break;
      case "mode-source":
        setMode("source");
        break;
      case "search":
        showSearch();
        break;
      case "functions":
        showFunctions();
        break;
      case "reload":
        window.location.reload();
        break;
      case "settings":
        openDialog("mq-dialog-settings");
        break;
      case "more":
        openDialog("mq-dialog-more");
        break;
      case "close-dialog":
        closeAllDialogs();
        break;
      case "print":
        closeAllDialogs();
        window.print();
        break;
      case "size":
        cycleTextSize();
        break;
      case "annotations":
        toggleAnnotationVisibility();
        break;
      case "variables":
        showUnsupported(
          "变量快照",
          "Trace v2 不提供逐语句赋值后的实际值；View 只能展示 Analysis 中的静态参数与赋值目标，绝不猜测运行时数据。"
        );
        break;
      case "memory":
        showUnsupported(
          "内存快照",
          "当前公开协议没有内存观测通道；该能力需等待未来独立、受权限审计的调试协议。"
        );
        break;
      case "about":
        openDialog("mq-dialog-about");
        break;
      case "run":
        // Until the explicit-run flow is implemented, never pretend to run.
        showUnsupported(
          "运行文档",
          "首版运行入口需要显式的风险评估与 CSRF 会话确认；在实现前该按钮不会执行任何文档。"
        );
        break;
      case "clear":
        var panel = $("#mq-run-panel");
        if (panel) panel.hidden = true;
        toast("已清理本地展示状态（源文件与缓存未修改）");
        break;
      default:
        break;
    }
  }

  document.addEventListener("click", function (event) {
    var trigger = event.target.closest ? event.target.closest("[data-action]") : null;
    if (!trigger) return;
    event.preventDefault();
    handleAction(trigger.dataset.action);
  });

  document.addEventListener("click", function (event) {
    var link = event.target.closest ? event.target.closest("[data-goto-anchor]") : null;
    if (link) {
      event.preventDefault();
      gotoAnchor(link.dataset.gotoAnchor);
      return;
    }
    var lineLink = event.target.closest ? event.target.closest("[data-goto-line]") : null;
    if (lineLink) {
      event.preventDefault();
      var line = parseInt(lineLink.dataset.gotoLine, 10);
      if (!isNaN(line)) gotoSourceLine(line);
    }
  });

  document.addEventListener("input", function (event) {
    if (event.target && event.target.id === "mq-search-input") {
      renderSearch(event.target.value);
    }
  });

  document.addEventListener("keydown", function (event) {
    var isMac = /Mac|iPhone|iPad/.test(navigator.platform || "");
    var modifier = isMac ? event.metaKey : event.ctrlKey;
    if (modifier && (event.key === "k" || event.key === "K")) {
      event.preventDefault();
      showSearch();
      return;
    }
    if (event.key === "Escape") {
      closeAllDialogs();
    }
  });

  document.addEventListener("toggle", function (event) {
    if (event.target && event.target.classList.contains("mq-diagnostics")) {
      try {
        window.localStorage.setItem("marqdo.view.diagnostics", event.target.open ? "1" : "0");
      } catch (error) {
        /* storage is optional */
      }
    }
  }, true);

  // Restore a URL-selected mode, then honour the persisted diagnostics state.
  try {
    var params = new URLSearchParams(window.location.search);
    if (params.get("mode") === "source") setMode("source");
    else setMode("document");
    var diagnosticsState = window.localStorage.getItem("marqdo.view.diagnostics");
    if (diagnosticsState === "1") {
      $$(".mq-diagnostics").forEach(function (details) {
        details.open = true;
      });
    }
  } catch (error) {
    setMode("document");
  }

  // Keep dock buttons visible above the last line on small screens.
  function syncDockInset() {
    var dock = $(".mq-dock");
    if (!dock) return;
    var height = dock.getBoundingClientRect().height + 36;
    reader.style.setProperty("--mq-dock-h", Math.round(height) + "px");
  }
  if (typeof ResizeObserver !== "undefined") {
    var dock = $(".mq-dock");
    if (dock) new ResizeObserver(syncDockInset).observe(dock);
  }
  window.addEventListener("resize", syncDockInset);
  syncDockInset();
})();

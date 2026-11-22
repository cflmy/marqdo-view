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

  function applyTheme(theme, announce) {
    var next = theme === "dark" ? "dark" : "light";
    reader.dataset.theme = next;
    try {
      window.localStorage.setItem("marqdo.view.theme", next);
    } catch (error) {
      /* storage is optional */
    }
    if (announce) toast(next === "dark" ? "已切换到深色主题" : "已切换到暖纸主题");
  }

  function cycleTheme() {
    applyTheme(reader.dataset.theme === "dark" ? "light" : "dark", true);
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

  function handleAction(action, trigger) {
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
      case "unsupported":
        showUnsupported(
          "变量快照 / 内存检查 / 调试器 / 证明状态",
          "Trace v2 不提供逐语句赋值后的实际值或内存快照，公开协议也没有断点/单步通道；" +
            "知识证明属于独立 Prove 工具。View 绝不猜测这些运行事实，相应能力在具备公开、受版本约束的协议前不会出现。"
        );
        break;
      case "theme":
        cycleTheme();
        break;
      case "about":
        openDialog("mq-dialog-about");
        break;
      case "run":
        openRunForMain();
        break;
      case "run-confirm":
        submitRun();
        break;
      case "cancel-run":
        cancelRun();
        break;
      case "call-function":
        if (trigger) {
          var params = [];
          try {
            params = JSON.parse(trigger.dataset.params || "[]");
          } catch (error) {
            params = [];
          }
          openRunForm({
            kind: "function",
            functionId: trigger.dataset.functionId,
            label: trigger.dataset.functionLabel,
            params: params
          });
        }
        break;
      case "clear-run":
      case "clear":
        clearRun();
        break;
      default:
        break;
    }
  }

  function csrfToken() {
    var meta = document.querySelector('meta[name="mq-csrf-token"]');
    return meta ? meta.getAttribute("content") : "";
  }

  var activeRun = null;
  var pendingRun = null;

  function randomId() {
    var bytes = new Uint8Array(16);
    if (window.crypto && window.crypto.getRandomValues) {
      window.crypto.getRandomValues(bytes);
    } else {
      for (var i = 0; i < bytes.length; i += 1) bytes[i] = Math.floor(Math.random() * 256);
    }
    var out = "";
    for (var j = 0; j < bytes.length; j += 1) {
      out += (bytes[j] < 16 ? "0" : "") + bytes[j].toString(16);
    }
    return "req-" + out;
  }

  function csrfToken() {
    var meta = document.querySelector('meta[name="mq-csrf-token"]');
    return meta ? meta.getAttribute("content") : "";
  }

  function stateLabel(state) {
    switch (state) {
      case "accepted": return "已接受";
      case "running": return "运行中";
      case "cancel_requested": return "正在取消";
      case "succeeded": return "成功";
      case "failed": return "失败";
      case "cancelled": return "已取消";
      case "timed_out": return "超时";
      case "unavailable": return "不可用";
      default: return String(state || "未知");
    }
  }

  function isTerminalState(state) {
    return ["succeeded", "failed", "cancelled", "timed_out", "unavailable"].indexOf(state) >= 0;
  }

  function runPanel() {
    return $("#mq-run-panel");
  }

  function setRunStatus(text) {
    var status = $("#mq-run-status");
    if (status) status.textContent = text;
  }

  function showRunPanel() {
    var panel = runPanel();
    if (panel) panel.hidden = false;
  }

  function setRunBusy(busy) {
    var button = document.querySelector('.mq-dock-btn[data-action="run"]');
    if (button) {
      if (busy) {
        button.setAttribute("aria-busy", "true");
        button.dataset.busy = "true";
        button.disabled = true;
        reader.dataset.runState = "running";
      } else {
        button.removeAttribute("aria-busy");
        delete button.dataset.busy;
        button.disabled = false;
      }
    }
    var cancel = $("#mq-run-actions");
    if (cancel) cancel.hidden = !busy;
  }

  function appendLine(container, tag, className, text) {
    var node = document.createElement(tag);
    if (className) node.className = className;
    node.textContent = text;
    container.appendChild(node);
    return node;
  }

  function clearRunOutput() {
    ["#mq-run-summary", "#mq-run-stdout", "#mq-run-stderr", "#mq-run-events"].forEach(function (selector) {
      var node = $(selector);
      if (node) node.textContent = "";
    });
    var stdout = $("#mq-run-stdout");
    if (stdout) stdout.hidden = true;
    var stderr = $("#mq-run-stderr");
    if (stderr) stderr.hidden = true;
    var traceWrap = $("#mq-run-trace-wrap");
    if (traceWrap) traceWrap.hidden = true;
    $$(".mq-run-chip[data-view-chip]").forEach(function (chip) {
      chip.remove();
    });
  }

  function closeActiveObserver() {
    if (activeRun && activeRun.source) {
      try {
        activeRun.source.close();
      } catch (error) {
        /* optional */
      }
      activeRun.source = null;
    }
  }

  function selectedMode() {
    var checked = document.querySelector('input[name="mq-mode"]:checked');
    return checked ? checked.value : "pure";
  }

  function grantList(id) {
    var field = document.getElementById(id);
    if (!field) return [];
    return field.value
      .split("\n")
      .map(function (line) { return line.trim(); })
      .filter(function (line) { return line.length > 0; });
  }

  function rawParams() {
    var out = [];
    $$("#mq-run-params [data-param-name]").forEach(function (input) {
      out.push({ name: input.dataset.paramName, expression: input.value, required: input.dataset.required === "true" });
    });
    return out;
  }

  function updateRunPreview() {
    var preview = $("#mq-run-command");
    if (!preview || !pendingRun) return;
    var mode = selectedMode();
    var parts = [];
    if (pendingRun.kind === "entry") {
      parts = ["marqdo", "run", reader.dataset.document, "--mode", mode, "--json", "--trace-file", "<临时文件>"];
    } else {
      parts = ["marqdo", "call", reader.dataset.document, "<函数 " + pendingRun.label + ">", "--mode", mode, "--json", "--trace-file", "<临时文件>"];
      rawParams().forEach(function (param) {
        parts.push("--arg");
        parts.push(param.name + "=" + (param.expression || "<常量表达式>"));
      });
    }
    if (mode === "project") {
      grantList("mq-grant-read").forEach(function (glob) { parts.push("--allow-read"); parts.push(glob); });
      grantList("mq-grant-write").forEach(function (glob) { parts.push("--allow-write"); parts.push(glob); });
    }
    preview.textContent = parts.join(" ");
    var modeLabel = $("#mq-run-target-mode");
    if (modeLabel) modeLabel.textContent = mode;
  }

  function renderRunParams(params) {
    var container = $("#mq-run-params");
    if (!container) return;
    container.textContent = "";
    if (!params.length) {
      appendLine(container, "p", "mq-colophon", "该目标没有参数。");
      return;
    }
    params.forEach(function (param) {
      var label = document.createElement("label");
      label.className = "mq-run-param";
      var title = document.createElement("span");
      var type = param.type ? ": " + param.type : "";
      var required = param.required ? "（必填）" : "（可选）";
      title.textContent = param.name + type + " " + required;
      var input = document.createElement("input");
      input.type = "text";
      input.dataset.paramName = param.name;
      input.dataset.required = param.required ? "true" : "false";
      input.spellcheck = false;
      input.placeholder = param.default ? "默认常量：" + param.default : "常量表达式，例如 5 或 \"text\"";
      if (param.required) input.setAttribute("aria-required", "true");
      input.addEventListener("input", updateRunPreview);
      label.appendChild(title);
      label.appendChild(input);
      container.appendChild(label);
    });
  }

  function openRunForm(target) {
    var dialog = $("#mq-dialog-run");
    if (!dialog) {
      toast("当前页面没有可用的运行表单；请确认已安装冻结核心");
      return;
    }
    pendingRun = {
      kind: target.kind,
      functionId: target.functionId || "",
      label: target.label || "入口 main",
      clientRequestId: randomId(),
      params: target.params || []
    };
    var label = $("#mq-run-target-label");
    if (label) label.textContent = target.kind === "entry" ? "入口 main" : "函数 " + (target.label || "");
    renderRunParams(pendingRun.params);
    var mode = document.querySelector('input[name="mq-mode"][value="pure"]');
    if (mode) mode.checked = true;
    var grants = $("#mq-run-grants");
    if (grants) grants.hidden = true;
    updateRunPreview();
    openDialog("mq-dialog-run");
  }

  function openRunForMain() {
    if (reader.dataset.coreAvailable !== "true") {
      showUnsupported(
        "运行文档",
        "当前页面没有可用的冻结核心（MARQDO_BIN），无法通过 CLI 执行；打开、刷新与导出仍然零副作用。"
      );
      return;
    }
    if (reader.dataset.hasMain !== "true") {
      toast("本文档没有可执行的 main；请从函数面板选择一个 H2 Exec 函数");
      showFunctions();
      return;
    }
    openRunForm({ kind: "entry", label: "入口 main" });
  }

  function submitRun() {
    if (!pendingRun) return;
    var params = rawParams();
    for (var i = 0; i < params.length; i += 1) {
      if (params[i].required && params[i].expression.trim() === "") {
        toast("缺少必填参数：" + params[i].name);
        return;
      }
    }
    var mode = selectedMode();
    var grants = mode === "project"
      ? { read: grantList("mq-grant-read"), write: grantList("mq-grant-write") }
      : { read: [], write: [] };
    if (mode === "project" && grants.read.length === 0 && grants.write.length === 0) {
      toast("project 模式必须至少声明一个相对项目根的 fs read/write glob");
      return;
    }
    if (mode === "project" &&
        !window.confirm("project 模式将按你填写的 glob 授予文件读写权限。它仍不是操作系统沙箱，是否继续？")) {
      return;
    }
    var payload = {
      document: reader.dataset.document,
      target: pendingRun.kind === "entry"
        ? { kind: "entry" }
        : { kind: "function", function_id: pendingRun.functionId },
      parameters: params.map(function (param) {
        return { name: param.name, expression: param.expression };
      }),
      analysis_digest: reader.dataset.analysisDigest,
      mode: mode,
      grants: grants,
      client_request_id: pendingRun.clientRequestId
    };
    closeAllDialogs();
    clearRunOutput();
    showRunPanel();
    setRunBusy(true);
    setRunStatus("正在提交运行请求…");
    fetch("/api/runs", {
      method: "POST",
      credentials: "same-origin",
      cache: "no-store",
      headers: {
        "Content-Type": "application/json",
        "Accept": "application/json",
        "X-View-CSRF": csrfToken()
      },
      body: JSON.stringify(payload)
    })
      .then(function (response) {
        return response.json().then(function (data) { return { ok: response.ok, status: response.status, data: data }; });
      })
      .then(function (result) {
        if (!result.ok || !result.data || !result.data.run_id) {
          setRunBusy(false);
          var reason = result.data && result.data.message ? result.data.message : "运行请求被拒绝";
          setRunStatus("运行未启动（" + result.status + "）：" + reason);
          toast("运行未启动：" + reason);
          return;
        }
        subscribeRun(result.data, pendingRun ? pendingRun.label : "运行");
      })
      .catch(function (error) {
        setRunBusy(false);
        setRunStatus("运行请求失败：" + error.message);
        toast("运行请求失败：" + error.message);
      });
  }

  function subscribeRun(job, targetLabel) {
    closeActiveObserver();
    activeRun = {
      runId: job.run_id,
      eventsUrl: job.events_url,
      cancelUrl: job.cancel_url,
      source: null,
      lastSeq: -1,
      targetLabel: targetLabel || "运行",
      terminal: false
    };
    clearRunOutput();
    showRunPanel();
    setRunStatus("状态：已接受 · " + activeRun.targetLabel);
    if (!window.EventSource) {
      setRunBusy(false);
      setRunStatus("浏览器不支持 EventSource，无法接收运行状态；请使用现代浏览器。");
      return;
    }
    var source = new EventSource(job.events_url, { withCredentials: true });
    activeRun.source = source;
    function apply(event) {
      var data;
      try {
        data = JSON.parse(event.data);
      } catch (error) {
        return;
      }
      if (!data ||
          data.schema !== "marqdo.view-run-events" ||
          data.schema_version !== 1 ||
          data.run_id !== job.run_id ||
          !Number.isSafeInteger(data.seq) ||
          data.seq <= activeRun.lastSeq) {
        return;
      }
      activeRun.lastSeq = data.seq;
      updateRunView(data);
    }
    ["run.state", "run.complete", "run.error", "run.snapshot"].forEach(function (kind) {
      source.addEventListener(kind, apply);
    });
    source.onerror = function () {
      if (activeRun && !activeRun.terminal) {
        setRunStatus("SSE 连接中断，正在自动恢复…（任务仍在服务端运行；不会重新执行）");
      }
    };
  }

  function updateRunView(data) {
    var panel = runPanel();
    if (panel) {
      panel.hidden = false;
      panel.dataset.runState = data.state;
    }
    reader.dataset.runState = data.state;
    setRunStatus("状态：" + stateLabel(data.state) + (data.at ? " · " + data.at : ""));
    if (!isTerminalState(data.state)) {
      setRunBusy(true);
      return;
    }
    if (activeRun) activeRun.terminal = true;
    closeActiveObserver();
    setRunBusy(false);
    if (data.result) renderRunResult(data.result, data.state, data.error);
    else if (data.error) renderRunResult(null, data.state, data.error);
    if (data.trace) renderTrace(data.trace);
    else if (data.result && data.result.trace_status) renderTraceStatus(data.result.trace_status);
    if (data.state === "succeeded") toast("运行完成");
    else toast("运行" + stateLabel(data.state));
  }

  function renderRunResult(result, state, error) {
    var summary = $("#mq-run-summary");
    if (!summary) return;
    summary.textContent = "";
    if (result) {
      if (result.target) {
        var label = result.target.kind === "entry" ? "入口 main" : "函数 " + (result.target.label || result.target.cli_name || "");
        appendLine(summary, "p", "mq-run-result", label + " · 模式 " + (result.mode || "pure") +
          (typeof result.duration_ms === "number" ? " · " + result.duration_ms + " ms" : "") +
          (result.exit_code === null || typeof result.exit_code === "undefined" ? "" : " · 退出码 " + result.exit_code));
      }
      if (result.value_text) {
        appendLine(summary, "p", "mq-run-result", "返回值：" + result.value_text);
      }
      if (result.stale) {
        appendLine(summary, "p", "mq-run-warning", "源码在运行后发生变化：结果可见但标记为过期，不在当前正文上绘制执行标注。");
      }
      if (result.stdout) {
        var stdout = $("#mq-run-stdout");
        if (stdout) {
          stdout.textContent = result.stdout;
          stdout.hidden = false;
        }
      }
      if (result.stderr) {
        var stderr = $("#mq-run-stderr");
        if (stderr) {
          stderr.textContent = result.stderr;
          stderr.hidden = false;
        }
      }
      if (Array.isArray(result.diagnostics) && result.diagnostics.length) {
        appendLine(summary, "p", "mq-run-warning", "诊断：" + result.diagnostics.length + " 条（详见运行详情）");
        var list = document.createElement("ol");
        list.className = "mq-run-events";
        result.diagnostics.slice(0, 20).forEach(function (diagnostic) {
          var code = diagnostic && diagnostic.code ? diagnostic.code : "diagnostic";
          var message = diagnostic && diagnostic.message ? diagnostic.message : "";
          appendLine(list, "li", "", code + "：" + message);
        });
        summary.appendChild(list);
      }
    }
    if (error && (error.code || error.message)) {
      appendLine(summary, "p", "mq-run-error", (error.code || "error") + "：" + (error.message || "运行失败"));
    }
    if (state === "cancelled") {
      appendLine(summary, "p", "mq-run-warning", "运行已按请求取消；进程组已终止。");
    }
  }

  function renderTraceStatus(status) {
    if (!status || status === "verified") return;
    var summary = $("#mq-run-summary");
    if (!summary) return;
    var message = status === "invalid"
      ? "Trace 与本次 Analysis 不匹配或格式无效：不绘制执行标注，也不声称观察到完整过程。"
      : "本次运行没有可用的 Trace v2（unavailable）：只展示 CLI 返回值与输出。";
    appendLine(summary, "p", "mq-run-warning", message);
  }

  function renderTrace(trace) {
    var summary = $("#mq-run-summary");
    var wrap = $("#mq-run-trace-wrap");
    var list = $("#mq-run-events");
    if (!wrap || !list) return;
    list.textContent = "";
    if (!trace || trace.status !== "verified") {
      wrap.hidden = true;
      renderTraceStatus(trace && trace.status ? trace.status : "unavailable");
      return;
    }
    wrap.hidden = false;
    if (summary) {
      if (trace.stale) {
        appendLine(summary, "p", "mq-run-warning", "Trace 属于运行时的源码：当前源码已变化，本次不绘制执行标注。");
      }
      if (trace.truncated) {
        var label = trace.truncated.label || (trace.truncated.reason + "（上限 " + trace.truncated.limit + "）");
        appendLine(summary, "p", "mq-run-warning", "Trace 被截断：" + label + "；这不代表程序终止。");
      }
    }
    var entryEvents = Array.isArray(trace.events) ? trace.events : [];
    entryEvents.slice(0, 200).forEach(function (event) {
      var location = event.span ? "span " + event.span.start + "–" + event.span.end : (event.module_id || "");
      var detail = event.detail ? " · " + event.detail : "";
      var operation = event.operation ? " · " + event.operation : "";
      appendLine(list, "li", "", event.kind + " · " + location + detail + operation);
    });
    if (entryEvents.length > 200) {
      appendLine(list, "li", "mq-colophon", "仅显示前 200 条（共 " + entryEvents.length + " 条）。");
    }
    if (!trace.stale) {
      var bySource = {};
      $$("[data-source-id]").forEach(function (element) {
        bySource[element.dataset.sourceId] = element;
      });
      var grouped = {};
      entryEvents.forEach(function (event) {
        if (!event.source_id || !bySource[event.source_id]) return;
        if (!grouped[event.source_id]) grouped[event.source_id] = [];
        grouped[event.source_id].push(event);
      });
      Object.keys(grouped).forEach(function (sourceId) {
        var group = grouped[sourceId];
        var last = group[group.length - 1];
        var kinds = [];
        group.forEach(function (event) {
          if (kinds.indexOf(event.kind) < 0) kinds.push(event.kind);
        });
        var chip = document.createElement("span");
        chip.className = "mq-run-chip";
        chip.dataset.viewChip = "1";
        chip.dataset.annotationSource = "trace";
        chip.dataset.runSeq = String(last.seq);
        chip.dataset.runKind = kinds.join(",");
        var text = last.detail || last.kind;
        if (group.length > 3) text += " ×" + group.length;
        chip.textContent = text;
        chip.title = "本次 Trace 事件：" + group.length + " 条（" + kinds.join(",") + "）";
        bySource[sourceId].appendChild(chip);
      });
    }
  }

  function cancelRun() {
    if (!activeRun || activeRun.terminal) return;
    setRunStatus("正在请求取消…");
    fetch(activeRun.cancelUrl, {
      method: "POST",
      credentials: "same-origin",
      cache: "no-store",
      headers: { "Accept": "application/json", "X-View-CSRF": csrfToken() }
    })
      .then(function (response) {
        return response.json().then(function (data) { return { ok: response.ok, status: response.status, data: data }; });
      })
      .then(function (result) {
        if (!result.ok) {
          var reason = result.data && result.data.message ? result.data.message : "取消失败";
          setRunStatus("取消请求被拒绝（" + result.status + "）：" + reason);
          return;
        }
        setRunStatus("已请求取消；等待唯一终态…");
      })
      .catch(function (error) {
        setRunStatus("取消请求失败：" + error.message);
      });
  }

  function clearRun() {
    closeAllDialogs();
    closeActiveObserver();
    var panel = runPanel();
    if (panel) {
      panel.hidden = true;
      panel.dataset.runState = "idle";
    }
    reader.dataset.runState = "idle";
    clearRunOutput();
    setRunStatus("尚未运行。点击 Dock 的“运行”执行 main，或在函数面板选择函数。");
    toast("已清理本地结果展示（未删除项目文件、lock 或服务端运行记录）");
  }

  document.addEventListener("click", function (event) {
    var trigger = event.target.closest ? event.target.closest("[data-action]") : null;
    if (!trigger) return;
    event.preventDefault();
    handleAction(trigger.dataset.action, trigger);
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
    if (event.target && (event.target.id === "mq-grant-read" || event.target.id === "mq-grant-write" || event.target.dataset.paramName)) {
      updateRunPreview();
    }
  });
  document.addEventListener("change", function (event) {
    if (event.target && event.target.name === "mq-mode") {
      var grants = $("#mq-run-grants");
      if (grants) grants.hidden = event.target.value !== "project";
      updateRunPreview();
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

  // Restore reading preferences (theme only; never secrets/permissions).
  try {
    var storedTheme = window.localStorage.getItem("marqdo.view.theme");
    if (storedTheme === "dark" || storedTheme === "light") {
      applyTheme(storedTheme, false);
    } else if (window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches) {
      applyTheme("dark", false);
    }
  } catch (error) {
    /* storage is optional */
  }

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

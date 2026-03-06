(function () {
  function createFallbackEditor(container, options) {
    container.innerHTML = "";
    var textarea = document.createElement("textarea");
    textarea.className = "files-editor-fallback files-editor-wrapper-textarea";
    textarea.value = (options && options.value) || "";
    textarea.spellcheck = false;
    container.appendChild(textarea);

    var editor = {
      kind: "fallback",
      textarea: textarea,
      language: (options && options.language) || "text",
    };
    container.__aosEditor = editor;
    return editor;
  }

  function safeString(value) {
    return typeof value === "string" ? value : "";
  }

  function normalize(target) {
    if (!target) return null;
    if (target.__aosEditor) return target.__aosEditor;
    return target;
  }

  var api = {
    create: function (container, options) {
      if (!container) return null;
      return createFallbackEditor(container, options || {});
    },
    setValue: function (target, value) {
      var instance = normalize(target);
      if (!instance) return;
      if (instance.kind === "fallback" && instance.textarea) {
        instance.textarea.value = safeString(value);
      }
    },
    getValue: function (target) {
      var instance = normalize(target);
      if (!instance) return "";
      if (instance.kind === "fallback" && instance.textarea) {
        return instance.textarea.value;
      }
      return "";
    },
    setLanguage: function (target, language) {
      var instance = normalize(target);
      if (!instance) return;
      instance.language = safeString(language) || "text";
      if (instance.kind === "fallback" && instance.textarea) {
        instance.textarea.setAttribute("data-language", instance.language);
      }
    },
    destroy: function (target) {
      var instance = normalize(target);
      if (!instance) return;
      if (instance.kind === "fallback" && instance.textarea && instance.textarea.parentNode) {
        instance.textarea.parentNode.removeChild(instance.textarea);
      }
    },
    isAvailable: function () {
      return true;
    },
  };

  window.AdapterOSCodeMirror = api;
})();

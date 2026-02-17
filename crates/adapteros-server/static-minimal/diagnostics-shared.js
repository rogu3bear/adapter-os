/*
 * Shared diagnostics helpers for static-minimal surfaces.
 *
 * Keeps minimal pages aligned with baseline diagnostics primitives without
 * importing the full static/index.html boot runtime.
 */
(function () {
  const BANNER_ID = "aos-minimal-diagnostics";

  function ensureBanner() {
    let banner = document.getElementById(BANNER_ID);
    if (banner) return banner;

    banner = document.createElement("div");
    banner.id = BANNER_ID;
    banner.style.position = "fixed";
    banner.style.right = "12px";
    banner.style.top = "12px";
    banner.style.zIndex = "2147483647";
    banner.style.padding = "8px 10px";
    banner.style.borderRadius = "6px";
    banner.style.font = "12px/1.2 monospace";
    banner.style.border = "1px solid #cfd8dc";
    banner.style.background = "#f5f5f5";
    banner.style.color = "#263238";
    banner.style.boxShadow = "0 2px 6px rgba(0,0,0,0.15)";
    banner.textContent = "adapterOS diagnostics: booting";
    document.body.appendChild(banner);
    return banner;
  }

  function setBanner(status, detail) {
    const banner = ensureBanner();
    const base = "adapterOS diagnostics";
    banner.textContent = detail ? `${base}: ${status} - ${detail}` : `${base}: ${status}`;

    if (status === "error") {
      banner.style.background = "#ffebee";
      banner.style.color = "#b71c1c";
      banner.style.borderColor = "#ef9a9a";
    } else if (status === "ready") {
      banner.style.background = "#e8f5e9";
      banner.style.color = "#1b5e20";
      banner.style.borderColor = "#a5d6a7";
    } else if (status === "pending") {
      banner.style.background = "#fff3e0";
      banner.style.color = "#e65100";
      banner.style.borderColor = "#ffcc80";
    } else {
      banner.style.background = "#f5f5f5";
      banner.style.color = "#263238";
      banner.style.borderColor = "#cfd8dc";
    }
  }

  function installGlobalErrorHooks() {
    window.addEventListener("error", function (event) {
      const message = event && event.message ? event.message : "window error";
      setBanner("error", message);
    });

    window.addEventListener("unhandledrejection", function (event) {
      const reason = event && event.reason ? String(event.reason) : "unhandled promise rejection";
      setBanner("error", reason);
    });
  }

  installGlobalErrorHooks();

  window.aosMinimalDiagnostics = {
    setStatus: setBanner,
    booting: function (detail) {
      setBanner("pending", detail || "initializing");
    },
    ready: function (detail) {
      setBanner("ready", detail || "ready");
    },
    error: function (detail) {
      setBanner("error", detail || "error");
    },
  };

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", function () {
      setBanner("pending", "dom-ready");
    });
  } else {
    setBanner("pending", "dom-ready");
  }
})();

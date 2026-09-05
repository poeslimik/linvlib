const routes = [];
let beforeLeave = null;

export function route(pattern, handler, { auth = false } = {}) {
  const keys = [];
  const regex = new RegExp(
    "^" +
      pattern
        .replace(/\//g, "\\/")
        .replace(/:([A-Za-z_]+)/g, (_, key) => {
          keys.push(key);
          return "([^/]+)";
        }) +
      "$"
  );
  routes.push({ regex, keys, handler, auth });
}

export function onBeforeLeave(fn) {
  beforeLeave = fn;
}

export function clearBeforeLeave() {
  beforeLeave = null;
}

export function navigate(path, { replace = false } = {}) {
  if (beforeLeave) {
    const ok = beforeLeave();
    if (ok === false) return;
  }
  if (replace) history.replaceState(null, "", path);
  else history.pushState(null, "", path);
  render();
}

export async function render() {
  clearBeforeLeave();
  const path = location.pathname || "/";
  const app = document.getElementById("app");
  for (const r of routes) {
    const match = path.match(r.regex);
    if (!match) continue;
    if (r.auth && !localStorage.getItem("linvlib_token")) {
      const next = encodeURIComponent(location.pathname + location.search);
      navigate(`/login?next=${next}`, { replace: true });
      return;
    }
    const params = {};
    r.keys.forEach((k, i) => {
      params[k] = decodeURIComponent(match[i + 1]);
    });
    await r.handler(app, params);
    return;
  }
  app.innerHTML = `<main class="page"><p class="empty">페이지를 찾을 수 없습니다.</p><a href="/" data-link>홈으로</a></main>`;
}

export function startRouter() {
  document.addEventListener("click", (e) => {
    const a = e.target.closest("a[data-link]");
    if (!a) return;
    const url = new URL(a.href, location.origin);
    if (url.origin !== location.origin) return;
    e.preventDefault();
    navigate(url.pathname + url.search);
  });
  window.addEventListener("popstate", () => {
    if (beforeLeave) {
      const ok = beforeLeave();
      if (ok === false) {
        history.pushState(null, "", location.href);
        return;
      }
    }
    render();
  });
  render();
}

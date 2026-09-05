export function cover(url, alt = "", className = "cover") {
  if (url) {
    return `<img class="${escapeAttr(className)}" src="${escapeAttr(url)}" alt="" title="${escapeAttr(alt)}" loading="lazy" onerror="(()=>{const d=document.createElement('div');d.className=this.className+' cover--empty';d.setAttribute('aria-hidden','true');this.replaceWith(d);})()" />`;
  }
  return `<div class="${className} cover--empty" aria-hidden="true"></div>`;
}

export function escapeHtml(s) {
  return String(s ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

export function escapeAttr(s) {
  return escapeHtml(s).replace(/'/g, "&#39;");
}

export function formatDate(value) {
  if (!value) return "—";
  const d = value.slice(0, 10);
  return d;
}

/** @param {string} status */
export function publishStatusLabel(status) {
  switch (status) {
    case "complete":
      return "완결";
    case "complete_partial":
      return "완결(번역 미완)";
    case "complete_stalled":
      return "완결(번역 중단)";
    case "ongoing_stalled":
      return "연재중(번역 중단)";
    case "hiatus":
      return "연재 중단(번역 미완)";
    case "hiatus_done":
      return "연재 중단";
    case "ongoing":
    default:
      return "연재중";
  }
}

/** Full label in list/detail (same wording as the select). */
export function publishStatusBadge(status) {
  const s = status || "ongoing";
  const label = publishStatusLabel(s);
  const kind =
    s === "ongoing"
      ? "ongoing"
      : s.includes("stalled") || s === "hiatus"
        ? "stalled"
        : s.startsWith("complete") || s === "hiatus_done"
          ? "complete"
          : "ongoing";
  return ` <span class="badge-publish badge-publish--${kind}">${escapeHtml(label)}</span>`;
}

export function aladinSearchUrl(title) {
  return `https://www.aladin.co.kr/search/wsearchresult.aspx?SearchTarget=Book&SearchWord=${encodeURIComponent(title || "")}`;
}

export function shell({ email, active, isAdmin = false }, content) {
  return `
    <div class="app-shell">
      <header class="topbar">
        <a class="brand" href="/series" data-link aria-label="linvlib 홈">
          <span class="brand__mark">linvlib</span>
        </a>
        <nav class="nav">
          <a href="/series" data-link class="${active === "series" ? "is-active" : ""}">작품</a>
          <a href="/import" data-link class="${active === "import" ? "is-active" : ""}">${isAdmin ? "추가" : "요청"}</a>
          <a href="/tierlist" data-link class="${active === "tierlist" ? "is-active" : ""}">티어리스트</a>
          <a href="/mypage" data-link class="${active === "mypage" ? "is-active" : ""}">마이페이지</a>
          ${
            isAdmin
              ? `<a href="/admin" data-link class="${active === "admin" ? "is-active" : ""}">관리</a>`
              : ""
          }
        </nav>
        <div class="topbar__user">
          <button type="button" class="btn btn--ghost btn--sm topbar__tour" id="tour-btn" title="첫 로그인 가이드">튜토리얼</button>
          <span class="topbar__email">${escapeHtml(email || "")}</span>
          <button type="button" class="btn btn--ghost btn--sm" id="logout-btn">로그아웃</button>
        </div>
      </header>
      ${content}
      <footer class="site-footer">
        <p>도서 정보는 <a href="https://www.aladin.co.kr/" target="_blank" rel="noopener noreferrer">알라딘 인터넷서점</a> Open API를 통해 제공됩니다.</p>
        <p class="site-footer__links">
          <a href="/terms" data-link>이용약관</a>
          <a href="/privacy" data-link>개인정보처리방침</a>
        </p>
      </footer>
    </div>
  `;
}

export function bindLogout() {
  const tourBtn = document.getElementById("tour-btn");
  if (tourBtn && tourBtn.dataset.bound !== "1") {
    tourBtn.dataset.bound = "1";
    tourBtn.addEventListener("click", async () => {
      const { startTour } = await import("./tour.js");
      await startTour({ force: true });
    });
  }

  const btn = document.getElementById("logout-btn");
  if (!btn || btn.dataset.bound === "1") return;
  btn.dataset.bound = "1";
  btn.addEventListener("click", async () => {
    const { clearAuth } = await import("./auth.js");
    const { navigate } = await import("./router.js");
    clearAuth();
    navigate("/login");
  });
}

export function toast(message, type = "info") {
  let el = document.querySelector(".toast");
  if (!el) {
    el = document.createElement("div");
    el.className = "toast";
    document.body.appendChild(el);
  }
  el.textContent = message;
  el.dataset.type = type;
  el.classList.add("is-show");
  clearTimeout(el._t);
  el._t = setTimeout(() => el.classList.remove("is-show"), 2600);
}

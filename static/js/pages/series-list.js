import { api } from "../api.js";
import { getUser } from "../auth.js";
import { shell, cover, escapeHtml, toast, bindLogout, publishStatusBadge } from "../ui.js";

const SORTS = [
  { value: "latest", label: "최신순" },
  { value: "oldest", label: "오래된순" },
  { value: "recently_read", label: "최근 읽은 순" },
  { value: "popular", label: "인기순" },
  { value: "title", label: "제목순" },
];

const FILTERS = [
  { value: "all", label: "전체" },
  { value: "unread", label: "안 읽음" },
  { value: "read", label: "읽음" },
  { value: "unrated", label: "미평가" },
];

function listQuery(params) {
  const sp = new URLSearchParams();
  if (params.sort && params.sort !== "latest") sp.set("sort", params.sort);
  if (params.status && params.status !== "all") sp.set("status", params.status);
  if (params.q) sp.set("q", params.q);
  if (params.page && params.page > 1) sp.set("page", String(params.page));
  const qs = sp.toString();
  return qs ? `/series?${qs}` : "/series";
}

function readParams() {
  const params = new URLSearchParams(location.search);
  return {
    sort: params.get("sort") || "latest",
    status: params.get("status") || "all",
    q: params.get("q") || "",
    page: Number(params.get("page") || "1") || 1,
  };
}

export async function renderSeriesList(root) {
  const state = readParams();

  root.innerHTML = shell(
    { email: getUser()?.email, active: "series", isAdmin: !!getUser()?.is_admin },
    `<main class="page">
      <div class="page__head">
        <div class="page__head--row">
          <h1>작품 목록</h1>
          ${
            getUser()?.is_admin
              ? `<button type="button" class="btn btn--ghost btn--sm" id="refresh-all-aladin" title="완결·번역 중단 작품을 제외하고 알라딘 권 목록을 다시 가져옵니다">
            알라딘 신간 갱신
          </button>`
              : ""
          }
        </div>
        <div class="series-toolbar">
          <label class="series-search">
            <span class="sr-only">작품 검색</span>
            <input
              id="series-q"
              type="search"
              placeholder="제목으로 검색"
              value="${escapeHtml(state.q)}"
              autocomplete="off"
            />
          </label>
          <div class="filter-tabs" role="group" aria-label="읽기 상태 필터">
            ${FILTERS.map(
              (f) =>
                `<button type="button" class="filter-tabs__item ${state.status === f.value ? "is-active" : ""}" data-status="${f.value}">${f.label}</button>`
            ).join("")}
          </div>
          <div class="sort-tabs" role="tablist">
            ${SORTS.map(
              (s) =>
                `<a href="${listQuery({ ...state, sort: s.value, page: 1 })}" data-link class="sort-tabs__item ${state.sort === s.value ? "is-active" : ""}">${s.label}</a>`
            ).join("")}
          </div>
        </div>
      </div>
      <div class="series-grid" id="series-grid"><p class="muted">불러오는 중…</p></div>
      <div class="pager" id="pager"></div>
    </main>`
  );
  bindLogout();

  const input = root.querySelector("#series-q");
  let debounceTimer = null;
  let requestSeq = 0;

  async function load(next = {}) {
    const cur = { ...readParams(), ...next };
    if (cur.page < 1) cur.page = 1;
    const url = listQuery(cur);
    if (url !== location.pathname + location.search) {
      history.replaceState(null, "", url);
    }

    // keep controls in sync without full re-render
    root.querySelectorAll("[data-status]").forEach((btn) => {
      btn.classList.toggle("is-active", btn.dataset.status === cur.status);
    });
    root.querySelectorAll(".sort-tabs__item").forEach((a) => {
      const sort = new URL(a.href).searchParams.get("sort") || "latest";
      a.classList.toggle("is-active", sort === cur.sort);
      a.setAttribute("href", listQuery({ ...cur, sort, page: 1 }));
    });

    const seq = ++requestSeq;
    const grid = root.querySelector("#series-grid");
    const pager = root.querySelector("#pager");
    try {
      const data = await api.listSeries({
        sort: cur.sort,
        page: cur.page,
        limit: 20,
        q: cur.q,
        status: cur.status,
      });
      if (seq !== requestSeq) return;

      if (!data.items.length) {
        const emptyMsg = cur.q.trim()
          ? `‘${escapeHtml(cur.q.trim())}’에 맞는 작품이 없습니다.`
          : cur.status !== "all"
            ? "이 필터에 해당하는 작품이 없습니다."
            : "아직 등록된 작품이 없습니다.";
        grid.innerHTML = `
          <div class="empty-state">
            <p>${emptyMsg}</p>
            ${
              !cur.q && cur.status === "all"
                ? `<a class="btn btn--primary" href="/import" data-link>작품 추가하기</a>`
                : cur.q
                  ? `<a class="btn btn--primary" href="/import?q=${encodeURIComponent(cur.q)}" data-link>알라딘에서 추가</a>`
                  : ""
            }
          </div>`;
        pager.innerHTML = "";
        return;
      }

      grid.innerHTML = data.items
        .map(
          (item) => `
      <a class="series-row" href="/series/${item.id}" data-link>
        <span class="series-row__rank">${item.rank}</span>
        ${cover(item.latest_cover_url, item.title, "cover cover--md")}
        <div class="series-row__meta">
          <h2 class="series-row__title">${escapeHtml(item.title)}${publishStatusBadge(
            item.publish_status || "ongoing"
          )}</h2>
          <p class="series-row__stats">
            ${item.read_volumes} / ${item.total_volumes}권 · ${item.progress_percent}%
          </p>
          <div class="progress" aria-hidden="true">
            <span style="width:${item.progress_percent}%"></span>
          </div>
        </div>
      </a>`
        )
        .join("");

      const totalPages = Math.max(1, Math.ceil(data.total / data.limit));
      if (totalPages > 1) {
        pager.innerHTML = `
          ${
            cur.page > 1
              ? `<button type="button" class="btn btn--ghost" data-page="${cur.page - 1}">이전</button>`
              : ""
          }
          <span class="muted">${cur.page} / ${totalPages}</span>
          ${
            cur.page < totalPages
              ? `<button type="button" class="btn btn--ghost" data-page="${cur.page + 1}">다음</button>`
              : ""
          }
        `;
        pager.querySelectorAll("[data-page]").forEach((btn) => {
          btn.addEventListener("click", () => load({ page: Number(btn.dataset.page) }));
        });
      } else {
        pager.innerHTML = "";
      }
    } catch (ex) {
      if (seq !== requestSeq) return;
      toast(ex.message, "error");
      grid.innerHTML = `<p class="empty">${escapeHtml(ex.message)}</p>`;
      pager.innerHTML = "";
    }
  }

  input.addEventListener("input", () => {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      load({ q: input.value.trim(), page: 1 });
    }, 220);
  });

  root.querySelectorAll("[data-status]").forEach((btn) => {
    btn.addEventListener("click", () => {
      load({ status: btn.dataset.status, page: 1 });
    });
  });

  root.querySelector("#refresh-all-aladin")?.addEventListener("click", async () => {
    if (
      !confirm(
        "알라딘에서 가져온 모든 작품의 권 목록을 다시 가져올까요?\n작품 수에 따라 수 분이 걸릴 수 있습니다."
      )
    ) {
      return;
    }
    const btn = root.querySelector("#refresh-all-aladin");
    btn.disabled = true;
    const prev = btn.textContent;
    btn.textContent = "갱신 중…";
    try {
      const res = await api.adminRefresh();
      toast(
        `신간 갱신 완료: ${res.refreshed}/${res.total} 성공` +
          (res.failed ? ` · 실패 ${res.failed}` : ""),
        res.failed ? "info" : "ok"
      );
      await load();
    } catch (ex) {
      toast(ex.message, "error");
    } finally {
      btn.disabled = false;
      btn.textContent = prev;
    }
  });

  await load();
}

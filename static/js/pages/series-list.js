import { api } from "../api.js";
import { getUser } from "../auth.js";
import { startTour } from "../tour.js";
import { shell, cover, escapeHtml, toast, bindLogout, publishStatusBadge, publishStatusLabel } from "../ui.js";

const SORTS = [
  { value: "latest", label: "최신순" },
  { value: "recently_read", label: "최근 읽은 순" },
  { value: "popular", label: "인기순" },
  { value: "title", label: "제목순" },
];

const PUBLISH_FILTERS = [
  "ongoing",
  "ongoing_stalled",
  "complete",
  "complete_partial",
  "complete_stalled",
  "hiatus",
  "hiatus_done",
];

const META_FILTERS = [
  { key: "read", label: "읽음" },
  { key: "rated", label: "평가함" },
];

/** @typedef {"off"|"in"|"ex"} TriState */

function sortDirectionIcon(order) {
  const desc = order !== "asc";
  const bars = desc
    ? [
        [3, 14],
        [8, 10],
        [13, 7],
        [18, 4],
      ]
    : [
        [3, 4],
        [8, 7],
        [13, 10],
        [18, 14],
      ];
  const arrow = desc
    ? `<path d="M5 3v14M5 17l-3.2-3.2M5 17l3.2-3.2" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>`
    : `<path d="M5 17V3M5 3l-3.2 3.2M5 3l3.2 3.2" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>`;
  const barPaths = bars
    .map(
      ([y, w]) =>
        `<rect x="11" y="${y}" width="${w}" height="2.6" rx="1.3" fill="currentColor"/>`
    )
    .join("");
  return `<svg class="sort-dir__icon" viewBox="0 0 24 22" width="22" height="20" aria-hidden="true">${arrow}${barPaths}</svg>`;
}

function triIcon(state) {
  if (state === "in") {
    return `<span class="tri-check tri-check--in" aria-hidden="true"><svg viewBox="0 0 16 16" width="14" height="14"><path d="M3.2 8.2 6.5 11.4 12.8 4.6" fill="none" stroke="#fff" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/></svg></span>`;
  }
  if (state === "ex") {
    return `<span class="tri-check tri-check--ex" aria-hidden="true"><svg viewBox="0 0 16 16" width="14" height="14"><path d="M3.5 8h9" fill="none" stroke="#fff" stroke-width="2.2" stroke-linecap="round"/></svg></span>`;
  }
  return `<span class="tri-check tri-check--off" aria-hidden="true"></span>`;
}

function cycleTriState(state) {
  if (state === "off") return "in";
  if (state === "in") return "ex";
  return "off";
}

function encodePubMap(map) {
  const pin = [];
  const pex = [];
  for (const key of PUBLISH_FILTERS) {
    if (map[key] === "in") pin.push(key);
    else if (map[key] === "ex") pex.push(key);
  }
  return { ps_in: pin.join(","), ps_ex: pex.join(",") };
}

function decodePubMap(psIn, psEx) {
  /** @type {Record<string, TriState>} */
  const map = Object.fromEntries(PUBLISH_FILTERS.map((k) => [k, "off"]));
  for (const key of String(psIn || "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean)) {
    if (key in map) map[key] = "in";
  }
  for (const key of String(psEx || "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean)) {
    if (key in map) map[key] = "ex";
  }
  return map;
}

function parseTriParam(raw) {
  const v = String(raw || "").trim().toLowerCase();
  if (v === "in" || v === "ex") return v;
  return "off";
}

function triParam(state) {
  return state === "in" || state === "ex" ? state : "";
}

function listQuery(params) {
  const sp = new URLSearchParams();
  let sort = params.sort || "latest";
  let order = params.order === "asc" ? "asc" : "desc";
  if (sort === "oldest") {
    sort = "latest";
    order = "asc";
  }
  if (sort !== "latest") sp.set("sort", sort);
  if (order !== "desc") sp.set("order", order);
  if (params.read_f) sp.set("read_f", params.read_f);
  if (params.rated_f) sp.set("rated_f", params.rated_f);
  if (params.ps_in) sp.set("ps_in", params.ps_in);
  if (params.ps_ex) sp.set("ps_ex", params.ps_ex);
  if (params.q) sp.set("q", params.q);
  if (params.page && params.page > 1) sp.set("page", String(params.page));
  const qs = sp.toString();
  return qs ? `/series?${qs}` : "/series";
}

function readParams() {
  const params = new URLSearchParams(location.search);
  let sort = params.get("sort") || "latest";
  let order = params.get("order") === "asc" ? "asc" : "desc";
  if (sort === "oldest") {
    sort = "latest";
    order = "asc";
  }
  if (!SORTS.some((s) => s.value === sort)) sort = "latest";

  let read_f = parseTriParam(params.get("read_f"));
  let rated_f = parseTriParam(params.get("rated_f"));
  // Legacy status tabs → tri-state
  if (read_f === "off" && rated_f === "off") {
    const legacy = params.get("status") || "all";
    if (legacy === "read") read_f = "in";
    else if (legacy === "unread") read_f = "ex";
    else if (legacy === "unrated") {
      read_f = "in";
      rated_f = "ex";
    }
  }

  return {
    sort,
    order,
    read_f: triParam(read_f),
    rated_f: triParam(rated_f),
    ps_in: params.get("ps_in") || "",
    ps_ex: params.get("ps_ex") || "",
    q: params.get("q") || "",
    page: Number(params.get("page") || "1") || 1,
  };
}

function filterActiveCount(meta, pubMap) {
  let n = 0;
  if (meta.read !== "off") n += 1;
  if (meta.rated !== "off") n += 1;
  n += PUBLISH_FILTERS.filter((k) => pubMap[k] !== "off").length;
  return n;
}

function setTriIcon(btn, state) {
  btn.dataset.state = state;
  const wrap = document.createElement("div");
  wrap.innerHTML = triIcon(state);
  const next = wrap.firstElementChild;
  const prev = btn.querySelector(".tri-check");
  if (prev && next) prev.replaceWith(next);
  else if (next) btn.prepend(next);
}

export async function renderSeriesList(root) {
  // load()가 목록 쿼리로 URL을 덮어쓰기 전에 tour 플래그를 확보
  const wantTour = new URLSearchParams(location.search).get("tour") === "1";
  const state = readParams();
  let pubMap = decodePubMap(state.ps_in, state.ps_ex);
  /** @type {{ read: TriState, rated: TriState }} */
  let meta = {
    read: parseTriParam(state.read_f),
    rated: parseTriParam(state.rated_f),
  };

  root.innerHTML = shell(
    { email: getUser()?.email, active: "series", isAdmin: !!getUser()?.is_admin },
    `<main class="page">
      <div class="page__head">
        <div class="page__head--row">
          <h1>작품 목록</h1>
          ${
            getUser()?.is_admin
              ? `<div class="page__head--actions">
            <a class="btn btn--primary btn--sm" href="/import" data-link>작품 추가</a>
            <button type="button" class="btn btn--ghost btn--sm" id="admin-new-release-refresh" title="알라딘 신간 목록을 백그라운드로 가져와 카탈로그 매칭 작품을 갱신합니다">
              신간 갱신
            </button>
          </div>`
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
          <div class="sort-row">
            <div class="sort-tabs" role="tablist" aria-label="정렬 기준">
              ${SORTS.map(
                (s) =>
                  `<button type="button" class="sort-tabs__item ${state.sort === s.value ? "is-active" : ""}" data-sort="${s.value}">${s.label}</button>`
              ).join("")}
            </div>
            <button
              type="button"
              class="sort-dir"
              id="sort-dir"
              aria-label="${state.order === "asc" ? "오름차순" : "내림차순"}"
              title="${state.order === "asc" ? "오름차순 (클릭하여 내림차순)" : "내림차순 (클릭하여 오름차순)"}"
              data-order="${state.order}"
            >${sortDirectionIcon(state.order)}</button>
          </div>
          <details class="pub-filter" id="pub-filter">
            <summary class="pub-filter__summary">
              필터
              <span class="pub-filter__count ${filterActiveCount(meta, pubMap) ? "" : "is-empty"}" id="pub-filter-count">${
                filterActiveCount(meta, pubMap) ? `${filterActiveCount(meta, pubMap)}개 선택` : "전체"
              }</span>
            </summary>
            <ul class="pub-filter__list" id="filter-list">
              ${META_FILTERS.map(
                (f) => `
                <li>
                  <button type="button" class="pub-filter__item" data-meta="${f.key}" data-state="${meta[f.key]}" aria-label="${f.label}">
                    ${triIcon(meta[f.key])}
                    <span>${f.label}</span>
                  </button>
                </li>`
              ).join("")}
              <li class="pub-filter__divider" aria-hidden="true"></li>
              ${PUBLISH_FILTERS.map(
                (key) => `
                <li>
                  <button type="button" class="pub-filter__item" data-pub="${key}" data-state="${pubMap[key]}" aria-label="${escapeHtml(publishStatusLabel(key))}">
                    ${triIcon(pubMap[key])}
                    <span>${escapeHtml(publishStatusLabel(key))}</span>
                  </button>
                </li>`
              ).join("")}
            </ul>
          </details>
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

  function syncFilterUi() {
    const count = filterActiveCount(meta, pubMap);
    const countEl = root.querySelector("#pub-filter-count");
    countEl.textContent = count ? `${count}개 선택` : "전체";
    countEl.classList.toggle("is-empty", !count);
    root.querySelectorAll("[data-meta]").forEach((btn) => {
      setTriIcon(btn, meta[btn.dataset.meta] || "off");
    });
    root.querySelectorAll("[data-pub]").forEach((btn) => {
      setTriIcon(btn, pubMap[btn.dataset.pub] || "off");
    });
  }

  function syncSortUi(cur) {
    root.querySelectorAll("[data-sort]").forEach((btn) => {
      btn.classList.toggle("is-active", btn.dataset.sort === cur.sort);
    });
    const dirBtn = root.querySelector("#sort-dir");
    dirBtn.dataset.order = cur.order;
    dirBtn.setAttribute("aria-label", cur.order === "asc" ? "오름차순" : "내림차순");
    dirBtn.title =
      cur.order === "asc" ? "오름차순 (클릭하여 내림차순)" : "내림차순 (클릭하여 오름차순)";
    dirBtn.innerHTML = sortDirectionIcon(cur.order);
  }

  async function load(next = {}) {
    const cur = { ...readParams(), ...next };
    if (cur.page < 1) cur.page = 1;
    const encoded = encodePubMap(pubMap);
    cur.ps_in = encoded.ps_in;
    cur.ps_ex = encoded.ps_ex;
    cur.read_f = triParam(meta.read);
    cur.rated_f = triParam(meta.rated);

    const url = listQuery(cur);
    if (url !== location.pathname + location.search) {
      history.replaceState(null, "", url);
    }

    syncSortUi(cur);
    syncFilterUi();

    const seq = ++requestSeq;
    const grid = root.querySelector("#series-grid");
    const pager = root.querySelector("#pager");
    try {
      const data = await api.listSeries({
        sort: cur.sort,
        order: cur.order,
        page: cur.page,
        limit: 20,
        q: cur.q,
        read_f: cur.read_f,
        rated_f: cur.rated_f,
        ps_in: cur.ps_in,
        ps_ex: cur.ps_ex,
      });
      if (seq !== requestSeq) return;

      const hasFilter = !!(cur.read_f || cur.rated_f || cur.ps_in || cur.ps_ex);
      if (!data.items.length) {
        const emptyMsg = cur.q.trim()
          ? `‘${escapeHtml(cur.q.trim())}’에 맞는 작품이 없습니다.`
          : hasFilter
            ? "이 필터에 해당하는 작품이 없습니다."
            : "아직 등록된 작품이 없습니다.";
        const qEnc = encodeURIComponent(cur.q.trim());
        const isAdmin = !!getUser()?.is_admin;
        grid.innerHTML = `
          <div class="empty-state">
            <p>${emptyMsg}</p>
            <div class="empty-state__actions">
            ${
              !cur.q && !hasFilter
                ? isAdmin
                  ? `<a class="btn btn--primary" href="/import" data-link>작품 추가하기</a>`
                  : `<a class="btn btn--primary" href="/import" data-link>작품 추가하기</a>`
                : cur.q
                  ? isAdmin
                    ? `<a class="btn btn--primary" href="/import?q=${qEnc}" data-link>알라딘에서 추가</a>
                       <a class="btn btn--ghost" href="/import?tab=manual&title=${qEnc}" data-link>직접 등록</a>`
                    : `<a class="btn btn--primary" href="/import?tab=add&q=${qEnc}" data-link>작품 추가 요청</a>
                       <a class="btn btn--ghost" href="/import?tab=search_improve&q=${qEnc}" data-link>검색 개선 요청</a>`
                  : ""
            }
            </div>
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

      if (cur.q.trim()) {
        const qEnc = encodeURIComponent(cur.q.trim());
        const isAdmin = !!getUser()?.is_admin;
        grid.insertAdjacentHTML(
          "beforeend",
          isAdmin
            ? `<div class="search-improve-hint">
            <p>목록에 없다면 추가하거나, 줄임말이면 별칭을 붙일 수 있습니다.</p>
            <div class="empty-state__actions">
              <a class="btn btn--primary btn--sm" href="/import?q=${qEnc}" data-link>알라딘에서 추가</a>
              <a class="btn btn--ghost btn--sm" href="/import?tab=manual&title=${qEnc}" data-link>직접 등록</a>
              <a class="btn btn--ghost btn--sm" href="/import?tab=aliases&q=${qEnc}" data-link>검색</a>
            </div>
          </div>`
            : `<div class="search-improve-hint">
            <p>원하는 작품이 없나요?</p>
            <a class="btn btn--ghost btn--sm" href="/import?tab=search_improve&q=${qEnc}" data-link>검색 개선 요청</a>
          </div>`
        );
      }

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

  root.querySelectorAll("[data-sort]").forEach((btn) => {
    btn.addEventListener("click", () => {
      load({ sort: btn.dataset.sort, page: 1 });
    });
  });

  root.querySelector("#sort-dir").addEventListener("click", () => {
    const cur = readParams();
    load({ order: cur.order === "asc" ? "desc" : "asc", page: 1 });
  });

  root.querySelector("#filter-list").addEventListener("click", (e) => {
    const metaBtn = e.target.closest("[data-meta]");
    if (metaBtn) {
      const key = metaBtn.dataset.meta;
      meta[key] = cycleTriState(meta[key] || "off");
      load({ page: 1 });
      return;
    }
    const pubBtn = e.target.closest("[data-pub]");
    if (pubBtn) {
      const key = pubBtn.dataset.pub;
      pubMap[key] = cycleTriState(pubMap[key] || "off");
      load({ page: 1 });
    }
  });

  root.querySelector("#admin-new-release-refresh")?.addEventListener("click", async () => {
    if (
      !confirm(
        "알라딘 신간 목록을 가져와 카탈로그에 있는 작품만 갱신할까요?\n목록에 없는 신간은 관리 → 추천에 추가됩니다.\n(백그라운드로 실행되며 완료까지 수 분 걸릴 수 있습니다)"
      )
    ) {
      return;
    }
    const btn = root.querySelector("#admin-new-release-refresh");
    btn.disabled = true;
    const prev = btn.textContent;
    btn.textContent = "갱신 중…";
    try {
      const start = await api.adminRefresh();
      toast(start.message || "신간 갱신을 시작했습니다.", "ok");
      const done = await api.waitForRefreshIdle();
      const note = done.last_refresh_note ? ` · ${done.last_refresh_note}` : "";
      toast(`신간 갱신 완료${note}`, "ok");
      await load();
    } catch (ex) {
      toast(ex.message, "error");
    } finally {
      btn.disabled = false;
      btn.textContent = prev;
    }
  });

  await load();
  if (wantTour) {
    await startTour({ force: true });
  }
}

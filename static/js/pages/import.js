import { api } from "../api.js";
import { getUser } from "../auth.js";
import { shell, cover, escapeHtml, escapeAttr, toast, bindLogout } from "../ui.js";

function volumeRowHtml(vol = {}, idx = 0) {
  return `
    <div class="manual-vol-row" data-idx="${idx}">
      <input type="number" min="1" class="manual-vol-num" value="${escapeAttr(vol.volume_number ?? idx + 1)}" title="권번호" />
      <input type="text" class="manual-vol-title" value="${escapeAttr(vol.title || "")}" placeholder="권 제목" />
      <input type="date" class="manual-vol-date" value="${escapeAttr(vol.published_at || "")}" title="출간일" />
      <input type="url" class="manual-vol-cover" value="${escapeAttr(vol.cover_url || "")}" placeholder="표지 URL" title="표지 URL" />
      <button type="button" class="btn btn--ghost btn--sm" data-remove-vol>삭제</button>
    </div>`;
}

export async function renderImport(root) {
  const user = getUser();
  const isAdmin = !!user?.is_admin;
  const params = new URLSearchParams(location.search);
  const q = params.get("q") || "";
  const rawTab = params.get("tab") || "";
  const tab =
    rawTab === "manual" ? "manual" : rawTab === "aliases" ? "aliases" : "aladin";
  const preTitle = params.get("title") || (tab === "manual" ? q : "") || "";
  const preAuthor = params.get("author") || "";
  const prePublisher = params.get("publisher") || "";
  const rawUserTab = params.get("tab");
  const aliasPrefill = tab === "aliases" ? q : "";
  const userTab =
    rawUserTab === "other"
      ? "other"
      : rawUserTab === "search_improve"
        ? "search_improve"
        : "add";

  if (!isAdmin) {
    root.innerHTML = shell(
      { email: user?.email, active: "import", isAdmin: false },
      `<main class="page">
        <div class="page__head">
          <h1>요청</h1>
          <p class="page__lead">작품 추가·검색 개선·수정 제안이나 사소한 건의도 보내 주세요. 운영자가 확인합니다.</p>
          <div class="import-tabs" role="tablist">
            <button type="button" class="import-tabs__item ${userTab === "add" ? "is-active" : ""}" data-user-tab="add">작품 추가</button>
            <button type="button" class="import-tabs__item ${userTab === "search_improve" ? "is-active" : ""}" data-user-tab="search_improve">검색 개선</button>
            <button type="button" class="import-tabs__item ${userTab === "other" ? "is-active" : ""}" data-user-tab="other">기타</button>
          </div>
        </div>

        <section id="tab-add" class="import-panel ${userTab === "add" ? "" : "is-hidden"}">
          <form class="search-form" id="import-form">
            <input name="q" type="search" placeholder="작품 제목 검색" value="${escapeHtml(userTab === "add" ? q : "")}" ${userTab === "add" ? "autofocus" : ""} />
            <button type="submit" class="btn btn--primary">검색</button>
          </form>
          <div id="import-results"></div>
          <section class="panel" style="margin-top:1.5rem">
            <h2>직접 요청</h2>
            <form id="request-form" class="manual-form">
              <label class="manual-field">
                <span>제목 *</span>
                <input id="req-title" type="text" required placeholder="시리즈 제목" value="${escapeAttr(userTab === "add" ? q : "")}" />
              </label>
              <div class="manual-field-row">
                <label class="manual-field"><span>작가</span><input id="req-author" type="text" /></label>
                <label class="manual-field"><span>출판사</span><input id="req-publisher" type="text" /></label>
              </div>
              <label class="manual-field">
                <span>메모</span>
                <textarea id="req-note" rows="3" placeholder="참고 링크, 권 수 등"></textarea>
              </label>
              <button type="submit" class="btn btn--primary">추가 요청 보내기</button>
            </form>
          </section>
          <p class="muted" style="margin-top:1rem">도서 정보 출처: <a href="https://www.aladin.co.kr/" target="_blank" rel="noopener noreferrer">알라딘 인터넷서점</a></p>
        </section>

        <section id="tab-search-improve" class="import-panel ${userTab === "search_improve" ? "" : "is-hidden"}">
          <section class="panel">
            <h2>검색 개선</h2>
            <p class="muted">줄임말·별칭으로 검색했을 때 원하는 작품이 안 나오면, 그 검색어가 어떤 작품으로 이어질지 알려 주세요.</p>
            <form id="search-improve-form" class="manual-form">
              <label class="manual-field">
                <span>검색어 *</span>
                <input id="si-query" type="text" required placeholder="예: 전생슬" value="${escapeAttr(userTab === "search_improve" ? q : "")}" ${userTab === "search_improve" ? "autofocus" : ""} />
              </label>
              <label class="manual-field">
                <span>연결할 작품 찾기</span>
                <div class="search-form" style="margin:0">
                  <input id="si-series-q" type="search" placeholder="작품 목록에서 검색" />
                  <button type="button" class="btn btn--ghost" id="si-series-search">검색</button>
                </div>
              </label>
              <div id="si-series-results" class="si-results"></div>
              <div class="manual-field">
                <span>선택된 작품</span>
                <ul id="si-selected" class="si-selected"></ul>
              </div>
              <label class="manual-field">
                <span>메모</span>
                <textarea id="si-note" rows="3" placeholder="추가 설명 (선택)"></textarea>
              </label>
              <button type="submit" class="btn btn--primary">검색 개선 요청 보내기</button>
            </form>
          </section>
        </section>

        <section id="tab-other" class="import-panel ${userTab === "other" ? "" : "is-hidden"}">
          <section class="panel">
            <h2>기타 요청</h2>
            <p class="muted">오타·표기, UI 불편, 질문 등 사소한 사항도 적어 주세요.</p>
            <form id="other-form" class="manual-form">
              <label class="manual-field">
                <span>제목</span>
                <input id="other-title" type="text" placeholder="한줄 요약 (선택)" ${userTab === "other" ? "autofocus" : ""} />
              </label>
              <label class="manual-field">
                <span>내용 *</span>
                <textarea id="other-note" rows="6" required placeholder="요청 내용을 적어 주세요"></textarea>
              </label>
              <button type="submit" class="btn btn--primary">요청 보내기</button>
            </form>
          </section>
        </section>
      </main>`
    );
    bindLogout();
    wireUserRequest();
    if (userTab === "add" && q) loadAladinUser();
    return;
  }

  root.innerHTML = shell(
    { email: user?.email, active: "import", isAdmin: true },
    `<main class="page">
      <div class="page__head">
        <h1>작품 · 별칭</h1>
        <p class="page__lead">알라딘 가져오기, 직접 등록, 검색 줄임말·묶음을 한곳에서 관리합니다.</p>
        <div class="import-tabs" role="tablist">
          <button type="button" class="import-tabs__item ${tab === "aladin" ? "is-active" : ""}" data-tab="aladin">알라딘</button>
          <button type="button" class="import-tabs__item ${tab === "manual" ? "is-active" : ""}" data-tab="manual">직접 등록</button>
          <button type="button" class="import-tabs__item ${tab === "aliases" ? "is-active" : ""}" data-tab="aliases">검색</button>
        </div>
      </div>
      <section id="tab-aladin" class="import-panel ${tab === "aladin" ? "" : "is-hidden"}">
        <form class="search-form" id="import-form">
          <input name="q" type="search" placeholder="추가할 작품 제목" value="${escapeHtml(tab === "aladin" ? q : "")}" ${tab === "aladin" ? "autofocus" : ""} />
          <button type="submit" class="btn btn--primary">검색</button>
        </form>
        <div id="import-results"></div>
        <section class="panel" style="margin-top:1.5rem">
          <h2>ItemId로 가져오기</h2>
          <p class="muted">알라딘 상품 URL 또는 ItemId를 여러 줄·쉼표로 붙여 넣으면 LookUp으로 권을 채웁니다. 검색이 빠뜨리는 전자책 시리즈에 사용하세요.</p>
          <label class="manual-field">
            <span>ItemId / URL</span>
            <textarea id="seed-item-ids" rows="6" placeholder="https://www.aladin.co.kr/shop/wproduct.aspx?ItemId=156790758&#10;164448182&#10;171163543"></textarea>
          </label>
          <button type="button" class="btn btn--primary" id="import-by-items">ItemId로 가져오기</button>
        </section>
      </section>
      <section id="tab-manual" class="import-panel ${tab === "manual" ? "" : "is-hidden"}">
        <form id="manual-form" class="manual-form">
          <label class="manual-field">
            <span>제목 *</span>
            <input name="title" id="manual-title" type="text" required placeholder="시리즈 제목" value="${escapeAttr(preTitle)}" ${tab === "manual" ? "autofocus" : ""} />
          </label>
          <div class="manual-field-row">
            <label class="manual-field"><span>작가</span><input id="manual-author" type="text" value="${escapeAttr(preAuthor)}" /></label>
            <label class="manual-field"><span>출판사</span><input id="manual-publisher" type="text" value="${escapeAttr(prePublisher)}" /></label>
          </div>
          <label class="manual-field">
            <span>대표 표지 URL</span>
            <input id="manual-cover" type="url" placeholder="https://…" />
          </label>
          <div class="manual-vols-head">
            <h2>권 목록</h2>
            <div class="manual-vols-actions">
              <button type="button" class="btn btn--ghost btn--sm" id="add-vol-btn">권 추가</button>
              <label class="manual-quick">
                <input type="number" id="quick-count" min="1" max="200" placeholder="N권" />
                <button type="button" class="btn btn--ghost btn--sm" id="quick-fill-btn">빈 권 채우기</button>
              </label>
            </div>
          </div>
          <div id="manual-vols">${volumeRowHtml({}, 0)}</div>
          <button type="submit" class="btn btn--primary" id="manual-submit">등록</button>
        </form>
      </section>
      <section id="tab-aliases" class="import-panel ${tab === "aliases" ? "" : "is-hidden"}">
        <section class="panel">
          <h2>줄임말 · 별칭</h2>
          <p class="muted">줄임말 하나에 작품을 여러 개 골라 검색어로 연결합니다.</p>
          <form id="admin-alias-form" class="manual-form">
            <label class="manual-field">
              <span>줄임말 · 별칭 *</span>
              <input id="aa-query" type="text" required maxlength="80" placeholder="예: 전생슬" value="${escapeAttr(aliasPrefill)}" ${tab === "aliases" ? "autofocus" : ""} />
            </label>
            <label class="manual-field">
              <span>연결할 작품 찾기</span>
              <div class="search-form" style="margin:0">
                <input id="aa-series-q" type="search" placeholder="작품 목록에서 검색" />
                <button type="button" class="btn btn--ghost" id="aa-series-search">검색</button>
              </div>
            </label>
            <div id="aa-series-results" class="si-results"></div>
            <div class="manual-field">
              <span>선택된 작품</span>
              <ul id="aa-selected" class="si-selected"></ul>
            </div>
            <button type="submit" class="btn btn--primary">선택한 작품에 별칭 적용</button>
          </form>
        </section>
        <section class="panel admin-search-bundle">
          <h2>작품 묶음</h2>
          <p class="muted">본편·외전처럼 제목이 달라도, 묶인 작품 중 하나를 검색하면 나머지도 함께 나옵니다.</p>
          <form id="admin-bundle-form" class="manual-form">
            <label class="manual-field">
              <span>묶을 작품 찾기</span>
              <div class="search-form" style="margin:0">
                <input id="ab-series-q" type="search" placeholder="작품 목록에서 검색" />
                <button type="button" class="btn btn--ghost" id="ab-series-search">검색</button>
              </div>
            </label>
            <div id="ab-series-results" class="si-results"></div>
            <div class="manual-field">
              <span>선택된 작품 (2편 이상)</span>
              <ul id="ab-selected" class="si-selected"></ul>
            </div>
            <button type="submit" class="btn btn--primary">선택한 작품 묶기</button>
          </form>
        </section>
      </section>
      <p class="muted">도서 정보 출처: <a href="https://www.aladin.co.kr/" target="_blank" rel="noopener noreferrer">알라딘 인터넷서점</a></p>
    </main>`
  );
  bindLogout();
  wireAdmin();

  function wireUserRequest() {
    const selected = new Map(); // id -> title

    function showUserTab(next) {
      root.querySelectorAll("[data-user-tab]").forEach((b) =>
        b.classList.toggle("is-active", b.dataset.userTab === next)
      );
      root.querySelector("#tab-add").classList.toggle("is-hidden", next !== "add");
      root.querySelector("#tab-search-improve").classList.toggle("is-hidden", next !== "search_improve");
      root.querySelector("#tab-other").classList.toggle("is-hidden", next !== "other");
    }

    function renderSelected() {
      const box = root.querySelector("#si-selected");
      if (!selected.size) {
        box.innerHTML = `<li class="muted">아직 선택한 작품이 없습니다.</li>`;
        return;
      }
      box.innerHTML = [...selected.entries()]
        .map(
          ([id, title]) => `
        <li class="si-selected__item">
          <span>${escapeHtml(title)}</span>
          <button type="button" class="btn btn--ghost btn--sm" data-si-remove="${escapeAttr(id)}">제거</button>
        </li>`
        )
        .join("");
      box.querySelectorAll("[data-si-remove]").forEach((btn) => {
        btn.addEventListener("click", () => {
          selected.delete(btn.dataset.siRemove);
          renderSelected();
        });
      });
    }

    async function searchCatalogSeries() {
      const box = root.querySelector("#si-series-results");
      const query = root.querySelector("#si-series-q").value.trim();
      if (!query) {
        toast("작품 검색어를 입력하세요", "error");
        return;
      }
      box.innerHTML = `<p class="muted">검색 중…</p>`;
      try {
        const results = await api.searchSeries(query);
        if (!results.length) {
          box.innerHTML = `<p class="empty">결과가 없습니다. 다른 제목으로 검색해 보세요.</p>`;
          return;
        }
        box.innerHTML = `<ul class="result-list">${results
          .map(
            (r) => `
          <li class="import-card">
            ${cover(r.latest_cover_url, r.title, "cover cover--sm")}
            <div class="import-card__body">
              <h3>${escapeHtml(r.title)}</h3>
              <button type="button" class="btn btn--ghost btn--sm" data-si-add="${escapeAttr(r.id)}" data-title="${escapeAttr(r.title)}" ${selected.has(r.id) ? "disabled" : ""}>
                ${selected.has(r.id) ? "선택됨" : "선택"}
              </button>
            </div>
          </li>`
          )
          .join("")}</ul>`;
        box.querySelectorAll("[data-si-add]").forEach((btn) => {
          btn.addEventListener("click", () => {
            selected.set(btn.dataset.siAdd, btn.dataset.title);
            btn.disabled = true;
            btn.textContent = "선택됨";
            renderSelected();
          });
        });
      } catch (ex) {
        box.innerHTML = `<p class="empty">${escapeHtml(ex.message)}</p>`;
      }
    }

    root.querySelectorAll("[data-user-tab]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const next = btn.dataset.userTab;
        const sp = new URLSearchParams(location.search);
        if (next === "add") sp.delete("tab");
        else sp.set("tab", next);
        history.replaceState(null, "", sp.toString() ? `/import?${sp}` : "/import");
        showUserTab(next);
      });
    });

    root.querySelector("#import-form").addEventListener("submit", (e) => {
      e.preventDefault();
      const next = String(new FormData(e.target).get("q") || "");
      history.pushState(null, "", `/import?q=${encodeURIComponent(next)}`);
      loadAladinUser();
    });
    root.querySelector("#request-form").addEventListener("submit", async (e) => {
      e.preventDefault();
      const title = root.querySelector("#req-title").value.trim();
      if (!title) return;
      try {
        await api.createCatalogRequest({
          request_type: "add",
          title,
          author: root.querySelector("#req-author").value.trim() || null,
          publisher: root.querySelector("#req-publisher").value.trim() || null,
          note: root.querySelector("#req-note").value.trim() || null,
        });
        toast("추가 요청을 보냈습니다", "ok");
        e.target.reset();
      } catch (ex) {
        toast(ex.message, "error");
      }
    });
    root.querySelector("#other-form").addEventListener("submit", async (e) => {
      e.preventDefault();
      const note = root.querySelector("#other-note").value.trim();
      if (!note) {
        toast("내용을 입력하세요", "error");
        return;
      }
      try {
        await api.createCatalogRequest({
          request_type: "other",
          title: root.querySelector("#other-title").value.trim() || null,
          note,
        });
        toast("요청을 보냈습니다", "ok");
        e.target.reset();
      } catch (ex) {
        toast(ex.message, "error");
      }
    });

    root.querySelector("#si-series-search")?.addEventListener("click", () => searchCatalogSeries());
    root.querySelector("#si-series-q")?.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        searchCatalogSeries();
      }
    });
    root.querySelector("#search-improve-form")?.addEventListener("submit", async (e) => {
      e.preventDefault();
      const query = root.querySelector("#si-query").value.trim();
      if (!query) {
        toast("검색어를 입력하세요", "error");
        return;
      }
      if (!selected.size) {
        toast("연결할 작품을 하나 이상 선택하세요", "error");
        return;
      }
      try {
        await api.createCatalogRequest({
          request_type: "search_improve",
          title: query,
          series_ids: [...selected.keys()],
          note: root.querySelector("#si-note").value.trim() || null,
        });
        toast("검색 개선 요청을 보냈습니다", "ok");
        selected.clear();
        renderSelected();
        root.querySelector("#si-series-results").innerHTML = "";
        root.querySelector("#si-note").value = "";
      } catch (ex) {
        toast(ex.message, "error");
      }
    });

    renderSelected();
  }

  async function loadAladinUser() {
    const box = root.querySelector("#import-results");
    const query = new URLSearchParams(location.search).get("q") || "";
    if (!query.trim()) {
      box.innerHTML = `<p class="muted">제목을 검색하세요.</p>`;
      return;
    }
    box.innerHTML = `<p class="muted">검색 중…</p>`;
    try {
      const results = await api.importSearch(query);
      if (!results.length) {
        box.innerHTML = `<p class="empty">결과가 없습니다.</p>`;
        return;
      }
      box.innerHTML = `<ul class="result-list">${results
        .map(
          (r) => `
        <li class="import-card">
          ${cover(r.cover_url, r.title, "cover cover--md")}
          <div class="import-card__body">
            <h3>${escapeHtml(r.title)}</h3>
            <p class="muted">${escapeHtml(r.author || "작가 미상")}${r.publisher ? ` · ${escapeHtml(r.publisher)}` : ""} · ${r.volume_count}권</p>
            <div class="import-card__actions">
              ${
                r.already_imported
                  ? `<a class="btn btn--ghost btn--sm" href="/series/${r.series_id}" data-link>이미 있음 · 보기</a>`
                  : `<button type="button" class="btn btn--primary btn--sm" data-request="${escapeAttr(r.aladin_series_id)}" data-title="${escapeAttr(r.title)}" data-author="${escapeAttr(r.author || "")}" data-publisher="${escapeAttr(r.publisher || "")}">추가 요청</button>`
              }
            </div>
          </div>
        </li>`
        )
        .join("")}</ul>`;
      box.querySelectorAll("[data-request]").forEach((btn) => {
        btn.addEventListener("click", async () => {
          btn.disabled = true;
          try {
            await api.createCatalogRequest({
              request_type: "add",
              title: btn.dataset.title,
              author: btn.dataset.author || null,
              publisher: btn.dataset.publisher || null,
              aladin_series_id: btn.dataset.request,
            });
            toast("추가 요청을 보냈습니다", "ok");
            btn.textContent = "요청 완료";
          } catch (ex) {
            toast(ex.message, "error");
            btn.disabled = false;
          }
        });
      });
    } catch (ex) {
      box.innerHTML = `<p class="empty">${escapeHtml(ex.message)}</p>`;
    }
  }

  function wireAdmin() {
    const selected = new Map();
    const bundleSelected = new Map();

    function showAdminTab(next) {
      root.querySelectorAll("[data-tab]").forEach((b) =>
        b.classList.toggle("is-active", b.dataset.tab === next)
      );
      root.querySelector("#tab-aladin")?.classList.toggle("is-hidden", next !== "aladin");
      root.querySelector("#tab-manual")?.classList.toggle("is-hidden", next !== "manual");
      root.querySelector("#tab-aliases")?.classList.toggle("is-hidden", next !== "aliases");
    }

    function renderAliasSelected() {
      const box = root.querySelector("#aa-selected");
      if (!box) return;
      if (!selected.size) {
        box.innerHTML = `<li class="muted">아직 선택한 작품이 없습니다.</li>`;
        return;
      }
      box.innerHTML = [...selected.entries()]
        .map(
          ([sid, title]) => `
        <li class="si-selected__item">
          <span>${escapeHtml(title)}</span>
          <button type="button" class="btn btn--ghost btn--sm" data-aa-remove="${escapeAttr(sid)}">제거</button>
        </li>`
        )
        .join("");
      box.querySelectorAll("[data-aa-remove]").forEach((btn) => {
        btn.addEventListener("click", () => {
          selected.delete(btn.dataset.aaRemove);
          renderAliasSelected();
        });
      });
    }

    async function searchAliasSeries() {
      const box = root.querySelector("#aa-series-results");
      const query = root.querySelector("#aa-series-q")?.value.trim() || "";
      if (!query) {
        toast("작품 검색어를 입력하세요", "error");
        return;
      }
      box.innerHTML = `<p class="muted">검색 중…</p>`;
      try {
        const results = await api.searchSeries(query);
        if (!results.length) {
          box.innerHTML = `<p class="empty">결과가 없습니다.</p>`;
          return;
        }
        box.innerHTML = `<ul class="result-list">${results
          .map(
            (r) => `
          <li class="import-card">
            ${cover(r.latest_cover_url, r.title, "cover cover--sm")}
            <div class="import-card__body">
              <h3>${escapeHtml(r.title)}</h3>
              <button type="button" class="btn btn--ghost btn--sm" data-aa-add="${escapeAttr(r.id)}" data-title="${escapeAttr(r.title)}" ${selected.has(r.id) ? "disabled" : ""}>
                ${selected.has(r.id) ? "선택됨" : "선택"}
              </button>
            </div>
          </li>`
          )
          .join("")}</ul>`;
        box.querySelectorAll("[data-aa-add]").forEach((btn) => {
          btn.addEventListener("click", () => {
            selected.set(btn.dataset.aaAdd, btn.dataset.title);
            btn.disabled = true;
            btn.textContent = "선택됨";
            renderAliasSelected();
          });
        });
      } catch (ex) {
        box.innerHTML = `<p class="empty">${escapeHtml(ex.message)}</p>`;
      }
    }

    root.querySelectorAll("[data-tab]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const next = btn.dataset.tab;
        const sp = new URLSearchParams(location.search);
        if (next === "manual") sp.set("tab", "manual");
        else if (next === "aliases") sp.set("tab", "aliases");
        else sp.delete("tab");
        history.replaceState(null, "", sp.toString() ? `/import?${sp}` : "/import");
        showAdminTab(next);
      });
    });

    root.querySelector("#import-form")?.addEventListener("submit", (e) => {
      e.preventDefault();
      const next = String(new FormData(e.target).get("q") || "");
      const sp = new URLSearchParams(location.search);
      sp.set("q", next);
      sp.delete("tab");
      history.pushState(null, "", `/import?${sp}`);
      loadAladinAdmin();
    });

    root.querySelector("#import-by-items")?.addEventListener("click", async () => {
      const raw = root.querySelector("#seed-item-ids")?.value || "";
      const seeds = raw
        .split(/[\s,;]+/)
        .map((s) => s.trim())
        .filter(Boolean);
      if (!seeds.length) {
        toast("ItemId 또는 상품 URL을 입력하세요", "error");
        return;
      }
      const btn = root.querySelector("#import-by-items");
      btn.disabled = true;
      btn.textContent = "가져오는 중…";
      try {
        const res = await api.importSeries({ seed_item_ids: seeds });
        toast(`가져왔습니다 (${res.volume_count}권)`, "ok");
        location.href = `/series/${res.series_id}`;
      } catch (ex) {
        toast(ex.message, "error");
        btn.disabled = false;
        btn.textContent = "ItemId로 가져오기";
      }
    });

    root.querySelector("#aa-series-search")?.addEventListener("click", () => searchAliasSeries());
    root.querySelector("#aa-series-q")?.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        searchAliasSeries();
      }
    });
    root.querySelector("#admin-alias-form")?.addEventListener("submit", async (e) => {
      e.preventDefault();
      const alias = root.querySelector("#aa-query")?.value.trim() || "";
      if (!alias) {
        toast("줄임말·별칭을 입력하세요", "error");
        return;
      }
      if (!selected.size) {
        toast("연결할 작품을 하나 이상 선택하세요", "error");
        return;
      }
      try {
        const res = await api.batchSearchAliases(alias, [...selected.keys()]);
        toast(`‘${alias}’을(를) ${res.series_count}편에 적용했습니다`, "ok");
        selected.clear();
        renderAliasSelected();
        const results = root.querySelector("#aa-series-results");
        if (results) results.innerHTML = "";
      } catch (ex) {
        toast(ex.message, "error");
      }
    });
    renderAliasSelected();

    function renderBundleSelected() {
      const box = root.querySelector("#ab-selected");
      if (!box) return;
      if (!bundleSelected.size) {
        box.innerHTML = `<li class="muted">아직 선택한 작품이 없습니다.</li>`;
        return;
      }
      box.innerHTML = [...bundleSelected.entries()]
        .map(
          ([sid, title]) => `
        <li class="si-selected__item">
          <span>${escapeHtml(title)}</span>
          <button type="button" class="btn btn--ghost btn--sm" data-ab-remove="${escapeAttr(sid)}">제거</button>
        </li>`
        )
        .join("");
      box.querySelectorAll("[data-ab-remove]").forEach((btn) => {
        btn.addEventListener("click", () => {
          bundleSelected.delete(btn.dataset.abRemove);
          renderBundleSelected();
        });
      });
    }

    async function searchBundleSeries() {
      const box = root.querySelector("#ab-series-results");
      const query = root.querySelector("#ab-series-q")?.value.trim() || "";
      if (!query) {
        toast("작품 검색어를 입력하세요", "error");
        return;
      }
      box.innerHTML = `<p class="muted">검색 중…</p>`;
      try {
        const results = await api.searchSeries(query);
        if (!results.length) {
          box.innerHTML = `<p class="empty">결과가 없습니다.</p>`;
          return;
        }
        box.innerHTML = `<ul class="result-list">${results
          .map(
            (r) => `
          <li class="import-card">
            ${cover(r.latest_cover_url, r.title, "cover cover--sm")}
            <div class="import-card__body">
              <h3>${escapeHtml(r.title)}</h3>
              <button type="button" class="btn btn--ghost btn--sm" data-ab-add="${escapeAttr(r.id)}" data-title="${escapeAttr(r.title)}" ${bundleSelected.has(r.id) ? "disabled" : ""}>
                ${bundleSelected.has(r.id) ? "선택됨" : "선택"}
              </button>
            </div>
          </li>`
          )
          .join("")}</ul>`;
        box.querySelectorAll("[data-ab-add]").forEach((btn) => {
          btn.addEventListener("click", () => {
            bundleSelected.set(btn.dataset.abAdd, btn.dataset.title);
            btn.disabled = true;
            btn.textContent = "선택됨";
            renderBundleSelected();
          });
        });
      } catch (ex) {
        box.innerHTML = `<p class="empty">${escapeHtml(ex.message)}</p>`;
      }
    }

    root.querySelector("#ab-series-search")?.addEventListener("click", () => searchBundleSeries());
    root.querySelector("#ab-series-q")?.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        searchBundleSeries();
      }
    });
    root.querySelector("#admin-bundle-form")?.addEventListener("submit", async (e) => {
      e.preventDefault();
      if (bundleSelected.size < 2) {
        toast("묶을 작품을 2편 이상 선택하세요", "error");
        return;
      }
      try {
        const res = await api.createSearchBundle([...bundleSelected.keys()]);
        toast(`${res.series_count}편을 묶었습니다`, "ok");
        bundleSelected.clear();
        renderBundleSelected();
        const results = root.querySelector("#ab-series-results");
        if (results) results.innerHTML = "";
      } catch (ex) {
        toast(ex.message, "error");
      }
    });
    renderBundleSelected();

    const volsBox = root.querySelector("#manual-vols");
    if (!volsBox) {
      if (tab === "aladin" && q) loadAladinAdmin();
      return;
    }
    function revealTitleEnd(input) {
      requestAnimationFrame(() => {
        input.scrollLeft = input.scrollWidth;
      });
    }
    function bindTitleReveal(input) {
      if (!input || input.dataset.titleRevealBound) return;
      input.dataset.titleRevealBound = "1";
      revealTitleEnd(input);
      input.addEventListener("blur", () => revealTitleEnd(input));
      input.addEventListener("input", () => {
        if (document.activeElement !== input) revealTitleEnd(input);
      });
    }
    function bindVolRows() {
      volsBox.querySelectorAll(".manual-vol-title").forEach(bindTitleReveal);
      volsBox.querySelectorAll("[data-remove-vol]").forEach((btn) => {
        btn.onclick = () => {
          if (volsBox.querySelectorAll(".manual-vol-row").length <= 1) return;
          btn.closest(".manual-vol-row")?.remove();
        };
      });
    }
    function readVolumes() {
      return [...volsBox.querySelectorAll(".manual-vol-row")].map((row) => ({
        volume_number: Number(row.querySelector(".manual-vol-num").value) || undefined,
        title: row.querySelector(".manual-vol-title").value.trim() || undefined,
        published_at: row.querySelector(".manual-vol-date").value || undefined,
        cover_url: row.querySelector(".manual-vol-cover").value.trim() || undefined,
      }));
    }
    root.querySelector("#add-vol-btn").addEventListener("click", () => {
      const idx = volsBox.querySelectorAll(".manual-vol-row").length;
      volsBox.insertAdjacentHTML("beforeend", volumeRowHtml({ volume_number: idx + 1 }, idx));
      bindVolRows();
    });
    root.querySelector("#quick-fill-btn").addEventListener("click", () => {
      const n = Number(root.querySelector("#quick-count").value);
      const title = root.querySelector("#manual-title").value.trim() || "작품";
      if (!n || n < 1) return toast("권 수를 입력하세요", "error");
      volsBox.innerHTML = Array.from({ length: Math.min(n, 200) }, (_, i) =>
        volumeRowHtml({ volume_number: i + 1, title: `${title} ${i + 1}권` }, i)
      ).join("");
      bindVolRows();
    });
    bindVolRows();

    root.querySelector("#manual-form").addEventListener("submit", async (e) => {
      e.preventDefault();
      const title = root.querySelector("#manual-title").value.trim();
      if (!title) return;
      const submit = root.querySelector("#manual-submit");
      submit.disabled = true;
      try {
        const res = await api.createManualSeries({
          title,
          author: root.querySelector("#manual-author").value.trim() || null,
          publisher: root.querySelector("#manual-publisher").value.trim() || null,
          cover_url: root.querySelector("#manual-cover").value.trim() || null,
          volumes: readVolumes(),
          force: true,
        });
        toast(`등록했습니다 (${res.volume_count}권)`, "ok");
        location.href = `/series/${res.series_id}`;
      } catch (ex) {
        toast(ex.message, "error");
      } finally {
        submit.disabled = false;
      }
    });

    if (tab === "aladin" && q) loadAladinAdmin();
  }

  async function loadAladinAdmin() {
    const box = root.querySelector("#import-results");
    const query = new URLSearchParams(location.search).get("q") || "";
    if (!query.trim()) {
      box.innerHTML = `<p class="muted">제목을 입력해 검색하세요.</p>`;
      return;
    }
    box.innerHTML = `<p class="muted">검색 중…</p>`;
    try {
      const results = await api.importSearch(query);
      if (!results.length) {
        box.innerHTML = `<p class="empty">결과가 없습니다.</p>`;
        return;
      }
      box.innerHTML = `<ul class="result-list">${results
        .map(
          (r) => `
        <li class="import-card">
          ${cover(r.cover_url, r.title, "cover cover--md")}
          <div class="import-card__body">
            <h3>${escapeHtml(r.title)}</h3>
            <p class="muted">${escapeHtml(r.author || "작가 미상")} · ${r.volume_count}권</p>
            <div class="import-card__actions">
              ${
                r.already_imported
                  ? `<a class="btn btn--ghost btn--sm" href="/series/${r.series_id}" data-link>보기</a>
                     <button type="button" class="btn btn--primary btn--sm" data-import="${escapeAttr(r.aladin_series_id)}" data-title="${escapeAttr(r.title)}">다시 가져오기</button>`
                  : `<button type="button" class="btn btn--primary btn--sm" data-import="${escapeAttr(r.aladin_series_id)}" data-title="${escapeAttr(r.title)}">가져오기</button>`
              }
            </div>
          </div>
        </li>`
        )
        .join("")}</ul>`;
      box.querySelectorAll("[data-import]").forEach((btn) => {
        btn.addEventListener("click", async () => {
          btn.disabled = true;
          try {
            const res = await api.importSeries({
              aladin_series_id: btn.dataset.import,
              title: btn.dataset.title,
            });
            toast(`가져왔습니다 (${res.volume_count}권)`, "ok");
            loadAladinAdmin();
          } catch (ex) {
            toast(ex.message, "error");
            btn.disabled = false;
          }
        });
      });
    } catch (ex) {
      box.innerHTML = `<p class="empty">${escapeHtml(ex.message)}</p>`;
    }
  }
}

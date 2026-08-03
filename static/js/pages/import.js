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
  const tab = params.get("tab") === "manual" ? "manual" : "aladin";
  const userTab = params.get("tab") === "other" ? "other" : "add";

  if (!isAdmin) {
    root.innerHTML = shell(
      { email: user?.email, active: "import", isAdmin: false },
      `<main class="page">
        <div class="page__head">
          <h1>요청</h1>
          <p class="page__lead">작품 추가·수정 제안이나 사소한 건의도 보내 주세요. 운영자가 확인합니다.</p>
          <div class="import-tabs" role="tablist">
            <button type="button" class="import-tabs__item ${userTab === "add" ? "is-active" : ""}" data-user-tab="add">작품 추가</button>
            <button type="button" class="import-tabs__item ${userTab === "other" ? "is-active" : ""}" data-user-tab="other">기타</button>
          </div>
        </div>

        <section id="tab-add" class="import-panel ${userTab === "add" ? "" : "is-hidden"}">
          <form class="search-form" id="import-form">
            <input name="q" type="search" placeholder="작품 제목 검색" value="${escapeHtml(q)}" ${userTab === "add" ? "autofocus" : ""} />
            <button type="submit" class="btn btn--primary">검색</button>
          </form>
          <div id="import-results"></div>
          <section class="panel" style="margin-top:1.5rem">
            <h2>직접 요청</h2>
            <form id="request-form" class="manual-form">
              <label class="manual-field">
                <span>제목 *</span>
                <input id="req-title" type="text" required placeholder="시리즈 제목" />
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
        <h1>작품 추가</h1>
        <p class="page__lead">관리자: 알라딘에서 가져오거나 직접 등록합니다.</p>
        <div class="import-tabs" role="tablist">
          <button type="button" class="import-tabs__item ${tab === "aladin" ? "is-active" : ""}" data-tab="aladin">알라딘</button>
          <button type="button" class="import-tabs__item ${tab === "manual" ? "is-active" : ""}" data-tab="manual">직접 등록</button>
        </div>
      </div>
      <section id="tab-aladin" class="import-panel ${tab === "aladin" ? "" : "is-hidden"}">
        <form class="search-form" id="import-form">
          <input name="q" type="search" placeholder="추가할 작품 제목" value="${escapeHtml(q)}" ${tab === "aladin" ? "autofocus" : ""} />
          <button type="submit" class="btn btn--primary">검색</button>
        </form>
        <div id="import-results"></div>
      </section>
      <section id="tab-manual" class="import-panel ${tab === "manual" ? "" : "is-hidden"}">
        <form id="manual-form" class="manual-form">
          <label class="manual-field">
            <span>제목 *</span>
            <input name="title" id="manual-title" type="text" required placeholder="시리즈 제목" />
          </label>
          <div class="manual-field-row">
            <label class="manual-field"><span>작가</span><input id="manual-author" type="text" /></label>
            <label class="manual-field"><span>출판사</span><input id="manual-publisher" type="text" /></label>
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
      <p class="muted">도서 정보 출처: <a href="https://www.aladin.co.kr/" target="_blank" rel="noopener noreferrer">알라딘 인터넷서점</a></p>
    </main>`
  );
  bindLogout();
  wireAdmin();

  function wireUserRequest() {
    root.querySelectorAll("[data-user-tab]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const next = btn.dataset.userTab;
        const sp = new URLSearchParams(location.search);
        if (next === "other") {
          sp.set("tab", "other");
          sp.delete("q");
        } else {
          sp.delete("tab");
        }
        history.replaceState(null, "", sp.toString() ? `/import?${sp}` : "/import");
        root.querySelectorAll("[data-user-tab]").forEach((b) =>
          b.classList.toggle("is-active", b.dataset.userTab === next)
        );
        root.querySelector("#tab-add").classList.toggle("is-hidden", next !== "add");
        root.querySelector("#tab-other").classList.toggle("is-hidden", next !== "other");
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
    root.querySelectorAll("[data-tab]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const next = btn.dataset.tab;
        const sp = new URLSearchParams(location.search);
        if (next === "manual") sp.set("tab", "manual");
        else sp.delete("tab");
        history.replaceState(null, "", sp.toString() ? `/import?${sp}` : "/import");
        root.querySelectorAll("[data-tab]").forEach((b) => b.classList.toggle("is-active", b.dataset.tab === next));
        root.querySelector("#tab-aladin").classList.toggle("is-hidden", next !== "aladin");
        root.querySelector("#tab-manual").classList.toggle("is-hidden", next !== "manual");
      });
    });

    root.querySelector("#import-form").addEventListener("submit", (e) => {
      e.preventDefault();
      const next = String(new FormData(e.target).get("q") || "");
      const sp = new URLSearchParams(location.search);
      sp.set("q", next);
      sp.delete("tab");
      history.pushState(null, "", `/import?${sp}`);
      loadAladinAdmin();
    });

    const volsBox = root.querySelector("#manual-vols");
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

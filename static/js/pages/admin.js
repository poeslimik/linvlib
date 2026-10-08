import { api } from "../api.js";
import { getUser } from "../auth.js";
import { navigate } from "../router.js";
import { shell, cover, escapeHtml, escapeAttr, formatDate, toast, bindLogout } from "../ui.js";

const TYPE_LABEL = {
  add: "추가",
  edit: "수정",
  delete: "삭제",
  other: "기타",
  search_improve: "검색 개선",
};
const STATUS_LABEL = {
  approved: "승인",
  rejected: "거절",
};
const TABS = [
  { id: "status", label: "상태" },
  { id: "requests", label: "요청" },
  { id: "suggestions", label: "추천" },
  { id: "manuals", label: "직접 등록" },
  { id: "users", label: "사용자" },
  { id: "backups", label: "백업" },
];

function tabFromHash() {
  const raw = (location.hash || "#status").replace(/^#/, "");
  return TABS.some((t) => t.id === raw) ? raw : "status";
}

function formatBytes(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(2)} MB`;
}

export async function renderAdmin(root) {
  const user = getUser();
  if (!user?.is_admin) {
    toast("관리자만 접근할 수 있습니다", "error");
    navigate("/series", { replace: true });
    return;
  }

  root.innerHTML = shell(
    { email: user.email, active: "admin", isAdmin: true },
    `<main class="page"><p class="muted">불러오는 중…</p></main>`
  );
  bindLogout();

  let tab = tabFromHash();
  let status = null;
  let requests = [];
  let requestHistory = [];
  let suggestions = [];
  let users = [];
  let manuals = [];
  let backups = [];
  let manualQuery = "";
  let manualFilter = "all";

  async function loadCore() {
    try {
      status = await api.adminStatus();
    } catch (ex) {
      root.querySelector("main").innerHTML = `<p class="empty">${escapeHtml(ex.message)}</p>`;
      return false;
    }
    return true;
  }

  async function loadRequests() {
    [requests, requestHistory] = await Promise.all([
      api.listAdminCatalogRequests("pending"),
      api.listAdminCatalogRequests("reviewed"),
    ]);
  }

  async function loadTabData() {
    try {
      if (tab === "requests") {
        await loadRequests();
      } else if (tab === "suggestions") {
        suggestions = await api.adminNewReleases();
      } else if (tab === "users") {
        users = await api.adminUsers();
      } else if (tab === "manuals") {
        manuals = await api.adminManualSeries();
      } else if (tab === "backups") {
        backups = await api.adminBackups();
      } else if (tab === "status") {
        status = await api.adminStatus();
      }
    } catch (ex) {
      toast(ex.message, "error");
    }
  }

  function filteredManuals() {
    const q = manualQuery.trim().toLowerCase();
    return manuals.filter((s) => {
      if (manualFilter === "no-cover" && s.latest_cover_url) return false;
      if (!q) return true;
      const hay = `${s.title} ${s.author || ""} ${s.publisher || ""}`.toLowerCase();
      return hay.includes(q);
    });
  }

  function tabNav() {
    const pending = status?.pending_requests || 0;
    const pendingSuggestions = status?.pending_suggestions ?? suggestions.length;
    const manualCount = status?.manual_series_count ?? manuals.length;
    const backupCount = status?.backup_count ?? backups.length;
    return `<nav class="admin-tabs" role="tablist" aria-label="관리 메뉴">
      ${TABS.map((t) => {
        let label = t.label;
        if (t.id === "requests" && pending) label += ` (${pending})`;
        if (t.id === "suggestions" && pendingSuggestions) label += ` (${pendingSuggestions})`;
        if (t.id === "manuals") label += ` (${manualCount})`;
        if (t.id === "backups") label += ` (${backupCount})`;
        return `<button type="button" role="tab" class="admin-tabs__item ${
          tab === t.id ? "is-active" : ""
        }" data-tab="${t.id}" aria-selected="${tab === t.id}">${label}</button>`;
      }).join("")}
    </nav>`;
  }

  function renderStatus() {
    if (!status) return `<p class="muted">상태를 불러오는 중…</p>`;
    return `
      <section class="panel" aria-labelledby="admin-status-title">
        <div class="admin-section-head">
          <h2 id="admin-status-title">상태</h2>
          <button type="button" class="btn btn--ghost btn--sm" id="admin-refresh" ${status.refresh_running ? "disabled" : ""} title="예스24 신간 목록을 백그라운드로 가져와 카탈로그를 갱신합니다">
            ${status.refresh_running ? "갱신 중…" : "지금 신간 갱신"}
          </button>
        </div>
        <dl class="detail-facts">
          <div><dt>서버 시각</dt><dd>${escapeHtml(status.server_time_kst || "—")}</dd></div>
          <div><dt>대기 요청</dt><dd>${status.pending_requests}</dd></div>
          <div><dt>신간 추천</dt><dd>${status.pending_suggestions ?? 0}</dd></div>
          <div><dt>직접 등록 작품</dt><dd>${status.manual_series_count}</dd></div>
          <div><dt>사용자</dt><dd>${status.user_count}</dd></div>
          <div><dt>신간 갱신</dt><dd>${status.refresh_running ? "진행 중" : "대기"}</dd></div>
          <div><dt>최근 갱신</dt><dd>${escapeHtml(status.last_refresh_at || "—")}</dd></div>
          <div><dt>스케줄 갱신일</dt><dd>${escapeHtml(status.last_scheduled_refresh_date || "—")}</dd></div>
          <div><dt>최근 백업</dt><dd>${escapeHtml(status.last_backup_at || "—")}</dd></div>
          <div><dt>보관 백업</dt><dd>${status.backup_count ?? 0}개 · ${status.backup_retain_days ?? 14}일</dd></div>
          <div><dt>예스24 API (오늘)</dt><dd>${status.yes24_quota_used ?? 0} / ${status.yes24_quota_soft_limit ?? 19500} <span class="muted">(한도 ${status.yes24_quota_hard_limit ?? 20000} · ${escapeHtml(status.yes24_quota_date || "—")})</span></dd></div>
        </dl>
        <p class="muted">${escapeHtml(status.last_refresh_note || "")}</p>
        <p class="muted">자동 갱신은 KST 기준입니다. 매일 23:30 예스24 신간으로 갱신·추천한 뒤 Discord 상태 보고(웹훅 설정 시)를 보내고, 자정에 DB를 백업합니다(최대 ${status.backup_retain_days ?? 14}일 보관). 예스24 호출은 초당 10회·일 소프트 ${status.yes24_quota_soft_limit ?? 19500}회로 제한합니다.</p>
      </section>`;
  }

  function renderSuggestions() {
    return `
      <section class="panel" aria-labelledby="admin-suggestions-title">
        <h2 id="admin-suggestions-title">신간 추천</h2>
        <p class="muted">예스24 신간 목록에 있으나 카탈로그에 없는 작품입니다. 가져오기 또는 숨길 수 있습니다.</p>
        ${
          suggestions.length
            ? `<ul class="request-list">${suggestions
                .map(
                  (s) => `
              <li class="request-item">
                <div class="admin-suggestion-row">
                  ${cover(s.cover_url, s.title, "cover cover--sm")}
                  <div>
                    <strong>${escapeHtml(s.title)}</strong>
                    <p class="muted">
                      ${escapeHtml(s.author || "작가 미상")}
                      ${s.publisher ? ` · ${escapeHtml(s.publisher)}` : ""}
                      ${s.pub_date ? ` · ${escapeHtml(s.pub_date)}` : ""}
                    </p>
                    <p class="muted">발견 ${formatDate(s.first_seen_at)} · 최근 ${formatDate(s.last_seen_at)}</p>
                  </div>
                </div>
                <div class="request-item__actions">
                  <button type="button" class="btn btn--primary btn--sm" data-import-suggestion="${s.id}">가져오기</button>
                  <button type="button" class="btn btn--ghost btn--sm" data-dismiss-suggestion="${s.id}">숨기기</button>
                </div>
              </li>`
                )
                .join("")}</ul>`
            : `<p class="muted">대기 중인 신간 추천이 없습니다. 「지금 신간 갱신」을 실행하면 채워집니다.</p>`
        }
      </section>`;
  }

  function renderRequestBody(r) {
    return `
      <div>
        <strong>${TYPE_LABEL[r.request_type] || r.request_type}</strong>
        · ${escapeHtml(r.title || r.series_title || (r.request_type === "other" ? "기타 요청" : "(제목 없음)"))}
        <span class="muted">${escapeHtml(r.user_email || "")}</span>
      </div>
      <p class="muted">${formatDate(r.created_at)}${r.aladin_series_id ? ` · ${escapeHtml(r.aladin_series_id)}` : ""}</p>
      ${
        r.request_type === "search_improve" && (r.related_series || []).length
          ? `<p class="muted">연결: ${(r.related_series || [])
              .map((s) => escapeHtml(s.title))
              .join(" · ")}</p>`
          : ""
      }
      ${r.note ? `<p>${escapeHtml(r.note)}</p>` : ""}`;
  }

  function renderPendingActions(r) {
    const showCatalogSearch = r.request_type !== "edit";
    return `
      <div class="request-item__actions">
        ${
          r.request_type === "add" && r.aladin_series_id
            ? `<button type="button" class="btn btn--primary btn--sm" data-import="${escapeAttr(r.aladin_series_id)}" data-title="${escapeAttr(r.title || "")}" data-rid="${r.id}">가져오기 후 승인</button>`
            : ""
        }
        ${
          r.request_type === "add" && !r.aladin_series_id
            ? `<a class="btn btn--primary btn--sm" href="/import?tab=manual&title=${encodeURIComponent(r.title || "")}&author=${encodeURIComponent(r.author || "")}&publisher=${encodeURIComponent(r.publisher || "")}" data-link>직접 등록</a>`
            : ""
        }
        <button type="button" class="btn btn--ghost btn--sm" data-approve="${r.id}">${
          r.request_type === "search_improve" ? "별칭 반영·승인" : "승인"
        }</button>
        <button type="button" class="btn btn--danger btn--sm" data-reject="${r.id}">거절</button>
        ${r.series_id ? `<a class="btn btn--ghost btn--sm" href="/series/${r.series_id}" data-link>작품</a>` : ""}
        ${(r.related_series || [])
          .slice(0, 3)
          .map(
            (s) =>
              `<a class="btn btn--ghost btn--sm" href="/series/${s.id}" data-link>${escapeHtml(s.title)}</a>`
          )
          .join("")}
        ${
          showCatalogSearch
            ? `<a class="btn btn--ghost btn--sm" href="/import?q=${encodeURIComponent(r.title || "")}" data-link>카탈로그 검색</a>`
            : ""
        }
      </div>`;
  }

  function renderHistoryItem(r) {
    const statusLabel = STATUS_LABEL[r.status] || r.status;
    const statusClass = r.status === "rejected" ? "badge-warn" : "";
    return `
      <li class="request-item request-item--history" data-id="${r.id}">
        ${renderRequestBody(r)}
        <p class="muted">
          <span class="${statusClass}">${escapeHtml(statusLabel)}</span>
          · 처리 ${formatDate(r.updated_at || r.created_at)}
          ${r.admin_note ? ` · ${escapeHtml(r.admin_note)}` : ""}
        </p>
        <div class="request-item__actions">
          ${r.series_id ? `<a class="btn btn--ghost btn--sm" href="/series/${r.series_id}" data-link>작품</a>` : ""}
          ${(r.related_series || [])
            .slice(0, 3)
            .map(
              (s) =>
                `<a class="btn btn--ghost btn--sm" href="/series/${s.id}" data-link>${escapeHtml(s.title)}</a>`
            )
            .join("")}
        </div>
      </li>`;
  }

  function renderRequests() {
    return `
      <section class="panel" aria-labelledby="admin-requests-title">
        <h2 id="admin-requests-title">대기 중인 요청</h2>
        ${
          requests.length
            ? `<ul class="request-list">${requests
                .map(
                  (r) => `
              <li class="request-item" data-id="${r.id}">
                ${renderRequestBody(r)}
                ${renderPendingActions(r)}
              </li>`
                )
                .join("")}</ul>`
            : `<p class="muted">대기 요청이 없습니다.</p>`
        }
      </section>
      <section class="panel admin-request-history" aria-labelledby="admin-request-history-title">
        <h2 id="admin-request-history-title">요청 기록${requestHistory.length ? ` (${requestHistory.length})` : ""}</h2>
        <p class="muted">승인·거절한 요청입니다. 최근 100건까지 표시합니다.</p>
        ${
          requestHistory.length
            ? `<ul class="request-list">${requestHistory.map(renderHistoryItem).join("")}</ul>`
            : `<p class="muted">처리한 요청이 없습니다.</p>`
        }
      </section>`;
  }

  function renderManuals() {
    const list = filteredManuals();
    return `
      <section class="panel" aria-labelledby="admin-manuals-title">
        <div class="admin-section-head">
          <h2 id="admin-manuals-title">직접 등록 작품</h2>
          <a class="btn btn--primary btn--sm" href="/import?tab=manual" data-link>새 작품 등록</a>
        </div>
        <div class="admin-manual-toolbar">
          <input type="search" id="manual-q" placeholder="제목·작가·출판사 검색" value="${escapeAttr(manualQuery)}" />
          <div class="filter-tabs" role="tablist" aria-label="직접 등록 필터">
            <button type="button" class="filter-tabs__item ${manualFilter === "all" ? "is-active" : ""}" data-manual-filter="all">전체</button>
            <button type="button" class="filter-tabs__item ${manualFilter === "no-cover" ? "is-active" : ""}" data-manual-filter="no-cover">표지 없음</button>
          </div>
        </div>
        <p class="muted admin-manual-meta">${list.length} / ${manuals.length}편</p>
        ${
          list.length
            ? `<ul class="admin-manual-list">${list
                .map(
                  (s) => `
              <li class="admin-manual-item">
                ${cover(s.latest_cover_url, s.title, "cover cover--sm")}
                <div class="admin-manual-item__body">
                  <div class="admin-manual-item__title">
                    <a href="/series/${s.id}" data-link>${escapeHtml(s.title)}</a>
                    ${s.latest_cover_url ? "" : `<span class="badge-warn">표지 없음</span>`}
                  </div>
                  <p class="muted">
                    ${escapeHtml(s.author || "작가 미상")}
                    ${s.publisher ? ` · ${escapeHtml(s.publisher)}` : ""}
                    · ${s.volume_count}권
                    · ${formatDate(s.created_at)}
                  </p>
                </div>
                <div class="admin-manual-item__actions">
                  <a class="btn btn--ghost btn--sm" href="/series/${s.id}?edit=1" data-link>수정</a>
                  <button type="button" class="btn btn--danger btn--sm" data-delete-manual="${s.id}" data-title="${escapeAttr(s.title)}">삭제</button>
                </div>
              </li>`
                )
                .join("")}</ul>`
            : `<p class="muted">${manuals.length ? "검색 결과가 없습니다." : "직접 등록한 작품이 없습니다."}</p>`
        }
      </section>`;
  }

  function renderUsers() {
    return `
      <section class="panel" aria-labelledby="admin-users-title">
        <h2 id="admin-users-title">사용자</h2>
        <ul class="request-list">
          ${
            users.length
              ? users
                  .map(
                    (u) => `
            <li class="request-item">
              ${escapeHtml(u.email)}
              ${u.is_admin ? " · 관리자" : ""}
              ${u.email_verified ? "" : " · 미인증"}
            </li>`
                  )
                  .join("")
              : `<li class="muted">사용자가 없습니다.</li>`
          }
        </ul>
      </section>`;
  }

  function renderBackups() {
    const retain = status?.backup_retain_days ?? 14;
    return `
      <section class="panel" aria-labelledby="admin-backups-title">
        <div class="admin-section-head">
          <h2 id="admin-backups-title">백업</h2>
          <button type="button" class="btn btn--primary btn--sm" id="backup-create">지금 백업</button>
        </div>
        <p class="muted">서비스 중지 없이 SQLite 스냅샷을 만듭니다. 매일 KST 자정 자동 백업 · ${retain}일 보관.</p>
        ${
          backups.length
            ? `<ul class="admin-backup-list">${backups
                .map(
                  (b) => `
              <li class="admin-backup-item">
                <div>
                  <strong>${escapeHtml(b.name)}</strong>
                  <p class="muted">${escapeHtml(b.modified_at)} · ${formatBytes(b.size_bytes)}</p>
                </div>
                <div class="admin-backup-item__actions">
                  <button type="button" class="btn btn--ghost btn--sm" data-download-backup="${escapeAttr(b.name)}">다운로드</button>
                  <button type="button" class="btn btn--danger btn--sm" data-delete-backup="${escapeAttr(b.name)}">삭제</button>
                </div>
              </li>`
                )
                .join("")}</ul>`
            : `<p class="muted">백업 파일이 없습니다. 「지금 백업」으로 만들 수 있습니다.</p>`
        }
      </section>`;
  }

  function panelHtml() {
    switch (tab) {
      case "requests":
        return renderRequests();
      case "suggestions":
        return renderSuggestions();
      case "manuals":
        return renderManuals();
      case "users":
        return renderUsers();
      case "backups":
        return renderBackups();
      default:
        return renderStatus();
    }
  }

  function paint({ focusManualSearch = false } = {}) {
    root.innerHTML = shell(
      { email: user.email, active: "admin", isAdmin: true },
      `<main class="page">
        <div class="page__head">
          <div class="page__head--row">
            <h1>관리</h1>
            <div class="admin-head-actions">
              <a class="btn btn--primary btn--sm" href="/import" data-link>카탈로그 추가</a>
              <a class="btn btn--ghost btn--sm" href="/import?tab=manual" data-link>직접 등록</a>
              <a class="btn btn--ghost btn--sm" href="/import?tab=aliases" data-link>검색</a>
            </div>
          </div>
        </div>
        ${tabNav()}
        <div class="admin-tab-panel" role="tabpanel">${panelHtml()}</div>
      </main>`
    );
    bindLogout();
    wire();
    if (focusManualSearch) {
      const input = root.querySelector("#manual-q");
      if (input) {
        input.focus();
        const len = input.value.length;
        input.setSelectionRange(len, len);
      }
    }
  }

  async function switchTab(next) {
    if (!TABS.some((t) => t.id === next)) return;
    tab = next;
    if (location.hash.replace(/^#/, "") !== tab) {
      history.replaceState(null, "", `#${tab}`);
    }
    paint();
    await loadTabData();
    paint();
  }

  function wire() {
    root.querySelectorAll("[data-tab]").forEach((btn) => {
      btn.addEventListener("click", () => switchTab(btn.dataset.tab));
    });

    const qInput = root.querySelector("#manual-q");
    qInput?.addEventListener("input", () => {
      manualQuery = qInput.value;
      paint({ focusManualSearch: true });
    });

    root.querySelectorAll("[data-manual-filter]").forEach((btn) => {
      btn.addEventListener("click", () => {
        manualFilter = btn.dataset.manualFilter;
        paint({ focusManualSearch: true });
      });
    });

    root.querySelector("#admin-refresh")?.addEventListener("click", async () => {
      if (!confirm("예스24 신간 목록을 가져와 카탈로그 작품을 갱신할까요?\n목록에 없는 신간은 추천 탭에 추가됩니다.\n(백그라운드로 실행되며 완료까지 수 분 걸릴 수 있습니다)")) return;
      const btn = root.querySelector("#admin-refresh");
      btn.disabled = true;
      btn.textContent = "갱신 중…";
      try {
        const start = await api.adminRefresh();
        toast(start.message || "신간 갱신을 시작했습니다.", "ok");
        const done = await api.waitForRefreshIdle();
        const note = done.last_refresh_note ? ` · ${done.last_refresh_note}` : "";
        toast(`신간 갱신 완료${note}`, "ok");
        status = done;
        await loadTabData();
        paint();
      } catch (ex) {
        if (ex.stillRunning) {
          toast(ex.message, "info");
          status = ex.status || status;
        } else {
          toast(ex.message, "error");
          try {
            status = await api.adminStatus();
          } catch {
            /* ignore */
          }
        }
        paint();
      }
    });

    root.querySelectorAll("[data-import-suggestion]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        btn.disabled = true;
        try {
          const res = await api.importNewRelease(btn.dataset.importSuggestion);
          toast(`가져오기 완료 (${res.volume_count}권)`, "ok");
          status = await api.adminStatus();
          suggestions = await api.adminNewReleases();
          paint();
        } catch (ex) {
          toast(ex.message, "error");
          btn.disabled = false;
        }
      });
    });

    root.querySelectorAll("[data-dismiss-suggestion]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        btn.disabled = true;
        try {
          await api.dismissNewRelease(btn.dataset.dismissSuggestion);
          toast("추천에서 숨겼습니다", "ok");
          status = await api.adminStatus();
          suggestions = await api.adminNewReleases();
          paint();
        } catch (ex) {
          toast(ex.message, "error");
          btn.disabled = false;
        }
      });
    });

    root.querySelector("#backup-create")?.addEventListener("click", async () => {
      const btn = root.querySelector("#backup-create");
      btn.disabled = true;
      btn.textContent = "백업 중…";
      try {
        const info = await api.adminCreateBackup();
        toast(`백업 완료: ${info.name}`, "ok");
        status = await api.adminStatus();
        backups = await api.adminBackups();
        paint();
      } catch (ex) {
        toast(ex.message, "error");
        btn.disabled = false;
        btn.textContent = "지금 백업";
      }
    });

    root.querySelectorAll("[data-download-backup]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        btn.disabled = true;
        try {
          await api.adminDownloadBackup(btn.dataset.downloadBackup);
          toast("다운로드를 시작했습니다", "ok");
        } catch (ex) {
          toast(ex.message, "error");
        } finally {
          btn.disabled = false;
        }
      });
    });

    root.querySelectorAll("[data-delete-backup]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const name = btn.dataset.deleteBackup;
        if (!confirm(`‘${name}’ 백업을 삭제할까요?`)) return;
        btn.disabled = true;
        try {
          await api.adminDeleteBackup(name);
          toast("삭제했습니다", "ok");
          status = await api.adminStatus();
          backups = await api.adminBackups();
          paint();
        } catch (ex) {
          toast(ex.message, "error");
          btn.disabled = false;
        }
      });
    });

    root.querySelectorAll("[data-delete-manual]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const title = btn.dataset.title || "이 작품";
        if (!confirm(`‘${title}’을(를) 삭제할까요? 읽음·평가·티어 기록도 함께 삭제됩니다.`)) {
          return;
        }
        btn.disabled = true;
        try {
          await api.deleteManualSeries(btn.dataset.deleteManual);
          toast("삭제했습니다", "ok");
          status = await api.adminStatus();
          manuals = await api.adminManualSeries();
          paint();
        } catch (ex) {
          toast(ex.message, "error");
          btn.disabled = false;
        }
      });
    });

    root.querySelectorAll("[data-approve]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        try {
          await api.reviewCatalogRequest(btn.dataset.approve, { status: "approved" });
          toast("승인했습니다", "ok");
          status = await api.adminStatus();
          await loadRequests();
          paint();
        } catch (ex) {
          toast(ex.message, "error");
        }
      });
    });

    root.querySelectorAll("[data-reject]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const note = prompt("거절 사유 (선택)");
        if (note === null) return;
        try {
          await api.reviewCatalogRequest(btn.dataset.reject, {
            status: "rejected",
            admin_note: note.trim() || null,
          });
          toast("거절했습니다", "ok");
          status = await api.adminStatus();
          await loadRequests();
          paint();
        } catch (ex) {
          toast(ex.message, "error");
        }
      });
    });

    root.querySelectorAll("[data-import]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        btn.disabled = true;
        try {
          const res = await api.importSeries({
            aladin_series_id: btn.dataset.import,
            title: btn.dataset.title || null,
          });
          await api.reviewCatalogRequest(btn.dataset.rid, {
            status: "approved",
            admin_note: `imported ${res.series_id}`,
          });
          toast(`가져오기 완료 (${res.volume_count}권)`, "ok");
          status = await api.adminStatus();
          await loadRequests();
          paint();
        } catch (ex) {
          toast(ex.message, "error");
          btn.disabled = false;
        }
      });
    });
  }

  window.addEventListener("hashchange", () => {
    const next = tabFromHash();
    if (next !== tab) switchTab(next);
  });

  if (!(await loadCore())) return;
  paint();
  await loadTabData();
  paint();
}

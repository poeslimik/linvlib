import { api } from "../api.js";
import { getUser } from "../auth.js";
import { navigate } from "../router.js";
import { shell, cover, escapeHtml, escapeAttr, formatDate, toast, bindLogout } from "../ui.js";

const TYPE_LABEL = { add: "추가", edit: "수정", delete: "삭제", other: "기타" };
const TABS = [
  { id: "status", label: "상태" },
  { id: "requests", label: "요청" },
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

  async function loadTabData() {
    try {
      if (tab === "requests") {
        requests = await api.listAdminCatalogRequests("pending");
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
    const manualCount = status?.manual_series_count ?? manuals.length;
    const backupCount = status?.backup_count ?? backups.length;
    return `<nav class="admin-tabs" role="tablist" aria-label="관리 메뉴">
      ${TABS.map((t) => {
        let label = t.label;
        if (t.id === "requests" && pending) label += ` (${pending})`;
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
          <button type="button" class="btn btn--ghost btn--sm" id="admin-refresh" title="연재중·완결(번역 미완)만 대상. 완결·번역 중단·연재 중단 등은 제외">지금 신간 갱신</button>
        </div>
        <dl class="detail-facts">
          <div><dt>서버 시각</dt><dd>${escapeHtml(status.server_time_kst || "—")}</dd></div>
          <div><dt>오늘 쿼터 (${escapeHtml(status.quota_date)})</dt><dd>사용 ${status.quota_used} · 남음 ${status.quota_remaining ?? Math.max(0, status.quota_soft - status.quota_used)} / 소프트 ${status.quota_soft} (한도 ${status.quota_hard})</dd></div>
          <div><dt>대기 요청</dt><dd>${status.pending_requests}</dd></div>
          <div><dt>직접 등록 작품</dt><dd>${status.manual_series_count}</dd></div>
          <div><dt>사용자</dt><dd>${status.user_count}</dd></div>
          <div><dt>최근 갱신</dt><dd>${escapeHtml(status.last_refresh_at || "—")}</dd></div>
          <div><dt>스케줄 갱신일</dt><dd>${escapeHtml(status.last_scheduled_refresh_date || "—")}</dd></div>
          <div><dt>최근 백업</dt><dd>${escapeHtml(status.last_backup_at || "—")}</dd></div>
          <div><dt>보관 백업</dt><dd>${status.backup_count ?? 0}개 · ${status.backup_retain_days ?? 14}일</dd></div>
        </dl>
        <p class="muted">${escapeHtml(status.last_refresh_note || "")}</p>
        ${
          Array.isArray(status.recent_quota) && status.recent_quota.length
            ? `<p class="muted">최근 사용: ${status.recent_quota
                .map((d) => `${escapeHtml(d.date)} ${d.used}`)
                .join(" · ")}</p>`
            : ""
        }
        <p class="muted">쿼터·자동 갱신은 KST 기준입니다. 매일 23:30에 남은 소프트 쿼터로 신간 갱신(자정에 중단), 자정에 DB 백업(최대 ${status.backup_retain_days ?? 14}일 보관)이 돌아갑니다.</p>
      </section>`;
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
                <div>
                  <strong>${TYPE_LABEL[r.request_type] || r.request_type}</strong>
                  · ${escapeHtml(r.title || r.series_title || (r.request_type === "other" ? "기타 요청" : "(제목 없음)"))}
                  <span class="muted">${escapeHtml(r.user_email || "")}</span>
                </div>
                <p class="muted">${formatDate(r.created_at)}${r.aladin_series_id ? ` · ${escapeHtml(r.aladin_series_id)}` : ""}</p>
                ${r.note ? `<p>${escapeHtml(r.note)}</p>` : ""}
                <div class="request-item__actions">
                  ${
                    r.request_type === "add" && r.aladin_series_id
                      ? `<button type="button" class="btn btn--primary btn--sm" data-import="${escapeAttr(r.aladin_series_id)}" data-title="${escapeAttr(r.title || "")}" data-rid="${r.id}">가져오기 후 승인</button>`
                      : ""
                  }
                  <button type="button" class="btn btn--ghost btn--sm" data-approve="${r.id}">승인</button>
                  <button type="button" class="btn btn--danger btn--sm" data-reject="${r.id}">거절</button>
                  ${r.series_id ? `<a class="btn btn--ghost btn--sm" href="/series/${r.series_id}" data-link>작품</a>` : ""}
                  <a class="btn btn--ghost btn--sm" href="/import?q=${encodeURIComponent(r.title || "")}" data-link>검색</a>
                </div>
              </li>`
                )
                .join("")}</ul>`
            : `<p class="muted">대기 요청이 없습니다.</p>`
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
              <a class="btn btn--ghost btn--sm" href="/import?tab=manual" data-link>직접 등록</a>
            </div>
          </div>
          <p class="page__lead">상태 · 요청 · 카탈로그 · 사용자 · 백업을 탭으로 나눕니다.</p>
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
      if (!confirm("알라딘 작품 일괄 갱신을 지금 실행할까요? (쿼터 소모)")) return;
      const btn = root.querySelector("#admin-refresh");
      btn.disabled = true;
      btn.textContent = "갱신 중…";
      try {
        const res = await api.adminRefresh();
        toast(`갱신 ${res.refreshed}/${res.total} · 실패 ${res.failed}`, "ok");
        await loadTabData();
        paint();
      } catch (ex) {
        toast(ex.message, "error");
        btn.disabled = false;
        btn.textContent = "지금 신간 갱신";
      }
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
          requests = await api.listAdminCatalogRequests("pending");
          paint();
        } catch (ex) {
          toast(ex.message, "error");
        }
      });
    });

    root.querySelectorAll("[data-reject]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const note = prompt("거절 사유 (선택)") || null;
        try {
          await api.reviewCatalogRequest(btn.dataset.reject, {
            status: "rejected",
            admin_note: note,
          });
          toast("거절했습니다", "ok");
          status = await api.adminStatus();
          requests = await api.listAdminCatalogRequests("pending");
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
          requests = await api.listAdminCatalogRequests("pending");
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

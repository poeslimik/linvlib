import { api } from "../api.js";
import { getUser, clearAuth } from "../auth.js";
import { navigate } from "../router.js";
import { shell, escapeHtml, formatDate, toast, bindLogout } from "../ui.js";

const TYPE_LABEL = { add: "추가", edit: "수정", delete: "삭제", other: "기타" };
const STATUS_LABEL = {
  pending: "대기",
  approved: "승인",
  rejected: "거절",
};

export async function renderMypage(root) {
  const user = getUser();
  root.innerHTML = shell(
    { email: user?.email, active: "mypage", isAdmin: !!user?.is_admin },
    `<main class="page"><p class="muted">불러오는 중…</p></main>`
  );
  bindLogout();

  let me;
  let requests;
  try {
    me = await api.me();
    requests = await api.listMyCatalogRequests();
  } catch (ex) {
    root.querySelector("main").innerHTML = `<p class="empty">${escapeHtml(ex.message)}</p>`;
    return;
  }

  root.innerHTML = shell(
    { email: me.email, active: "mypage", isAdmin: !!me.is_admin },
    `<main class="page">
      <div class="page__head">
        <h1>마이페이지</h1>
        <p class="page__lead">계정 정보와 요청 현황을 확인합니다.</p>
      </div>
      <section class="panel">
        <h2>계정</h2>
        <dl class="detail-facts">
          <div><dt>이메일</dt><dd>${escapeHtml(me.email)}</dd></div>
          <div><dt>인증</dt><dd>${me.email_verified ? "완료" : "미완료"}</dd></div>
          <div><dt>역할</dt><dd>${me.is_admin ? "관리자" : "일반 사용자"}</dd></div>
        </dl>
      </section>
      <section class="panel">
        <div class="page__head--row">
          <h2>내 요청</h2>
          <a class="btn btn--ghost btn--sm" href="/import" data-link>새 요청</a>
        </div>
        <div id="req-list">
          ${
            requests.length
              ? `<ul class="request-list">${requests
                  .map(
                    (r) => `
                <li class="request-item">
                  <div>
                    <strong>${TYPE_LABEL[r.request_type] || r.request_type}</strong>
                    · ${escapeHtml(r.title || r.series_title || (r.request_type === "other" ? "기타 요청" : "(제목 없음)"))}
                    <span class="request-status request-status--${escapeHtml(r.status)}">${STATUS_LABEL[r.status] || r.status}</span>
                  </div>
                  <p class="muted">${formatDate(r.created_at)}${r.note ? ` · ${escapeHtml(r.note)}` : ""}</p>
                  ${r.admin_note ? `<p class="muted">운영자: ${escapeHtml(r.admin_note)}</p>` : ""}
                </li>`
                  )
                  .join("")}</ul>`
              : `<p class="muted">요청 내역이 없습니다.</p>`
          }
        </div>
      </section>
      <section class="panel panel--danger">
        <h2>회원 탈퇴</h2>
        <p class="muted">탈퇴하면 읽음·평가·티어리스트·카탈로그 요청 기록이 모두 삭제되며 복구할 수 없습니다. 공유 작품 카탈로그는 삭제되지 않습니다.</p>
        <form id="delete-form" class="manual-form" style="max-width:28rem">
          <label class="manual-field">
            <span>비밀번호 확인</span>
            <input type="password" id="delete-password" required minlength="8" autocomplete="current-password" />
          </label>
          <button type="submit" class="btn btn--danger" id="delete-submit">탈퇴하기</button>
        </form>
      </section>
    </main>`
  );
  bindLogout();

  root.querySelector("#delete-form").addEventListener("submit", async (e) => {
    e.preventDefault();
    if (
      !confirm(
        "정말 탈퇴할까요? 읽음·평가·티어리스트·요청 기록이 영구 삭제됩니다."
      )
    ) {
      return;
    }
    const password = root.querySelector("#delete-password").value;
    const btn = root.querySelector("#delete-submit");
    btn.disabled = true;
    try {
      await api.deleteAccount(password);
      clearAuth();
      toast("탈퇴가 완료되었습니다", "ok");
      navigate("/login", { replace: true });
    } catch (ex) {
      toast(ex.message, "error");
      btn.disabled = false;
    }
  });
}

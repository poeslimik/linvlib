import { api } from "../api.js";
import { setAuth } from "../auth.js";
import { navigate } from "../router.js";
import { escapeHtml, toast } from "../ui.js";

export async function renderResetPassword(root) {
  const token = new URLSearchParams(location.search).get("token") || "";

  root.innerHTML = `
    <main class="landing">
      <div class="landing__atmosphere" aria-hidden="true"></div>
      <div class="landing__stage">
        <p class="landing__brand">linvlib</p>
        <h1 class="landing__title">비밀번호 재설정</h1>
        <p class="landing__lead" id="reset-lead">새 비밀번호를 입력해 주세요.</p>
        <form class="auth-form auth-form--inline" id="reset-form">
          <label>
            <span>새 비밀번호</span>
            <input name="password" type="password" required minlength="8" autocomplete="new-password" />
          </label>
          <label>
            <span>새 비밀번호 확인</span>
            <input name="password2" type="password" required minlength="8" autocomplete="new-password" />
          </label>
          <p class="auth-error" id="reset-error" hidden></p>
          <div class="auth-actions">
            <a class="btn btn--ghost" href="/login" data-link>로그인</a>
            <button type="submit" class="btn btn--primary" id="reset-submit">변경</button>
          </div>
        </form>
      </div>
    </main>
  `;

  const form = root.querySelector("#reset-form");
  const err = root.querySelector("#reset-error");
  const lead = root.querySelector("#reset-lead");
  const submit = root.querySelector("#reset-submit");

  if (!token) {
    lead.textContent = "재설정 토큰이 없습니다. 비밀번호 찾기에서 메일을 다시 요청해 주세요.";
    form.hidden = true;
    return;
  }

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    err.hidden = true;
    const fd = new FormData(form);
    const password = String(fd.get("password") || "");
    const password2 = String(fd.get("password2") || "");
    if (password !== password2) {
      err.textContent = "비밀번호가 일치하지 않습니다";
      err.hidden = false;
      return;
    }
    submit.disabled = true;
    try {
      const res = await api.resetPassword(token, password);
      setAuth(res.access_token);
      const me = await api.me();
      setAuth(res.access_token, me);
      toast("비밀번호가 변경되었습니다", "ok");
      navigate("/series", { replace: true });
    } catch (ex) {
      err.textContent = escapeHtml(ex.message);
      err.hidden = false;
    } finally {
      submit.disabled = false;
    }
  });
}

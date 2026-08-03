import { api } from "../api.js";
import { setAuth } from "../auth.js";
import { navigate } from "../router.js";
import { escapeHtml, toast } from "../ui.js";

export async function renderVerify(root) {
  const token = new URLSearchParams(location.search).get("token") || "";
  root.innerHTML = `
    <main class="landing">
      <div class="landing__stage">
        <p class="landing__brand">linvlib</p>
        <h1 class="landing__title">이메일 인증</h1>
        <p class="landing__lead" id="verify-msg">인증 처리 중…</p>
        <div class="landing__cta">
          <a class="btn btn--primary" href="/login" data-link>로그인</a>
        </div>
      </div>
    </main>
  `;
  const msg = root.querySelector("#verify-msg");
  if (!token) {
    msg.textContent = "인증 토큰이 없습니다.";
    return;
  }
  try {
    const res = await api.verifyEmail(token);
    setAuth(res.access_token);
    const me = await api.me();
    setAuth(res.access_token, me);
    msg.textContent = "인증이 완료되었습니다. 잠시 후 이동합니다…";
    toast("이메일 인증 완료", "ok");
    setTimeout(() => navigate("/series", { replace: true }), 800);
  } catch (ex) {
    msg.textContent = escapeHtml(ex.message);
  }
}

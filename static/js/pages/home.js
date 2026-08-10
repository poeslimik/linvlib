import { api } from "../api.js";
import { setAuth, isLoggedIn } from "../auth.js";
import { navigate } from "../router.js";
import { escapeHtml, toast } from "../ui.js";

export async function renderHome(root) {
  if (isLoggedIn()) {
    navigate("/series", { replace: true });
    return;
  }

  root.innerHTML = `
    <main class="landing">
      <div class="landing__atmosphere" aria-hidden="true"></div>
      <div class="landing__stage">
        <p class="landing__brand">linvlib</p>
        <h1 class="landing__title">라이트노벨 도서관을<br />내 손으로</h1>
        <p class="landing__lead">읽은 권수를 기록하고, 평가하고, 나만의 티어리스트를 만드세요.</p>
        <div class="landing__cta">
          <button type="button" class="btn btn--primary" data-mode="login">로그인</button>
          <button type="button" class="btn btn--secondary" data-mode="register">회원가입</button>
        </div>
        <p class="landing__attr">도서 정보 출처: <a href="https://www.aladin.co.kr/" target="_blank" rel="noopener noreferrer">알라딘 인터넷서점</a></p>
        <p class="landing__legal">
          <a href="/terms" data-link>이용약관</a>
          <a href="/privacy" data-link>개인정보처리방침</a>
        </p>
      </div>
      <aside class="landing__visual" aria-hidden="true">
        <div class="landing__spine landing__spine--1">
          <div class="landing__cover">
            <div class="landing__bars"><span></span><span></span><span></span></div>
            <span class="landing__vol">1</span>
          </div>
          <div class="landing__obi"></div>
        </div>
        <div class="landing__spine landing__spine--2">
          <div class="landing__cover">
            <div class="landing__bars"><span></span><span></span><span></span></div>
            <span class="landing__vol">3</span>
          </div>
          <div class="landing__obi"></div>
        </div>
        <div class="landing__spine landing__spine--3">
          <div class="landing__cover">
            <div class="landing__bars"><span></span><span></span><span></span></div>
            <span class="landing__vol">2</span>
          </div>
          <div class="landing__obi"></div>
        </div>
        <div class="landing__spine landing__spine--4">
          <div class="landing__cover">
            <div class="landing__bars"><span></span><span></span><span></span></div>
            <span class="landing__vol">4</span>
          </div>
          <div class="landing__obi"></div>
        </div>
      </aside>

      <dialog class="auth-dialog" id="auth-dialog">
        <form class="auth-form" id="auth-form" method="dialog">
          <h2 id="auth-title">로그인</h2>
          <label id="auth-email-label">
            <span>이메일</span>
            <input name="email" type="email" required autocomplete="email" />
          </label>
          <label id="auth-password-label">
            <span>비밀번호</span>
            <input name="password" type="password" required minlength="8" autocomplete="current-password" />
          </label>
          <p class="auth-forgot" id="auth-forgot-wrap">
            <button type="button" class="auth-forgot__btn" id="auth-forgot">비밀번호 찾기</button>
          </p>
          <div class="auth-consent" id="auth-consent" hidden>
            <label>
              <input name="accept_terms" type="checkbox" value="1" />
              <span><a href="/terms" target="_blank" rel="noopener noreferrer">이용약관</a>에 동의합니다 (필수)</span>
            </label>
            <label>
              <input name="accept_privacy" type="checkbox" value="1" />
              <span><a href="/privacy" target="_blank" rel="noopener noreferrer">개인정보처리방침</a>에 동의합니다 (필수)</span>
            </label>
          </div>
          <p class="auth-error" id="auth-error" hidden></p>
          <p class="muted auth-hint" id="auth-hint" hidden></p>
          <div class="auth-actions">
            <button type="button" class="btn btn--ghost" id="auth-cancel">닫기</button>
            <button type="button" class="btn btn--ghost" id="auth-resend" hidden>인증 메일 재발송</button>
            <button type="button" class="btn btn--ghost" id="auth-back-login" hidden>로그인으로</button>
            <button type="submit" class="btn btn--primary" id="auth-submit">확인</button>
          </div>
        </form>
      </dialog>
    </main>
  `;

  const dialog = root.querySelector("#auth-dialog");
  const form = root.querySelector("#auth-form");
  const title = root.querySelector("#auth-title");
  const err = root.querySelector("#auth-error");
  const hint = root.querySelector("#auth-hint");
  const resendBtn = root.querySelector("#auth-resend");
  const backLoginBtn = root.querySelector("#auth-back-login");
  const forgotWrap = root.querySelector("#auth-forgot-wrap");
  const passwordLabel = root.querySelector("#auth-password-label");
  const passwordInput = form.querySelector('[name="password"]');
  const consent = root.querySelector("#auth-consent");
  let mode = "login";
  let lastEmail = "";

  function setMode(next) {
    mode = next;
    title.textContent =
      mode === "login" ? "로그인" : mode === "register" ? "회원가입" : "비밀번호 찾기";
    err.hidden = true;
    hint.hidden = true;
    resendBtn.hidden = true;
    backLoginBtn.hidden = mode !== "forgot";
    forgotWrap.hidden = mode !== "login";
    const showConsent = mode === "register";
    consent.hidden = !showConsent;
    consent.classList.toggle("is-visible", showConsent);
    form.querySelector('[name="accept_terms"]').checked = false;
    form.querySelector('[name="accept_privacy"]').checked = false;

    const showPassword = mode !== "forgot";
    passwordLabel.hidden = !showPassword;
    passwordInput.required = showPassword;
    passwordInput.disabled = !showPassword;
    if (!showPassword) passwordInput.value = "";

    passwordInput.autocomplete =
      mode === "login" ? "current-password" : mode === "register" ? "new-password" : "off";
    root.querySelector("#auth-submit").textContent =
      mode === "forgot" ? "재설정 메일 보내기" : "확인";
  }

  root.querySelectorAll("[data-mode]").forEach((btn) => {
    btn.addEventListener("click", () => {
      form.reset();
      setMode(btn.dataset.mode);
      dialog.showModal();
    });
  });

  root.querySelector("#auth-forgot").addEventListener("click", () => {
    const email = String(new FormData(form).get("email") || "").trim();
    setMode("forgot");
    if (email) form.querySelector('[name="email"]').value = email;
  });

  backLoginBtn.addEventListener("click", () => {
    const email = String(new FormData(form).get("email") || "").trim();
    setMode("login");
    if (email) form.querySelector('[name="email"]').value = email;
  });

  root.querySelector("#auth-cancel").addEventListener("click", () => dialog.close());

  resendBtn.addEventListener("click", async () => {
    const email = String(new FormData(form).get("email") || lastEmail).trim();
    if (!email) return;
    try {
      const res = await api.resendVerification(email);
      hint.textContent = res.message || "인증 메일을 다시 보냈습니다.";
      hint.hidden = false;
      if (res.verification_token) {
        hint.textContent += ` (개발용 토큰: /verify?token=${res.verification_token})`;
      }
      toast("인증 메일을 다시 보냈습니다", "ok");
    } catch (ex) {
      err.textContent = escapeHtml(ex.message);
      err.hidden = false;
    }
  });

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    err.hidden = true;
    hint.hidden = true;
    const fd = new FormData(form);
    const email = String(fd.get("email") || "").trim();
    const password = String(fd.get("password") || "");
    lastEmail = email;
    const submit = root.querySelector("#auth-submit");
    submit.disabled = true;
    try {
      if (mode === "forgot") {
        const res = await api.forgotPassword(email);
        hint.textContent = res.message || "메일을 확인해 주세요.";
        if (res.reset_token) {
          hint.innerHTML = `${escapeHtml(res.message)} <a href="/reset-password?token=${escapeHtml(res.reset_token)}">재설정하러 가기</a>`;
        }
        hint.hidden = false;
        toast("안내 메일을 보냈습니다", "info");
      } else if (mode === "login") {
        const res = await api.login(email, password);
        setAuth(res.access_token);
        const me = await api.me();
        setAuth(res.access_token, me);
        dialog.close();
        toast("환영합니다", "ok");
        navigate("/series");
      } else {
        const accept_terms = fd.get("accept_terms") === "1";
        const accept_privacy = fd.get("accept_privacy") === "1";
        if (!accept_terms || !accept_privacy) {
          err.textContent = "약관 및 개인정보 처리방침에 동의해 주세요";
          err.hidden = false;
          return;
        }
        const res = await api.register(email, password, {
          accept_terms,
          accept_privacy,
        });
        if (res.access_token) {
          setAuth(res.access_token);
          const me = await api.me();
          setAuth(res.access_token, me);
          dialog.close();
          toast(res.message || "가입이 완료되었습니다", "ok");
          navigate("/series");
        } else {
          hint.textContent = res.message || "이메일 인증이 필요합니다.";
          if (res.verification_token) {
            hint.innerHTML = `${escapeHtml(res.message)} <a href="/verify?token=${escapeHtml(res.verification_token)}">인증하러 가기</a>`;
          }
          hint.hidden = false;
          resendBtn.hidden = false;
          toast("이메일을 확인해 주세요", "info");
        }
      }
    } catch (ex) {
      err.textContent = escapeHtml(ex.message);
      err.hidden = false;
      if (ex.status === 403 && mode === "login") resendBtn.hidden = false;
    } finally {
      submit.disabled = false;
    }
  });
}

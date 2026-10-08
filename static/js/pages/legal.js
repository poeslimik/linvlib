import { isLoggedIn } from "../auth.js";

const CONTACT_EMAIL = ""; // 공개 문의 메일 (비우면 문구만 표시)
const LEGAL_VERSION = "2026-09-24";

function contactHtml() {
  if (CONTACT_EMAIL) {
    return `문의: <a href="mailto:${CONTACT_EMAIL}">${CONTACT_EMAIL}</a>`;
  }
  return "문의: 서비스 관리자";
}

function legalShell(title, active, bodyHtml) {
  return `
    <main class="legal-page">
      <header class="legal-page__header">
        <a class="legal-page__brand" href="/login" data-link>linvlib</a>
        <nav class="legal-page__nav">
          <a href="/terms" data-link class="${active === "terms" ? "is-active" : ""}">이용약관</a>
          <a href="/privacy" data-link class="${active === "privacy" ? "is-active" : ""}">개인정보처리방침</a>
          <a href="${isLoggedIn() ? "/series" : "/login"}" data-link>${isLoggedIn() ? "서비스로" : "로그인"}</a>
        </nav>
      </header>
      <article class="legal-doc">
        <h1>${title}</h1>
        <p class="legal-doc__meta">시행일 · 문서 버전 ${LEGAL_VERSION}</p>
        ${bodyHtml}
      </article>
    </main>
  `;
}

export async function renderTerms(root) {
  root.innerHTML = legalShell(
    "이용약관",
    "terms",
    `
    <section>
      <h2>1. 목적</h2>
      <p>본 약관은 linvlib(이하 “서비스”)가 제공하는 라이트노벨 읽기 기록·평가·티어리스트 기능의 이용 조건과 운영자·이용자의 권리·의무를 정합니다.</p>
    </section>
    <section>
      <h2>2. 서비스 내용</h2>
      <p>서비스는 이용자가 작품·권 단위로 읽음 여부를 기록하고, 평가를 남기며, 티어리스트를 구성할 수 있도록 돕습니다. 도서 메타데이터는 예스24 Open API 등 외부 출처를 통해 제공될 수 있으며, 출처는 서비스 화면에 표시됩니다.</p>
    </section>
    <section>
      <h2>3. 계정</h2>
      <ul>
        <li>이용자는 정확한 이메일로 가입하고, 인증·비밀번호 관리에 책임을 집니다.</li>
        <li>타인의 계정을 무단으로 사용하거나, 서비스를 방해하는 행위를 해서는 안 됩니다.</li>
        <li>회원 탈퇴 시 개인 기록(읽음·평가·티어리스트·카탈로그 요청 등)은 삭제되며, 공유 작품 카탈로그는 남을 수 있습니다.</li>
      </ul>
    </section>
    <section>
      <h2>4. 금지 행위</h2>
      <ul>
        <li>법령 또는 타인의 권리를 침해하는 행위</li>
        <li>자동화 수단으로 과도하게 API·서버에 부하를 주는 행위</li>
        <li>서비스 또는 외부 데이터 제공자(예스24 등)를 사칭하거나 오인하게 하는 행위</li>
        <li>음란·혐오·불법 콘텐츠를 유포하는 행위</li>
      </ul>
    </section>
    <section>
      <h2>5. 콘텐츠·데이터</h2>
      <p>도서 정보·표지 등의 권리는 원권리자 또는 제공 사업자에게 있습니다. 서비스는 개인·소규모 기록 용도로 제공되며, 운영자는 사전 고지 후 기능을 변경·중단할 수 있습니다.</p>
    </section>
    <section>
      <h2>6. 면책</h2>
      <p>천재지변, 통신 장애, 외부 API 중단, 이용자 귀책 등으로 인한 손해에 대해 운영자는 법령이 허용하는 범위에서 책임을 제한합니다. 서비스는 “있는 그대로” 제공됩니다.</p>
    </section>
    <section>
      <h2>7. 준거법</h2>
      <p>본 약관은 대한민국 법률에 따릅니다. ${contactHtml()}</p>
    </section>
  `
  );
}

export async function renderPrivacy(root) {
  root.innerHTML = legalShell(
    "개인정보처리방침",
    "privacy",
    `
    <section>
      <h2>1. 개인정보처리자</h2>
      <p>linvlib 운영자<br />${contactHtml()}</p>
    </section>
    <section>
      <h2>2. 수집 항목과 목적</h2>
      <ul>
        <li><strong>이메일, 비밀번호(해시)</strong> — 회원가입, 로그인, 본인 확인, 인증 메일 발송</li>
        <li><strong>읽음·평가·티어리스트·카탈로그 요청</strong> — 서비스 핵심 기능 제공</li>
        <li><strong>서비스 이용 기록(서버 로그)</strong> — 보안·장애 대응 (IP 등 포함될 수 있음)</li>
      </ul>
    </section>
    <section>
      <h2>3. 보관 기간</h2>
      <p>회원 탈퇴 시 개인 계정과 연관된 기록은 지체 없이 삭제합니다. 관계 법령에 별도 보관 의무가 있는 경우 해당 기간 동안 보관할 수 있습니다. 이메일 인증 토큰은 발급 후 최대 48시간 유효합니다.</p>
    </section>
    <section>
      <h2>4. 제3자 제공·처리 위탁</h2>
      <ul>
        <li><strong>예스24 Open API</strong> — 도서 검색·메타데이터·신간 조회 (이용자 이메일을 예스24에 전달하지 않음)</li>
        <li><strong>이메일 발송(SMTP)</strong> — 가입·인증 메일 전송을 위해 이메일 주소가 메일 서버로 전달됨</li>
      </ul>
      <p>법령에 근거하거나 이용자 동의가 있는 경우를 제외하고 개인정보를 제3자에게 판매·제공하지 않습니다.</p>
    </section>
    <section>
      <h2>5. 이용자 권리</h2>
      <p>이용자는 마이페이지에서 계정 정보를 확인하고, 회원 탈퇴로 개인정보 삭제를 요청할 수 있습니다. 문의 메일로도 열람·정정·삭제 요청이 가능합니다.</p>
    </section>
    <section>
      <h2>6. 쿠키·로컬 저장소</h2>
      <p>서비스는 로그인 유지를 위해 브라우저 <code>localStorage</code>에 JWT 등 인증 정보를 저장합니다. 브라우저에서 해당 데이터를 삭제하면 로그아웃됩니다.</p>
    </section>
    <section>
      <h2>7. 안전 조치</h2>
      <p>비밀번호는 해시로 저장하며, 공개 서비스에서는 HTTPS 사용을 권장합니다. 인터넷 특성상 보안을 완벽히 보장하지는 않습니다.</p>
    </section>
    <section>
      <h2>8. 방침 변경</h2>
      <p>본 방침을 변경하는 경우 서비스 내 공지 또는 문서 버전 갱신으로 알립니다. 시행일·버전: ${LEGAL_VERSION}</p>
    </section>
  `
  );
}

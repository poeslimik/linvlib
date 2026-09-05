import { api } from "./api.js";

const DEMO_EMAIL = "example@linvlib.cloud";
const TEARMOON_ID = "5cc17307-3bca-4206-9088-0f3c0f0d48df";

let active = false;
let hostEl = null;

function esc(s) {
  return String(s ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function cover(url, alt = "", className = "cover cover--sm") {
  if (!url) return `<div class="${className} cover--empty" aria-hidden="true"></div>`;
  return `<img class="${className}" src="${esc(url)}" alt="" title="${esc(alt)}" loading="lazy" />`;
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

function formatDate(v) {
  if (!v) return "—";
  return String(v).slice(0, 10);
}

export function isTourPreviewFrame() {
  try {
    return window.self !== window.top;
  } catch {
    return true;
  }
}

async function waitFor(doc, selector, timeout = 20000) {
  const start = Date.now();
  while (Date.now() - start < timeout) {
    if (doc.querySelector(selector)) return true;
    await sleep(100);
  }
  return false;
}

function freezeDoc(doc) {
  const stop = (e) => {
    e.preventDefault();
    e.stopPropagation();
  };
  doc.addEventListener("click", stop, true);
  doc.addEventListener("submit", stop, true);
  doc.addEventListener("keydown", stop, true);
  doc.addEventListener("pointerdown", stop, true);

  const style = doc.createElement("style");
  style.textContent = `
    html, body, #app, .app-shell {
      height: auto !important;
      min-height: 0 !important;
      max-height: none !important;
      overflow: visible !important;
    }
    .site-footer { display: none !important; }
    body { overflow: hidden !important; }
  `;
  doc.head.appendChild(style);
}

function fitIframe(iframe) {
  const doc = iframe.contentDocument;
  if (!doc) return;
  const app = doc.getElementById("app");
  const h = Math.max(
    app?.scrollHeight || 0,
    doc.body?.scrollHeight || 0,
    doc.documentElement?.scrollHeight || 0,
    700
  );
  iframe.style.height = `${h + 32}px`;
}

function patchEmail(doc, email = DEMO_EMAIL) {
  const el = doc.querySelector(".topbar__email");
  if (el) el.textContent = email;
}

function patchStep1(doc) {
  doc.querySelectorAll(".series-row__stats").forEach((el) => {
    const m = el.textContent.match(/\/\s*(\d+)/);
    const total = m ? m[1] : "0";
    el.textContent = `0 / ${total}권 · 0%`;
  });
  doc.querySelectorAll(".progress > span").forEach((el) => {
    el.style.width = "0%";
  });
}

function patchStep2(doc, demo) {
  const tm = demo.tearmoon;
  const pct = tm.progress_percent ?? 0;
  doc.querySelectorAll(".detail-facts div").forEach((box) => {
    const dt = box.querySelector("dt")?.textContent?.trim();
    const dd = box.querySelector("dd");
    if (!dd) return;
    if (dt === "첫 출간") dd.textContent = formatDate(tm.first_published_at);
    if (dt === "최신 출간") dd.textContent = formatDate(tm.latest_published_at);
    if (dt === "진행률") {
      dd.textContent = `${tm.read_volumes} / ${tm.total_volumes}권 (${pct}%)`;
    }
  });

  const byId = new Map((tm.volumes || []).map((v) => [v.id, v]));
  doc.querySelectorAll(".volume-item[data-vid]").forEach((li) => {
    const vol = byId.get(li.dataset.vid);
    if (!vol) return;
    const cb = li.querySelector('input[type="checkbox"]');
    if (cb) {
      cb.checked = !!vol.is_read;
      cb.disabled = false;
    }
    li.classList.toggle("is-read", !!vol.is_read);
  });

  const rating = typeof tm.rating === "string" ? tm.rating : tm.rating || "None";
  const sel = doc.querySelector("#rating-select, .rating-field select, .detail-actions select");
  if (sel) {
    sel.value = rating;
    [...sel.options].forEach((o) => {
      o.selected = o.value === rating;
    });
    sel.disabled = false;
  }
}

async function patchStep3(doc) {
  const methodStyle = doc.createElement("style");
  methodStyle.textContent = `
    .request-methods__label {
      margin: 0 0 0.55rem;
      font-size: 1.12rem;
      font-weight: 800;
      letter-spacing: -0.02em;
      color: #0f4f4a;
      line-height: 1.3;
    }
    .request-methods__label::before {
      content: "";
      display: inline-block;
      width: 0.35rem;
      height: 1.05em;
      margin-right: 0.45rem;
      vertical-align: -0.15em;
      border-radius: 2px;
      background: #1f7a74;
    }
  `;
  doc.head.appendChild(methodStyle);

  const addPanel = doc.querySelector("#tab-add");
  if (addPanel && !addPanel.querySelector("[data-tour-method]")) {
    const searchForm = addPanel.querySelector("#import-form");
    if (searchForm) {
      searchForm.insertAdjacentHTML(
        "beforebegin",
        `<p class="request-methods__label" data-tour-method="1">방법 1 · 검색 후 추가 요청</p>`
      );
    }
    const manual = addPanel.querySelector("#request-form")?.closest("section");
    if (manual) {
      const h2 = manual.querySelector("h2");
      if (h2) {
        h2.outerHTML = `<p class="request-methods__label" data-tour-method="2">방법 2 · 직접 요청</p>`;
      } else {
        manual.insertAdjacentHTML(
          "afterbegin",
          `<p class="request-methods__label" data-tour-method="2">방법 2 · 직접 요청</p>`
        );
      }
    }
  }

  const q = doc.querySelector('#import-form input[name="q"]');
  if (q) q.value = "인생역전";

  const title = doc.querySelector("#req-title");
  const author = doc.querySelector("#req-author");
  const publisher = doc.querySelector("#req-publisher");
  const note = doc.querySelector("#req-note");
  if (title) title.value = "제발 날 내버려 둬";
  if (author) author.value = "아이사키 카베기와";
  if (publisher) publisher.value = "GA문고";
  if (note) {
    note.value = "";
    note.placeholder = "참고 링크, 권 수 등";
  }

  const results = doc.querySelector("#import-results");
  if (!results) return;
  try {
    const listData = await api.listSeries({ q: "인생역전", limit: 5 });
    const hit =
      (listData.items || []).find((s) => (s.title || "").includes("인생")) ||
      (listData.items || [])[0];
    if (!hit) {
      results.innerHTML = `<p class="empty">결과가 없습니다.</p>`;
      return;
    }

    let authorText = "";
    let publisherText = "";
    let coverUrl = hit.latest_cover_url || "";
    let volumeCount = hit.total_volumes;
    try {
      const detail = await api.getSeries(hit.id, "desc");
      authorText = detail.author || "";
      publisherText = detail.publisher || "";
      coverUrl = detail.latest_cover_url || detail.cover_url || coverUrl;
      volumeCount = detail.total_volumes ?? volumeCount;
    } catch {}

    const meta = [authorText, publisherText, volumeCount != null ? `${volumeCount}권` : null]
      .filter(Boolean)
      .join(" · ");
    results.innerHTML = `
      <ul class="result-list">
        <li class="import-card">
          ${cover(coverUrl, hit.title, "cover cover--md")}
          <div class="import-card__body">
            <h3>${esc(hit.title)}</h3>
            <p class="muted">${esc(meta)}</p>
            <div class="import-card__actions">
              <button type="button" class="btn btn--primary btn--sm" tabindex="-1">추가 요청</button>
            </div>
          </div>
        </li>
      </ul>`;
  } catch {}
}

function patchStep4(doc, demo) {
  const byTier = Object.fromEntries(
    (demo.tierlist?.tiers || []).map((t) => [t.tier, t.entries || []])
  );
  doc.querySelectorAll(".tier-row").forEach((row) => {
    const label = row.querySelector(".tier-row__label")?.textContent?.trim();
    const track = row.querySelector(".tier-row__track");
    if (!label || !track) return;
    const entries = byTier[label] || [];
    track.innerHTML = entries.map((e) => cover(e.cover_url, e.title, "cover cover--sm")).join("");
  });
}

const STEPS = [
  {
    title: "작품을 찾아보세요",
    copy: "목록에서 제목을 검색하고, 정렬·필터로 원하는 시리즈를 찾을 수 있어요.",
    path: `/series?sort=popular&ps_in=complete`,
    ready: ".series-row, .series-grid, .page__head h1",
    patch: async (doc) => patchStep1(doc),
  },
  {
    title: "읽은 권을 기록하세요",
    copy: "시리즈 상세에서 권 단위로 읽음을 체크하고, 작품 평가를 남길 수 있어요.",
    path: `/series/${TEARMOON_ID}`,
    ready: ".detail-facts, .volume-item, .detail-hero h1",
    patch: async (doc, demo) => patchStep2(doc, demo),
  },
  {
    title: "없으면 요청하세요",
    copy: "검색으로 찾아 추가를 요청하거나, 직접 입력해 요청할 수 있어요.",
    path: `/import`,
    ready: "#import-form, #req-title, .import-tabs",
    patch: async (doc) => patchStep3(doc),
  },
  {
    title: "티어리스트로 정리하세요",
    copy: "작품을 티어에 배치하고, 이미지로 저장하거나 복사해 공유할 수 있어요.",
    path: `/tierlist`,
    ready: ".tier-row, .tier-board, .page__head h1",
    patch: async (doc, demo) => patchStep4(doc, demo),
  },
];

function buildHost() {
  const el = document.createElement("div");
  el.id = "tour-root";
  el.innerHTML = `
    <div class="tour-backdrop" role="presentation">
      <div class="tour-dialog" role="dialog" aria-modal="true" aria-labelledby="tour-heading">
        <div class="tour-hero">
          <div>
            <p class="tour-kicker">첫 로그인 가이드</p>
            <h2 id="tour-heading">linvlib에 오신 걸 환영해요</h2>
          </div>
          <div class="tour-dots" data-tour-dots></div>
        </div>
        <div class="tour-body">
          <p class="tour-step-label" data-tour-label>1 / 4</p>
          <h3 data-tour-title></h3>
          <p class="tour-lead" data-tour-copy></p>
          <div class="tour-preview" data-tour-preview></div>
        </div>
        <div class="tour-actions">
          <button type="button" class="tour-btn tour-btn--ghost" data-tour-skip>건너뛰기</button>
          <div class="tour-actions__right">
            <button type="button" class="tour-btn tour-btn--secondary" data-tour-prev disabled>이전</button>
            <button type="button" class="tour-btn tour-btn--primary" data-tour-next>다음</button>
          </div>
        </div>
      </div>
    </div>`;
  return el;
}

/**
 * @param {{ force?: boolean }} [opts]
 */
export async function startTour(opts = {}) {
  if (active || isTourPreviewFrame()) return;
  active = true;
  document.documentElement.classList.add("tour-open");

  hostEl = buildHost();
  document.body.appendChild(hostEl);

  let index = 0;
  let demo = null;
  let loadToken = 0;

  const $ = (sel) => hostEl.querySelector(sel);

  function renderDots() {
    $("[data-tour-dots]").innerHTML = STEPS.map(
      (_, i) => `<span class="${i === index ? "is-active" : ""}"></span>`
    ).join("");
  }

  async function mountStep() {
    const token = ++loadToken;
    const step = STEPS[index];
    $("[data-tour-label]").textContent = `${index + 1} / ${STEPS.length}`;
    $("[data-tour-title]").textContent = step.title;
    $("[data-tour-copy]").textContent = step.copy;
    $("[data-tour-next]").textContent = index === STEPS.length - 1 ? "시작하기" : "다음";
    $("[data-tour-prev]").disabled = index === 0;
    const preview = $("[data-tour-preview]");
    preview.scrollTop = 0;
    renderDots();

    preview.innerHTML = `
      <div class="tour-preview-stack">
        <iframe title="미리보기" src="${esc(step.path)}" scrolling="no"></iframe>
      </div>`;

    const iframe = preview.querySelector("iframe");
    await new Promise((resolve) => {
      iframe.addEventListener("load", resolve, { once: true });
      setTimeout(resolve, 15000);
    });
    if (token !== loadToken) return;

    try {
      const doc = iframe.contentDocument;
      if (!doc) throw new Error("no iframe document");
      const ready = await waitFor(doc, step.ready, 20000);
      if (token !== loadToken) return;
      if (!ready) throw new Error("page not ready");
      if (doc.querySelector('input[type="password"], .auth-card')) {
        throw new Error("login required");
      }

      if (index === 1 || index === 3) {
        try {
          demo = await api.tourDemo();
        } catch {}
      }

      freezeDoc(doc);
      patchEmail(doc, demo?.display_email || DEMO_EMAIL);
      await step.patch(doc, demo);
      patchEmail(doc, demo?.display_email || DEMO_EMAIL);
      fitIframe(iframe);
      setTimeout(() => {
        if (token === loadToken) fitIframe(iframe);
      }, 400);
      setTimeout(() => {
        if (token === loadToken) fitIframe(iframe);
      }, 1200);
    } catch (err) {
      console.warn("[tour] step failed", err);
    }
  }

  async function closeTour() {
    hostEl?.remove();
    hostEl = null;
    active = false;
    document.documentElement.classList.remove("tour-open");
  }

  $("[data-tour-skip]").onclick = () => closeTour();
  $("[data-tour-prev]").onclick = () => {
    if (index > 0) {
      index -= 1;
      mountStep();
    }
  };
  $("[data-tour-next]").onclick = () => {
    if (index >= STEPS.length - 1) return closeTour();
    index += 1;
    mountStep();
  };

  // force unused but kept for API symmetry / future replay
  void opts.force;
  await mountStep();
}

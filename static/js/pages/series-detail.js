import { api } from "../api.js";
import { getUser } from "../auth.js";
import { onBeforeLeave } from "../router.js";
import { shell, cover, escapeHtml, escapeAttr, formatDate, toast, bindLogout, catalogSearchUrl, publishStatusBadge, publishStatusLabel, sortDirectionIcon, seriesListReturnUrl } from "../ui.js";

const RATINGS = ["None", "S", "A", "B", "C", "D", "F"];

const PUBLISH_STATUSES = [
  { value: "complete", label: "완결" },
  { value: "complete_partial", label: "완결(번역 미완)" },
  { value: "complete_stalled", label: "완결(번역 중단)" },
  { value: "ongoing", label: "연재중" },
  { value: "ongoing_stalled", label: "연재중(번역 중단)" },
  { value: "hiatus", label: "연재 중단(번역 미완)" },
  { value: "hiatus_done", label: "연재 중단" },
];

function currentPublishStatus(series) {
  return series.publish_status || "ongoing";
}

function formatVolumeTitle(title = "") {
  return title.replace(/사족\s+편/g, "사족편");
}

function formatVolumeLabel(volumeNumber, title = "", sssCount = 1, exCount = 1, sajokCount = 1) {
  if (title) {
    const cleaned = title
      .replace(/&quot;|&#34;/g, "\"")
      .replace(/&apos;|&#39;/g, "'")
      .replace(/&amp;/g, "&")
      .replace(/[`"“”‘’']/g, "")
      .replace(/\s*[-–,]\s*(?:Premium\s+)?Extreme\s+Novel\b.*/i, "")
      .replace(/\s*[-–,]\s*Novel\s*Engine\b.*/i, "")
      .replace(/\s*[-–,]\s*NT\s*Novel\b.*/i, "")
      .replace(/\s*[-–,]\s*S\s*Novel\+?(?:\s*초판.*)?$/i, "")
      .replace(/\s*[-–,]\s*L\s*Novel\b.*/i, "")
      .replace(/\s*[-–,]\s*L\s*Books\b.*/i, "")
      .replace(/\s*[-–,]\s*JM\s*노벨\b.*/i, "")
      .replace(/\s*[-–,]\s*V\+\b.*/i, "")
      .replace(/\s*,\s*V\+\s*$/i, "")
      .replace(/\s*[-–,]\s*[^,-]{0,40}노벨\b.*/i, "")
      .replace(/\s*[,，]?\s*[①②③④⑤].*$/, "")
      .replace(/\s*[-–,/]\s*완결(?:\s*\/.*)?\s*$/i, "")
      .replace(/\s*\/\s*초판\s*한정.*$/i, "")
      .replace(/\s*\([^)]*(?:한정|초회|박스|특장|특별|일반|소책자)[^)]*\)\s*/g, " ")
      .replace(/사족\s+편/g, "사족편")
      .trim();
    if (/SSS/i.test(cleaned)) {
      if (sssCount <= 1) return "SSS";
      const m = cleaned.match(/SSS\s*(\d+)/i);
      return m ? `SSS ${m[1]}권` : "SSS";
    }
    if (/(?:^|[\s!])Ex(?:\s|$|[-–—,，]|\d)/i.test(cleaned) || /!\s*Ex\b/i.test(cleaned)) {
      if (exCount <= 1) return "Ex";
      const m = cleaned.match(/\bEx\s*(\d+)/i);
      return m ? `Ex ${m[1]}권` : "Ex";
    }
    if (/사족편/.test(cleaned)) {
      if (sajokCount <= 1) return "사족편";
      const m =
        cleaned.match(/(\d+(?:\.\d+)?)\s*권?\s*[-–—:]\s*사족편/) ||
        cleaned.match(/사족편\s*(\d+)/) ||
        cleaned.match(/(?:제)?(\d+(?:\.\d+)?)\s*권/) ||
        cleaned.match(/(?:\s|\.)(\d+(?:\.\d+)?)\s*(?:권)?\s*[-–—:,，(]/);
      return m ? `사족편 ${m[1]}권` : "사족편";
    }
    if (/단편집/.test(cleaned)) return "단편집";
    if (/SS\s*집|ＳＳ\s*집/i.test(cleaned)) return "SS집";
    const chapter = cleaned.match(/[:：]\s*(.+의\s*장)\s*$/);
    if (chapter) return chapter[1].trim();
    // 춘하추동 대행자 봄의 춤 : 상 → 봄의 춤 상
    if (/^춘하추동\s*대행자/.test(cleaned)) {
      const rest = cleaned
        .replace(/^춘하추동\s*대행자\s*/, "")
        .replace(/\s*[:：]\s*/g, " ")
        .trim();
      if (rest) return rest;
    }
    const yearMatch = cleaned.match(/(\d+)학년\s*편/);
    const year = yearMatch ? Number(yearMatch[1]) : 1;
    const storyMatch = cleaned.match(/제(\d+)부/);
    const story = storyMatch ? Number(storyMatch[1]) : 0;
    let withoutArc = cleaned
      .replace(/\s*\d+학년\s*편\s*/g, " ")
      .replace(/\s*제\d+부\s*[:：]?\s*[^\d]*/g, " ")
      .replace(/\s+/g, " ")
      .trim();
    const part =
      withoutArc.match(/(\d+(?:\.\d+)?)\s*권?\s*[\(（]\s*(상|중|하)\s*[\)）]/) ||
      withoutArc.match(/(\d+(?:\.\d+)?)\s*권?\s*[-–—:\s]+(상|중|하)(?:\s*[-–—:,，]|$)/);
    if (part) {
      const body = `${part[1]}${part[2]}`;
      if (year >= 2) return `${year}학년 ${body}`;
      if (story >= 1) return `${story}부 ${body}`;
      return body;
    }
    const hyphenVol = withoutArc.match(/(?:\s|\.)(\d+)\s*[-–—]\s*(\d+)(?=\s|$|권|[-–—,，)])/);
    if (hyphenVol) {
      const body = `${hyphenVol[1]}-${hyphenVol[2]}권`;
      if (year >= 2) return `${year}학년 ${body}`;
      if (story >= 1) return `${story}부 ${body}`;
      return body;
    }
    const vol =
      withoutArc.match(/(?:제)?(\d+(?:\.\d+)?)\s*권/) ||
      withoutArc.match(/(?:\s|\.)(\d+(?:\.\d+)?)\s*(?:권)?\s*[-–—:,，(]/) ||
      withoutArc.match(/(?:\s|\.)(\d+(?:\.\d+)?)\s*$/) ||
      withoutArc.match(/^(\d+(?:\.\d+)?)$/);
    if (vol) {
      const body = `${vol[1]}권`;
      if (year >= 2) return `${year}학년 ${body}`;
      if (story >= 1) return `${story}부 ${body}`;
      return body;
    }
  }
  return `${volumeNumber}권`;
}

function displayVolumeLabel(volume, sssCount = 1, exCount = 1, sajokCount = 1) {
  const custom = (volume?.label || "").trim();
  if (custom) return custom;
  return formatVolumeLabel(volume?.volume_number, volume?.title, sssCount, exCount, sajokCount);
}

function volumeDateValue(publishedAt) {
  if (!publishedAt) return "";
  return String(publishedAt).slice(0, 10);
}

function editVolumeRowHtml(vol = {}, idx = 0) {
  const unreleased = !!vol.is_unreleased;
  return `
    <div class="manual-vol-row manual-vol-row--edit" data-idx="${idx}" data-vid="${escapeAttr(vol.id || "")}">
      <span class="manual-vol-drag" title="끌어서 순서 변경" draggable="false" aria-hidden="true">⠿</span>
      <label class="manual-vol-pick" title="이동·분리할 권 선택">
        <input type="checkbox" class="manual-vol-check" ${vol.id ? "" : "disabled"} />
      </label>
      <input type="number" min="1" class="manual-vol-num" value="${escapeAttr(vol.volume_number ?? idx + 1)}" title="권번호" />
      <input type="text" class="manual-vol-label" value="${escapeAttr(vol.label || "")}" placeholder="표시명" title="목록에 보이는 짧은 이름 (비우면 자동)" />
      <input type="text" class="manual-vol-title" value="${escapeAttr(vol.title || "")}" placeholder="권 제목" />
      <input type="date" class="manual-vol-date" value="${escapeAttr(volumeDateValue(vol.published_at))}" title="출간일" />
      <input type="url" class="manual-vol-cover" value="${escapeAttr(vol.cover_url || "")}" placeholder="표지 URL" title="표지 URL" />
      <label class="manual-vol-unreleased" title="현지 발매만 있고 국내 정식 출간 없음">
        <input type="checkbox" class="manual-vol-unreleased-check" ${unreleased ? "checked" : ""} />
        미정발
      </label>
      <button type="button" class="btn btn--ghost btn--sm" data-remove-vol>삭제</button>
    </div>`;
}

async function pickSeriesTarget(promptLabel) {
  const q = prompt(promptLabel);
  if (q == null) return null;
  const query = q.trim();
  if (!query) {
    toast("검색어를 입력하세요", "error");
    return null;
  }
  try {
    const data = await api.listSeries({ q: query, limit: 10, sort: "title" });
    const items = (data.items || []).filter(Boolean);
    if (!items.length) {
      toast("검색 결과가 없습니다", "error");
      return null;
    }
    const lines = items
      .map((it, i) => `${i + 1}. ${it.title} (${it.total_volumes}권)`)
      .join("\n");
    const pick = prompt(`대상 시리즈 번호를 고르세요:\n${lines}`);
    if (pick == null) return null;
    const n = Number(pick);
    if (!n || n < 1 || n > items.length) {
      toast("올바른 번호가 아닙니다", "error");
      return null;
    }
    return items[n - 1];
  } catch (ex) {
    toast(ex.message, "error");
    return null;
  }
}

export async function renderSeriesDetail(root, { id }) {
  let volumeOrder = new URLSearchParams(location.search).get("order") === "asc" ? "asc" : "desc";

  root.innerHTML = shell(
    { email: getUser()?.email, active: "series", isAdmin: !!getUser()?.is_admin },
    `<main class="page"><p class="muted">불러오는 중…</p></main>`
  );
  bindLogout();

  let series;
  try {
    series = await api.getSeries(id, volumeOrder);
  } catch (ex) {
    root.querySelector("main").innerHTML = `<p class="empty">${escapeHtml(ex.message)}</p>`;
    return;
  }

  const readMap = new Map(series.volumes.map((v) => [v.id, v.is_read]));
  let dirty = false;
  let editing =
    !!getUser()?.is_admin &&
    new URLSearchParams(location.search).get("edit") === "1";
  let rating = series.rating;
  let savedReadCount = series.volumes.filter((v) => v.is_read).length;

  const unloadHandler = (e) => {
    if (!dirty) return;
    e.preventDefault();
    e.returnValue = "";
  };
  window.addEventListener("beforeunload", unloadHandler);

  let lastReadClickIndex = null;

  function markDirty(v = true) {
    dirty = v;
    const saveBtn = root.querySelector("#save-reads");
    if (saveBtn) saveBtn.disabled = !dirty;
  }

  function paint() {
    const readCount = [...readMap.values()].filter(Boolean).length;
    const pct = series.total_volumes
      ? Math.round((readCount / series.total_volumes) * 100)
      : 0;
    const sssCount = series.volumes.filter((v) => /SSS/i.test(v.title || "")).length;
    const exCount = series.volumes.filter((v) =>
      /(?:^|[\s!])Ex(?:\s|$|[-–—,，]|\d)/i.test(v.title || "")
    ).length;
    const sajokCount = series.volumes.filter((v) => /사족\s*편/.test(v.title || "")).length;

    const isAdmin = !!getUser()?.is_admin;
    const pubStatus = currentPublishStatus(series);
    const completeToggle = isAdmin
      ? `<label class="publish-status-field" title="목록·검색 표시용 발매 상태 (신간 갱신 대상과는 무관)">
           <span class="visually-hidden">발매 상태</span>
           <select id="publish-status-select" aria-label="발매 상태">
             ${PUBLISH_STATUSES.map(
               (o) =>
                 `<option value="${o.value}" ${o.value === pubStatus ? "selected" : ""}>${escapeHtml(o.label)}</option>`
             ).join("")}
           </select>
         </label>`
      : "";
    const actionButtons = isAdmin
      ? `<button type="button" class="btn btn--ghost btn--sm" id="edit-catalog">${editing ? "수정 닫기" : "수정"}</button>
         ${
           series.is_manual
             ? ""
             : `<button type="button" class="btn btn--ghost btn--sm" id="refresh-volumes" title="이 작품만 카탈로그에서 다시 가져와 새 권을 추가합니다">권 목록 갱신</button>`
         }
         <button type="button" class="btn btn--danger btn--sm" id="delete-series">삭제</button>
         ${completeToggle}`
      : `<button type="button" class="btn btn--ghost btn--sm" id="request-edit">수정 요청</button>`;

    const editPanel =
      isAdmin && editing
        ? `<section class="manual-edit" id="manual-edit">
          <h2>작품 수정</h2>
          ${
            series.is_manual
              ? ""
              : `<p class="muted">카탈로그 갱신은 새 권만 추가하고, 같은 권의 다른 에디션 중복은 합칩니다. 순서·제목은 여기서 직접 관리하세요.</p>`
          }
          <form id="manual-edit-form" class="manual-form">
            <label class="manual-field">
              <span>제목 *</span>
              <input name="title" id="edit-title" type="text" required value="${escapeAttr(series.title)}" />
            </label>
            <div class="manual-field-row">
              <label class="manual-field">
                <span>작가</span>
                <input name="author" id="edit-author" type="text" value="${escapeAttr(series.author || "")}" />
              </label>
              <label class="manual-field">
                <span>출판사</span>
                <input name="publisher" id="edit-publisher" type="text" value="${escapeAttr(series.publisher || "")}" />
              </label>
            </div>
            <label class="manual-field">
              <span>대표 표지 URL</span>
              <input name="cover_url" id="edit-cover" type="url" value="${escapeAttr(series.cover_url || "")}" placeholder="https://… (비우면 최신 권 표지)" />
            </label>
            ${
              series.is_manual
                ? `<div class="manual-field-row">
              <label class="manual-field">
                <span>출처 문구</span>
                <input name="source_label" id="edit-source-label" type="text" value="${escapeAttr(series.source_label || "")}" placeholder="비우면 불명 · 예: 작가 서재" />
              </label>
              <label class="manual-field">
                <span>출처 링크</span>
                <input name="source_url" id="edit-source-url" type="url" value="${escapeAttr(series.source_url || "")}" placeholder="https://…" />
              </label>
            </div>
            <p class="muted">문구·링크 중 하나만 있어도 표시됩니다. 둘 다 비우면 「불명」으로 표시됩니다.</p>`
                : ""
            }
            <div class="alias-edit">
              <h3>검색</h3>
              <p class="muted">줄임말·별칭과 함께 검색됩니다. 띄어쓰기·특수문자는 무시합니다. 예: 전생슬, SAO.</p>
              ${
                (series.search_bundle_peers || []).length
                  ? `<p class="muted">함께 검색됨: ${(series.search_bundle_peers || [])
                      .map(
                        (p) =>
                          `<a href="/series/${p.id}" data-link>${escapeHtml(p.title)}</a>`
                      )
                      .join(" · ")}</p>`
                  : ""
              }
              <ul id="alias-list" class="alias-list">
                ${(series.search_aliases || [])
                  .map((a) => {
                    const src =
                      a.source === "admin" ? "직접" : a.source === "user" ? "요청" : "자동";
                    const canRemove = a.source !== "auto";
                    return `<li class="alias-list__item" data-alias-id="${escapeAttr(a.id)}">
                      <span><strong>${escapeHtml(a.alias)}</strong> <span class="muted">(${src})</span></span>
                      ${
                        canRemove
                          ? `<button type="button" class="btn btn--ghost btn--sm" data-alias-remove="${escapeAttr(a.id)}">삭제</button>`
                          : ""
                      }
                    </li>`;
                  })
                  .join("") || `<li class="muted">등록된 별칭이 없습니다.</li>`}
              </ul>
              <div class="alias-add-row">
                <input type="text" id="alias-input" maxlength="80" placeholder="줄임말 또는 별칭" />
                <button type="button" class="btn btn--ghost btn--sm" id="alias-add">추가</button>
              </div>
            </div>
            <div class="manual-vols-head">
              <h3>권 목록</h3>
              <div class="manual-vols-actions">
                <button type="button" class="btn btn--ghost btn--sm" id="edit-add-vol">권 추가</button>
                <button type="button" class="btn btn--ghost btn--sm" id="edit-move-vols">선택 이동</button>
                <button type="button" class="btn btn--ghost btn--sm" id="edit-split-vols">선택 분리</button>
                <button type="button" class="btn btn--ghost btn--sm" id="edit-merge-series">시리즈 병합</button>
              </div>
            </div>
            <div id="edit-vols">
              ${[...series.volumes]
                .sort((a, b) => a.volume_number - b.volume_number)
                .map((v, i) => editVolumeRowHtml(v, i))
                .join("")}
            </div>
            <div class="manual-edit__actions">
              <button type="submit" class="btn btn--primary" id="edit-save">저장</button>
              <button type="button" class="btn btn--ghost" id="edit-cancel">취소</button>
            </div>
          </form>
        </section>`
        : "";

    root.innerHTML = shell(
      { email: getUser()?.email, active: "series", isAdmin },
      `<main class="page series-detail">
        <a class="back-link" href="${escapeAttr(seriesListReturnUrl())}" data-link>← 목록</a>
        <section class="detail-hero">
          ${cover(series.latest_cover_url, series.title, "cover cover--lg")}
          <div class="detail-hero__info">
            <h1>${escapeHtml(series.title)}</h1>
            <p class="detail-hero__sub">
              ${escapeHtml(series.author || "작가 미상")}
              ${series.publisher ? ` · ${escapeHtml(series.publisher)}` : ""}
              ${series.is_manual ? ` · <span class="badge-manual">직접 등록</span>` : ""}
          ${(() => {
            const b = publishStatusBadge(currentPublishStatus(series));
            return b ? ` ·${b}` : "";
          })()}
            </p>
            ${(() => {
              if (series.is_manual) {
                const label = (series.source_label || "").trim();
                const url = (series.source_url || "").trim();
                let body;
                if (label && url) {
                  body = `<a href="${escapeAttr(url)}" target="_blank" rel="noopener noreferrer">${escapeHtml(label)}</a>`;
                } else if (label) {
                  body = escapeHtml(label);
                } else if (url) {
                  body = `<a href="${escapeAttr(url)}" target="_blank" rel="noopener noreferrer">${escapeHtml(url)}</a>`;
                } else {
                  body = "불명";
                }
                return `<p class="source-line">출처: ${body}</p>`;
              }
              return `<p class="source-line">
              출처: <a href="${catalogSearchUrl(series.title)}" target="_blank" rel="noopener noreferrer">예스24에서 검색</a>
            </p>`;
            })()}
            <dl class="detail-facts">
              <div><dt>첫 출간</dt><dd>${formatDate(series.first_published_at)}</dd></div>
              <div><dt>최신 출간</dt><dd>${formatDate(series.latest_published_at)}</dd></div>
              <div><dt>진행률</dt><dd>${readCount} / ${series.total_volumes}권 (${pct}%)</dd></div>
            </dl>
            <div class="detail-actions">
              <label class="rating-field">
                <span>평가</span>
                <select id="rating-select" ${savedReadCount === 0 ? "disabled title=\"한 권 이상 읽은 뒤 평가할 수 있습니다\"" : ""}>
                  ${RATINGS.map(
                    (r) =>
                      `<option value="${r}" ${r === rating ? "selected" : ""}>${r}</option>`
                  ).join("")}
                </select>
              </label>
              ${actionButtons}
            </div>
          </div>
        </section>
        ${editPanel}

        <section class="volume-section">
          <div class="volume-toolbar">
            <button
              type="button"
              class="sort-dir"
              id="volume-order-dir"
              aria-label="${volumeOrder === "asc" ? "1권부터" : "신간부터"}"
              title="${volumeOrder === "asc" ? "1권부터 (클릭하여 신간부터)" : "신간부터 (클릭하여 1권부터)"}"
              data-order="${volumeOrder}"
            >${sortDirectionIcon(volumeOrder)}</button>
            <div class="volume-toolbar__actions">
              <span class="volume-read-hint" title="PC에서 Shift+클릭으로 구간 선택">Shift+클릭 구간</span>
              <button type="button" class="btn btn--ghost btn--sm" id="select-all">전체 선택</button>
              <button type="button" class="btn btn--primary btn--sm" id="save-reads" ${dirty ? "" : "disabled"}>저장</button>
            </div>
          </div>
          <ul class="volume-list">
            ${series.volumes
              .map(
                (v) => `
              <li class="volume-item ${readMap.get(v.id) ? "is-read" : ""}" data-vid="${v.id}">
                <label class="volume-item__check">
                  <input type="checkbox" ${readMap.get(v.id) ? "checked" : ""} />
                  <span class="sr-only">${escapeHtml(formatVolumeTitle(v.title))} 읽음</span>
                </label>
                ${cover(v.cover_url, formatVolumeTitle(v.title), "cover cover--sm")}
                <div class="volume-item__meta">
                  <h3>${escapeHtml(formatVolumeTitle(v.title))}</h3>
                  <p>${escapeHtml(displayVolumeLabel(v, sssCount, exCount, sajokCount))} · ${formatDate(v.published_at)}${v.is_unreleased ? ` <span class="badge-unreleased" title="현지 발매 · 국내 미정발">미정발</span>` : ""}</p>
                </div>
              </li>`
              )
              .join("")}
          </ul>
        </section>
      </main>`
    );
    bindLogout();
    wire();
  }

  function selectedEditVolumeIds() {
    return [...root.querySelectorAll(".manual-vol-row")]
      .filter((row) => row.querySelector(".manual-vol-check")?.checked && row.dataset.vid)
      .map((row) => row.dataset.vid);
  }

  function wireEditForm() {
    const volsBox = root.querySelector("#edit-vols");
    if (!volsBox) return;

    const SOURCE_LABEL = { admin: "직접", user: "요청", auto: "자동" };

    function renderAliasList(aliases) {
      const box = root.querySelector("#alias-list");
      if (!box) return;
      series.search_aliases = aliases || [];
      if (!series.search_aliases.length) {
        box.innerHTML = `<li class="muted">등록된 별칭이 없습니다.</li>`;
        return;
      }
      box.innerHTML = series.search_aliases
        .map((a) => {
          const src = SOURCE_LABEL[a.source] || a.source;
          const canRemove = a.source !== "auto";
          return `<li class="alias-list__item" data-alias-id="${escapeAttr(a.id)}">
            <span><strong>${escapeHtml(a.alias)}</strong> <span class="muted">(${src})</span></span>
            ${
              canRemove
                ? `<button type="button" class="btn btn--ghost btn--sm" data-alias-remove="${escapeAttr(a.id)}">삭제</button>`
                : ""
            }
          </li>`;
        })
        .join("");
      box.querySelectorAll("[data-alias-remove]").forEach((btn) => {
        btn.addEventListener("click", async () => {
          btn.disabled = true;
          try {
            await api.deleteSeriesAlias(id, btn.dataset.aliasRemove);
            series.search_aliases = (series.search_aliases || []).filter(
              (a) => a.id !== btn.dataset.aliasRemove
            );
            renderAliasList(series.search_aliases);
            toast("별칭을 삭제했습니다", "ok");
          } catch (ex) {
            toast(ex.message, "error");
            btn.disabled = false;
          }
        });
      });
    }

    root.querySelector("#alias-add")?.addEventListener("click", async () => {
      const input = root.querySelector("#alias-input");
      const alias = input?.value.trim() || "";
      if (!alias) {
        toast("줄임말·별칭을 입력하세요", "error");
        return;
      }
      try {
        const res = await api.addSeriesAlias(id, alias);
        renderAliasList(res.search_aliases || []);
        if (input) input.value = "";
        toast("별칭을 추가했습니다", "ok");
      } catch (ex) {
        toast(ex.message, "error");
      }
    });
    root.querySelector("#alias-input")?.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        root.querySelector("#alias-add")?.click();
      }
    });
    root.querySelectorAll("[data-alias-remove]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        btn.disabled = true;
        try {
          await api.deleteSeriesAlias(id, btn.dataset.aliasRemove);
          series.search_aliases = (series.search_aliases || []).filter(
            (a) => a.id !== btn.dataset.aliasRemove
          );
          renderAliasList(series.search_aliases);
          toast("별칭을 삭제했습니다", "ok");
        } catch (ex) {
          toast(ex.message, "error");
          btn.disabled = false;
        }
      });
    });

    function reindex() {
      volsBox.querySelectorAll(".manual-vol-row").forEach((row, i) => {
        row.dataset.idx = String(i);
        const num = row.querySelector(".manual-vol-num");
        if (num) num.value = String(i + 1);
      });
    }

    async function persistOrder() {
      reindex();
      const ids = [...volsBox.querySelectorAll(".manual-vol-row")]
        .map((r) => r.dataset.vid)
        .filter(Boolean);
      if (!ids.length || ids.length !== series.volumes.filter((v) => v.id).length) {
        return;
      }
      try {
        await api.reorderVolumes(id, ids);
        series.volumes = [...series.volumes]
          .sort((a, b) => ids.indexOf(a.id) - ids.indexOf(b.id))
          .map((v, i) => ({ ...v, volume_number: i + 1 }));
        toast("순서를 저장했습니다", "ok");
      } catch (ex) {
        toast(ex.message, "error");
      }
    }

    function moveRowToIndex(row, toIndex) {
      const rows = [...volsBox.querySelectorAll(".manual-vol-row")];
      const from = rows.indexOf(row);
      if (from < 0) return;
      const clamped = Math.max(0, Math.min(toIndex, rows.length - 1));
      if (from === clamped) return;
      rows.splice(from, 1);
      rows.splice(clamped, 0, row);
      rows.forEach((r) => volsBox.appendChild(r));
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

    function bindRemove() {
      volsBox.querySelectorAll("[data-remove-vol]").forEach((btn) => {
        btn.onclick = () => {
          if (volsBox.querySelectorAll(".manual-vol-row").length <= 1) {
            toast("권은 최소 1권 필요합니다", "error");
            return;
          }
          btn.closest(".manual-vol-row")?.remove();
          reindex();
        };
      });
    }

    let dragRow = null;
    function bindDrag(row) {
      bindTitleReveal(row.querySelector(".manual-vol-title"));
      const handle = row.querySelector(".manual-vol-drag");

      // 핸들에서만 행 드래그 시작 (입력 필드 드래그는 텍스트 선택용으로 남김)
      handle?.addEventListener("mousedown", () => {
        row.setAttribute("draggable", "true");
      });
      row.addEventListener("dragstart", (e) => {
        // 핸들에서 mousedown으로 draggable을 켠 경우에만 허용
        if (row.getAttribute("draggable") !== "true") {
          e.preventDefault();
          return;
        }
        dragRow = row;
        row.classList.add("is-dragging");
        e.dataTransfer.effectAllowed = "move";
        e.dataTransfer.setData("text/plain", row.dataset.vid || "");
      });
      row.addEventListener("dragend", async () => {
        row.classList.remove("is-dragging");
        row.removeAttribute("draggable");
        dragRow = null;
        await persistOrder();
      });
      row.addEventListener("dragover", (e) => {
        e.preventDefault();
        e.dataTransfer.dropEffect = "move";
        if (!dragRow || dragRow === row) return;
        const rect = row.getBoundingClientRect();
        const before = e.clientY < rect.top + rect.height / 2;
        volsBox.insertBefore(dragRow, before ? row : row.nextSibling);
      });

      const numInput = row.querySelector(".manual-vol-num");
      if (numInput && !numInput.dataset.orderBound) {
        numInput.dataset.orderBound = "1";
        numInput.addEventListener("change", async () => {
          const total = volsBox.querySelectorAll(".manual-vol-row").length;
          let target = Number(numInput.value);
          if (!Number.isFinite(target) || target < 1) target = 1;
          if (target > total) target = total;
          numInput.value = String(target);
          moveRowToIndex(row, target - 1);
          await persistOrder();
        });
      }
    }

    // 입력·버튼에서는 행 드래그가 시작되지 않도록
    volsBox.addEventListener("mousedown", (e) => {
      if (e.target.closest(".manual-vol-drag")) return;
      const row = e.target.closest(".manual-vol-row");
      if (row) row.removeAttribute("draggable");
    });

    volsBox.querySelectorAll(".manual-vol-row").forEach(bindDrag);

    root.querySelector("#edit-add-vol")?.addEventListener("click", () => {
      const idx = volsBox.querySelectorAll(".manual-vol-row").length;
      const status = currentPublishStatus(series);
      const defaultUnreleased =
        status === "ongoing_stalled" ||
        status === "complete_stalled" ||
        status === "complete_partial";
      volsBox.insertAdjacentHTML(
        "beforeend",
        editVolumeRowHtml(
          { volume_number: idx + 1, is_unreleased: defaultUnreleased },
          idx
        )
      );
      bindRemove();
      const newRow = volsBox.lastElementChild;
      if (newRow) bindDrag(newRow);
    });

    root.querySelector("#edit-cancel")?.addEventListener("click", () => {
      editing = false;
      const url = new URL(location.href);
      url.searchParams.delete("edit");
      history.replaceState(null, "", url.pathname + url.search);
      paint();
    });

    root.querySelector("#edit-move-vols")?.addEventListener("click", async () => {
      const volume_ids = selectedEditVolumeIds();
      if (!volume_ids.length) {
        toast("이동할 권을 선택하세요", "error");
        return;
      }
      const target = await pickSeriesTarget("옮길 대상 시리즈 제목 검색");
      if (!target) return;
      if (target.id === id) {
        toast("같은 시리즈입니다", "error");
        return;
      }
      if (!confirm(`선택 ${volume_ids.length}권을 ‘${target.title}’(으)로 옮길까요?`)) return;
      try {
        await api.moveVolumes(volume_ids, target.id);
        toast("이동했습니다", "ok");
        window.removeEventListener("beforeunload", unloadHandler);
        location.href = `/series/${id}?edit=1`;
      } catch (ex) {
        toast(ex.message, "error");
      }
    });

    root.querySelector("#edit-split-vols")?.addEventListener("click", async () => {
      const volume_ids = selectedEditVolumeIds();
      if (!volume_ids.length) {
        toast("분리할 권을 선택하세요", "error");
        return;
      }
      const title = prompt("새 시리즈 제목");
      if (title == null) return;
      if (!title.trim()) {
        toast("제목을 입력하세요", "error");
        return;
      }
      try {
        const res = await api.splitVolumes({
          volume_ids,
          title: title.trim(),
          author: series.author || null,
          publisher: series.publisher || null,
          force: true,
        });
        toast(`새 시리즈로 분리했습니다 (${res.volume_count}권)`, "ok");
        window.removeEventListener("beforeunload", unloadHandler);
        location.href = `/series/${res.series_id}?edit=1`;
      } catch (ex) {
        toast(ex.message, "error");
      }
    });

    root.querySelector("#edit-merge-series")?.addEventListener("click", async () => {
      const target = await pickSeriesTarget("이 작품을 합칠 대상 시리즈 제목 검색");
      if (!target) return;
      if (target.id === id) {
        toast("같은 시리즈입니다", "error");
        return;
      }
      if (
        !confirm(
          `‘${series.title}’의 모든 권을 ‘${target.title}’(으)로 합치고 이 시리즈를 삭제할까요?`
        )
      ) {
        return;
      }
      try {
        const res = await api.mergeSeries(id, target.id);
        toast(`병합했습니다 (${res.volume_count}권)`, "ok");
        window.removeEventListener("beforeunload", unloadHandler);
        location.href = `/series/${res.series_id}?edit=1`;
      } catch (ex) {
        toast(ex.message, "error");
      }
    });

    root.querySelector("#manual-edit-form")?.addEventListener("submit", async (e) => {
      e.preventDefault();
      const title = root.querySelector("#edit-title").value.trim();
      if (!title) {
        toast("제목을 입력하세요", "error");
        return;
      }
      const volumes = [...volsBox.querySelectorAll(".manual-vol-row")].map((row, i) => {
        const vid = row.dataset.vid || "";
        return {
          id: vid || null,
          volume_number: i + 1,
          title: row.querySelector(".manual-vol-title").value.trim() || null,
          published_at: row.querySelector(".manual-vol-date").value || null,
          cover_url: row.querySelector(".manual-vol-cover").value.trim() || null,
          is_unreleased: !!row.querySelector(".manual-vol-unreleased-check")?.checked,
          label: row.querySelector(".manual-vol-label")?.value.trim() || null,
        };
      });
      const saveBtn = root.querySelector("#edit-save");
      saveBtn.disabled = true;
      saveBtn.textContent = "저장 중…";
      try {
        const res = await api.updateManualSeries(id, {
          title,
          author: root.querySelector("#edit-author").value.trim() || null,
          publisher: root.querySelector("#edit-publisher").value.trim() || null,
          cover_url: root.querySelector("#edit-cover").value.trim() || null,
          source_label: root.querySelector("#edit-source-label")?.value.trim() || null,
          source_url: root.querySelector("#edit-source-url")?.value.trim() || null,
          volumes,
          force: true,
        });
        toast(`저장했습니다 (${res.volume_count}권)`, "ok");
        window.removeEventListener("beforeunload", unloadHandler);
        location.href = `/series/${res.series_id}?edit=1`;
      } catch (ex) {
        toast(ex.message, "error");
        saveBtn.disabled = false;
        saveBtn.textContent = "저장";
      }
    });

    bindRemove();
  }

  function wire() {
    onBeforeLeave(() => {
      if (!dirty) {
        window.removeEventListener("beforeunload", unloadHandler);
        return true;
      }
      const leave = confirm("저장하지 않은 읽음 상태가 있습니다. 페이지를 나갈까요?");
      if (leave) window.removeEventListener("beforeunload", unloadHandler);
      return leave;
    });

    const items = [...root.querySelectorAll(".volume-item")];
    items.forEach((li, index) => {
      const volumeId = li.dataset.vid;
      const input = li.querySelector("input");
      input.addEventListener("click", (e) => {
        if (e.shiftKey && lastReadClickIndex != null && lastReadClickIndex !== index) {
          const start = Math.min(lastReadClickIndex, index);
          const end = Math.max(lastReadClickIndex, index);
          const checked = input.checked;
          for (let i = start; i <= end; i++) {
            const row = items[i];
            if (!row) continue;
            const vid = row.dataset.vid;
            const cb = row.querySelector("input");
            readMap.set(vid, checked);
            if (cb) cb.checked = checked;
            row.classList.toggle("is-read", checked);
          }
          markDirty(true);
          updateProgressLabel();
        }
        lastReadClickIndex = index;
      });
      input.addEventListener("change", () => {
        readMap.set(volumeId, input.checked);
        li.classList.toggle("is-read", input.checked);
        markDirty(true);
        updateProgressLabel();
      });
    });

    root.querySelector("#select-all").addEventListener("click", () => {
      const values = [...readMap.values()];
      const allOn = values.length > 0 && values.every(Boolean);
      for (const volumeId of readMap.keys()) readMap.set(volumeId, !allOn);
      markDirty(true);
      paint();
    });

    root.querySelector("#volume-order-dir")?.addEventListener("click", () => {
      volumeOrder = volumeOrder === "asc" ? "desc" : "asc";
      series.volumes.reverse();
      lastReadClickIndex = null;
      const sp = new URLSearchParams(location.search);
      if (volumeOrder === "asc") sp.set("order", "asc");
      else sp.delete("order");
      const qs = sp.toString();
      history.replaceState(null, "", `/series/${id}${qs ? `?${qs}` : ""}`);
      paint();
    });

    root.querySelector("#save-reads").addEventListener("click", async () => {
      const reads = [...readMap.entries()].map(([volume_id, is_read]) => ({
        volume_id,
        is_read,
      }));
      try {
        await api.saveReads(id, reads);
        savedReadCount = [...readMap.values()].filter(Boolean).length;
        if (savedReadCount === 0) {
          rating = "None";
          const select = root.querySelector("#rating-select");
          if (select) select.value = "None";
        }
        syncRatingEnabled();
        markDirty(false);
        toast("읽음 상태를 저장했습니다", "ok");
      } catch (ex) {
        toast(ex.message, "error");
      }
    });

    root.querySelector("#rating-select").addEventListener("change", async (e) => {
      const next = e.target.value;
      try {
        await api.saveRating(id, next);
        rating = next;
        toast("평가를 저장했습니다", "ok");
      } catch (ex) {
        e.target.value = rating;
        toast(ex.message, "error");
      }
    });

    const refreshBtn = root.querySelector("#refresh-volumes");
    if (refreshBtn) {
      refreshBtn.addEventListener("click", async () => {
        if (dirty && !confirm("저장하지 않은 읽음 상태가 있습니다. 그래도 갱신할까요?")) {
          return;
        }
        refreshBtn.disabled = true;
        refreshBtn.textContent = "갱신 중…";
        try {
          const res = await api.importSeries({
            aladin_series_id: series.aladin_series_id,
            title: series.title,
          });
          toast(`권 목록을 갱신했습니다 (${res.volume_count}권)`, "ok");
          window.removeEventListener("beforeunload", unloadHandler);
          location.href = `/series/${res.series_id}`;
        } catch (ex) {
          toast(ex.message, "error");
          refreshBtn.disabled = false;
          refreshBtn.textContent = "권 목록 갱신";
        }
      });
    }

    root.querySelector("#publish-status-select")?.addEventListener("change", async (e) => {
      const next = e.target.value;
      const prev = currentPublishStatus(series);
      if (next === prev) return;
      try {
        const res = await api.setSeriesPublishStatus(id, next);
        series.publish_status = res.publish_status || next;
        toast(`${publishStatusLabel(series.publish_status)}(으)로 표시했습니다`, "ok");
        paint();
      } catch (ex) {
        e.target.value = prev;
        toast(ex.message, "error");
      }
    });

    root.querySelector("#edit-catalog")?.addEventListener("click", () => {
      if (dirty && !confirm("저장하지 않은 읽음 상태가 있습니다. 수정을 열까요?")) {
        return;
      }
      editing = !editing;
      const url = new URL(location.href);
      if (editing) url.searchParams.set("edit", "1");
      else url.searchParams.delete("edit");
      history.replaceState(null, "", url.pathname + url.search);
      paint();
    });

    root.querySelector("#delete-series")?.addEventListener("click", async () => {
      if (
        !confirm(
          `‘${series.title}’을(를) 삭제할까요?\n모든 사용자의 읽음·평가·티어리스트 기록도 함께 삭제됩니다.`
        )
      ) {
        return;
      }
      try {
        await api.deleteManualSeries(id);
        toast("삭제했습니다", "ok");
        window.removeEventListener("beforeunload", unloadHandler);
        location.href = seriesListReturnUrl();
      } catch (ex) {
        toast(ex.message, "error");
      }
    });

    root.querySelector("#request-edit")?.addEventListener("click", async () => {
      const note = prompt("수정이 필요한 내용을 적어 주세요");
      if (note === null) return;
      try {
        await api.createCatalogRequest({
          request_type: "edit",
          series_id: id,
          title: series.title,
          note: note.trim() || null,
        });
        toast("수정 요청을 보냈습니다", "ok");
      } catch (ex) {
        toast(ex.message, "error");
      }
    });

    if (editing) wireEditForm();
  }

  function updateProgressLabel() {
    const readCount = [...readMap.values()].filter(Boolean).length;
    const pct = series.total_volumes
      ? Math.round((readCount / series.total_volumes) * 100)
      : 0;
    const dd = root.querySelector(".detail-facts div:last-child dd");
    if (dd) dd.textContent = `${readCount} / ${series.total_volumes}권 (${pct}%)`;
  }

  function syncRatingEnabled() {
    const select = root.querySelector("#rating-select");
    if (!select) return;
    select.disabled = savedReadCount === 0;
    select.title = savedReadCount === 0 ? "한 권 이상 읽은 뒤 평가할 수 있습니다" : "";
  }

  paint();
}

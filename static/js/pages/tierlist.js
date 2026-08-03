import { toPng, toBlob } from "https://cdn.jsdelivr.net/npm/html-to-image@1.11.11/+esm";
import { api } from "../api.js";
import { getUser } from "../auth.js";
import { shell, cover, escapeHtml, toast, bindLogout } from "../ui.js";

const TIER_ORDER = ["S", "A", "B", "C", "D", "F"];
const COLS_STORAGE_KEY = "linvlib.tierlist.cols";
const GATE_STORAGE_KEY = "linvlib.tierlist.gate";
const COLS_CHOICES = [4, 5, 6, 7, 8, 10, 12];

const CAPTURE_OPTS = {
  pixelRatio: 2,
  backgroundColor: "#e8eef3",
  skipFonts: true,
  cacheBust: true,
};

function loadColsMode() {
  const raw = localStorage.getItem(COLS_STORAGE_KEY) || "auto";
  if (raw === "auto") return "auto";
  const n = Number(raw);
  return COLS_CHOICES.includes(n) ? String(n) : "auto";
}

function loadGateMode() {
  return localStorage.getItem(GATE_STORAGE_KEY) === "1";
}

function resolveAutoCols(trackWidth) {
  const mobile = window.matchMedia("(max-width: 640px)").matches;
  const gap = mobile ? 6 : 9;
  const minCover = mobile ? 46 : 68;
  const width = Math.max(trackWidth || 0, 200);
  const cols = Math.floor((width + gap) / (minCover + gap));
  return Math.min(14, Math.max(mobile ? 3 : 4, cols || (mobile ? 4 : 8)));
}

function colsSelectHtml(mode) {
  const options = [
    `<option value="auto"${mode === "auto" ? " selected" : ""}>자동</option>`,
    ...COLS_CHOICES.map(
      (n) => `<option value="${n}"${mode === String(n) ? " selected" : ""}>${n}개</option>`
    ),
  ];
  return `
    <label class="tier-cols">
      <span>한 줄</span>
      <select id="tier-cols" aria-label="한 줄에 표시할 표지 수">${options.join("")}</select>
    </label>`;
}

function emptyGates() {
  return Object.fromEntries(
    TIER_ORDER.map((t) => [t, { above_series_id: null, below_series_id: null }])
  );
}

function normalizeGates(list) {
  const map = emptyGates();
  for (const g of list || []) {
    if (!map[g.tier]) continue;
    map[g.tier] = {
      above_series_id: g.above_series_id || null,
      below_series_id: g.below_series_id || null,
    };
  }
  return map;
}

function findEntryEverywhere(tiers, noneEntries, id) {
  for (const t of TIER_ORDER) {
    const e = (tiers[t] || []).find((x) => x.series_id === id);
    if (e) return e;
  }
  return (noneEntries || []).find((x) => x.series_id === id) || null;
}

function pruneGates(gates, tiers) {
  const next = emptyGates();
  for (const tier of TIER_ORDER) {
    const ids = new Set((tiers[tier] || []).map((e) => e.series_id));
    const g = gates[tier] || {};
    let above = g.above_series_id && ids.has(g.above_series_id) ? g.above_series_id : null;
    let below = g.below_series_id && ids.has(g.below_series_id) ? g.below_series_id : null;
    if (tier === "S") above = null;
    if (tier === "F") below = null;
    if (above && above === below) below = null;
    next[tier] = { above_series_id: above, below_series_id: below };
  }
  return next;
}

export async function renderTierlist(root) {
  root.innerHTML = shell(
    { email: getUser()?.email, active: "tierlist", isAdmin: !!getUser()?.is_admin },
    `<main class="page"><p class="muted">불러오는 중…</p></main>`
  );
  bindLogout();

  let tiers;
  let noneEntries;
  let gates;
  try {
    const data = await api.getTierlist();
    tiers = normalizeTiers(data.tiers || []);
    noneEntries = normalizeNone(data.none_entries || []);
    gates = normalizeGates(data.gatekeepers || []);
  } catch (ex) {
    root.querySelector("main").innerHTML = `<p class="empty">${escapeHtml(ex.message)}</p>`;
    return;
  }

  let dirty = false;
  let dragItem = null;
  let capturing = false;
  let colsMode = loadColsMode();
  let gateMode = loadGateMode();
  let selectedId = null;
  let resizeObs = null;

  function applyTierCols() {
    const board = root.querySelector("#tier-board");
    const none = root.querySelector("#tier-none");
    const sampleTrack =
      board?.querySelector("[data-drop]") || none?.querySelector("[data-drop]");
    const trackWidth = sampleTrack?.clientWidth || board?.clientWidth || 0;
    const cols = colsMode === "auto" ? resolveAutoCols(trackWidth) : Number(colsMode);
    const gap = window.matchMedia("(max-width: 640px)").matches ? "0.35rem" : "0.55rem";
    for (const el of [board, none]) {
      if (!el) continue;
      el.style.setProperty("--tier-cols", String(cols));
      el.style.setProperty("--tier-gap", gap);
    }
    const select = root.querySelector("#tier-cols");
    if (select && select.value !== colsMode) select.value = colsMode;
  }

  function markDirty() {
    dirty = true;
    const btn = root.querySelector("#save-tierlist");
    if (btn) btn.disabled = false;
  }

  function paint() {
    root.innerHTML = shell(
      { email: getUser()?.email, active: "tierlist", isAdmin: !!getUser()?.is_admin },
      `<main class="page${gateMode ? " gate-on" : ""}">
        <div class="page__head page__head--row">
          <div>
            <h1>티어리스트</h1>
            <p class="page__lead">드래그로 순서를 바꿉니다. 티어를 옮기면 평가도 함께 변경됩니다.</p>
          </div>
          <div class="tier-actions">
            <label class="tier-gate-toggle">
              <input type="checkbox" id="gate-mode" ${gateMode ? "checked" : ""} />
              <span>수문장 모드</span>
            </label>
            ${colsSelectHtml(colsMode)}
            <button type="button" class="btn btn--ghost" id="copy-image">이미지 복사</button>
            <button type="button" class="btn btn--ghost" id="download-image">이미지 저장</button>
            <button type="button" class="btn btn--primary" id="save-tierlist" ${dirty ? "" : "disabled"}>저장</button>
          </div>
        </div>
        ${
          gateMode
            ? `<p class="muted tier-gate-hint">표지를 선택한 뒤 윗/아래 슬롯을 누르면 수문장으로 지정됩니다. (S 윗·F 아래 제외) 저장해야 서버에 반영됩니다.</p>`
            : ""
        }
        <div class="tier-board" id="tier-board">
          ${TIER_ORDER.map((tier) =>
            renderTierRow(tier, tiers[tier] || [], {
              gateMode,
              gates: gates[tier],
              selectedId,
              findEntry: (id) => findEntryEverywhere(tiers, noneEntries, id),
            })
          ).join("")}
        </div>
        <div class="tier-none" id="tier-none" aria-label="미평가">
          ${renderTierRow("None", noneEntries, { separate: true })}
        </div>
      </main>`
    );
    bindLogout();
    requestAnimationFrame(() => {
      applyTierCols();
      wire();
    });
  }

  function setCaptureBusy(busy) {
    capturing = busy;
    for (const id of ["copy-image", "download-image"]) {
      const btn = root.querySelector(`#${id}`);
      if (btn) btn.disabled = busy;
    }
  }

  function wire() {
    resizeObs?.disconnect();
    const board = root.querySelector("#tier-board");
    if (board && typeof ResizeObserver !== "undefined") {
      resizeObs = new ResizeObserver(() => {
        if (colsMode === "auto") applyTierCols();
      });
      resizeObs.observe(board);
    }

    root.querySelector("#tier-cols")?.addEventListener("change", (e) => {
      colsMode = e.target.value;
      localStorage.setItem(COLS_STORAGE_KEY, colsMode);
      applyTierCols();
    });

    root.querySelector("#gate-mode")?.addEventListener("change", (e) => {
      gateMode = e.target.checked;
      localStorage.setItem(GATE_STORAGE_KEY, gateMode ? "1" : "0");
      selectedId = null;
      paint();
    });

    root.querySelector("#tier-board")?.addEventListener("click", (e) => {
      if (!gateMode || capturing) return;
      const clear = e.target.closest("[data-clear-gate]");
      if (clear) {
        e.preventDefault();
        e.stopPropagation();
        const tier = clear.dataset.clearGate;
        const side = clear.dataset.clearSide;
        if (!gates[tier]) return;
        gates[tier][side === "above" ? "above_series_id" : "below_series_id"] = null;
        selectedId = null;
        markDirty();
        paint();
        return;
      }

      const slot = e.target.closest("[data-gate-tier]");
      if (slot) {
        e.preventDefault();
        const tier = slot.dataset.gateTier;
        const side = slot.dataset.gateSide;
        if (!selectedId) {
          toast("먼저 같은 티어의 표지를 선택하세요", "error");
          return;
        }
        const inTier = (tiers[tier] || []).some((x) => x.series_id === selectedId);
        if (!inTier) {
          toast("수문장은 같은 티어 작품만 지정할 수 있습니다", "error");
          return;
        }
        if ((tier === "S" && side === "above") || (tier === "F" && side === "below")) return;
        const key = side === "above" ? "above_series_id" : "below_series_id";
        const other = side === "above" ? "below_series_id" : "above_series_id";
        if (gates[tier][other] === selectedId) gates[tier][other] = null;
        gates[tier][key] = selectedId;
        selectedId = null;
        markDirty();
        paint();
        return;
      }

      const card = e.target.closest(".tier-card");
      if (!card || card.closest("#tier-none")) return;
      // Don't steal clicks during drag end noise
      if (document.body.classList.contains("is-tier-dragging")) return;
      const id = card.dataset.id;
      selectedId = selectedId === id ? null : id;
      paint();
    });

    const LONG_PRESS_MS = 200;
    const CANCEL_MOVE_PX = 14;

    let activeCard = null;
    let ghost = null;
    let grabOffsetX = 0;
    let grabOffsetY = 0;
    let pressTimer = null;
    let startX = 0;
    let startY = 0;
    let pendingCard = null;
    let touchId = null;
    let rafMove = 0;
    let lastX = 0;
    let lastY = 0;
    let lastSlotKey = "";

    function clearPressTimer() {
      if (pressTimer) {
        clearTimeout(pressTimer);
        pressTimer = null;
      }
    }

    function positionGhost(x, y) {
      if (!ghost) return;
      ghost.style.transform = `translate3d(${Math.round(x - grabOffsetX)}px, ${Math.round(y - grabOffsetY)}px, 0)`;
    }

    function findTouch(e) {
      if (touchId == null) return e.changedTouches?.[0] || e.touches?.[0] || null;
      const list = [...(e.touches || []), ...(e.changedTouches || [])];
      return list.find((t) => t.identifier === touchId) || null;
    }

    function beginTouchDrag(card, x, y) {
      clearPressTimer();
      pendingCard = null;
      const rect = card.getBoundingClientRect();
      activeCard = card;
      grabOffsetX = x - rect.left;
      grabOffsetY = y - rect.top;
      dragItem = {
        id: card.dataset.id,
        from: card.closest("[data-drop]")?.dataset.drop,
      };

      ghost = card.cloneNode(true);
      ghost.classList.add("tier-card--ghost");
      ghost.setAttribute("aria-hidden", "true");
      ghost.style.width = `${rect.width}px`;
      document.body.appendChild(ghost);
      positionGhost(x, y);

      card.classList.add("is-dragging-source");
      document.body.classList.add("is-tier-dragging");
      lastSlotKey = "";
      updateTouchSlot(x, y);
    }

    function endTouchDrag(commit) {
      clearPressTimer();
      pendingCard = null;
      touchId = null;
      if (rafMove) {
        cancelAnimationFrame(rafMove);
        rafMove = 0;
      }
      root.querySelectorAll(".tier-row__track").forEach((t) => t.classList.remove("is-over"));
      if (ghost) {
        ghost.remove();
        ghost = null;
      }
      if (activeCard) {
        activeCard.classList.remove("is-dragging-source");
        activeCard = null;
      }
      document.body.classList.remove("is-tier-dragging");
      if (commit && dragItem) {
        syncFromDom();
        gates = pruneGates(gates, tiers);
        markDirty();
      }
      dragItem = null;
      lastSlotKey = "";
    }

    function updateTouchSlot(x, y) {
      const el = document.elementFromPoint(x, y);
      const track = el?.closest?.("[data-drop]");
      root.querySelectorAll(".tier-row__track").forEach((t) => t.classList.remove("is-over"));
      if (!track || !activeCard || !ghost) return;
      track.classList.add("is-over");
      const after = getDragAfterElement(track, x, y, null, activeCard);
      const key = `${track.dataset.drop}:${after?.dataset.id || "end"}`;
      if (key === lastSlotKey) return;
      lastSlotKey = key;
      if (after) track.insertBefore(activeCard, after);
      else track.appendChild(activeCard);
    }

    root.querySelectorAll(".tier-card").forEach((card) => {
      card.addEventListener("dragstart", (e) => {
        if (gateMode && selectedId) {
          // Allow drag; selection clears after drop via sync
        }
        dragItem = {
          id: card.dataset.id,
          from: card.closest("[data-drop]")?.dataset.drop,
        };
        card.classList.add("is-dragging");
        e.dataTransfer.effectAllowed = "move";
        try {
          e.dataTransfer.setData("text/plain", card.dataset.id);
        } catch {
          /* ignore */
        }
      });
      card.addEventListener("dragend", () => {
        card.classList.remove("is-dragging");
        root.querySelectorAll(".tier-row__track").forEach((t) => t.classList.remove("is-over"));
        if (dragItem) {
          syncFromDom();
          gates = pruneGates(gates, tiers);
          selectedId = null;
          markDirty();
          paint();
        }
        dragItem = null;
      });

      card.addEventListener(
        "touchstart",
        (e) => {
          if (capturing) return;
          const t = e.changedTouches?.[0];
          if (!t) return;
          touchId = t.identifier;
          pendingCard = card;
          startX = t.clientX;
          startY = t.clientY;
          clearPressTimer();
          pressTimer = setTimeout(() => {
            if (pendingCard === card) beginTouchDrag(card, startX, startY);
          }, LONG_PRESS_MS);
        },
        { passive: true }
      );

      card.addEventListener(
        "touchmove",
        (e) => {
          const t = findTouch(e);
          if (!t) return;
          lastX = t.clientX;
          lastY = t.clientY;
          if (pendingCard && !activeCard) {
            const dx = t.clientX - startX;
            const dy = t.clientY - startY;
            if (Math.hypot(dx, dy) > CANCEL_MOVE_PX) {
              clearPressTimer();
              pendingCard = null;
            }
            return;
          }
          if (!activeCard) return;
          e.preventDefault();
          positionGhost(t.clientX, t.clientY);
          if (!rafMove) {
            rafMove = requestAnimationFrame(() => {
              rafMove = 0;
              updateTouchSlot(lastX, lastY);
            });
          }
        },
        { passive: false }
      );

      card.addEventListener("touchend", () => endTouchDrag(true));
      card.addEventListener("touchcancel", () => endTouchDrag(false));
    });

    root.querySelectorAll("[data-drop]").forEach((track) => {
      track.addEventListener("dragover", (e) => {
        e.preventDefault();
        track.classList.add("is-over");
        const dragging = root.querySelector(".tier-card.is-dragging");
        if (!dragging) return;
        const after = getDragAfterElement(track, e.clientX, e.clientY, null, dragging);
        if (after) track.insertBefore(dragging, after);
        else track.appendChild(dragging);
      });
      track.addEventListener("dragleave", () => track.classList.remove("is-over"));
      track.addEventListener("drop", (e) => {
        e.preventDefault();
        track.classList.remove("is-over");
        if (!dragItem) return;
        syncFromDom();
        gates = pruneGates(gates, tiers);
        selectedId = null;
        markDirty();
        paint();
      });
    });

    root.querySelector("#save-tierlist").addEventListener("click", async () => {
      syncFromDom();
      gates = pruneGates(gates, tiers);
      const payload = {
        tiers: TIER_ORDER.map((tier) => ({
          tier,
          series_ids: (tiers[tier] || []).map((e) => e.series_id),
        })),
        gatekeepers: TIER_ORDER.map((tier) => ({
          tier,
          above_series_id: gates[tier]?.above_series_id || null,
          below_series_id: gates[tier]?.below_series_id || null,
        })),
      };
      try {
        const saved = await api.saveTierlist(payload);
        tiers = normalizeTiers(saved.tiers || []);
        noneEntries = normalizeNone(saved.none_entries || []);
        gates = normalizeGates(saved.gatekeepers || []);
        dirty = false;
        selectedId = null;
        toast("티어리스트를 저장했습니다", "ok");
        paint();
      } catch (ex) {
        toast(ex.message, "error");
      }
    });

    root.querySelector("#download-image").addEventListener("click", async () => {
      if (capturing) return;
      setCaptureBusy(true);
      try {
        const dataUrl = await captureBoard(root.querySelector("#tier-board"), "png");
        const a = document.createElement("a");
        a.href = dataUrl;
        a.download = "linvlib-tierlist.png";
        a.click();
        toast("이미지를 저장했습니다", "ok");
      } catch (ex) {
        console.error(ex);
        toast(ex?.message || "이미지 저장에 실패했습니다", "error");
      } finally {
        setCaptureBusy(false);
      }
    });

    root.querySelector("#copy-image").addEventListener("click", async () => {
      if (capturing) return;
      if (!navigator.clipboard?.write || typeof ClipboardItem === "undefined") {
        toast("이 브라우저에서는 복사를 지원하지 않습니다", "error");
        return;
      }
      setCaptureBusy(true);
      try {
        const blob = await captureBoard(root.querySelector("#tier-board"), "blob");
        if (!blob) throw new Error("이미지 생성에 실패했습니다");
        await navigator.clipboard.write([
          new ClipboardItem({ "image/png": Promise.resolve(blob) }),
        ]);
        toast("클립보드에 복사했습니다", "ok");
      } catch (ex) {
        console.error(ex);
        toast(ex?.message || "이미지 복사에 실패했습니다", "error");
      } finally {
        setCaptureBusy(false);
      }
    });
  }

  function syncFromDom() {
    const next = Object.fromEntries(TIER_ORDER.map((t) => [t, []]));
    const nextNone = [];
    const byId = Object.fromEntries([
      ...TIER_ORDER.flatMap((t) => (tiers[t] || []).map((e) => [e.series_id, e])),
      ...noneEntries.map((e) => [e.series_id, e]),
    ]);
    root.querySelectorAll("[data-drop]").forEach((track) => {
      const tier = track.dataset.drop;
      track.querySelectorAll(".tier-card").forEach((card, i) => {
        const base = byId[card.dataset.id] || {
          series_id: card.dataset.id,
          title: card.querySelector(".tier-card__title")?.textContent || "",
          cover_url: card.querySelector("img")?.src || null,
        };
        const entry = { ...base, position: i };
        if (tier === "None") nextNone.push(entry);
        else if (next[tier]) next[tier].push(entry);
      });
    });
    tiers = next;
    noneEntries = nextNone;
  }

  paint();
}

async function captureBoard(board, mode) {
  if (!board) throw new Error("티어보드를 찾을 수 없습니다");
  const restore = await inlineCoverImages(board);
  try {
    await waitForPaint();
    const opts = {
      ...CAPTURE_OPTS,
      width: board.scrollWidth,
      height: board.scrollHeight,
      style: {
        transform: "none",
        inset: "auto",
      },
    };
    if (mode === "blob") {
      return await toBlob(board, opts);
    }
    return await toPng(board, opts);
  } finally {
    restore();
  }
}

async function inlineCoverImages(rootEl) {
  const imgs = [...rootEl.querySelectorAll("img")];
  const backups = imgs.map((img) => ({
    img,
    srcAttr: img.getAttribute("src"),
    loading: img.getAttribute("loading"),
  }));

  await Promise.all(
    imgs.map(async (img) => {
      const src = img.getAttribute("src") || img.currentSrc || img.src;
      if (!src || src.startsWith("data:") || src.startsWith("blob:")) {
        await img.decode?.().catch(() => {});
        return;
      }
      try {
        const proxyUrl = `/api/v1/media/proxy?url=${encodeURIComponent(src)}`;
        const res = await fetch(proxyUrl);
        if (!res.ok) throw new Error(`proxy ${res.status}`);
        const blob = await res.blob();
        img.src = await blobToDataUrl(blob);
        img.removeAttribute("loading");
        await img.decode().catch(() => {});
      } catch (err) {
        console.warn("cover embed failed", src, err);
      }
    })
  );

  return () => {
    for (const b of backups) {
      if (b.srcAttr == null) b.img.removeAttribute("src");
      else b.img.setAttribute("src", b.srcAttr);
      if (b.loading == null) b.img.removeAttribute("loading");
      else b.img.setAttribute("loading", b.loading);
    }
  };
}

function waitForPaint() {
  return new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(resolve));
  });
}

function blobToDataUrl(blob) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result);
    reader.onerror = () => reject(reader.error || new Error("read failed"));
    reader.readAsDataURL(blob);
  });
}

function renderGateSlot(tier, side, gates, findEntry) {
  const disabled =
    (tier === "S" && side === "above") || (tier === "F" && side === "below");
  const tag = side === "above" ? "윗" : "아래";
  if (disabled) {
    return `<div class="gate-slot gate-slot--disabled" title="${
      side === "above" ? "S 티어는 윗 수문장 없음" : "F 티어는 아래 수문장 없음"
    }"><span class="gate-slot__tag">${tag}</span><span class="gate-slot__placeholder">—</span></div>`;
  }
  const id = gates?.[side === "above" ? "above_series_id" : "below_series_id"] || null;
  const entry = id ? findEntry(id) : null;
  const filled = !!entry;
  return `
    <div class="gate-slot ${filled ? "is-filled" : "is-empty"}"
         data-gate-tier="${tier}" data-gate-side="${side}">
      <span class="gate-slot__tag">${tag} 수문장</span>
      ${
        filled
          ? `${cover(entry.cover_url, entry.title, "cover cover--tier gate-slot__cover")}
             <button type="button" class="gate-slot__clear" data-clear-gate="${tier}" data-clear-side="${side}">해제</button>`
          : `<span class="gate-slot__placeholder">선택 후<br/>탭</span>`
      }
    </div>`;
}

function renderTierRow(
  tier,
  entries,
  { separate = false, gateMode = false, gates = null, selectedId = null, findEntry = () => null } = {}
) {
  const showGates = gateMode && !separate && tier !== "None";
  const cards = entries
    .map((e) => {
      const role =
        showGates && gates
          ? gates.above_series_id === e.series_id
            ? "윗"
            : gates.below_series_id === e.series_id
              ? "아래"
              : null
          : null;
      return `
          <article class="tier-card${selectedId === e.series_id ? " is-selected" : ""}${
            role ? " is-gate" : ""
          }" draggable="true" data-id="${e.series_id}">
            ${cover(e.cover_url, e.title, "cover cover--tier")}
            <p class="tier-card__title">${escapeHtml(e.title)}</p>
            ${role ? `<span class="tier-gate-badge">${role} 수문장</span>` : ""}
          </article>`;
    })
    .join("");

  const body = showGates
    ? `<div class="tier-row__body">
        ${renderGateSlot(tier, "above", gates, findEntry)}
        <div class="tier-row__track" data-drop="${tier}">${cards}</div>
        ${renderGateSlot(tier, "below", gates, findEntry)}
      </div>`
    : `<div class="tier-row__track" data-drop="${tier}">${cards}</div>`;

  return `
    <section class="tier-row${separate ? " tier-row--none" : ""}" data-tier="${tier}">
      <div class="tier-row__label tier-row__label--${tier}">${tier}</div>
      ${body}
    </section>`;
}

function normalizeTiers(list) {
  const map = Object.fromEntries(TIER_ORDER.map((t) => [t, []]));
  for (const row of list) {
    if (!map[row.tier]) continue;
    map[row.tier] = (row.entries || []).slice().sort((a, b) => a.position - b.position);
  }
  return map;
}

function normalizeNone(list) {
  return (list || []).slice().sort((a, b) => a.position - b.position);
}

function getDragAfterElement(container, x, y, placeholder, activeCard) {
  const els = [
    ...container.querySelectorAll(
      ".tier-card:not(.is-dragging):not(.tier-card--ghost)"
    ),
  ].filter(
    (el) =>
      el !== placeholder &&
      el !== activeCard &&
      !el.classList.contains("tier-card--placeholder")
  );

  return els.reduce(
    (closest, child) => {
      const box = child.getBoundingClientRect();
      const offsetX = x - box.left - box.width / 2;
      const offsetY = y - box.top - box.height / 2;
      const sameRow = Math.abs(offsetY) <= box.height * 0.55;
      if (!sameRow) return closest;
      if (offsetX < 0 && offsetX > closest.offset) {
        return { offset: offsetX, element: child };
      }
      return closest;
    },
    { offset: Number.NEGATIVE_INFINITY, element: null }
  ).element;
}

import { getToken, clearAuth } from "./auth.js";

const API = "/api/v1";

export class ApiError extends Error {
  constructor(message, status, data = null) {
    super(message);
    this.status = status;
    this.data = data;
  }
}

async function request(path, options = {}) {
  const headers = { ...(options.headers || {}) };
  if (options.body && !headers["Content-Type"]) {
    headers["Content-Type"] = "application/json";
  }
  const token = getToken();
  if (token) headers.Authorization = `Bearer ${token}`;

  const res = await fetch(`${API}${path}`, { ...options, headers });
  if (res.status === 401 && !path.startsWith("/auth/")) {
    clearAuth();
    if (!location.pathname.startsWith("/login")) {
      location.href = "/login";
    }
    throw new ApiError("로그인이 필요합니다", 401);
  }

  const text = await res.text();
  let data = null;
  if (text) {
    try {
      data = JSON.parse(text);
    } catch {
      const gateway = res.status === 502 || res.status === 503 || res.status === 504;
      throw new ApiError(
        gateway
          ? "서버 또는 카탈로그 API 연결이 잠시 실패했습니다. 잠시 후 다시 시도해 주세요."
          : `서버 응답이 올바르지 않습니다 (${res.status || "network"}).`,
        res.status || 502,
      );
    }
  }
  if (!res.ok) {
    throw new ApiError(data?.error || res.statusText, res.status, data);
  }
  return data;
}

export const api = {
  register: (email, password, { accept_terms = false, accept_privacy = false } = {}) =>
    request("/auth/register", {
      method: "POST",
      body: JSON.stringify({ email, password, accept_terms, accept_privacy }),
    }),
  login: (email, password) =>
    request("/auth/login", {
      method: "POST",
      body: JSON.stringify({ email, password }),
    }),
  verifyEmail: (token) =>
    request("/auth/verify", {
      method: "POST",
      body: JSON.stringify({ token }),
    }),
  resendVerification: (email) =>
    request("/auth/resend-verification", {
      method: "POST",
      body: JSON.stringify({ email }),
    }),
  forgotPassword: (email) =>
    request("/auth/forgot-password", {
      method: "POST",
      body: JSON.stringify({ email }),
    }),
  resetPassword: (token, password) =>
    request("/auth/reset-password", {
      method: "POST",
      body: JSON.stringify({ token, password }),
    }),
  me: () => request("/auth/me"),
  tourDemo: () => request("/tour/demo"),
  deleteAccount: (password) =>
    request("/auth/me", {
      method: "DELETE",
      body: JSON.stringify({ password }),
    }),
  listSeries: ({
    sort = "latest",
    order = "desc",
    page = 1,
    limit = 20,
    q = "",
    status = "all",
    read_f = "",
    rated_f = "",
    ps_in = "",
    ps_ex = "",
  } = {}) => {
    const sp = new URLSearchParams({
      sort,
      order,
      page: String(page),
      limit: String(limit),
      status,
    });
    if (q) sp.set("q", q);
    if (read_f) sp.set("read_f", read_f);
    if (rated_f) sp.set("rated_f", rated_f);
    if (ps_in) sp.set("ps_in", ps_in);
    if (ps_ex) sp.set("ps_ex", ps_ex);
    return request(`/series?${sp}`);
  },
  getSeries: (id, order = "desc") =>
    request(`/series/${id}?order=${encodeURIComponent(order)}`),
  saveReads: (id, reads) =>
    request(`/series/${id}/reads`, {
      method: "PUT",
      body: JSON.stringify({ reads }),
    }),
  saveRating: (id, rating) =>
    request(`/series/${id}/rating`, {
      method: "PUT",
      body: JSON.stringify({ rating }),
    }),
  importSearch: (q) => request(`/imports/search?q=${encodeURIComponent(q)}`),
  importSeries: (payload) =>
    request("/imports", { method: "POST", body: JSON.stringify(payload) }),
  createManualSeries: (payload) =>
    request("/series", { method: "POST", body: JSON.stringify(payload) }),
  updateManualSeries: (id, payload) =>
    request(`/series/${id}`, { method: "PUT", body: JSON.stringify(payload) }),
  reorderVolumes: (id, volume_ids) =>
    request(`/series/${id}/volumes/order`, {
      method: "PUT",
      body: JSON.stringify({ volume_ids }),
    }),
  moveVolumes: (volume_ids, target_series_id) =>
    request("/admin/volumes/move", {
      method: "POST",
      body: JSON.stringify({ volume_ids, target_series_id }),
    }),
  mergeSeries: (source_series_id, target_series_id) =>
    request("/admin/series/merge", {
      method: "POST",
      body: JSON.stringify({ source_series_id, target_series_id }),
    }),
  splitVolumes: (payload) =>
    request("/admin/volumes/split", {
      method: "POST",
      body: JSON.stringify(payload),
    }),
  deleteManualSeries: (id) =>
    request(`/series/${id}`, { method: "DELETE" }),
  searchSeries: (q) => request(`/search?q=${encodeURIComponent(q)}`),
  setSeriesPublishStatus: (id, publish_status) =>
    request(`/series/${id}/publish-status`, {
      method: "PUT",
      body: JSON.stringify({ publish_status }),
    }),
  addSeriesAlias: (id, alias) =>
    request(`/series/${id}/aliases`, {
      method: "POST",
      body: JSON.stringify({ alias }),
    }),
  deleteSeriesAlias: (id, aliasId) =>
    request(`/series/${id}/aliases/${aliasId}`, { method: "DELETE" }),
  batchSearchAliases: (alias, series_ids) =>
    request("/admin/search-aliases", {
      method: "POST",
      body: JSON.stringify({ alias, series_ids }),
    }),
  createSearchBundle: (series_ids) =>
    request("/admin/search-bundles", {
      method: "POST",
      body: JSON.stringify({ series_ids }),
    }),
  createCatalogRequest: (payload) =>
    request("/catalog-requests", {
      method: "POST",
      body: JSON.stringify(payload),
    }),
  listMyCatalogRequests: () => request("/catalog-requests"),
  listAdminCatalogRequests: (status = "pending") =>
    request(`/admin/catalog-requests?status=${encodeURIComponent(status)}`),
  reviewCatalogRequest: (id, payload) =>
    request(`/admin/catalog-requests/${id}`, {
      method: "PUT",
      body: JSON.stringify(payload),
    }),
  adminStatus: () => request("/admin/status"),
  adminUsers: () => request("/admin/users"),
  adminManualSeries: () => request("/admin/manual-series"),
  adminRefresh: () => request("/admin/refresh", { method: "POST" }),
  /**
   * Poll until background refresh finishes.
   * New-release jobs often take 8–15+ minutes; keep polling long enough.
   * On timeout: if the job already finished, return status (no false error).
   * If still running, throw with `stillRunning: true` so UI can warn without treating it as failure.
   */
  waitForRefreshIdle: async ({ intervalMs = 2000, maxAttempts = 900 } = {}) => {
    let status = null;
    for (let i = 0; i < maxAttempts; i += 1) {
      status = await request("/admin/status");
      if (!status.refresh_running) return status;
      await new Promise((r) => setTimeout(r, intervalMs));
    }
    status = await request("/admin/status");
    if (!status.refresh_running) return status;
    const err = new Error(
      "갱신이 아직 백그라운드에서 진행 중입니다. 관리 → 상태에서 완료 여부를 확인해 주세요."
    );
    err.stillRunning = true;
    err.status = status;
    throw err;
  },
  adminNewReleases: () => request("/admin/new-releases"),
  importNewRelease: (id) =>
    request(`/admin/new-releases/${id}/import`, { method: "POST" }),
  dismissNewRelease: (id) =>
    request(`/admin/new-releases/${id}`, { method: "DELETE" }),
  adminBackups: () => request("/admin/backups"),
  adminCreateBackup: () => request("/admin/backups", { method: "POST" }),
  adminDeleteBackup: (name) =>
    request(`/admin/backups/${encodeURIComponent(name)}`, { method: "DELETE" }),
  adminDownloadBackup: async (name) => {
    const token = getToken();
    const res = await fetch(`${API}/admin/backups/${encodeURIComponent(name)}/download`, {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    });
    if (res.status === 401) {
      clearAuth();
      location.href = "/login";
      throw new ApiError("로그인이 필요합니다", 401);
    }
    if (!res.ok) {
      let msg = res.statusText;
      try {
        const data = await res.json();
        msg = data?.error || msg;
      } catch {
        /* ignore */
      }
      throw new ApiError(msg, res.status);
    }
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  },
  getTierlist: () => request("/tierlist"),
  saveTierlist: (payload) =>
    request("/tierlist", {
      method: "PUT",
      body: JSON.stringify(Array.isArray(payload) ? { tiers: payload } : payload),
    }),
};

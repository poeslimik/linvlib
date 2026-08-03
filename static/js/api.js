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
  const data = text ? JSON.parse(text) : null;
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
  me: () => request("/auth/me"),
  deleteAccount: (password) =>
    request("/auth/me", {
      method: "DELETE",
      body: JSON.stringify({ password }),
    }),
  listSeries: ({ sort = "latest", page = 1, limit = 20, q = "", status = "all" } = {}) => {
    const sp = new URLSearchParams({
      sort,
      page: String(page),
      limit: String(limit),
      status,
    });
    if (q) sp.set("q", q);
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
  setSeriesComplete: (id, is_complete) =>
    request(`/series/${id}/complete`, {
      method: "PUT",
      body: JSON.stringify({ is_complete }),
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

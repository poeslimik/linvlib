import { route, startRouter, navigate } from "./router.js";
import { isLoggedIn } from "./auth.js";
import { renderHome } from "./pages/home.js";
import { renderSeriesList } from "./pages/series-list.js";
import { renderSeriesDetail } from "./pages/series-detail.js";
import { renderImport } from "./pages/import.js";
import { renderTierlist } from "./pages/tierlist.js";
import { renderMypage } from "./pages/mypage.js";
import { renderAdmin } from "./pages/admin.js";
import { renderVerify } from "./pages/verify.js";
import { renderResetPassword } from "./pages/reset-password.js";
import { renderTerms, renderPrivacy } from "./pages/legal.js";

route("/", async (root) => {
  if (isLoggedIn()) navigate("/series", { replace: true });
  else navigate("/login", { replace: true });
});

route("/login", renderHome);
route("/verify", renderVerify);
route("/reset-password", renderResetPassword);
route("/terms", renderTerms);
route("/privacy", renderPrivacy);
route("/series", renderSeriesList, { auth: true });
route("/series/:id", renderSeriesDetail, { auth: true });
route("/search", async (_root) => {
  const q = new URLSearchParams(location.search).get("q") || "";
  const dest = q ? `/series?q=${encodeURIComponent(q)}` : "/series";
  navigate(dest, { replace: true });
}, { auth: true });
route("/import", renderImport, { auth: true });
route("/tierlist", renderTierlist, { auth: true });
route("/mypage", renderMypage, { auth: true });
route("/admin", renderAdmin, { auth: true });

startRouter();

const { invoke } = window.__TAURI__.core;
const $ = s => document.querySelector(s);
const esc = s => s.replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const S = Object.assign({ fav: [], hidden: [], recent: {}, manual: [], ui: {}, stats: {}, map: {} }, (() => { try { return JSON.parse(localStorage.gh) } catch { return {} } })());
const save = () => localStorage.gh = JSON.stringify(S);
const DEF = { bg: "#0f1115", accent: "#8f7bff", radius: 16, size: 170 };
const PRESETS = { "Ночь": { bg: "#0f1115", accent: "#8f7bff" }, "Закат": { bg: "#1a1114", accent: "#ff8a5b" }, "Лес": { bg: "#0d1512", accent: "#5fd0a0" }, "Бумага": { bg: "#f3efe7", accent: "#5b3cff" } };
let games = [], filter = "all", q = "", ctx = null, running = new Set();

const lum = h => (parseInt(h.slice(1, 3), 16) * .299 + parseInt(h.slice(3, 5), 16) * .587 + parseInt(h.slice(5, 7), 16) * .114) / 255;
const ui = () => ({ ...DEF, ...S.ui });
function applyUI() {
  const u = ui(), r = document.documentElement.style, light = lum(u.bg) > .5;
  r.setProperty("--bg", u.bg); r.setProperty("--accent", u.accent);
  r.setProperty("--radius", u.radius + "px"); r.setProperty("--size", u.size + "px");
  r.setProperty("--mix", light ? "#000" : "#fff");
  r.setProperty("--text", light ? "#1c1b18" : "#ece9e2"); r.setProperty("--mute", light ? "#6d6a62" : "#8b909c");
  $("#c-accent").value = u.accent; $("#c-bg").value = u.bg; $("#r-radius").value = u.radius; $("#r-size").value = u.size;
}
const setUI = p => { S.ui = { ...S.ui, ...p }; save(); applyUI(); };

const cover = g => { const a = g.appid || S.map[g.id]; return a ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${a}/library_600x900.jpg` : ""; };
const hue = n => [...n].reduce((a, c) => (a * 31 + c.charCodeAt(0)) >>> 0, 7) % 360;
const art = g => `linear-gradient(160deg,hsl(${hue(g.name)} 45% 38%),hsl(${(hue(g.name) + 40) % 360} 50% 18%))`;
const ini = n => n.split(/\s+/).slice(0, 2).map(w => w[0] || "").join("").toUpperCase();
const byId = id => games.find(g => g.id === id);
const toast = t => { const e = $("#toast"); e.textContent = t; e.classList.add("show"); clearTimeout(toast.t); toast.t = setTimeout(() => e.classList.remove("show"), 2600); };

function visible() {
  return games.filter(g => {
    if (S.hidden.includes(g.id) || (q && !g.name.toLowerCase().includes(q))) return false;
    if (filter === "fav") return S.fav.includes(g.id);
    if (filter === "recent") return g.id in S.recent;
    if (filter.startsWith("store:")) return g.store === filter.slice(6);
    return true;
  }).sort((a, b) => filter === "recent" ? S.recent[b.id] - S.recent[a.id] : 0);
}

function render() {
  const list = visible(), shown = games.filter(g => !S.hidden.includes(g.id)), stores = {};
  shown.forEach(g => stores[g.store] = (stores[g.store] || 0) + 1);
  const items = [["all", "Все игры", shown.length], ["fav", "Избранное", ""], ["recent", "Недавние", ""], ["stats", "Статистика", ""], ...Object.entries(stores).sort().map(([s, n]) => ["store:" + s, s, n])];
  $("#nav").innerHTML = items.map(([k, t, n]) => `<button data-f="${esc(k)}" class="${k === filter ? "on" : ""}">${esc(t)}<i>${n}</i></button>`).join("");
  $("#title").textContent = ({ all: "Все игры", fav: "Избранное", recent: "Недавние", stats: "Статистика" })[filter] || filter.slice(6);

  const last = filter === "all" && !q ? list.filter(g => g.id in S.recent).sort((a, b) => S.recent[b.id] - S.recent[a.id])[0] : null;
  $("#hero").innerHTML = last ? `<div class="hero" style="--art:${art(last)}">${cover(last) ? `<img src="${cover(last)}" onerror="this.remove()">` : ""}
    <div><small>Продолжить</small><h3>${esc(last.name)}</h3><button data-hero="${esc(last.id)}">▶ Играть</button></div></div>` : "";

  if (filter === "stats") { $("#hero").innerHTML = ""; $("#grid").innerHTML = statsHTML(); return; }
  $("#grid").innerHTML = list.length ? list.map(g => `<div class="poster" data-id="${esc(g.id)}" style="--art:${art(g)}">
    <span class="ini">${esc(ini(g.name))}</span>${running.has(g.id) ? `<span class="live">В игре</span>` : ""}${cover(g) ? `<img loading="lazy" src="${cover(g)}" onerror="this.remove()">` : ""}
    <button class="fav ${S.fav.includes(g.id) ? "on" : ""}" data-a="fav">★</button><button class="play" data-a="play">▶ Играть</button>
    <div class="cap"><b>${esc(g.name)}</b><small><span>${esc(g.store)}</span><span>${playSec(g) ? fmt(playSec(g)) : ""}</span></small></div></div>`).join("")
    : `<p class="empty" style="grid-column:1/-1">Здесь пока пусто.<br>Нажмите «Обновить список» или добавьте игру вручную.</p>`;
}

function launch(g) {
  invoke("launch_game", { launch: g.launch })
    .then(() => { S.recent[g.id] = Date.now(); save(); render(); toast("Запускаю " + g.name); })
    .catch(e => toast("Не удалось запустить: " + e));
}
function toggle(k, g) { const l = S[k], i = l.indexOf(g.id); i < 0 ? l.push(g.id) : l.splice(i, 1); save(); render(); }

async function scan() {
  $("#grid").innerHTML = `<p class="empty">Ищу игры…</p>`;
  const found = await invoke("scan_games"), seen = new Set(found.map(g => g.id));
  games = [...found, ...S.manual.filter(g => !seen.has(g.id))];
  render();
  resolveCovers();
}

$("#nav").onclick = e => { const b = e.target.closest("[data-f]"); if (b) { filter = b.dataset.f; render(); } };
$("#q").oninput = e => { q = e.target.value.trim().toLowerCase(); render(); };
$("#hero").onclick = e => { const b = e.target.closest("[data-hero]"); if (b) launch(byId(b.dataset.hero)); };
$("#grid").onclick = e => {
  const p = e.target.closest(".poster"); if (!p) return;
  const g = byId(p.dataset.id), a = e.target.dataset.a;
  if (a === "fav") toggle("fav", g); else if (a === "play") launch(g); else openDetail(g);
};
$("#grid").oncontextmenu = e => {
  const p = e.target.closest(".poster"); if (!p) return;
  e.preventDefault(); const g = byId(p.dataset.id), m = $("#menu");
  m.innerHTML = `<button data-m="play">Играть</button><button data-m="fav">${S.fav.includes(g.id) ? "Убрать из избранного" : "В избранное"}</button>${g.folder ? `<button data-m="dir">Открыть папку с игрой</button>` : ""}<button data-m="hide">Скрыть из библиотеки</button>`;
  m.style.display = "block"; m.style.left = Math.min(e.clientX, innerWidth - 220) + "px"; m.style.top = Math.min(e.clientY, innerHeight - 170) + "px"; ctx = g;
};
$("#menu").onclick = e => {
  const a = e.target.dataset.m, g = ctx; if (!a) return;
  if (a === "play") launch(g); else if (a === "fav") toggle("fav", g); else if (a === "hide") toggle("hidden", g); else invoke("open_folder", { path: g.folder });
};
addEventListener("click", () => $("#menu").style.display = "none");
addEventListener("keydown", e => { if (e.key === "Escape") { $("#menu").style.display = "none"; $("#drawer").classList.remove("open"); closeDetail(); } });
addEventListener("contextmenu", e => { if (!e.target.closest(".poster")) e.preventDefault(); });

$("#rescan").onclick = scan;
$("#add").onclick = async () => { const g = await invoke("pick_exe"); if (g) { S.manual.push(g); save(); games.push(g); render(); } };
$("#theme").onclick = e => { e.stopPropagation(); $("#drawer").classList.toggle("open"); };
$("#close").onclick = () => $("#drawer").classList.remove("open");
$("#unhide").onclick = () => { S.hidden = []; save(); render(); };
$("#reset").onclick = () => { S.ui = {}; save(); applyUI(); };
$("#presets").innerHTML = Object.entries(PRESETS).map(([n, p]) => `<button data-p="${n}"><span style="background:${p.accent}"></span>${n}</button>`).join("");
$("#presets").onclick = e => { const b = e.target.closest("[data-p]"); if (b) setUI(PRESETS[b.dataset.p]); };
[["c-accent", "accent"], ["c-bg", "bg"], ["r-radius", "radius"], ["r-size", "size"]].forEach(([id, k]) =>
  $("#" + id).oninput = e => setUI({ [k]: e.target.type === "range" ? +e.target.value : e.target.value }));

// ---------- статистика, обложки, страница игры ----------
const RV = { "Overwhelmingly Positive": "Крайне положительные", "Very Positive": "Очень положительные", "Positive": "Положительные", "Mostly Positive": "В основном положительные", "Mixed": "Смешанные", "Mostly Negative": "В основном отрицательные", "Negative": "Отрицательные", "Very Negative": "Очень отрицательные", "Overwhelmingly Negative": "Крайне отрицательные" };
const playSec = g => g.playtime != null ? g.playtime * 60 : (S.stats[g.id]?.sec || 0);
const fmt = s => s < 60 ? "< 1 мин" : s < 3600 ? Math.round(s / 60) + " мин" : (s / 3600).toFixed(1).replace(/\.0$/, "") + " ч";
const lastTs = g => Math.max(S.recent[g.id] || 0, S.stats[g.id]?.last || 0, (g.last_played || 0) * 1000);
const ago = t => { if (!t) return "ещё не запускали"; const d = Math.floor((Date.now() - t) / 864e5); return d < 1 ? "сегодня" : d === 1 ? "вчера" : d < 30 ? d + " дн. назад" : new Date(t).toLocaleDateString("ru"); };
const plain = h => new DOMParser().parseFromString(h || "", "text/html").documentElement.textContent.trim();
const detailOpen = () => $("#detail").classList.contains("open");

// Свой трекер: раз в 20 секунд смотрим, запущены ли игры (работает, пока открыт GameHub).
setInterval(async () => {
  if (!games.length) return;
  let ids; try { ids = await invoke("running", { watch: games.map(g => [g.id, g.watch]) }); } catch { return; }
  ids.forEach(id => { const s = S.stats[id] ||= { sec: 0, n: 0, last: 0 }; s.sec += 20; s.last = Date.now(); if (!running.has(id)) s.n++; });
  const changed = ids.length !== running.size || ids.some(i => !running.has(i));
  running = new Set(ids); save(); if (changed && !detailOpen()) render();
}, 20000);

// Находим игры в Steam по названию — отсюда обложки, описания и видео для любых лаунчеров.
async function resolveCovers() {
  let n = 0;
  for (const g of games) {
    if (g.appid || g.id in S.map) continue;
    try { S.map[g.id] = (await invoke("resolve_appid", { name: g.name })) || 0; n++; } catch { break; }
    if (n % 6 === 0) { save(); if (!detailOpen()) render(); }
    await new Promise(r => setTimeout(r, 250));
  }
  save(); if (n && !detailOpen()) render();
}

async function getInfo(aid) {
  const k = "gi:" + aid;
  try { const c = JSON.parse(localStorage[k]); if (c && Date.now() - c.t < 6048e5) return c.d; } catch { }
  const d = await invoke("game_info", { appid: aid });
  try { localStorage[k] = JSON.stringify({ t: Date.now(), d }); } catch { }
  return d;
}

function statsHTML() {
  const mine = games.filter(g => !S.hidden.includes(g.id));
  const rows = mine.filter(g => playSec(g) > 0).sort((a, b) => playSec(b) - playSec(a));
  const total = rows.reduce((a, g) => a + playSec(g), 0), max = rows.length ? playSec(rows[0]) : 1, by = {};
  rows.forEach(g => by[g.store] = (by[g.store] || 0) + playSec(g));
  const bar = (n, s, m) => `<div class="bar"><span>${esc(n)}</span><div><i style="width:${Math.max(2, s / m * 100)}%"></i></div><em>${fmt(s)}</em></div>`;
  return `<div class="stats" style="grid-column:1/-1"><div class="kpis"><div><b>${fmt(total)}</b><span>всего в играх</span></div><div><b>${rows.length}</b><span>игр с наигранным временем</span></div><div><b>${mine.length}</b><span>игр в библиотеке</span></div></div>
  ${rows.length ? `<h4>Больше всего времени</h4>${rows.slice(0, 10).map(g => bar(g.name, playSec(g), max)).join("")}<h4>По лаунчерам</h4>${Object.entries(by).sort((a, b) => b[1] - a[1]).map(([k, s]) => bar(k, s, total)).join("")}`
    : `<p class="empty">Пока нет данных. Время в Steam-играх подтянется само, а для остальных лаунчеров GameHub считает его, пока он запущен.</p>`}</div>`;
}

function closeDetail() { $("#detail").classList.remove("open"); }
async function openDetail(g) {
  const d = $("#detail"), aid = g.appid || S.map[g.id], st = S.stats[g.id];
  d.classList.add("open");
  const hero = aid ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${aid}/library_hero.jpg` : "";
  d.innerHTML = `<div class="d-top" style="--art:${art(g)}">${hero ? `<img src="${hero}" onerror="this.remove()">` : ""}
    <button class="back" id="back">← Назад</button>
    <div class="d-title"><h2>${esc(g.name)}</h2><div class="d-actions"><button class="go" id="d-play">▶ Играть</button><button id="d-fav">${S.fav.includes(g.id) ? "★ В избранном" : "☆ В избранное"}</button></div></div></div>
    <div class="d-strip"><div><small>Наиграно</small><b>${playSec(g) ? fmt(playSec(g)) : "—"}</b></div><div><small>Последний запуск</small><b>${ago(lastTs(g))}</b></div><div><small>Запусков через GameHub</small><b>${st?.n || 0}</b></div><div><small>Лаунчер</small><b>${esc(g.store)}</b></div></div>
    <div class="d-body"><div id="d-main"><p class="empty">${aid ? "Загружаю информацию…" : "Для этой игры не нашлось страницы в Steam, поэтому описания и видео нет."}</p></div><aside class="d-side" id="d-side"></aside></div>`;
  $("#back").onclick = closeDetail; $("#d-play").onclick = () => launch(g);
  $("#d-fav").onclick = () => { toggle("fav", g); $("#d-fav").textContent = S.fav.includes(g.id) ? "★ В избранном" : "☆ В избранное"; };
  if (!aid) return;
  try {
    const i = await getInfo(aid); if (!$("#d-main")) return;
    const items = [...i.movies.map(m => ({ v: m.url, p: m.thumb })), ...i.shots.map(s => ({ p: s }))];
    $("#d-main").innerHTML = (items.length ? `<div id="stage"></div><div class="thumbs">${items.map((m, n) => `<button data-n="${n}"><img loading="lazy" src="${esc(m.p)}">${m.v ? "<i>▶</i>" : ""}</button>`).join("")}</div>` : "") + `<p class="desc">${esc(plain(i.desc))}</p>`;
    const show = n => { const m = items[n]; $("#stage").innerHTML = m.v ? `<video controls preload="metadata" poster="${esc(m.p)}" src="${esc(m.v)}"></video>` : `<img src="${esc(m.p)}">`; };
    if (items.length) { show(0); $("#d-main .thumbs").onclick = e => { const b = e.target.closest("[data-n]"); if (b) show(+b.dataset.n); }; }
    const row = (k, v) => v ? `<div><small>${k}</small><span>${esc(String(v))}</span></div>` : "";
    $("#d-side").innerHTML = `<img class="cov" src="${cover(g)}" onerror="this.remove()">` + row("Жанры", i.genres.join(", ")) + row("Разработчик", i.devs.join(", ")) + row("Дата выхода", i.date)
      + row("Отзывы в Steam", i.review && `${RV[i.review] || i.review} (${i.positive}% из ${i.reviews.toLocaleString("ru")})`) + row("Metacritic", i.metacritic) + `<button id="d-store">Страница в Steam</button>`;
    $("#d-store").onclick = () => invoke("open_url", { url: "https://store.steampowered.com/app/" + aid });
  } catch (e) { if ($("#d-main")) $("#d-main").innerHTML = `<p class="empty">Не удалось загрузить информацию: ${esc(String(e))}</p>`; }
}

applyUI();
scan();

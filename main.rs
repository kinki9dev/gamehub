#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde::Serialize;
use serde_json::Value;
use std::{collections::{HashMap, HashSet}, env, fs, path::{Path, PathBuf}, process::Command, time::Duration};
use winreg::{enums::*, RegKey};

#[derive(Serialize, Clone)]
struct Game { id: String, name: String, store: String, launch: Vec<String>, folder: String, appid: Option<String>, watch: Vec<String>, playtime: Option<u64>, last_played: Option<u64> }

fn mk(store: &str, name: &str, launch: Vec<String>, folder: &str, appid: Option<String>) -> Game {
    Game { id: format!("{store}:{name}"), name: name.into(), store: store.into(), launch, folder: folder.into(), appid, watch: if folder.is_empty() { vec![] } else { vec![folder.to_lowercase()] }, playtime: None, last_played: None }
}
fn pd() -> PathBuf { PathBuf::from(env::var("ProgramData").unwrap_or("C:\\ProgramData".into())) }
fn vdf(line: &str, key: &str) -> Option<String> {
    let v: Vec<&str> = line.split('"').collect();
    (v.len() >= 4 && v[1].eq_ignore_ascii_case(key)).then(|| v[3].to_string())
}
fn size(p: &Path) -> u64 { fs::metadata(p).map(|m| m.len()).unwrap_or(0) }

fn find_exe(dir: &Path, depth: u32) -> Option<PathBuf> {
    let mut best: Option<PathBuf> = None;
    let mut take = |p: PathBuf| if best.as_ref().map_or(true, |b| size(&p) > size(b)) { best = Some(p) };
    for e in fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        let n = e.file_name().to_string_lossy().to_lowercase();
        if p.is_dir() {
            if depth > 0 { if let Some(x) = find_exe(&p, depth - 1) { take(x) } }
        } else if n.ends_with(".exe") && !["unins", "crash", "setup", "redist", "helper", "update", "launcher"].iter().any(|b| n.contains(b)) {
            take(p)
        }
    }
    best
}

fn scan_steam(o: &mut Vec<Game>) {
    let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam") else { return };
    let Ok(root) = k.get_value::<String, _>("SteamPath") else { return };
    let mut libs = vec![root.clone()];
    if let Ok(t) = fs::read_to_string(Path::new(&root).join("steamapps/libraryfolders.vdf")) {
        for l in t.lines() { if let Some(p) = vdf(l, "path") { libs.push(p.replace("\\\\", "\\")) } }
    }
    let mut seen = HashSet::new();
    let pt = steam_playtimes(&root);
    for lib in libs.into_iter().filter(|l| seen.insert(l.to_lowercase())) {
        for e in fs::read_dir(Path::new(&lib).join("steamapps")).into_iter().flatten().flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if !(n.starts_with("appmanifest_") && n.ends_with(".acf")) { continue }
            let Ok(t) = fs::read_to_string(e.path()) else { continue };
            let f = |k: &str| t.lines().find_map(|l| vdf(l, k));
            let (Some(id), Some(name), Some(dir)) = (f("appid"), f("name"), f("installdir")) else { continue };
            if ["Steamworks", "Proton", "Steam Linux Runtime", "Redistributable"].iter().any(|b| name.contains(b)) { continue }
            let folder = Path::new(&lib).join("steamapps/common").join(dir).to_string_lossy().to_string();
            let mut g = mk("Steam", &name, vec![format!("steam://rungameid/{id}")], &folder, Some(id.clone()));
            if let Some(&(m, l)) = pt.get(&id) { g.playtime = Some(m); g.last_played = Some(l); }
            o.push(g);
        }
    }
}

fn scan_epic(o: &mut Vec<Game>) {
    let dir = pd().join("Epic/EpicGamesLauncher/Data/Manifests");
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().extension().map_or(true, |x| x != "item") { continue }
        let Ok(t) = fs::read_to_string(e.path()) else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) else { continue };
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        let app = s("AppName");
        if let Some(m) = v["MainGameAppName"].as_str() { if m != app { continue } }
        if s("DisplayName").is_empty() { continue }
        let uri = format!("com.epicgames.launcher://apps/{}%3A{}%3A{}?action=launch&silent=true", s("CatalogNamespace"), s("CatalogItemId"), app);
        o.push(mk("Epic", &s("DisplayName"), vec![uri], &s("InstallLocation"), None));
    }
}

fn scan_gog(o: &mut Vec<Game>) {
    let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SOFTWARE\\WOW6432Node\\GOG.com\\Games") else { return };
    for n in k.enum_keys().flatten() {
        let Ok(s) = k.open_subkey(&n) else { continue };
        let g = |x: &str| s.get_value::<String, _>(x).unwrap_or_default();
        if !g("gameName").is_empty() && !g("exe").is_empty() { o.push(mk("GOG", &g("gameName"), vec![g("exe")], &g("path"), None)) }
    }
}

fn scan_ubisoft(o: &mut Vec<Game>) {
    let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SOFTWARE\\WOW6432Node\\Ubisoft\\Launcher\\Installs") else { return };
    for id in k.enum_keys().flatten() {
        let Ok(s) = k.open_subkey(&id) else { continue };
        let d: String = s.get_value("InstallDir").unwrap_or_default();
        let name = d.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("").to_string();
        if !name.is_empty() { o.push(mk("Ubisoft", &name, vec![format!("uplay://launch/{id}/0")], &d, None)) }
    }
}

fn scan_riot(o: &mut Vec<Game>) {
    let base = pd().join("Riot Games");
    let Ok(t) = fs::read_to_string(base.join("RiotClientInstalls.json")) else { return };
    let Some(rc) = serde_json::from_str::<serde_json::Value>(&t).ok().and_then(|v| v["rc_default"].as_str().map(String::from)) else { return };
    for e in fs::read_dir(base.join("Metadata")).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        let Some((k, patch)) = n.split_once('.') else { continue };
        let title = match k { "valorant" => "VALORANT", "league_of_legends" => "League of Legends", "bacon" => "Legends of Runeterra", _ => continue };
        let mut g = mk("Riot", title, vec![rc.clone(), format!("--launch-product={k}"), format!("--launch-patchline={patch}")], "", None);
        g.watch = vec![match k { "valorant" => "valorant-win64-shipping.exe", "league_of_legends" => "league of legends.exe", _ => "lor.exe" }.into()];
        o.push(g);
    }
}

fn scan_xbox(o: &mut Vec<Game>) {
    for d in 'C'..='H' {
        for e in fs::read_dir(format!("{d}:\\XboxGames")).into_iter().flatten().flatten() {
            let Ok(t) = fs::read_to_string(e.path().join("Content/appxmanifest.xml")) else { continue };
            let Some(i) = t.find("Executable=\"") else { continue };
            let exe = t[i + 12..].split('"').next().unwrap_or("");
            let p = e.path().join("Content").join(exe);
            if p.exists() { o.push(mk("Xbox", &e.file_name().to_string_lossy(), vec![p.to_string_lossy().into()], &e.path().to_string_lossy(), None)) }
        }
    }
}

fn scan_hoyo(o: &mut Vec<Game>) {
    for pf in [env::var("ProgramFiles"), env::var("ProgramFiles(x86)")].into_iter().flatten() {
        for e in fs::read_dir(Path::new(&pf).join("HoYoPlay/games")).into_iter().flatten().flatten() {
            if let Some(x) = find_exe(&e.path(), 2) {
                o.push(mk("HoYoPlay", &e.file_name().to_string_lossy().replace(" game", ""), vec![x.to_string_lossy().into()], &e.path().to_string_lossy(), None))
            }
        }
    }
}

fn scan_uninstall(o: &mut Vec<Game>) {
    for (h, f) in [(HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY), (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY), (HKEY_CURRENT_USER, 0)] {
        let Ok(base) = RegKey::predef(h).open_subkey_with_flags("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall", KEY_READ | f) else { continue };
        for n in base.enum_keys().flatten() {
            let Ok(k) = base.open_subkey(&n) else { continue };
            let g = |x: &str| k.get_value::<String, _>(x).unwrap_or_default();
            let (name, publ, loc) = (g("DisplayName"), g("Publisher").to_lowercase(), g("InstallLocation"));
            let store = if publ.contains("electronic arts") { "EA" } else if publ.contains("blizzard") { "Battle.net" }
                else if publ.contains("cognosphere") || publ.contains("hoyoverse") { "HoYoPlay" } else { continue };
            let ln = name.to_lowercase();
            if name.is_empty() || ["launcher", "client", "battle.net", "origin", "redistributable", "runtime", "ea app", "ea desktop"].iter().any(|b| ln.contains(b)) { continue }
            if let Some(x) = find_exe(Path::new(&loc), 2) { o.push(mk(store, &name, vec![x.to_string_lossy().into()], &loc, None)) }
        }
    }
}

#[derive(Serialize)] struct Movie { thumb: String, url: String }
#[derive(Serialize)] struct Info { name: String, desc: String, genres: Vec<String>, devs: Vec<String>, date: String, metacritic: Option<u64>, shots: Vec<String>, movies: Vec<Movie>, review: String, positive: Option<u64>, reviews: u64 }

fn steam_playtimes(root: &str) -> HashMap<String, (u64, u64)> {
    let mut map = HashMap::new();
    for u in fs::read_dir(Path::new(root).join("userdata")).into_iter().flatten().flatten() {
        let Ok(t) = fs::read_to_string(u.path().join("config/localconfig.vdf")) else { continue };
        let (mut stack, mut pending): (Vec<String>, String) = (vec![], String::new());
        for l in t.lines() {
            let s = l.trim();
            if s == "{" { stack.push(std::mem::take(&mut pending)); }
            else if s == "}" { stack.pop(); }
            else {
                let v: Vec<&str> = s.split('"').collect();
                if v.len() == 3 { pending = v[1].to_string(); }
                else if v.len() >= 5 && stack.len() >= 2 && stack[stack.len() - 2] == "apps" {
                    let e = map.entry(stack[stack.len() - 1].clone()).or_insert((0u64, 0u64));
                    match v[1] { "Playtime" => e.0 = e.0.max(v[3].parse().unwrap_or(0)), "LastPlayed" => e.1 = e.1.max(v[3].parse().unwrap_or(0)), _ => {} }
                }
            }
        }
    }
    map
}

async fn get_json(url: &str, q: &[(&str, &str)]) -> Result<Value, String> {
    reqwest::Client::builder().user_agent("GameHub/0.1").timeout(Duration::from_secs(12)).build().map_err(|e| e.to_string())?
        .get(url).query(q).send().await.map_err(|e| e.to_string())?
        .json::<Value>().await.map_err(|e| e.to_string())
}
fn strs(v: &Value, k: &str) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|i| i[k].as_str().or(i.as_str()).map(String::from)).collect()).unwrap_or_default()
}

/// Какие из отслеживаемых игр сейчас запущены (по папке установки или имени процесса).
#[tauri::command]
async fn running(watch: Vec<(String, Vec<String>)>) -> Vec<String> {
    let mut sys = sysinfo::System::new();
    sys.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, sysinfo::ProcessRefreshKind::everything());
    let procs: Vec<(String, String)> = sys.processes().values()
        .map(|p| (p.name().to_string_lossy().to_lowercase(), p.exe().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default())).collect();
    watch.into_iter()
        .filter(|(_, w)| w.iter().any(|x| procs.iter().any(|(n, e)| if x.contains(['\\', '/']) { e.starts_with(x.as_str()) } else { n == x })))
        .map(|(id, _)| id).collect()
}

/// Ищет игру в магазине Steam по точному названию — так находятся обложки и описания для игр из любых лаунчеров.
#[tauri::command]
async fn resolve_appid(name: String) -> Result<Option<String>, String> {
    let v = get_json("https://store.steampowered.com/api/storesearch/", &[("term", &name), ("l", "english"), ("cc", "us")]).await?;
    let norm = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let n = norm(&name);
    Ok(v["items"].as_array()
        .and_then(|a| a.iter().find(|i| i["type"] == "app" && norm(i["name"].as_str().unwrap_or("")) == n))
        .and_then(|i| i["id"].as_u64()).map(|x| x.to_string()))
}

#[tauri::command]
async fn game_info(appid: String) -> Result<Info, String> {
    let v = get_json("https://store.steampowered.com/api/appdetails", &[("appids", &appid), ("l", "russian"), ("cc", "ru")]).await?;
    let d = &v[appid.as_str()]["data"];
    if d.is_null() { return Err("в Steam нет данных об этой игре".into()) }
    let https = |u: &str| u.replacen("http://", "https://", 1);
    let movies: Vec<Movie> = d["movies"].as_array().map(|a| a.iter().filter_map(|m| {
        let url = ["mp4", "webm"].iter().find_map(|f| m[*f]["max"].as_str().or(m[*f]["480"].as_str()))?;
        Some(Movie { thumb: https(m["thumbnail"].as_str().unwrap_or("")), url: https(url) })
    }).collect()).unwrap_or_default();
    let shots: Vec<String> = d["screenshots"].as_array().map(|a| a.iter().filter_map(|s| s["path_full"].as_str().map(|p| https(p))).take(12).collect()).unwrap_or_default();
    let (mut review, mut positive, mut reviews) = (String::new(), None, 0u64);
    if let Ok(r) = get_json(&format!("https://store.steampowered.com/appreviews/{appid}"), &[("json", "1"), ("num_per_page", "0"), ("language", "all"), ("purchase_type", "all")]).await {
        let q = &r["query_summary"];
        let tot = q["total_reviews"].as_u64().unwrap_or(0);
        if tot > 0 { review = q["review_score_desc"].as_str().unwrap_or("").into(); reviews = tot; positive = Some(q["total_positive"].as_u64().unwrap_or(0) * 100 / tot); }
    }
    Ok(Info {
        name: d["name"].as_str().unwrap_or("").into(), desc: d["short_description"].as_str().unwrap_or("").into(),
        genres: strs(&d["genres"], "description"), devs: strs(&d["developers"], "x"), date: d["release_date"]["date"].as_str().unwrap_or("").into(),
        metacritic: d["metacritic"]["score"].as_u64(), shots, movies, review, positive, reviews,
    })
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://store.steampowered.com/") { return Err("адрес не разрешён".into()) }
    Command::new("explorer.exe").arg(url).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn scan_games() -> Vec<Game> {
    let mut o = vec![];
    for f in [scan_steam, scan_epic, scan_gog, scan_ubisoft, scan_riot, scan_xbox, scan_hoyo, scan_uninstall] { f(&mut o) }
    let mut seen = HashSet::new();
    o.retain(|g| seen.insert(g.id.to_lowercase()));
    o.sort_by_key(|g| g.name.to_lowercase());
    o
}

#[tauri::command]
fn launch_game(launch: Vec<String>) -> Result<(), String> {
    let first = launch.first().ok_or("Пустая команда запуска")?;
    let uri = launch.len() == 1 && first.contains("://");
    let r = if uri { Command::new("explorer.exe").arg(first).spawn() } else {
        let mut c = Command::new(first);
        c.args(&launch[1..]);
        if let Some(d) = Path::new(first).parent() { c.current_dir(d); }
        c.spawn()
    };
    match r {
        Ok(_) => Ok(()),
        // например, игре нужны права администратора — отдаём запуск системе
        Err(_) if !uri => Command::new("explorer.exe").arg(first).spawn().map(|_| ()).map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
fn open_folder(path: String) { let _ = Command::new("explorer.exe").arg(path).spawn(); }

#[tauri::command]
async fn pick_exe() -> Option<Game> {
    let p = rfd::FileDialog::new().add_filter("Программы", &["exe"]).pick_file()?;
    let name = p.file_stem()?.to_string_lossy().to_string();
    Some(mk("Другое", &name, vec![p.to_string_lossy().into()], &p.parent()?.to_string_lossy(), None))
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![scan_games, launch_game, open_folder, pick_exe, running, resolve_appid, game_info, open_url])
        .run(tauri::generate_context!())
        .expect("не удалось запустить GameHub");
}

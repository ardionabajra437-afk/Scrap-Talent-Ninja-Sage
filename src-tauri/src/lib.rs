use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Participant {
    id: u64,
    name: String,
    trophy: f64,
    squad: String,
    rank: u64,
    #[serde(rename = "trophy_per_hour")]
    trophy_per_hour: f64,
    #[serde(rename = "trophy_per_day")]
    trophy_per_day: f64,
    #[serde(rename = "avg_per_hour")]
    avg_per_hour: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StreamData {
    participants: Vec<Participant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Player {
    rank: u32,
    name: String,
    id: String,
    trophy: String,
    thr: String,
    avg_hr: String,
    avg_day: String,
    #[serde(default)]
    talent1: String,
    #[serde(default)]
    talent2: String,
    #[serde(default)]
    talent3: String,
    #[serde(default)]
    char_class: String,
    #[serde(default)]
    senjutsu: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SquadPlayers {
    name: String,
    buff: String,
    debuff: String,
    players: Vec<Player>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScrapeResult {
    squads: Vec<SquadPlayers>,
    total_players: u32,
    output_path: String,
}

struct AppState {
    progress: Mutex<String>,
}

const SQUAD_BUFFS: [(&str, &str, &str); 5] = [
    ("Assault", "Rampage +20% Damage", "Isolate -20% Crit Chance"),
    ("Ambush", "Wider +20% Max CP", "Clarify -15% Purify Chance"),
    ("Medic", "Wellness +5% HP Recovery", "Botched -20% Max CP"),
    ("Kage", "Reflect +20% Reactive Force", "Holdback -20% Damage"),
    ("HQ", "Evade +20% Dodge", "Unwell -20% Max HP"),
];

#[tauri::command]
async fn get_progress(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.progress.lock().unwrap().clone())
}

#[tauri::command]
async fn scrape_sw(state: State<'_, AppState>) -> Result<ScrapeResult, String> {
    let client = reqwest::Client::new();

    // Step 1: Connect to SSE stream to get all players
    *state.progress.lock().unwrap() = "Connecting to leaderboard stream...".to_string();

    // SSE streams stay open forever — use bytes_stream + take first chunk
    let resp = client
        .get("https://ninjajolay.id/api/sw/stream?stream=sw")
        .header("Accept", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .send()
        .await
        .map_err(|e| format!("SSE connect failed: {}", e))?;

    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();
    let mut participants = Vec::new();

    // Read chunks until we get the first "data: {...}" line
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| e.to_string())?;
        buffer.push_str(&String::from_utf8_lossy(&bytes));

        // Check if we have a complete SSE data line
        for line in buffer.lines() {
            if let Some(json_str) = line.strip_prefix("data: ") {
                if let Ok(stream_data) = serde_json::from_str::<StreamData>(json_str) {
                    participants = stream_data.participants;
                    break;
                }
            }
        }
        if !participants.is_empty() {
            break;
        }
    }

    if participants.is_empty() {
        return Err("No players found in SSE stream".to_string());
    }

    *state.progress.lock().unwrap() =
        format!("Got {} players. Fetching talents...", participants.len());

    // Step 2: Group by squad, take top 50 each
    let mut by_squad: HashMap<String, Vec<&Participant>> = HashMap::new();
    for p in &participants {
        by_squad.entry(p.squad.clone()).or_default().push(p);
    }

    // Sort each squad by rank
    for squad_players in by_squad.values_mut() {
        squad_players.sort_by_key(|p| p.rank);
    }

    let base = "https://ninjajolay.id/api/sw";
    let mut all_squads = Vec::new();
    let mut total_players: u32 = 0;

    for (squad_name, buff, debuff) in SQUAD_BUFFS {
        let squad_list = by_squad.get(squad_name).cloned().unwrap_or_default();
        let take = squad_list.len().min(50);
        let mut squad_players = Vec::new();

        for (i, p) in squad_list.iter().take(50).enumerate() {
            *state.progress.lock().unwrap() =
                format!("{}: talent {}/{}", squad_name, i + 1, take);

            let char_url = format!("{}/character_info/{}", base, p.id);
            let mut char_data: Option<serde_json::Value> = None;
            for attempt in 0..5 {
                if attempt > 0 {
                    let delay_ms = if attempt < 3 { 1500 * attempt as u64 } else { 3000 * attempt as u64 };
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                }
                match client.get(&char_url).send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        if status.is_server_error() {
                            continue;
                        }
                        match resp.json::<serde_json::Value>().await {
                            Ok(v) => {
                                let ch = &v["character"];
                                let has_talent = ch.get("talent1").and_then(|v| v.as_str()).map_or(false, |s| !s.is_empty() && s != "None")
                                    || ch.get("talent2").and_then(|v| v.as_str()).map_or(false, |s| !s.is_empty() && s != "None")
                                    || ch.get("talent3").and_then(|v| v.as_str()).map_or(false, |s| !s.is_empty() && s != "None");
                                if has_talent || attempt >= 4 {
                                    char_data = Some(v);
                                    break;
                                }
                                // talent empty but retries left — retry
                            }
                            Err(_) => continue,
                        }
                    }
                    Err(_) => continue,
                }
            }
            let char_data = char_data.unwrap_or_else(|| serde_json::json!({"character": {}}));
            let ch = &char_data["character"];
            let clean = |v: &serde_json::Value| -> String {
                v.as_str().filter(|s| *s != "None" && !s.is_empty()).unwrap_or("").to_string()
            };
            squad_players.push(Player {
                rank: (i + 1) as u32,
                name: p.name.clone(),
                id: p.id.to_string(),
                trophy: format!("{}", p.trophy as i64),
                thr: format!("{}", p.trophy_per_hour as i64),
                avg_hr: format!("{}", p.avg_per_hour as i64),
                avg_day: format!("{}", p.trophy_per_day as i64),
                talent1: clean(&ch["talent1"]),
                talent2: clean(&ch["talent2"]),
                talent3: clean(&ch["talent3"]),
                char_class: clean(&ch["char_class"]),
                senjutsu: clean(&ch["senjutsu"]),
            });

            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        }

        total_players += squad_players.len() as u32;

        all_squads.push(SquadPlayers {
            name: squad_name.to_string(),
            buff: buff.to_string(),
            debuff: debuff.to_string(),
            players: squad_players,
        });
    }

    *state.progress.lock().unwrap() = "Saving data...".to_string();

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    let json_path = format!("{}/sw_data.json", home);
    let json_str = serde_json::to_string_pretty(&all_squads).map_err(|e| e.to_string())?;
    std::fs::write(&json_path, &json_str).map_err(|e| e.to_string())?;

    *state.progress.lock().unwrap() = "Generating XLSX...".to_string();
    let xlsx_path = generate_xlsx(&all_squads)?;

    *state.progress.lock().unwrap() = format!("Done! Saved to {}", xlsx_path);

    Ok(ScrapeResult {
        squads: all_squads,
        total_players,
        output_path: xlsx_path,
    })
}

fn generate_xlsx(squads: &[SquadPlayers]) -> Result<String, String> {
    use rust_xlsxwriter::*;
    use std::collections::HashMap;

    let mut wb = Workbook::new();

    let squad_colors: HashMap<&str, u32> = HashMap::from([
        ("Assault", 0xFEE2E2), ("Ambush", 0xDBEAFE), ("Medic", 0xD1FAE5),
        ("Kage", 0xEDE9FE),    ("HQ", 0xFEF3C7),
    ]);

    let title_fmt = Format::new().set_bold().set_font_size(14).set_font_name("Calibri")
        .set_font_color(Color::RGB(0x1F2937));
    let subtitle_fmt = Format::new().set_italic().set_font_size(10).set_font_name("Calibri")
        .set_font_color(Color::RGB(0x6B7280));
    let header_fmt = Format::new().set_bold().set_font_size(11).set_font_name("Calibri")
        .set_font_color(Color::RGB(0xFFFFFF))
        .set_background_color(Color::RGB(0x1F2937))
        .set_align(FormatAlign::Center);
    let bold_fmt = Format::new().set_bold().set_font_size(10).set_font_name("Calibri")
        .set_font_color(Color::RGB(0x374151));
    let border_fmt = Format::new().set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xD1D5DB));

    for squad in squads {
        let ws = wb.add_worksheet();
        ws.set_name(&squad.name).map_err(|e| e.to_string())?;
        let n = squad.players.len() as u32;
        let color = squad_colors.get(squad.name.as_str()).copied().unwrap_or(0xFFFFFF);
        let border_fill = border_fmt.clone().set_background_color(Color::RGB(color));

        // Title
        ws.merge_range(0, 0, 0, 5,
            &format!("SHADOW WAR — {} ({} players)", squad.name.to_uppercase(), n),
            &title_fmt).map_err(|e| e.to_string())?;
        ws.set_row_height(0, 30).map_err(|e| e.to_string())?;

        // Subtitle
        ws.merge_range(1, 0, 1, 5,
            &format!("Buff: {}  |  Debuff: {}", squad.buff, squad.debuff),
            &subtitle_fmt).map_err(|e| e.to_string())?;

        // Player List label
        ws.write_string_with_format(3, 0, "Player List", &bold_fmt).map_err(|e| e.to_string())?;

        // Column headers
        let headers = ["Rank", "Player", "ID", "Talent 1", "Talent 2", "Talent 3"];
        for (ci, h) in headers.iter().enumerate() {
            ws.write_string_with_format(4, ci as u16, *h, &header_fmt).map_err(|e| e.to_string())?;
        }

        // Talent counters
        let mut all_c: HashMap<String, u32> = HashMap::new();
        let mut t1c: HashMap<String, u32> = HashMap::new();
        let mut t2c: HashMap<String, u32> = HashMap::new();
        let mut t3c: HashMap<String, u32> = HashMap::new();

        // Player rows
        let mut row = 5u32;
        for p in &squad.players {
            let is_even = p.rank % 2 == 0;
            let rf = if is_even { &border_fill } else { &border_fmt };

            ws.write_number_with_format(row, 0, p.rank as f64, &rf).map_err(|e| e.to_string())?;
            ws.write_string_with_format(row, 1, &p.name, &rf).map_err(|e| e.to_string())?;
            ws.write_string_with_format(row, 2, &p.id, &rf).map_err(|e| e.to_string())?;
            ws.write_string_with_format(row, 3, &p.talent1, &rf).map_err(|e| e.to_string())?;
            ws.write_string_with_format(row, 4, &p.talent2, &rf).map_err(|e| e.to_string())?;
            ws.write_string_with_format(row, 5, &p.talent3, &rf).map_err(|e| e.to_string())?;

            for (talent, counter) in [(&p.talent1, &mut t1c), (&p.talent2, &mut t2c), (&p.talent3, &mut t3c)] {
                if !talent.is_empty() {
                    *counter.entry(talent.clone()).or_insert(0) += 1;
                    *all_c.entry(talent.clone()).or_insert(0) += 1;
                }
            }
            row += 1;
        }

        row += 1;
        ws.write_string_with_format(row, 0, "Talent Count", &bold_fmt).map_err(|e| e.to_string())?;
        row += 1;

        let sections: Vec<(&str, &HashMap<String, u32>, u32)> = vec![
            ("All (T1+T2+T3)", &all_c, n * 3),
            ("Talent 1", &t1c, n),
            ("Talent 2", &t2c, n),
            ("Talent 3", &t3c, n),
        ];

        for (label, counter, denom) in &sections {
            ws.write_string_with_format(row, 0, *label, &bold_fmt).map_err(|e| e.to_string())?;
            row += 1;
            for (ci, h) in ["Talent", "Count", "%"].iter().enumerate() {
                ws.write_string_with_format(row, ci as u16, *h, &header_fmt).map_err(|e| e.to_string())?;
            }
            row += 1;

            let mut sorted: Vec<_> = counter.iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(a.1));

            for (talent, count) in &sorted {
                let pct = if *denom > 0 { **count as f64 / *denom as f64 * 100.0 } else { 0.0 };
                ws.write_string_with_format(row, 0, talent.as_str(), &border_fmt).map_err(|e| e.to_string())?;
                ws.write_number_with_format(row, 1, **count as f64, &border_fmt).map_err(|e| e.to_string())?;
                ws.write_string_with_format(row, 2, &format!("{:.1}%", pct), &border_fmt).map_err(|e| e.to_string())?;
                row += 1;
            }
            row += 1;
        }

        ws.set_column_width(0, 28.0).map_err(|e| e.to_string())?;
        ws.set_column_width(1, 22.0).map_err(|e| e.to_string())?;
        ws.set_column_width(2, 10.0).map_err(|e| e.to_string())?;
        ws.set_column_width(3, 24.0).map_err(|e| e.to_string())?;
        ws.set_column_width(4, 24.0).map_err(|e| e.to_string())?;
        ws.set_column_width(5, 24.0).map_err(|e| e.to_string())?;
        ws.set_freeze_panes(5, 0).map_err(|e| e.to_string())?;
    }

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    let outpath = format!("{}/SW_Talent.xlsx", home);
    wb.save(&outpath).map_err(|e| e.to_string())?;
    Ok(outpath)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            progress: Mutex::new(String::new()),
        })
        .invoke_handler(tauri::generate_handler![scrape_sw, get_progress])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

# Shadow War Talent Scraper

Desktop app untuk scrape data talent player Shadow War Season 26 dari [ninjajolay.id](https://ninjajolay.id/sw).

## Fitur

- Scrape leaderboard top 50 player per squad (Assault, Ambush, Medic, Kage, HQ)
- Fetch talent, class, senjutsu untuk setiap player
- Generate XLSX dengan talent count & persentase per squad
- Auto-retry + backoff handle server error (502/503)
- Strip `"None"` string dari API response

## Download

Download `.exe` atau `.msi` dari [Releases](https://github.com/ardionabajra437-afk/Scrap-Talent-Ninja-Sage/releases/tag/v0.1.0).

## Output

| File | Isi |
|------|-----|
| `~/sw_data.json` | Raw data JSON per squad |
| `~/SW_Talent_S26.xlsx` | XLSX — player list + talent count stats |

## Tech Stack

- **Backend:** Rust + Tauri 2
- **Frontend:** Vanilla HTML/JS
- **XLSX:** Python (openpyxl)

## Development

```bash
# Install dependencies
npm install

# Run dev
npm run tauri dev

# Build release
npm run tauri build
```

### Requirements

- [Node.js](https://nodejs.org/) 20+
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- Python 3 + `openpyxl` (untuk generate XLSX)

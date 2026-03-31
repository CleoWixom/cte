# 📡 Cell Triangulation Evaluator

Web application that evaluates cell tower triangulation accuracy at any clicked map point.

## Architecture

```
trieval/
├── crates/
│   ├── core/       # Math: IDW, Kriging, LM, Monte Carlo, metrics
│   ├── wasm/       # wasm-bindgen wrapper → runs in browser
│   └── server/     # Axum REST API + WebSocket
├── web/            # Svelte 4 + Leaflet frontend
└── migrations/     # PostGIS schema
```

## Quick Start

```bash
# 1. Start DB + Redis
docker compose up -d

# 2. Run server
cp .env.example .env
# Edit .env: add OPENCELLID_KEY
cargo run -p trieval-server

# 3. Build WASM
rustup target add wasm32-unknown-unknown
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
wasm-pack build crates/wasm --target web --out-dir ../../web/src/wasm

# 4. Start frontend
cd web && npm install && npm run dev
```

Open http://localhost:5173 — click the map to run triangulation.

## Features

- **IDW & Ordinary Kriging** RSSI spatial models with automatic variogram fitting
- **Levenberg-Marquardt** nonlinear least squares solver
- **Monte Carlo** uncertainty estimation (300 iterations, 8 dBm noise)
- **CEP50 / CEP95** circular error probable metrics
- **GDOP** geometric dilution of precision
- **PCA error ellipse** — 95% confidence region
- **OpenCelliD API** live tower fetching with Redis cache
- **CSV import** — streaming ingest of OpenCelliD measurement dumps (~900 MB)
- **Android WebSocket** — real-time measurements from Termux or Kotlin app

## Data Sources

| Source | How |
|--------|-----|
| OpenCelliD API | Auto-fetched on map click (requires API key) |
| OCI CSV dump | `POST /api/upload` — streaming, batched insert |
| Android/Termux | `WS /api/ws/android` |

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET`  | `/api/cells?lat=&lon=&radius_m=` | Towers near point |
| `GET`  | `/api/measurements?lat=&lon=&radius_m=` | RSSI measurements |
| `POST` | `/api/upload` | Import CSV (multipart) |
| `WS`   | `/api/ws/android` | Real-time Android data |

## Release Process

Versioning is **fully automated** via Conventional Commits:

| Commit prefix | Version bump |
|---------------|-------------|
| `feat!:` / `BREAKING CHANGE` | **major** (1.0.0 → 2.0.0) |
| `feat:` | **minor** (0.1.0 → 0.2.0) |
| `fix:`, `perf:`, `chore:`, etc. | **patch** (0.1.0 → 0.1.1) |

Every push to `main` automatically:
1. Detects bump type from commit messages
2. Updates `Cargo.toml` version
3. Builds server binary + WASM + frontend
4. Creates a tagged GitHub Release with archives

## Environment Variables

```env
DATABASE_URL=postgres://trieval:trieval@localhost:5432/trieval
REDIS_URL=redis://127.0.0.1:6379
OPENCELLID_KEY=your_key_here
LISTEN_ADDR=0.0.0.0:8080
RUST_LOG=info
```

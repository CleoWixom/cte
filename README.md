# 📡 Cell Triangulation Evaluator

Web app that evaluates the accuracy of cell tower triangulation at any map point.
Click the map → the engine fetches nearby towers, builds RSSI spatial models,
runs Levenberg-Marquardt optimisation, and estimates position uncertainty via Monte Carlo.

[![CI](https://github.com/CleoWixom/cte/actions/workflows/ci.yml/badge.svg)](https://github.com/CleoWixom/cte/actions/workflows/ci.yml)
[![Release](https://github.com/CleoWixom/cte/actions/workflows/release.yml/badge.svg)](https://github.com/CleoWixom/cte/releases/latest)

---

## Architecture

```
trieval/
├── crates/
│   ├── core/       # Math: IDW, Ordinary Kriging, LM solver, Monte Carlo, CEP/GDOP/PCA
│   ├── wasm/       # wasm-bindgen wrapper — runs in browser, no server round-trip
│   └── server/     # Axum REST API + WebSocket for Android data
├── web/            # Svelte 4 + Leaflet dark-theme frontend
├── android/        # Termux Python collector + Kotlin snippet
└── migrations/     # PostGIS schema (cell_towers + measurements)
```

## Quick Start

```bash
# 1. Start Postgres + Redis
docker compose up -d

# 2. Configure
cp .env.example .env
# edit .env — add OPENCELLID_KEY (free at opencellid.org)

# 3. Run backend
cargo run -p trieval-server

# 4. Build WASM (one-time)
rustup target add wasm32-unknown-unknown
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
wasm-pack build crates/wasm --target web --out-dir ../../web/public/wasm

# 5. Start frontend
cd web && npm install && npm run dev
# → http://localhost:5173
```

## Features

| Feature | Details |
|---------|---------|
| **IDW** | Inverse distance weighting, β=2, per-tower spatial model |
| **Ordinary Kriging** | Spherical variogram, auto-fit nugget/sill/range, fallback to IDW if <5 measurements |
| **Levenberg-Marquardt** | Weighted non-linear least squares, analytical Jacobian, max 100 iter |
| **Monte Carlo** | 300 iterations, 8 dBm Gaussian noise, LCG RNG (no_std) |
| **CEP50 / CEP95** | Circular error probable from MC cloud |
| **GDOP** | Geometric dilution of precision from tower geometry |
| **PCA error ellipse** | 95% confidence ellipse via eigendecomposition |
| **OpenCelliD API** | Auto-fetched on click, Redis-cached 1 h |
| **CSV import** | Streaming ingest of OCI measurement dumps (sync reader, 1K-row batches) |
| **Android / Termux** | Real-time measurements via WebSocket |

## API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check |
| `GET` | `/api/cells?lat=&lon=&radius_m=` | Towers near point (DB + OCI fallback) |
| `GET` | `/api/measurements?lat=&lon=&radius_m=` | RSSI measurements |
| `POST` | `/api/upload` | Import OpenCelliD CSV (multipart: `file`, `source`) |
| `WS` | `/api/ws/android` | Real-time Android cell data stream |

## Android Data Collection

**Termux** (no APK needed):
```bash
pkg install termux-api python
pip install websocket-client
python3 android/termux_collector.py --server ws://YOUR_SERVER:8080 --gps
```

**Kotlin** — see `android/AndroidCollector.kt` for a drop-in class using
`TelephonyManager.getAllCellInfo()` and OkHttp WebSocket.

## Versioning & Releases

Every push to `main` triggers an automatic release via Conventional Commits:

| Prefix | Bump |
|--------|------|
| `feat!:` / `BREAKING CHANGE` | **major** |
| `feat:` | **minor** |
| `fix:` `perf:` `refactor:` `chore:` | **patch** |

Each release publishes:
- `trieval-server-linux-x86_64.tar.gz` — server binary + migrations
- `trieval-web.tar.gz` — pre-built static frontend

## Environment Variables

```env
DATABASE_URL=postgres://trieval:trieval@localhost:5432/trieval
REDIS_URL=redis://127.0.0.1:6379
OPENCELLID_KEY=your_key_here          # get free at opencellid.org
LISTEN_ADDR=0.0.0.0:8080
RUST_LOG=info
```

## Deploy

```
Frontend (static + WASM)  →  Cloudflare Pages
Backend  (Rust binary)    →  Fly.io / VPS / Docker
Database                  →  Supabase (PostGIS) / self-hosted
Redis                     →  Upstash free tier / co-located
```

```bash
# Docker
docker build -t trieval-server .
docker run -p 8080:8080 --env-file .env trieval-server
```

# Полный план — Cell Triangulation Evaluator

## 1. Что именно строим — чёткое определение

Веб-приложение, которое:
1. Принимает точку на карте (клик пользователя)
2. Подтягивает соты и реальные RSSI-замеры из всех доступных источников в радиусе
3. Строит пространственную RSSI-модель для каждой соты через Kriging/IDW
4. Симулирует обратную задачу триангуляции методом нелинейного МНК (LM)
5. Через Monte Carlo оценивает качество триангуляции и выдаёт метрики точности

**Ключевой инсайт про OpenCelliD:** API возвращает только агрегированные данные башен (`lat, lon, range, samples, avgSignal`). Поле `avgSignal` у большинства записей равно 0 — это известная проблема базы. Сырые измерения с координатами доступны только через CSV-дамп (~900MB gz). Это нужно учесть в архитектуре.

---

## 2. Структура Cargo workspace

```
trieval/
├── Cargo.toml                  ← workspace
├── crates/
│   ├── core/                   ← чистая математика, #![no_std] где возможно
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── signal.rs       ← нормализация RSSI по радиотехнологии
│   │       ├── spatial.rs      ← IDW + Kriging (variogram, kriging system)
│   │       ├── lm.rs           ← Левенберг-Марквардт поверх levenberg-marquardt
│   │       ├── montecarlo.rs   ← N=500 итераций с шумом
│   │       ├── metrics.rs      ← CEP50/95, GDOP, PCA эллипс
│   │       └── geo.rs          ← геодезические расчёты (haversine + vincenty)
│   │
│   ├── wasm/                   ← тонкий #[wasm_bindgen] слой над core
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   │
│   └── server/                 ← Axum бэкенд
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── state.rs        ← AppState (pool, redis, http client)
│           ├── routes/
│           │   ├── mod.rs
│           │   ├── cells.rs    ← GET /api/cells
│           │   ├── upload.rs   ← POST /api/upload (CSV/JSON)
│           │   └── android.rs  ← WS /api/ws/android
│           ├── opencellid.rs   ← клиент OCI API
│           ├── normalizer.rs   ← нормализация форматов данных
│           └── db.rs           ← sqlx запросы
│
├── web/                        ← Svelte фронтенд
│   ├── package.json
│   ├── vite.config.js
│   └── src/
│       ├── App.svelte
│       ├── lib/
│       │   ├── MapView.svelte
│       │   ├── MetricsPanel.svelte
│       │   ├── SourcesPanel.svelte
│       │   └── UploadPanel.svelte
│       └── wasm/               ← wasm-pack output (gitignored, генерируется)
│
├── migrations/                 ← sqlx миграции
│   ├── 001_init.sql
│   └── 002_measurements.sql
│
└── docker-compose.yml          ← postgres+postgis, redis
```

---

## 3. Зависимости — полный Cargo.toml по крейтам

### `crates/core/Cargo.toml`
```toml
[dependencies]
levenberg-marquardt = "0.14"   # LM специализированный крейт, не argmin
nalgebra = "0.33"               # матрицы; используется levenberg-marquardt
geo = "0.28"                    # геометрия, Point/haversine
libm = "0.2"                    # no_std совместимая математика (sin/cos/sqrt/ln)

[features]
default = []
std = []
```

**Важно:** `levenberg-marquardt` — отдельный специализированный крейт, не через `argmin`. Он нативно работает с `nalgebra` и компилируется в WASM без проблем. `argmin` имеет экспериментальную WASM поддержку — риск.

### `crates/wasm/Cargo.toml`
```toml
[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
core = { path = "../core" }
wasm-bindgen = "0.2"
serde = { version = "1", features = ["derive"] }
serde-wasm-bindgen = "0.6"    # не устаревший JsValue::from_serde
js-sys = "0.3"
```

### `crates/server/Cargo.toml`
```toml
[dependencies]
axum = { version = "0.7", features = ["ws", "multipart"] }
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.5", features = ["cors", "trace", "compression-gzip"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "migrate", "uuid", "chrono"] }
geozero = { version = "0.14", features = ["with-postgis-sqlx"] }   # PostGIS + sqlx
reqwest = { version = "0.12", features = ["json", "gzip"] }
redis = { version = "0.26", features = ["tokio-comp"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
dotenvy = "0.15"
csv-async = { version = "1", features = ["tokio"] }   # async CSV стриминг
```

---

## 4. База данных — схема

### migrations/001_init.sql
```sql
CREATE EXTENSION IF NOT EXISTS postgis;

-- Башни сот (из OCI API или CSV-дампа)
CREATE TABLE cell_towers (
    id          BIGSERIAL PRIMARY KEY,
    radio       TEXT NOT NULL,          -- GSM/UMTS/LTE/NR
    mcc         SMALLINT NOT NULL,
    mnc         SMALLINT NOT NULL,
    lac         INTEGER NOT NULL,       -- LAC или TAC (LTE)
    cid         BIGINT NOT NULL,        -- Cell ID (может быть длинным для LTE)
    lat         DOUBLE PRECISION NOT NULL,
    lon         DOUBLE PRECISION NOT NULL,
    range_m     INTEGER,                -- оценочный радиус покрытия
    samples     INTEGER DEFAULT 0,
    changeable  BOOLEAN DEFAULT TRUE,  -- false = координаты точные от оператора
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    geom        GEOMETRY(Point, 4326) GENERATED ALWAYS AS
                    (ST_SetSRID(ST_MakePoint(lon, lat), 4326)) STORED,
    UNIQUE(radio, mcc, mnc, lac, cid)
);
CREATE INDEX cell_towers_geom_idx ON cell_towers USING GIST(geom);
CREATE INDEX cell_towers_mnc_idx ON cell_towers(mcc, mnc);
```

### migrations/002_measurements.sql
```sql
-- Реальные RSSI-замеры (из CSV-дампа OCI, загруженных файлов, Android)
CREATE TABLE measurements (
    id          BIGSERIAL PRIMARY KEY,
    cell_id     BIGINT REFERENCES cell_towers(id) ON DELETE CASCADE,
    lat         DOUBLE PRECISION NOT NULL,
    lon         DOUBLE PRECISION NOT NULL,
    signal_dbm  SMALLINT,               -- нормализован в dBm (см. signal.rs)
    raw_signal  SMALLINT,               -- оригинальное значение из источника
    radio       TEXT,
    source      TEXT NOT NULL,          -- 'opencellid_csv'|'upload'|'android'
    reliability FLOAT DEFAULT 1.0,      -- [0..1] вес замера
    measured_at TIMESTAMPTZ,
    geom        GEOMETRY(Point, 4326) GENERATED ALWAYS AS
                    (ST_SetSRID(ST_MakePoint(lon, lat), 4326)) STORED
);
CREATE INDEX measurements_geom_idx ON measurements USING GIST(geom);
CREATE INDEX measurements_cell_id_idx ON measurements(cell_id);
```

**Зачем хранить сырые данные в БД, а не только дёргать API:** OpenCelliD API возвращает только башни без индивидуальных замеров. Для IDW/Kriging нужны замеры с координатами. Их единственный источник — CSV-дамп (~50M строк). Загружается один раз для нужного региона, хранится локально.

---

## 5. Бэкенд — API эндпоинты

### `GET /api/cells?lat=&lon=&radius_m=`

Возвращает башни + агрегированные данные для отрисовки на карте.

Стратегия:
1. Сначала проверяем БД (PostGIS `ST_DWithin`)
2. Если в БД мало данных (< 3 башен) → запрос к OCI API `cell/getInArea` с BBOX
3. Результат кэшируем в Redis с TTL 1 час

Ограничения OCI API: BBOX не более 4 000 000 м². При радиусе > ~1.1 км нужно делить на несколько запросов.

```
Response: {
  towers: [{id, radio, mcc, mnc, lat, lon, range_m, samples}],
  measurements_count: N,   // сколько замеров в БД для этой области
  source: "db"|"api"
}
```

### `GET /api/measurements?lat=&lon=&radius_m=&cell_id=`

Возвращает замеры для WASM-ядра. Берутся только из БД.

```
Response: {
  measurements: [{cell_id, lat, lon, signal_dbm, reliability}]
}
```

### `POST /api/upload`

Принимает multipart: файл (CSV или JSON) + `source` поле.

Поддерживаемые форматы CSV:
- **OpenCelliD measurements CSV**: `radio,mcc,mnc,lac,cellid,lon,lat,signal,...`
- **Произвольный**: автодетект колонок по заголовку

Загрузка стриминговая через `csv-async` — файлы в несколько GB не ломают память.

Логика: парсинг → нормализация RSSI в dBm (`signal.rs`) → upsert в БД батчами по 1000 строк.

### `WS /api/ws/android`

Принимает JSON-сообщения:
```json
{
  "type": "measurement",
  "cells": [
    {"radio":"LTE","mcc":250,"mnc":1,"lac":1234,"cid":567890,
     "rssi":-85,"lat":55.75,"lon":37.62}
  ]
}
```

Сохраняет в БД с `source='android'`, рассылает подтверждение.

---

## 6. WASM ядро — математика детально

### 6.1. Нормализация сигнала (`signal.rs`)

OpenCelliD хранит значения по-разному в зависимости от радио:

```
GSM:   RSSI [dBm] = (2 × ASU) − 113       диапазон: −113..−51 dBm
UMTS:  RSCP [dBm] = ASU − 116              диапазон: −121..−25 dBm
LTE:   RSRP [dBm] = ASU − 140             диапазон: −140..−44 dBm
NR:    SSRSRP в dBm напрямую
```

Нормализуем всё к единой шкале dBm перед дальнейшими расчётами. Это критично — сравнивать GSM RSSI и LTE RSRP без нормализации бессмысленно.

### 6.2. IDW (`spatial.rs`)

```
RSSI_IDW(p) = Σᵢ [wᵢ · rssiᵢ] / Σᵢ wᵢ
где wᵢ = 1 / d(p, pᵢ)^β,  β = 2 (степень затухания)
d — геодезическое расстояние (haversine)
```

Строится отдельно для каждой соты по её замерам в БД. Результат — функция `rssi(lat, lon) → dBm` для каждой башни.

### 6.3. Ordinary Kriging (`spatial.rs`)

Kriging точнее IDW: учитывает пространственную автокорреляцию замеров и даёт дисперсию предсказания (нужна для взвешенного МНК).

**Шаги:**

1. **Эмпирическая вариограмма** по замерам одной соты:
```
γ(h) = 1/(2·N(h)) · Σ [rssiᵢ - rssiⱼ]²
где N(h) = пары замеров на расстоянии ~h
```

2. **Подгонка теоретической модели** (сферическая):
```
γ_model(h) = nugget + sill·[1.5·(h/range) - 0.5·(h/range)³], h ≤ range
           = nugget + sill,                                     h > range
```
Параметры `nugget, sill, range` находятся через МНК подгонкой к эмпирической.

3. **Система Кригинга** (решается для каждой новой точки):
```
[Γ  1] [λ]   [γ(p)]
[1ᵀ 0] [μ] = [1   ]

Γᵢⱼ = γ_model(d(pᵢ, pⱼ))
γ(p)ᵢ = γ_model(d(p, pᵢ))
```
Решение: `λ = Γ⁻¹ · γ` через LU-декомпозицию (`nalgebra::LU`).

4. **Предсказание и дисперсия**:
```
rssi_kriging(p) = Σ λᵢ · rssiᵢ
σ²_kriging(p)   = γ(p)ᵀ·λ + μ    ← это и есть неопределённость
```

### 6.4. Левенберг-Марквардт (`lm.rs`)

Задача: по наблюдаемым RSSI от N сот найти (lat, lon) устройства.

**Резидуалы** (для каждой соты `i`):
```
rᵢ(x,y) = rssi_observed_i − rssi_model_i(x,y)
```
где `rssi_model_i(x,y)` — IDW или Kriging для соты `i` в точке `(x,y)`.

**Матрица весов** (диагональная):
```
Wᵢᵢ = 1 / σ²_kriging_i(x,y)   ← меньше уверенности → меньше вес
```

**Взвешенная целевая функция**:
```
F(x,y) = ½ · rᵀ W r = ½ · Σ wᵢ · rᵢ²
```

**Якобиан** (аналитически, не численно):

Если `rssi_model_i(x,y)` = IDW:
```
∂rssi_IDW/∂x = Σⱼ wⱼ·(rssiⱼ - rssi_IDW) / (dⱼ² · Σwⱼ) · ∂dⱼ/∂x
∂dⱼ/∂x = (x - xⱼ) / (dⱼ · R)  ← где R = радиус Земли
```

LM алгоритм итеративно решает:
```
(JᵀWJ + λI) · δ = JᵀWr
x ← x + δ
λ уменьшается при успехе, увеличивается при неудаче
```

**Начальное приближение**: взвешенный центроид башен (по `1/range_m`).

Используем крейт `levenberg-marquardt` — реализован поверх `nalgebra`, WASM-совместим.

### 6.5. Monte Carlo (`montecarlo.rs`)

```
for _ in 0..500:
    для каждой соты i:
        rssi_noisy_i = rssi_observed_i + N(0, σ_noise)
        σ_noise = 8.0 dBm  # типичная нестабильность RSSI
    
    (lat_est, lon_est) = LM_solve(rssi_noisy)
    results.push((lat_est, lon_est))
```

### 6.6. Метрики (`metrics.rs`)

Из облака 500 оценок:

**CEP50 и CEP95** — радиусы окружностей:
```
dᵢ = haversine(truth_point, estimate_i)
CEP50 = percentile(d, 50)
CEP95 = percentile(d, 95)
```

**Эллипс ошибки** через PCA облака:
```
Cov = (1/N) · Xᵀ X   (X — центрированные оценки)
eigendecomp(Cov) → (λ₁, λ₂, v₁, v₂)
полуоси эллипса: a = k·√λ₁, b = k·√λ₂  (k=2.45 для 95%)
угол поворота: atan2(v₁.y, v₁.x)
```

**GDOP** (Geometric Dilution of Precision) — оценка геометрии сот:
```
H = якобиан направлений к сотам
GDOP = sqrt(trace((HᵀH)⁻¹))
```
Маленький GDOP (≈1) = соты окружают точку равномерно = хорошо.
Большой GDOP (> 6) = соты все с одной стороны = плохо.

---

## 7. WASM биндинг (`crates/wasm/src/lib.rs`)

```rust
#[wasm_bindgen]
pub struct TriangulationEngine {
    towers: Vec<TowerData>,
    measurements: Vec<MeasurementData>,
}

#[wasm_bindgen]
impl TriangulationEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self { ... }
    
    pub fn load_towers(&mut self, data: JsValue) -> Result<(), JsValue> { ... }
    pub fn load_measurements(&mut self, data: JsValue) -> Result<(), JsValue> { ... }
    
    pub fn solve(&self, query_lat: f64, query_lon: f64) -> JsValue {
        // → TriangulationResult { lat, lon, cep50_m, cep95_m,
        //                         ellipse, gdop, cloud_points }
    }
    
    pub fn set_model(&mut self, model: &str) { ... }  // "idw" | "kriging"
}
```

Сериализация через `serde-wasm-bindgen` (не устаревший `JsValue::from_serde`).

---

## 8. Бэкенд — Axum server (`crates/server`)

### AppState
```rust
pub struct AppState {
    pub db:      PgPool,
    pub redis:   redis::Client,
    pub http:    reqwest::Client,
    pub oci_key: String,
}
```

### Роутер
```rust
Router::new()
    .route("/api/cells",        get(cells_handler))
    .route("/api/measurements", get(measurements_handler))
    .route("/api/upload",       post(upload_handler))
    .route("/api/ws/android",   get(android_ws_handler))
    .layer(CorsLayer::permissive())
    .layer(CompressionLayer::new())    // gzip JSON ответов
    .layer(TraceLayer::new_for_http())
    .with_state(Arc::new(state))
```

### CORS
Фронтенд на Cloudflare Pages (другой домен) → нужен `tower-http::cors::CorsLayer`.

### Android WebSocket handler
Используется `axum::extract::ws` (встроено, не `tokio-tungstenite` напрямую):

```rust
async fn android_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_android_socket(socket, state))
}
```

---

## 9. Фронтенд (`web/`)

### Стек
```
Svelte 5 + Vite 5
Leaflet 1.9         — карта (OSM тайлы, без ключа)
```

Никаких UI-фреймворков — нативный Svelte достаточен.

### Компоненты

**MapView.svelte** — основной:
- Клик по карте → emit `pointSelected(lat, lon)`
- Отрисовка башен: `L.circleMarker` с радиусом пропорциональным `range_m`
- Отрисовка замеров: мелкие точки, цвет по RSSI (градиент от красного к зелёному)
- Отрисовка результата: большой маркер + эллипс (`L.ellipse` через плагин или SVG overlay)
- Отрисовка Monte Carlo облака: полупрозрачные точки

**MetricsPanel.svelte**:
- CEP50 / CEP95 в метрах
- GDOP с текстовой интерпретацией (отлично/хорошо/плохо/неприемлемо)
- Количество сот и замеров, использованных в расчёте
- Переключатель IDW ↔ Kriging
- Переключатель отображения Monte Carlo облака

**SourcesPanel.svelte**:
- Статус источников: OCI API (✓/✗), CSV-дамп (N замеров), Android (подключён/нет)
- Форма загрузки файла

**UploadPanel.svelte**:
- Drag-and-drop CSV/JSON
- Прогресс загрузки (SSE или polling)
- Отображение числа импортированных замеров

### Инициализация WASM
```js
// main.js
import init, { TriangulationEngine } from './wasm/trieval_wasm.js';

const wasm = await init();
window.engine = new TriangulationEngine();
```

---

## 10. Android-сбор данных

Два варианта, реализовать оба:

### Termux (без APK, для тестирования)
```python
# termux-api пакет + termux-telephony-cellinfo
import subprocess, json, websocket, time

def get_cells():
    raw = subprocess.check_output(['termux-telephony-cellinfo'])
    return json.loads(raw)

ws = websocket.WebSocket()
ws.connect("ws://your-server/api/ws/android")

while True:
    cells = get_cells()
    ws.send(json.dumps({"type": "measurement", "cells": cells}))
    time.sleep(5)
```

### Android app (Kotlin, минимальный)
```kotlin
val tm = getSystemService(TELEPHONY_SERVICE) as TelephonyManager
val cells = tm.allCellInfo  // TelephonyManager.getAllCellInfo()
// → парсинг CellInfoLte / CellInfoGsm / CellInfoNr
// → отправка через OkHttp WebSocket
```

---

## 11. Docker Compose (разработка)

```yaml
services:
  postgres:
    image: postgis/postgis:16-3.4-alpine
    environment:
      POSTGRES_DB: trieval
      POSTGRES_PASSWORD: trieval
    ports: ["5432:5432"]
    volumes: ["pgdata:/var/lib/postgresql/data"]

  redis:
    image: redis:7-alpine
    ports: ["6379:6379"]

volumes:
  pgdata:
```

---

## 12. Деплой

```
Фронтенд (статика + WASM)  →  Cloudflare Pages
Бэкенд (Rust бинарь)       →  Fly.io или VPS с Docker
База данных                 →  Supabase (managed PostGIS) или своя
Redis                       →  Upstash (free tier) или рядом с бэком
```

### Dockerfile (multi-stage, итог ~20MB)
```dockerfile
FROM rust:1.78-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY . .
RUN cargo build --release -p server

FROM alpine:3.19
RUN apk add --no-cache ca-certificates
COPY --from=builder /app/target/release/server /server
COPY --from=builder /app/migrations /migrations
CMD ["/server"]
```

WASM собирается в CI отдельно:
```bash
wasm-pack build crates/wasm --target web --out-dir ../../web/src/wasm
```

---

## 13. Порядок реализации

### Фаза 0 — Инфраструктура (1-2 дня)
1. Cargo workspace + все `Cargo.toml`
2. `docker-compose up` → postgres+postgis, redis
3. sqlx миграции (`sqlx migrate run`)
4. Минимальный Axum сервер с health check (`GET /health`)
5. Убедиться что всё компилируется

### Фаза 1 — Данные (2-3 дня)
6. `opencellid.rs` — клиент к OCI API `cell/getInArea`
7. `GET /api/cells` — запрос к API + кэш Redis + сохранение в БД
8. Загрузить CSV-дамп OCI для тестового региона → импорт через `POST /api/upload`
9. Проверить PostGIS spatial queries работают корректно

### Фаза 2 — Математическое ядро (3-5 дней)
10. `geo.rs` — haversine + vincenty, unit-тесты
11. `signal.rs` — нормализация RSSI, unit-тесты с таблицами значений
12. `spatial.rs` — IDW, unit-тесты на синтетических данных
13. `lm.rs` — LM оптимизатор, тест: восстанавливает известную точку по синтетике
14. `spatial.rs` — Kriging (variogram → kriging system → prediction)
15. `montecarlo.rs` — Monte Carlo, проверить что CEP50 уменьшается с добавлением сот
16. `metrics.rs` — CEP, GDOP, PCA эллипс

### Фаза 3 — WASM интеграция (1-2 дня)
17. `crates/wasm/src/lib.rs` — биндинги
18. `wasm-pack build` — убедиться что компилируется
19. Тест в браузерной консоли: вызвать `engine.solve()` с тестовыми данными

### Фаза 4 — Фронтенд (3-4 дня)
20. Svelte + Vite + Leaflet, базовая карта
21. Клик → запрос `/api/cells` → отрисовка башен
22. Запрос `/api/measurements` → подключение к WASM
23. Отрисовка результата: маркер + эллипс + Monte Carlo облако
24. MetricsPanel — CEP50/95, GDOP
25. SourcesPanel + UploadPanel

### Фаза 5 — Android и финальная полировка (1-2 дня)
26. Termux скрипт сбора данных
27. `WS /api/ws/android` обработчик
28. Dockerfile + деплой

---

## 14. Ловушки которые важно знать заранее

**OpenCelliD avgSignal = 0** — у большинства башен в CSV. Это означает что без реального CSV-дампа с измерениями IDW/Kriging работать не будет. Нужно загрузить дамп `measurements` (не только `cells`) или собирать данные самому через Android.

**BBOX лимит OCI API** — максимум 4 000 000 м². Радиус запроса ~1.1 км. При большем радиусе нужно разбивать на тайлы.

**levenberg-marquardt требует статически известных размерностей** через nalgebra const generics. Для переменного числа сот нужен `DMatrix`/`DVector` (dynamic), это поддерживается крейтом.

**Kriging плохо работает при < 7-10 замерах** для одной соты. Нужен фолбэк на IDW при малом количестве данных.

**WASM и многопоточность** — SharedArrayBuffer требует COOP/COEP заголовков. Проще запускать Monte Carlo в одном потоке или использовать `rayon` с wasm-bindgen-rayon (сложнее). Рекомендую для MVP: 500 итераций в одном потоке, это ~50ms в WASM — приемлемо.

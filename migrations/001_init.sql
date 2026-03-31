CREATE EXTENSION IF NOT EXISTS postgis;

-- Cell towers from OpenCelliD API or CSV dump
CREATE TABLE IF NOT EXISTS cell_towers (
    id          BIGSERIAL PRIMARY KEY,
    radio       TEXT NOT NULL,
    mcc         SMALLINT NOT NULL,
    mnc         SMALLINT NOT NULL,
    lac         INTEGER NOT NULL,
    cid         BIGINT NOT NULL,
    lat         DOUBLE PRECISION NOT NULL,
    lon         DOUBLE PRECISION NOT NULL,
    range_m     INTEGER,
    samples     INTEGER DEFAULT 0,
    changeable  BOOLEAN DEFAULT TRUE,
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    geom        GEOMETRY(Point, 4326) GENERATED ALWAYS AS
                    (ST_SetSRID(ST_MakePoint(lon, lat), 4326)) STORED,
    UNIQUE(radio, mcc, mnc, lac, cid)
);

CREATE INDEX IF NOT EXISTS cell_towers_geom_idx ON cell_towers USING GIST(geom);
CREATE INDEX IF NOT EXISTS cell_towers_mnc_idx  ON cell_towers(mcc, mnc);

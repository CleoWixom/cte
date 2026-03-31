-- RSSI measurements from CSV dump, file uploads, or Android
CREATE TABLE IF NOT EXISTS measurements (
    id          BIGSERIAL PRIMARY KEY,
    cell_id     BIGINT REFERENCES cell_towers(id) ON DELETE CASCADE,
    lat         DOUBLE PRECISION NOT NULL,
    lon         DOUBLE PRECISION NOT NULL,
    signal_dbm  SMALLINT,
    raw_signal  SMALLINT,
    radio       TEXT,
    source      TEXT NOT NULL DEFAULT 'upload',
    reliability FLOAT DEFAULT 1.0,
    measured_at TIMESTAMPTZ,
    geom        GEOMETRY(Point, 4326) GENERATED ALWAYS AS
                    (ST_SetSRID(ST_MakePoint(lon, lat), 4326)) STORED
);

CREATE INDEX IF NOT EXISTS measurements_geom_idx    ON measurements USING GIST(geom);
CREATE INDEX IF NOT EXISTS measurements_cell_id_idx ON measurements(cell_id);
CREATE INDEX IF NOT EXISTS measurements_source_idx  ON measurements(source);

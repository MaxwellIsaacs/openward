-- OpenWard SQLite Schema

-- Core detainee table: indexed columns for search/filter.
CREATE TABLE IF NOT EXISTS detainees (
    id                    TEXT PRIMARY KEY,
    surname               TEXT NOT NULL,
    given_names           TEXT NOT NULL,
    preferred_name        TEXT,
    sex                   TEXT NOT NULL CHECK (sex IN ('Male', 'Female', 'Other')),
    date_of_birth         TEXT,
    nationality           TEXT,
    national_id           TEXT,
    detention_basis_label TEXT NOT NULL,
    detention_basis_data  TEXT NOT NULL,
    facility_status       TEXT NOT NULL DEFAULT 'Present',
    intake_date           TEXT NOT NULL,
    housing_unit_id       TEXT REFERENCES housing_units(id),
    identity_extra        TEXT NOT NULL DEFAULT '{}',
    legal_reference       TEXT,
    legal_representation  TEXT,
    emergency_contacts    TEXT NOT NULL DEFAULT '[]',
    created_at            TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at            TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_detainees_surname ON detainees(surname);
CREATE INDEX IF NOT EXISTS idx_detainees_status ON detainees(facility_status);
CREATE INDEX IF NOT EXISTS idx_detainees_basis ON detainees(detention_basis_label);
CREATE INDEX IF NOT EXISTS idx_detainees_intake ON detainees(intake_date);
CREATE INDEX IF NOT EXISTS idx_detainees_housing ON detainees(housing_unit_id);
CREATE INDEX IF NOT EXISTS idx_detainees_national_id ON detainees(national_id) WHERE national_id IS NOT NULL;

-- Aliases for name search.
CREATE TABLE IF NOT EXISTS detainee_aliases (
    detainee_id TEXT NOT NULL REFERENCES detainees(id),
    alias       TEXT NOT NULL,
    PRIMARY KEY (detainee_id, alias)
);

CREATE INDEX IF NOT EXISTS idx_aliases_alias ON detainee_aliases(alias);

-- Commitment orders / warrants.
CREATE TABLE IF NOT EXISTS commitment_orders (
    id                 TEXT PRIMARY KEY,
    detainee_id        TEXT NOT NULL REFERENCES detainees(id),
    order_type         TEXT NOT NULL,
    order_data         TEXT NOT NULL,
    external_reference TEXT,
    issuing_authority  TEXT NOT NULL,
    issuing_officer    TEXT,
    date_issued        TEXT NOT NULL,
    date_received      TEXT NOT NULL,
    valid_until        TEXT,
    offence_description TEXT,
    sentence_details   TEXT,
    is_active          INTEGER NOT NULL DEFAULT 1,
    registered_by      TEXT NOT NULL,
    batch_id           TEXT,
    document_hash      TEXT,
    created_at         TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_warrants_detainee ON commitment_orders(detainee_id);
CREATE INDEX IF NOT EXISTS idx_warrants_active ON commitment_orders(detainee_id, is_active) WHERE is_active = 1;
CREATE INDEX IF NOT EXISTS idx_warrants_batch ON commitment_orders(batch_id) WHERE batch_id IS NOT NULL;

-- Court dates.
CREATE TABLE IF NOT EXISTS court_dates (
    id             TEXT PRIMARY KEY,
    detainee_id    TEXT NOT NULL REFERENCES detainees(id),
    scheduled_date TEXT NOT NULL,
    court_name     TEXT NOT NULL,
    purpose        TEXT NOT NULL,
    outcome        TEXT,
    created_at     TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_court_dates_detainee ON court_dates(detainee_id);
CREATE INDEX IF NOT EXISTS idx_court_dates_date ON court_dates(scheduled_date);
CREATE INDEX IF NOT EXISTS idx_court_dates_pending ON court_dates(scheduled_date) WHERE outcome IS NULL;

-- Property items.
CREATE TABLE IF NOT EXISTS property_items (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    detainee_id  TEXT NOT NULL REFERENCES detainees(id),
    description  TEXT NOT NULL,
    quantity     INTEGER NOT NULL DEFAULT 1,
    logged_date  TEXT NOT NULL,
    logged_by    TEXT NOT NULL,
    returned     INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_property_detainee ON property_items(detainee_id);

-- Notes.
CREATE TABLE IF NOT EXISTS notes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    detainee_id TEXT NOT NULL REFERENCES detainees(id),
    content     TEXT NOT NULL,
    author      TEXT NOT NULL,
    timestamp   TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_notes_detainee ON notes(detainee_id);

-- Housing units (reference data).
CREATE TABLE IF NOT EXISTS housing_units (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL UNIQUE,
    capacity            INTEGER NOT NULL,
    unit_type           TEXT NOT NULL,
    designated_sex      TEXT,
    designated_age_group TEXT
);

-- Daily counts.
CREATE TABLE IF NOT EXISTS daily_counts (
    id                   TEXT PRIMARY KEY,
    date                 TEXT NOT NULL UNIQUE,
    opening_count        INTEGER NOT NULL,
    admissions           INTEGER NOT NULL DEFAULT 0,
    transfers_in         INTEGER NOT NULL DEFAULT 0,
    court_returns        INTEGER NOT NULL DEFAULT 0,
    hospital_returns     INTEGER NOT NULL DEFAULT 0,
    releases             INTEGER NOT NULL DEFAULT 0,
    transfers_out        INTEGER NOT NULL DEFAULT 0,
    to_court             INTEGER NOT NULL DEFAULT 0,
    to_hospital          INTEGER NOT NULL DEFAULT 0,
    escapes              INTEGER NOT NULL DEFAULT 0,
    deaths               INTEGER NOT NULL DEFAULT 0,
    computed_closing     INTEGER NOT NULL,
    actual_closing_count INTEGER,
    is_balanced          INTEGER,
    closing_by_basis     TEXT NOT NULL DEFAULT '{}',
    closing_by_sex       TEXT NOT NULL DEFAULT '{}',
    discrepancy_note     TEXT,
    counted_by           TEXT,
    finalized_by         TEXT,
    finalized_at         TEXT,
    created_at           TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at           TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Unit headcounts (per-block counts within a daily count).
CREATE TABLE IF NOT EXISTS unit_headcounts (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    daily_count_id TEXT NOT NULL REFERENCES daily_counts(id),
    unit_id       TEXT NOT NULL REFERENCES housing_units(id),
    count         INTEGER NOT NULL,
    counted_by    TEXT NOT NULL,
    counted_at    TEXT NOT NULL,
    received_at   TEXT NOT NULL,
    was_offline   INTEGER NOT NULL DEFAULT 0,
    UNIQUE(daily_count_id, unit_id)
);

-- Audit trail (append-only).
CREATE TABLE IF NOT EXISTS audit_entries (
    id         TEXT PRIMARY KEY,
    timestamp  TEXT NOT NULL,
    operator   TEXT NOT NULL,
    module     TEXT NOT NULL,
    action     TEXT NOT NULL,
    target     TEXT,
    before_data TEXT,
    after_data  TEXT,
    self_hash  BLOB NOT NULL,
    chain_hash BLOB NOT NULL,
    epoch      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_entries(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_target ON audit_entries(target) WHERE target IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_audit_epoch ON audit_entries(epoch);
CREATE INDEX IF NOT EXISTS idx_audit_operator ON audit_entries(operator);

-- Audit epoch checkpoints.
CREATE TABLE IF NOT EXISTS audit_epochs (
    epoch           INTEGER PRIMARY KEY,
    started_at      TEXT NOT NULL,
    checkpoint_hash BLOB NOT NULL,
    entry_count     INTEGER NOT NULL DEFAULT 0
);

-- Query audit trail (high volume, separate).
CREATE TABLE IF NOT EXISTS query_audit (
    id           TEXT PRIMARY KEY,
    timestamp    TEXT NOT NULL,
    operator     TEXT NOT NULL,
    query_data   TEXT NOT NULL,
    result_count INTEGER NOT NULL,
    exported     INTEGER NOT NULL DEFAULT 0,
    export_format TEXT
);

-- Trend snapshots (one row per metric per day).
CREATE TABLE IF NOT EXISTS trend_snapshots (
    date   TEXT NOT NULL,
    metric TEXT NOT NULL,
    value  REAL NOT NULL,
    PRIMARY KEY (date, metric)
);

-- Batch admission records (for traceability).
CREATE TABLE IF NOT EXISTS batch_admissions (
    id              TEXT PRIMARY KEY,
    shared_warrant  TEXT NOT NULL,
    intake_date     TEXT NOT NULL,
    admitted_by     TEXT NOT NULL,
    detainee_count  INTEGER NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Transfer records.
CREATE TABLE IF NOT EXISTS transfers (
    id             TEXT PRIMARY KEY,
    detainee_id    TEXT NOT NULL REFERENCES detainees(id),
    from_facility  TEXT,
    to_facility    TEXT,
    transfer_date  TEXT NOT NULL,
    reason         TEXT NOT NULL,
    authorized_by  TEXT NOT NULL,
    created_at     TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_transfers_detainee ON transfers(detainee_id);

-- Pending offline mutations (store-and-forward queue).
CREATE TABLE IF NOT EXISTS pending_mutations (
    client_id        TEXT PRIMARY KEY,
    client_timestamp TEXT NOT NULL,
    device_id        TEXT NOT NULL,
    operator         TEXT NOT NULL,
    action_type      TEXT NOT NULL,
    payload          TEXT NOT NULL,
    received_at      TEXT,
    processed        INTEGER NOT NULL DEFAULT 0,
    error            TEXT
);

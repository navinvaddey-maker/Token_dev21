-- migrations/0004_create_use_cases.sql
CREATE TABLE use_cases (
    id             TEXT PRIMARY KEY,
    key            TEXT NOT NULL UNIQUE,
    version        TEXT NOT NULL DEFAULT '1.0.0',
    role_frame     TEXT NOT NULL,
    output_format  TEXT NOT NULL,
    chunk_strategy TEXT NOT NULL DEFAULT 'sentence',
    description    TEXT,
    active         INTEGER NOT NULL DEFAULT 1,
    created_at     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Note: In Postgres we use gen_random_uuid(). In 0004_create_use_cases.sql we seed it.
INSERT INTO use_cases (id, key, role_frame, output_format, chunk_strategy, description) VALUES
(gen_random_uuid(), 'ticket',     'Role: senior support analyst. Context: customer support triage.',        'Output: JSON [{id,priority,category,action}]',      'newline',   'Customer support ticket triage'),
(gen_random_uuid(), 'legal',      'Role: legal analyst. Context: contract risk identification.',            'Output: bullet — risk · clause · severity',         'paragraph', 'Legal contract analysis'),
(gen_random_uuid(), 'resume',     'Role: talent screener. Context: candidate evaluation.',                  'Output: scorecard — candidate · fit_score',         'paragraph', 'Resume screening'),
(gen_random_uuid(), 'code',       'Role: senior software engineer. Context: codebase documentation.',       'Output: markdown — function · purpose · returns',   'paragraph', 'Code documentation'),
(gen_random_uuid(), 'research',   'Role: research analyst. Context: academic paper summarization.',          'Output: structured — finding · evidence',           'paragraph', 'Research paper summarization');

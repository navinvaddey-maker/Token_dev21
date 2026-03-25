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
    created_at     TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO use_cases (id, key, role_frame, output_format, chunk_strategy, description) VALUES
(lower(hex(randomblob(16))), 'ticket',     'Role: senior support analyst. Context: customer support triage.',        'Output: JSON [{id,priority,category,action}]',      'newline',   'Customer support ticket triage'),
(lower(hex(randomblob(16))), 'legal',      'Role: legal analyst. Context: contract risk identification.',            'Output: bullet — risk · clause · severity',         'paragraph', 'Legal contract analysis'),
(lower(hex(randomblob(16))), 'resume',     'Role: talent screener. Context: candidate evaluation.',                  'Output: scorecard — candidate · fit_score',         'paragraph', 'Resume screening'),
(lower(hex(randomblob(16))), 'code',       'Role: senior software engineer. Context: codebase documentation.',       'Output: markdown — function · purpose · returns',   'paragraph', 'Code documentation'),
(lower(hex(randomblob(16))), 'research',   'Role: research analyst. Context: academic paper summarization.',          'Output: structured — finding · evidence',           'paragraph', 'Research paper summarization'),
(lower(hex(randomblob(16))), 'transcript', 'Role: meeting coordinator. Context: action item extraction.',             'Output: action list — owner · action · deadline',   'newline',   'Meeting transcript extraction'),
(lower(hex(randomblob(16))), 'financial',  'Role: financial analyst. Context: quarterly earnings assessment.',        'Output: health report — metric · value · signal',   'paragraph', 'Financial report analysis'),
(lower(hex(randomblob(16))), 'generic',    'Role: analyst. Context: document processing and structured extraction.', 'Output: structured summary',                        'sentence',  'Generic document processing');

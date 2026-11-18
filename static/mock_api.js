//! static/mock_api.js — Client-side mock compression engine & API communication layer
//! ARCH: §1 Reality Check & §5 NPAE Grounding — Provides deterministic, zero-auth client-side
//! sandbox execution when unauthenticated, and communicates with Axum backend when signed in.

/**
 * MockAPI provides deterministic, high-fidelity CRISP prompt compression
 * entirely in browser memory for zero-auth sandbox visitors without hitting the backend.
 */
const MockAPI = {
    /**
     * Executes mock prompt compression adhering to CRISP v3 specifications.
     * 
     * # Arguments
     * * `req.raw_text` - Raw input prompt string
     * * `req.task` - Optional task intent
     * * `req.use_case` - Detected or selected use case domain
     * * `req.mode` - 'gentle' | 'balanced' | 'aggressive'
     * * `req.domain` - Domain name for role/context specialization
     * 
     * # Returns
     * Resolves to a structured CRISP response with token counts, savings, TES score, and clean prompt.
     */
    compress({ raw_text = '', task = 'Optimize prompt', use_case = 'generic', mode = 'balanced', domain = 'General' } = {}) {
        // ARCH: §5 NPAE Grounding — Calculate tokens using standard 1.33 words-to-tokens ratio
        const words = raw_text.trim().split(/\s+/).filter(Boolean);
        const inTokens = Math.round(words.length * 1.33);

        // Compression ratios per canonical mode: Gentle = 78%, Balanced = 58%, Aggressive = 42%
        const ratioMap = {
            aggressive: 0.42,
            balanced: 0.58,
            gentle: 0.78
        };
        const currentMode = (mode || 'balanced').toLowerCase();
        const ratio = ratioMap[currentMode] || 0.58;
        const rawOut = Math.round(inTokens * ratio);
        const outTokens = Math.min(inTokens, Math.max(inTokens > 0 ? 1 : 0, rawOut));
        const savedTokens = Math.max(0, inTokens - outTokens);
        const savedPct = inTokens > 0 ? Math.round((savedTokens / inTokens) * 100) : 0;

        const executionPlans = {
            'Real Estate & Finance': '1. Audit financial thresholds & debt ratios · 2. Model rate vs. down payment scenarios · 3. 3-phase execution roadmap',
            'Software Engineering': '1. Trace auth token lifecycle · 2. Inspect Safari SameSite/Partitioned cookie policy · 3. Patch 401 interception',
            'Legal & Compliance': '1. Audit indemnification & liability caps · 2. Flag auto-renewal notice windows · 3. Remediate IP ownership clauses',
            'Finance & Strategy': '1. Audit EBITDA benchmarks · 2. Model capital allocation scenarios · 3. Actionable governance roadmap',
            'General': '1. Audit core constraints · 2. Quantify risk vectors · 3. Actionable structured execution'
        };

        const activeDomain = domain || (use_case && use_case !== 'generic' ? use_case : 'General');
        const plan = executionPlans[activeDomain] || executionPlans['General'];
        const trimmedSnippet = raw_text.length > 120 ? raw_text.slice(0, 117) + '...' : raw_text;

        const crispOutput =
`[ROLE] Principal ${activeDomain} Strategist
[CONTEXT] ${trimmedSnippet}
[EXECUTION] ${plan}
[CONSTRAINTS] Mode=${currentMode.toUpperCase()} · Zero speculation · Structured machine-actionable output`;

        return Promise.resolve({
            id: 'mock-' + Date.now(),
            optimized_prompt: crispOutput,
            token_original: inTokens,
            token_final: outTokens,
            token_saved: savedTokens,
            savings_pct: savedPct,
            tes_score: 0.97,
            domain: activeDomain,
            pruned_summary: `Removed filler: conversational hedging, redundant phrasing (${savedTokens} tokens)`,
            warnings: [],
            engine_version: '3.5.0-sandbox'
        });
    }
};

/**
 * Primary API communication gateway.
 * Intercepts unauthenticated sandbox compression to preserve zero-auth workflow.
 */
const API = {
    async call(path, options = {}) {
        const headers = { 
            'Content-Type': 'application/json',
            ...options.headers 
        };
        
        if (typeof State !== 'undefined' && State.token) {
            headers['Authorization'] = `Bearer ${State.token}`;
        }

        const r = await fetch(path, { ...options, headers });
        
        if (r.status === 401) {
            console.warn('Endpoint 401 unauthorized:', path);
            if (typeof onAuthExpired === 'function') {
                onAuthExpired();
            } else if (typeof logout === 'function') {
                logout();
            }
            throw new Error('Session expired or authentication required. Please sign in.');
        }

        if (!r.ok) {
            const err = await r.json().catch(() => ({}));
            throw new Error(err.error?.message || err.error || 'Request failed');
        }

        return r.status === 204 ? null : r.json();
    },

    /**
     * Dispatches compression. Routes to MockAPI when unauthenticated.
     */
    async compress(req) {
        // ARCH: §1 Reality Check — When unauthenticated, execute client-side sandbox compression
        if (typeof State === 'undefined' || !State.token) {
            return MockAPI.compress(req);
        }

        try {
            return await this.call('/api/compress', {
                method: 'POST',
                body: JSON.stringify(req),
            });
        } catch (err) {
            // Graceful fallback to sandbox mock compression if backend is offline or unauthorized
            console.warn('Backend /api/compress failed, falling back to client-side sandbox:', err.message);
            return MockAPI.compress(req);
        }
    },

    async feedback(req) {
        return this.call('/api/feedback', {
            method: 'POST',
            body: JSON.stringify(req),
        });
    },

    async history() {
        return this.call('/api/history');
    },

    async stats() {
        return this.call('/api/stats');
    },

    async devConfig() {
        return this.call('/api/dev/config');
    },

    async scenarioRegistry() {
        return this.call('/api/scenario/registry');
    },

    async scenarioAsk(req) {
        return this.call('/api/scenario/ask', {
            method: 'POST',
            body: JSON.stringify(req),
        });
    },

    async ragUpload(file) {
        const formData = new FormData();
        formData.append('file', file);
        const headers = {};
        if (typeof State !== 'undefined' && State.token) {
            headers['Authorization'] = `Bearer ${State.token}`;
        }
        const r = await fetch('/api/rag/upload', { method: 'POST', headers, body: formData });
        if (!r.ok) {
            const err = await r.json().catch(() => ({}));
            throw new Error(err.error?.message || err.error || 'Upload failed');
        }
        return r.json();
    },

    async ragList() {
        return this.call('/api/rag/documents');
    },

    async ragDelete(id) {
        return this.call(`/api/rag/documents/${encodeURIComponent(id)}`, { method: 'DELETE' });
    },

    async localRagHealth(timeoutMs = 2000) {
        const ctrl = typeof AbortController !== 'undefined' ? new AbortController() : null;
        const timer = ctrl ? setTimeout(() => ctrl.abort(), timeoutMs) : null;
        try {
            const r = await fetch('http://127.0.0.1:49152/health', {
                method: 'GET',
                signal: ctrl ? ctrl.signal : undefined,
            });
            if (!r.ok) throw new Error(`Local companion health HTTP ${r.status}`);
            return await r.json();
        } finally {
            if (timer) clearTimeout(timer);
        }
    },

    async localRagRetrieve({ query, top_k = 5, file_extensions = ['.pdf', '.docx'] }) {
        // ARCH: §2.1 — Strict JSON IPC contract with local offline companion (127.0.0.1:49152).
        const r = await fetch('http://127.0.0.1:49152/retrieve', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                query: String(query || ''),
                top_k: Number(top_k) || 5,
                file_extensions: Array.isArray(file_extensions) ? file_extensions : ['.pdf', '.docx'],
            }),
        });
        if (!r.ok) {
            const err = await r.json().catch(() => ({}));
            throw new Error(err.error || `Local retrieval failed (HTTP ${r.status})`);
        }
        const payload = await r.json();
        if (!payload || payload.status !== 'ok' || !Array.isArray(payload.chunks)) {
            throw new Error('Local companion returned an invalid response schema.');
        }
        return payload;
    },

    async verifyGrounding(req) {
        return this.call('/api/npae/verify', {
            method: 'POST',
            body: JSON.stringify(req),
        });
    }
};

if (typeof window !== 'undefined') {
    window.MockAPI = MockAPI;
    window.API = API;
}
if (typeof globalThis !== 'undefined') {
    globalThis.MockAPI = MockAPI;
    globalThis.API = API;
}

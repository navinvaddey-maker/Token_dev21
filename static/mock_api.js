// static/mock_api.js — Development fallback + API wrapper
const MockAPI = {
    compress({ raw_text, task }) {
        const words   = raw_text.split(/\s+/);
        const filler  = /\b(just|basically|actually|please|really|very)\b/gi;
        const cleaned = raw_text.replace(filler, '').replace(/\s{2,}/g, ' ').trim();
        return Promise.resolve({
            id:               'mock-' + Date.now(),
            optimized_prompt: `Role: analyst.\n\nTask: ${task}\n\n---\n\n${cleaned}`,
            token_original:   Math.ceil(words.length * 1.3),
            token_final:      Math.ceil(cleaned.split(/\s+/).length * 1.3),
            token_saved:      Math.ceil((words.length - cleaned.split(/\s+/).length) * 1.3),
            warnings:         [],
            engine_version:   '1.0.0-mock',
        });
    }
};

const API = {
    async compress(req) {
        try {
            const r = await fetch('/api/compress', {
                method:  'POST',
                headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${State.token}` },
                body:    JSON.stringify(req),
            });
            if (!r.ok) throw new Error((await r.json()).error?.message ?? 'compress failed');
            return r.json();
        } catch (e) {
            console.warn('Backend unreachable — using MockAPI:', e.message);
            return MockAPI.compress(req);
        }
    },

    async feedback(req) {
        const r = await fetch('/api/feedback', {
            method:  'POST',
            headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${State.token}` },
            body:    JSON.stringify(req),
        });
        if (!r.ok) throw new Error('Feedback failed');
    },

    async history() {
        const r = await fetch('/api/history', {
            headers: { 'Authorization': `Bearer ${State.token}` },
        });
        if (!r.ok) throw new Error('Failed to load history');
        return r.json();
    },

    async stats() {
        const r = await fetch('/api/stats', {
            headers: { 'Authorization': `Bearer ${State.token}` },
        });
        if (!r.ok) throw new Error('Failed to load stats');
        return r.json();
    },

    async devConfig() {
        const r = await fetch('/api/dev/config', {
            headers: { 'Authorization': `Bearer ${State.token}` },
        });
        if (!r.ok) throw new Error('Failed to load config');
        return r.json();
    }
};

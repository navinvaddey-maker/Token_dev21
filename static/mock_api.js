// static/mock_api.js — Development fallback + API wrapper
const MockAPI = {
    compress({ raw_text, task, deliverables, constraints, reproducibility }) {
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
    async call(path, options = {}) {
        const headers = { 
            'Content-Type': 'application/json',
            ...options.headers 
        };
        
        if (State.token) {
            headers['Authorization'] = `Bearer ${State.token}`;
        }

        const r = await fetch(path, { ...options, headers });
        
        if (r.status === 401) {
            console.error('Session expired or unauthorized');
            if (typeof logout === 'function') logout();
            throw new Error('Session expired. Please log in again.');
        }

        if (!r.ok) {
            const err = await r.json().catch(() => ({}));
            throw new Error(err.error?.message || err.error || 'Request failed');
        }

        return r.status === 204 ? null : r.json();
    },

    async compress(req) {
        try {
            return await this.call('/api/compress', {
                method: 'POST',
                body: JSON.stringify(req),
            });
        } catch (e) {
            console.warn('Backend error — using MockAPI fallback:', e.message);
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
    }
};

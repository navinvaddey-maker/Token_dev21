// static/app.js — Controller
const State = {
    userId:     null,
    token:      null,
    username:   null,
    businessType: null,
    lastResult: null,
    lastTs:     null,
};

// ── Auth ────────────────────────────────────────────────────────────────────

function switchAuthTab(tab) {
    document.querySelectorAll('.auth-tab').forEach(t => t.classList.remove('active'));
    document.querySelectorAll('.auth-form').forEach(f => f.classList.remove('active'));
    document.getElementById('tab-' + tab).classList.add('active');
    document.getElementById('form-' + tab).classList.add('active');
}

async function handleLogin(e) {
    e.preventDefault();
    const username = document.getElementById('login-username').value.trim();
    const password = document.getElementById('login-password').value;
    if (!username || !password) return showToast('Username and password required', 'error');

    try {
        console.log('Attempting login for:', username);
        const r = await fetch('/api/login', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ username, password }),
        });
        console.log('Login response status:', r.status);
        console.log('Login response ok:', r.ok);
        console.log('Login response headers:', Object.fromEntries(r.headers.entries()));
        
        if (!r.ok) {
            const err = await r.json();
            console.log('Login error response:', err);
            throw new Error(err.error?.message || 'Login failed');
        }
        console.log('Response received, parsing JSON...');
        const data = await r.json();
        
        // Validate required fields FIRST before accessing any properties
        if (!data || typeof data !== 'object') {
            throw new Error('Invalid login response: data is not an object');
        }
        
        // Now it's safe to log data properties
        console.log('Login response data:', data);
        console.log('Login response data type:', typeof data);
        console.log('Login response data keys:', Object.keys(data || {}));
        
        // Additional debugging
        console.log('data.business_type:', data.business_type);
        console.log('type of data.business_type:', typeof data.business_type);
        
        // Validate required fields
        if (!data.token) {
            throw new Error('Invalid login response: missing token');
        }
        if (!data.user_id) {
            throw new Error('Invalid login response: missing user_id');
        }
        if (!data.username) {
            throw new Error('Invalid login response: missing username');
        }
        if (!data.business_type) {
            throw new Error('Invalid login response: missing business_type');
        }
        console.log('Setting State.businessType to:', `"${data.business_type}"`);
        State.token    = data.token;
        State.userId   = data.user_id;
        State.username = data.username;
        localStorage.setItem('tce_token', data.token);
        localStorage.setItem('tce_user_id', data.user_id);
        localStorage.setItem('tce_username', data.username);
        localStorage.setItem('tce_business_type', data.business_type);
        State.businessType = data.business_type;
        console.log('Login response business_type:', `"${data.business_type}"`);
        showMainSection();
        showToast('Welcome back, ' + data.username, 'success');
    } catch (err) {
        console.error('Login error:', err);
        console.error('Login error stack:', err.stack);
        showToast(err.message, 'error');
    }
}

async function handleRegister(e) {
    e.preventDefault();
    const username      = document.getElementById('reg-username').value.trim();
    const email         = document.getElementById('reg-email').value.trim();
    const password      = document.getElementById('reg-password').value;
    const business_type = document.getElementById('reg-business').value;

    if (!username || !email || !password) return showToast('All fields required', 'error');

    try {
        const r = await fetch('/api/register', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ username, email, password, business_type }),
        });
        if (!r.ok) {
            const err = await r.json();
            throw new Error(err.error?.message || 'Registration failed');
        }
        showToast('Account created! Please sign in.', 'success');
        switchAuthTab('login');
    } catch (err) {
        showToast(err.message, 'error');
    }
}

function logout() {
    State.token = null;
    State.userId = null;
    State.username = null;
    localStorage.removeItem('tce_token');
    localStorage.removeItem('tce_user_id');
    localStorage.removeItem('tce_username');
    localStorage.removeItem('tce_business_type');
    document.getElementById('auth-section').style.display = 'block';
    document.getElementById('main-section').style.display = 'none';
}

function showMainSection() {
    document.getElementById('auth-section').style.display = 'none';
    document.getElementById('main-section').style.display = 'block';
    document.getElementById('display-username').textContent = State.username;
    
    console.log('Showing main section. businessType:', `"${State.businessType}"`, 'Length:', State.businessType ? State.businessType.length : 'null');
    console.log('Trimmed and lowercased:', `"${State.businessType && State.businessType.trim().toLowerCase()}"`);
    
    if (State.businessType && State.businessType.trim().toLowerCase() === 'admin') {
        console.log('Showing admin link');
        document.getElementById('admin-link').style.display = 'inline-flex';
    } else {
        console.log('Hiding admin link');
        document.getElementById('admin-link').style.display = 'none';
    }
}

// ── Tabs ────────────────────────────────────────────────────────────────────

function switchTab(tab) {
    document.querySelectorAll('.nav-tab').forEach(t => t.classList.remove('active'));
    document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
    document.getElementById('nav-' + tab).classList.add('active');

    if (tab === 'compress') {
        document.getElementById('tab-compress').classList.add('active');
    } else if (tab === 'history') {
        document.getElementById('tab-history-content').classList.add('active');
        loadHistory();
    } else if (tab === 'stats') {
        document.getElementById('tab-stats-content').classList.add('active');
        loadStats();
    }
}

// ── Compress ────────────────────────────────────────────────────────────────

async function compress() {
    const raw     = document.getElementById('prompt-input').value.trim();
    const task    = document.getElementById('task-input').value.trim();
    const deliverables = document.getElementById('deliverables-input').value.trim();
    const constraints  = document.getElementById('constraints-input').value.trim();
    const reproducibility = document.getElementById('reproducibility-input').value.trim();
    const useCase = document.getElementById('use-case').value;
    const model   = document.getElementById('model-select').value;
    const mode    = document.getElementById('mode').value;

    if (!raw) return showToast('Raw Prompt is required', 'error');
    if (!deliverables) return showToast('Deliverable Guidance is required', 'error');
    if (!constraints) return showToast('Constraints field is required', 'error');
    if (!reproducibility) return showToast('Reproducibility field is required', 'error');

    // detect use_case from DevEngine if not manually set
    const detectedUseCase = useCase || DevEngine.detectUseCase(raw);

    const btn = document.getElementById('compress-btn');
    btn.innerHTML = '<span class="spinner"></span> Compressing...';
    btn.disabled = true;

    try {
        const result = await API.compress({
            raw_text:  raw,
            task:      task || 'Optimize prompt',
            deliverables: deliverables,
            constraints:  constraints,
            reproducibility: reproducibility,
            use_case:  detectedUseCase,
            model:     model,
            mode:      mode,
        });
        State.lastResult = result;
        State.lastTs     = Date.now();
        renderResult(result);
    } catch (err) {
        showToast(err.message, 'error');
    } finally {
        btn.innerHTML = '⚡ Compress Prompt';
        btn.disabled = false;
    }
}

function renderResult(result) {
    document.getElementById('result-section').style.display = 'block';
    document.getElementById('stat-original').textContent = result.token_original;
    document.getElementById('stat-final').textContent    = result.token_final;
    document.getElementById('stat-saved').textContent    = result.token_saved;
    document.getElementById('result-output').textContent = result.optimized_prompt;
}

async function submitFeedback(rating) {
    if (!State.lastResult) return;
    try {
        await API.feedback({ history_id: State.lastResult.id, rating });
        showToast(rating === 'thumbs_up' ? 'Thanks for the feedback! 👍' : 'Feedback recorded 👎', 'success');
    } catch (err) {
        showToast(err.message, 'error');
    }
}

function copyResult() {
    const text = document.getElementById('result-output').textContent;
    navigator.clipboard.writeText(text).then(() => showToast('Copied to clipboard!', 'success'));
}

// ── History ─────────────────────────────────────────────────────────────────

async function loadHistory() {
    try {
        const records = await API.history();
        const list = document.getElementById('history-list');
        if (!records.length) {
            list.innerHTML = '<p style="color:var(--text-muted);text-align:center;padding:20px;">No compression history yet</p>';
            return;
        }
        list.innerHTML = records.map(r => `
            <div class="history-item" onclick="document.getElementById('prompt-input').value='${r.original_prompt.replace(/'/g, "\\'")}';switchTab('compress');">
                <div style="font-size:0.9rem;color:var(--text-primary);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">
                    ${escapeHtml(r.original_prompt.substring(0, 100))}${r.original_prompt.length > 100 ? '...' : ''}
                </div>
                <div class="history-meta">
                    <span><span class="badge badge-accent">${r.use_case}</span> <span class="badge badge-success">${r.tokens_saved} saved</span></span>
                    <span>${new Date(r.created_at).toLocaleDateString()}</span>
                </div>
            </div>
        `).join('');
    } catch (err) {
        showToast(err.message, 'error');
    }
}

// ── Stats ───────────────────────────────────────────────────────────────────

async function loadStats() {
    try {
        const stats = await API.stats();
        document.getElementById('global-stats').innerHTML = `
            <div class="stat-card">
                <div class="stat-value">${stats.total_compressions || 0}</div>
                <div class="stat-label">Total Compressions</div>
            </div>
            <div class="stat-card">
                <div class="stat-value green">${stats.total_tokens_saved || 0}</div>
                <div class="stat-label">Total Tokens Saved</div>
            </div>
            <div class="stat-card">
                <div class="stat-value">${Math.round(stats.avg_tokens_saved || 0)}</div>
                <div class="stat-label">Avg Tokens Saved</div>
            </div>
        `;
    } catch (err) {
        showToast(err.message, 'error');
    }
}

// ── Utils ───────────────────────────────────────────────────────────────────

function showToast(msg, type) {
    const existing = document.querySelector('.toast');
    if (existing) existing.remove();

    const toast = document.createElement('div');
    toast.className = 'toast toast-' + type;
    toast.textContent = msg;
    document.body.appendChild(toast);
    setTimeout(() => toast.remove(), 4000);
}

function escapeHtml(text) {
    const d = document.createElement('div');
    d.textContent = text;
    return d.innerHTML;
}

// ── Init ────────────────────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
    const token    = localStorage.getItem('tce_token');
    const userId   = localStorage.getItem('tce_user_id');
    const username = localStorage.getItem('tce_username');
    const businessType = localStorage.getItem('tce_business_type');
    if (token && userId && username) {
        State.token    = token;
        State.userId   = userId;
        State.username = username;
        State.businessType = businessType;
        showMainSection();
    }
});

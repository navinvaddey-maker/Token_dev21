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
    if (typeof validatePromptLength === 'function') validatePromptLength();
    if (typeof loadScenarioRegistry === 'function') loadScenarioRegistry();
}

// ── Tabs ────────────────────────────────────────────────────────────────────

function switchTab(tab) {
    document.querySelectorAll('.nav-tab').forEach(t => t.classList.remove('active'));
    document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
    document.getElementById('nav-' + tab).classList.add('active');

    if (tab === 'compress') {
        document.getElementById('tab-compress').classList.add('active');
        if (typeof validatePromptLength === 'function') validatePromptLength();
    } else if (tab === 'ask') {
        document.getElementById('tab-ask-content').classList.add('active');
        loadScenarioRegistry();
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
    console.log("Starting Neuro-Compression...");
    const raw     = document.getElementById('prompt-input').value.trim();
    const task    = document.getElementById('task-input').value.trim();
    const useCase = document.getElementById('use-case').value;
    const mode    = document.getElementById('mode').value;

    if (!raw) return showToast('Raw Prompt is required', 'error');

    const words = raw.trim().split(/\s+/).filter(Boolean);
    if (words.length < 8) {
        return showToast('Raw Prompt must be at least 8 words', 'error');
    }



    // Show Neuro Progress Overlay
    const overlay = document.getElementById('neuro-progress-overlay');
    const status = document.getElementById('progress-status');
    const subtext = document.querySelector('.progress-subtext');
    
    if (overlay) {
        console.log("Displaying progress overlay");
        overlay.style.display = 'flex';
    } else {
        console.error("Progress overlay element not found!");
    }

    const phases = [
        { s: "Initializing Neuro-Engine...", sub: "Calibrating neural activation thresholds" },
        { s: "Mapping prompt topology...", sub: "Analyzing semantic clusters and activation peaks" },
        { s: "Deconstructing raw syntax...", sub: "Identifying high-entropy token sequences" },
        { s: "Instantiating neuro-fields...", sub: "Applying CRISP framework constraints" },
        { s: "Extracting fidelity markers...", sub: "Validating intent retention across layers" },
        { s: "Optimizing token weights...", sub: "Refining fidelity coefficients via Hebbian loop" },
        { s: "Rendering neuro-ambient result...", sub: "Injecting scope-optimized deltas" },
        { s: "Finalizing optimization...", sub: "Encoding high-fidelity compression output" }
    ];

    let phaseIdx = 0;
    const phaseTimer = setInterval(() => {
        phaseIdx = (phaseIdx + 1) % phases.length;
        if (status) status.textContent = phases[phaseIdx].s;
        if (subtext) subtext.textContent = phases[phaseIdx].sub;
    }, 1800);

    // detect use_case from DevEngine if not manually set
    const detectedUseCase = useCase || (typeof DevEngine !== 'undefined' ? DevEngine.detectUseCase(raw) : useCase);

    const btn = document.getElementById('compress-btn');
    if (btn) btn.disabled = true;

    // Initialize Neuron Animation
    let neuronAnim = null;
    const canvas = document.getElementById('neuron-canvas');
    if (canvas && typeof NeuronAnimation !== 'undefined') {
        neuronAnim = new NeuronAnimation('neuron-canvas');
        neuronAnim.start();
    }

    const delayPromise = new Promise(resolve => setTimeout(resolve, 15000));

    try {
        console.log("Calling API.compress...");
        const result = await API.compress({
            raw_text:  raw,
            task:      task || 'Optimize prompt',
            use_case:  detectedUseCase,
            mode:      mode,
        });

        // Ensure we wait AT LEAST 15 seconds for "brain attraction" regardless of API speed
        console.log("API returned. Maintaining neuro-ambient phase until 15s mark...");
        await delayPromise;

        console.log("Compression successful");
        State.lastResult = result;
        State.lastTs     = Date.now();
        renderResult(result);
        showToast('Neural optimization complete', 'success');
    } catch (err) {
        console.error("Compression error:", err);
        // Even on error, we wait for the 15s attraction phase
        await delayPromise;
        showToast(err.message, 'error');
    } finally {
        clearInterval(phaseTimer);
        if (neuronAnim) neuronAnim.stop();
        if (overlay) overlay.style.display = 'none';
        if (btn) btn.disabled = false;
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

// ── Validation ──────────────────────────────────────────────────────────────
function validatePromptLength() {
    const promptInput = document.getElementById('prompt-input');
    const compressBtn = document.getElementById('compress-btn');
    const errorDiv = document.getElementById('prompt-error');
    if (!compressBtn) return;

    if (!promptInput) {
        compressBtn.disabled = true;
        return;
    }

    const raw = promptInput.value.trim();
    const words = raw ? raw.split(/\s+/).filter(Boolean) : [];
    
    if (words.length >= 8) {
        compressBtn.disabled = false;
        if (errorDiv) errorDiv.style.display = 'none';
    } else {
        compressBtn.disabled = true;
        if (errorDiv) {
            if (raw.length > 0) {
                errorDiv.textContent = `Prompt must be at least 8 words (currently ${words.length} / 8).`;
                errorDiv.style.display = 'block';
            } else {
                errorDiv.style.display = 'none';
            }
        }
    }
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
        loadScenarioRegistry();
    }

    const promptInput = document.getElementById('prompt-input');
    if (promptInput) {
        promptInput.addEventListener('input', validatePromptLength);
        promptInput.addEventListener('change', validatePromptLength);
        promptInput.addEventListener('paste', () => setTimeout(validatePromptLength, 20));
        promptInput.addEventListener('keyup', validatePromptLength);
    }
    // Button starts disabled and only enables when >= 120 words
    validatePromptLength();
});

// ── Scenario RAG Tab logic ──────────────────────────────────────────────────

let scenarioRegistryLoaded = false;

async function loadScenarioRegistry() {
    if (scenarioRegistryLoaded) return;
    try {
        const reg = await API.scenarioRegistry();
        const domainSel = document.getElementById('scenario-domain-select');
        const styleSel  = document.getElementById('scenario-style-select');

        if (domainSel && reg.domains) {
            domainSel.innerHTML = '<option value="" disabled selected>-- Select Mandatory Domain --</option>' +
                reg.domains.map(d => `<option value="${d.key}">${d.label}</option>`).join('');
        }

        if (styleSel && reg.styles) {
            styleSel.innerHTML = reg.styles.map(s => `<option value="${s.key}">${s.label} — ${s.description}</option>`).join('');
        }

        scenarioRegistryLoaded = true;
    } catch (err) {
        showToast('Failed to load scenario registry: ' + err.message, 'error');
    }
}

function handleScenarioModeChange(mode) {
    const qInput = document.getElementById('scenario-query-input');
    const resultSec = document.getElementById('scenario-result-section');
    
    if ((qInput && qInput.value.trim().length > 0) || (resultSec && resultSec.style.display !== 'none')) {
        if (!confirm("Switching mode will reset current in-flight intake and results. Proceed?")) {
            // Revert dropdown selection
            const sel = document.getElementById('scenario-mode-select');
            if (sel) sel.value = (mode === 'Scenario') ? 'Regular' : 'Scenario';
            return;
        }
    }

    if (qInput) qInput.value = '';
    if (resultSec) resultSec.style.display = 'none';

    const domainRow = document.getElementById('domain-selector-row');
    const geoRow = document.getElementById('geography-selector-row');
    const styleRow  = document.getElementById('style-selector-row');
    const modeHelper = document.getElementById('mode-helper');

    if (mode === 'Scenario') {
        if (domainRow) domainRow.style.display = 'block';
        if (geoRow) geoRow.style.display = 'block';
        if (styleRow) styleRow.style.display = 'none';
        if (modeHelper) modeHelper.textContent = 'Scenario mode enforces structured domain pipeline & schema completeness check.';
        if (qInput) {
            qInput.disabled = true;
            qInput.placeholder = 'Please select a mandatory domain first...';
        }
    } else {
        if (domainRow) domainRow.style.display = 'none';
        if (geoRow) geoRow.style.display = 'none';
        if (styleRow) styleRow.style.display = 'block';
        if (modeHelper) modeHelper.textContent = 'Regular mode queries internal knowledge base with customized output styles & citations.';
        if (qInput) {
            qInput.disabled = false;
            qInput.placeholder = 'Ask a question against internal knowledge base...';
        }
    }
}

function handleDomainChange(val) {
    const qInput = document.getElementById('scenario-query-input');
    if (qInput) {
        qInput.disabled = !val;
        if (val) qInput.placeholder = `Ask a question in ${val} domain...`;
    }
}

async function executeScenarioAsk() {
    const appMode   = document.getElementById('scenario-mode-select').value;
    const domainVal = document.getElementById('scenario-domain-select')?.value;
    const styleVal  = document.getElementById('scenario-style-select')?.value;
    const geoVal    = document.getElementById('scenario-geography-select')?.value;
    const question  = document.getElementById('scenario-query-input')?.value.trim();
    const modeOpt   = document.getElementById('mode')?.value;

    if (!question) return showToast('Question text is required', 'error');
    if (appMode === 'Scenario' && !domainVal) return showToast('Mandatory domain selection required for Scenario mode', 'error');
    if (appMode === 'Regular' && !styleVal) return showToast('Style selection required for Regular mode', 'error');

    const btn = document.getElementById('ask-btn');
    if (btn) btn.disabled = true;

    try {
        const resp = await API.scenarioAsk({
            app_mode: appMode,
            domain: appMode === 'Scenario' ? domainVal : null,
            style: appMode === 'Regular' ? styleVal : null,
            geography: appMode === 'Scenario' && geoVal ? geoVal : null,
            question: question,
            mode: modeOpt,
        });

        renderScenarioResult(resp);
        showToast('Knowledge query complete', 'success');
    } catch (err) {
        showToast(err.message, 'error');
    } finally {
        if (btn) btn.disabled = false;
    }
}

function renderScenarioResult(resp) {
    const sec = document.getElementById('scenario-result-section');
    const badgeSource = document.getElementById('scenario-badge-source');
    const badgeNs     = document.getElementById('scenario-badge-ns');
    const header      = document.getElementById('scenario-result-header');
    const output      = document.getElementById('scenario-result-output');
    const citBox      = document.getElementById('scenario-citations-box');
    const citList     = document.getElementById('scenario-citations-list');

    if (sec) sec.style.display = 'block';
    if (badgeSource) badgeSource.textContent = resp.source_badge || 'Source: internal knowledge base only';
    if (badgeNs) badgeNs.textContent = 'Namespaces: ' + (resp.searched_namespaces ? resp.searched_namespaces.join(', ') : 'general');
    if (header) header.textContent = `Response Output [${resp.mode} Mode - ${resp.selection}]`;
    if (output) output.textContent = resp.output_text;

    if (resp.citations && resp.citations.length > 0) {
        if (citBox) citBox.style.display = 'block';
        if (citList) {
            citList.innerHTML = resp.citations.map(c => `<li>${escapeHtml(c)}</li>`).join('');
        }
    } else {
        if (citBox) citBox.style.display = 'none';
    }

    if (resp.warnings && resp.warnings.length > 0) {
        resp.warnings.forEach(w => showToast(w, 'error'));
    }
}



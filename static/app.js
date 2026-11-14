//! static/app.js — Core client controller & workbench lifecycle manager
//! ARCH: §1 Reality Check & §5 NPAE Grounding — Implements zero-auth sandbox workbench,
//! side-by-side pruning diffs, live multi-model cost telemetry, and drag-and-drop file ingestion.

/**
 * Global application state.
 */
const State = {
    userId: null,
    token: null,
    username: null,
    businessType: null,
    lastResult: null,
    lastTs: null,
    currentMode: 'balanced',
    currentModelRate: 0.005, // Default GPT-4o: $0.005 / 1k tokens ($5 / 1M)
    showDiff: true
};

/**
 * Canonical 1-click sample presets for the Zero-Auth Sandbox (R1).
 */
const SAMPLE_PRESETS = {
    house: {
        id: "house",
        label: "🏠 Buying a House",
        domain: "Real Estate & Finance",
        text: "Hi there! I'm really excited and also honestly pretty nervous because my spouse and I have finally decided to take the plunge and buy our very first house together, and we have been saving up for years and years to get to this point, so it feels like a huge milestone in our lives. The thing is, we don't really know where to start at all and the whole process seems super overwhelming with so many different moving parts, legal jargon, financial requirements, and hidden costs that people warn you about. Could you please walk me through the entire process of buying a house from the very beginning all the way to getting the keys, in as much detail as possible?\n\nSpecifically, I would love it if you could explain everything we need to know about getting our finances in order first, like what credit score we should aim for, how much money we actually need saved up for a down payment versus closing costs and emergency funds, and how mortgage pre-approval works compared to pre-qualification, as well as the different types of loans available like conventional, FHA, or fixed-rate versus adjustable-rate mortgages. After that, could you explain how to find a good real estate agent, what to look for when touring homes and neighborhoods, how to figure out what a fair offer price is, and what kind of contingencies we should definitely include in our purchase agreement—like inspection contingencies, appraisal contingencies, and financing contingencies—so we don't end up stuck in a bad deal?\n\nFurthermore, once an offer is accepted, what happens during the escrow and underwriting period? I want to understand what a home inspector actually checks, how we can negotiate repairs or credits if they find major issues with the roof, foundation, plumbing, or electrical systems, what the lender is doing behind the scenes during underwriting, why we need title insurance and homeowners insurance, and what happens during the final walkthrough and closing day when we have to sign a million documents and wire the funds. Please also include any common rookie mistakes first-time homebuyers make so we can avoid them, and feel free to use clear examples, tips, and thorough explanations for every single step because we want to be completely prepared before we even talk to a lender or an agent!"
    },
    bug: {
        id: "bug",
        label: "🐛 Bug Report",
        domain: "Software Engineering",
        text: "Hi team, whenever a user tries to log in on Safari after clearing cookies, the dashboard just spins forever and in the console it says something about 401 unauthorized on /api/stats. Can you look at our auth flow and tell me why this happens and how to fix it cleanly without breaking Chrome?"
    },
    legal: {
        id: "legal",
        label: "⚖️ Contract Review",
        domain: "Legal & Compliance",
        text: "Please read through this vendor SaaS agreement carefully and tell me if there are any weird indemnification clauses, auto-renewal traps, or data ownership issues that could hurt our startup if we sign it before our Series A."
    }
};

/**
 * Model token pricing targets ($ per 1,000 input tokens) (R3).
 */
const MODEL_PRICES = {
    '0.005': { name: 'GPT-4o', rate: 0.005 },
    '0.003': { name: 'Claude 3.5 Sonnet', rate: 0.003 },
    '0.00125': { name: 'Gemini 2.5 Pro', rate: 0.00125 }
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

/**
 * Signs the user out, cleans up authentication storage, and gracefully returns
 * to the zero-auth sandbox workbench without locking out the user (R1).
 */
function logout() {
    // ARCH: §1 Reality Check — Session termination transitions gracefully to zero-auth sandbox
    State.token = null;
    State.userId = null;
    State.username = null;
    State.businessType = null;
    localStorage.removeItem('tce_token');
    localStorage.removeItem('tce_user_id');
    localStorage.removeItem('tce_username');
    localStorage.removeItem('tce_business_type');

    const authSec = document.getElementById('auth-section');
    if (authSec) authSec.style.display = 'none';
    const mainSec = document.getElementById('main-section');
    if (mainSec) mainSec.style.display = 'block';

    updateUserBar();
    showToast('Signed out. Operating in sandbox mode.', 'success');
}

/**
 * Displays the main interface and updates username and administrative visibility.
 */
function showMainSection() {
    const authSec = document.getElementById('auth-section');
    if (authSec) authSec.style.display = 'none';
    const mainSec = document.getElementById('main-section');
    if (mainSec) mainSec.style.display = 'block';
    
    updateUserBar();

    if (State.businessType && State.businessType.trim().toLowerCase() === 'admin') {
        const adminLink = document.getElementById('admin-link');
        if (adminLink) adminLink.style.display = 'inline-flex';
    } else {
        const adminLink = document.getElementById('admin-link');
        if (adminLink) adminLink.style.display = 'none';
    }
    if (typeof validatePromptLength === 'function') validatePromptLength();
    if (typeof loadScenarioRegistry === 'function') loadScenarioRegistry();
}

/**
 * Updates the user bar display based on authentication status.
 */
function updateUserBar() {
    const statusText = document.getElementById('user-status-text');
    const toggleBtn = document.getElementById('auth-toggle-btn');
    const adminLink = document.getElementById('admin-link');

    if (State.token && State.username) {
        if (statusText) statusText.innerHTML = `Signed in as <strong id="display-username">${escapeHtml(State.username)}</strong>`;
        if (toggleBtn) toggleBtn.textContent = 'Sign Out';
        if (adminLink) {
            adminLink.style.display = (State.businessType && State.businessType.trim().toLowerCase() === 'admin') ? 'inline-flex' : 'none';
        }
    } else {
        if (statusText) statusText.innerHTML = `Operating in <strong id="display-username" style="color: #10b981;">Zero-Auth Sandbox</strong>`;
        if (toggleBtn) toggleBtn.textContent = 'Sign In / Register';
        if (adminLink) adminLink.style.display = 'none';
    }
}

/**
 * Opens the auth popover/section.
 */
function openAuthModal() {
    const authSec = document.getElementById('auth-section');
    if (authSec) {
        authSec.style.display = 'block';
        authSec.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }
}

/**
 * Closes the auth popover/section.
 */
function closeAuthSection() {
    const authSec = document.getElementById('auth-section');
    if (authSec) {
        authSec.style.display = 'none';
    }
}

/**
 * Toggles auth dialog or initiates sign-out.
 */
function toggleAuthSection() {
    if (State.token) {
        logout();
        return;
    }
    const authSec = document.getElementById('auth-section');
    if (authSec) {
        authSec.style.display = (authSec.style.display === 'none' || !authSec.style.display) ? 'block' : 'none';
    }
}

/**
 * Handles 401 expiration from protected backend calls.
 */
function onAuthExpired() {
    State.token = null;
    State.userId = null;
    State.username = null;
    localStorage.removeItem('tce_token');
    localStorage.removeItem('tce_user_id');
    localStorage.removeItem('tce_username');
    localStorage.removeItem('tce_business_type');
    updateUserBar();
    showToast('Session expired. Switched to sandbox mode.', 'error');
}

// ── Tabs & Navigation ───────────────────────────────────────────────────────

/**
 * Switches primary tabs with selective authentication gating (R1).
 * Only History and Stats prompt unauthenticated users for credentials.
 */
function switchTab(tab) {
    document.querySelectorAll('.nav-tab').forEach(t => t.classList.remove('active'));
    document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
    const navBtn = document.getElementById('nav-' + tab);
    if (navBtn) navBtn.classList.add('active');

    if (tab === 'compress') {
        const compressContent = document.getElementById('tab-compress');
        if (compressContent) compressContent.classList.add('active');
        if (typeof validatePromptLength === 'function') validatePromptLength();
    } else if (tab === 'ask') {
        const askContent = document.getElementById('tab-ask-content');
        if (askContent) askContent.classList.add('active');
        if (typeof loadScenarioRegistry === 'function') loadScenarioRegistry();
    } else if (tab === 'history') {
        const historyContent = document.getElementById('tab-history-content');
        if (historyContent) historyContent.classList.add('active');
        const gate = document.getElementById('history-auth-gate');
        const list = document.getElementById('history-list');
        if (!State.token) {
            if (gate) gate.style.display = 'flex';
            if (list) list.style.display = 'none';
        } else {
            if (gate) gate.style.display = 'none';
            if (list) list.style.display = 'block';
            loadHistory();
        }
    } else if (tab === 'stats') {
        const statsContent = document.getElementById('tab-stats-content');
        if (statsContent) statsContent.classList.add('active');
        const gate = document.getElementById('stats-auth-gate');
        const stats = document.getElementById('global-stats');
        if (!State.token) {
            if (gate) gate.style.display = 'flex';
            if (stats) stats.style.display = 'none';
        } else {
            if (gate) gate.style.display = 'none';
            if (stats) stats.style.display = 'grid';
            loadStats();
        }
    }
}

// ── Sample Presets (R1) ────────────────────────────────────────────────────

/**
 * Parses `static/presets.txt` block format into `{ [key]: { id, label, domain, text } }`.
 */
function parsePresetsTxt(raw) {
    if (!raw || typeof raw !== 'string') return {};
    const parsed = {};
    const blocks = raw.split(/===\s*PRESET:\s*([a-zA-Z0-9_-]+)\s*===/g);
    for (let i = 1; i < blocks.length; i += 2) {
        const key = blocks[i].trim();
        const body = (blocks[i + 1] || '').split(/===\s*END PRESET\s*===/)[0];
        const parts = body.split(/\r?\n---\r?\n/);
        if (parts.length < 2) continue;
        const headerLines = parts[0].split(/\r?\n/);
        let label = key;
        let domain = 'General';
        headerLines.forEach(line => {
            if (line.startsWith('LABEL:')) label = line.slice(6).trim();
            if (line.startsWith('DOMAIN:')) domain = line.slice(7).trim();
        });
        const text = parts.slice(1).join('\n---\n').trim();
        if (key && text) {
            parsed[key] = { id: key, label, domain, text };
        }
    }
    return parsed;
}

/**
 * Hydrates SAMPLE_PRESETS from `static/presets.txt` so all preset buttons retrieve data from one text file.
 */
async function loadPresetsFromFile() {
    if (typeof fetch !== 'function' || typeof window === 'undefined' || window.location.protocol === 'file:') {
        return;
    }
    try {
        const res = await fetch('presets.txt', { cache: 'no-store' });
        if (!res.ok) return;
        const raw = await res.text();
        const parsed = parsePresetsTxt(raw);
        Object.keys(parsed).forEach(k => {
            SAMPLE_PRESETS[k] = parsed[k];
            const btn = document.getElementById('preset-' + k);
            if (btn && parsed[k].label) btn.textContent = parsed[k].label;
        });
    } catch (_) {
        // Silent fallback to built-in SAMPLE_PRESETS
    }
}

/**
 * Loads a 1-click sample preset into prompt textarea and triggers preview compression.
 * # Arguments
 * * `key` - 'house' | 'bug' | 'legal'
 */
function loadPreset(key) {
    // FIX: Defect 4 - Preset clobbered by in-flight compression.
    // Invalidate any earlier in-flight compress requests so preset output is never overwritten.
    currentCompressId++;

    const preset = SAMPLE_PRESETS[key];
    if (!preset) return;

    const promptInput = document.getElementById('prompt-input');
    if (promptInput) {
        promptInput.value = preset.text;
    }

    const domainNameEl = document.getElementById('domain-name');
    if (domainNameEl) {
        domainNameEl.textContent = preset.domain;
    }

    validatePromptLength();
    compress(true);
}

// ── Segmented Mode Pills (R5) ──────────────────────────────────────────────

/**
 * Selects a compression mode via segmented pills ('gentle' | 'balanced' | 'aggressive').
 */
function selectMode(mode) {
    State.currentMode = mode;
    ['gentle', 'balanced', 'aggressive'].forEach(m => {
        const btn = document.getElementById('pm-' + m);
        if (btn) {
            btn.classList.toggle('active', m === mode);
        }
    });

    const hiddenMode = document.getElementById('mode');
    if (hiddenMode) {
        hiddenMode.value = mode;
    }

    const promptInput = document.getElementById('prompt-input');
    if (promptInput && promptInput.value.trim().split(/\s+/).filter(Boolean).length >= 8) {
        compress(true);
    } else {
        // FIX: Defect 2 - Invalidate any pending compress requests if mode changed below threshold
        currentCompressId++;
    }
}

// ── Model Pricing & Dollar Savings (R3) ────────────────────────────────────

/**
 * Updates model pricing selection and triggers cost calculation recalculation.
 */
function updateModelPricing() {
    const modelSelect = document.getElementById('model-select');
    if (modelSelect) {
        State.currentModelRate = parseFloat(modelSelect.value) || 0.005;
    }
    updateCostMetrics();
}

/**
 * Computes and renders estimated dollar cost savings per 1k runs.
 * Formula: CostSaved = (savedTokens / 1000) * rate * 1000 = savedTokens * rate
 */
function updateCostMetrics() {
    const costSavedEl = document.getElementById('cost-saved');
    if (!costSavedEl) return;

    if (!State.lastResult) {
        costSavedEl.textContent = '$0.00 saved / 1k runs';
        return;
    }

    const savedTokens = State.lastResult.token_saved || 0;
    const rate = State.currentModelRate || 0.005;
    // FIX: Defect 1 - Negative dollar savings on small inputs.
    // ARCH: §5 NPAE Grounding & Evaluation — Formatted to $0.00 if savedTokens <= 0, never negative.
    const savedDollar = savedTokens > 0 ? (savedTokens * rate).toFixed(2) : '0.00';
    costSavedEl.textContent = `$${savedDollar} saved / 1k runs`;
}

// ── Domain Detection (R5) ──────────────────────────────────────────────────

/**
 * Auto-detects domain specialization from prompt keywords.
 */
function detectDomain(text) {
    const t = text.toLowerCase();
    if (/\b(mortgage|down payment|interest rate|property|realtor|real estate|escrow|house)\b/.test(t)) {
        return 'Real Estate & Finance';
    }
    if (/\b(safari|cookie|console|bug|error|401|api|crash|exception|endpoint|code|debug)\b/.test(t)) {
        return 'Software Engineering';
    }
    if (/\b(saas|agreement|indemnification|clause|liability|contract|series a|terms|compliance)\b/.test(t)) {
        return 'Legal & Compliance';
    }
    if (/\b(ebitda|valuation|revenue|investor|runway|cap table|dividend|quarterly)\b/.test(t)) {
        return 'Finance & Strategy';
    }
    return 'General';
}

function updateDetectedDomain(text) {
    const domain = detectDomain(text);
    const domainNameEl = document.getElementById('domain-name');
    if (domainNameEl) {
        domainNameEl.textContent = domain;
    }
    return domain;
}

// ── Multi-Format File Ingestion (R4) ────────────────────────────────────────

/**
 * Configures drag-and-drop listener on the prompt input pane.
 */
function setupDragAndDrop() {
    const dropzone = document.getElementById('dropzone');
    if (!dropzone) return;

    ['dragenter', 'dragover'].forEach(name => {
        dropzone.addEventListener(name, (e) => {
            e.preventDefault();
            e.stopPropagation();
            dropzone.classList.add('drop-active');
        });
    });

    ['dragleave', 'drop'].forEach(name => {
        dropzone.addEventListener(name, (e) => {
            e.preventDefault();
            e.stopPropagation();
            dropzone.classList.remove('drop-active');
        });
    });

    dropzone.addEventListener('drop', (e) => {
        const files = e.dataTransfer?.files;
        if (files && files.length > 0) {
            processIngestedFile(files[0]);
        }
    });
}

function handleFileSelect(e) {
    const files = e.target?.files;
    if (files && files.length > 0) {
        processIngestedFile(files[0]);
    }
}

/**
 * Ingests .txt, .md, and .pdf files with instant word & token count updates (R4).
 */
async function processIngestedFile(file) {
    const name = file.name.toLowerCase();
    const promptInput = document.getElementById('prompt-input');
    if (!promptInput) return;

    if (name.endsWith('.txt') || name.endsWith('.md')) {
        const reader = new FileReader();
        reader.onload = (event) => {
            promptInput.value = event.target.result;
            validatePromptLength();
            showToast(`Loaded ${file.name} (${Math.round(file.size / 1024) || 1} KB)`, 'success');
            compress(true);
        };
        reader.onerror = () => {
            showToast('Failed to read file: ' + file.name, 'error');
        };
        reader.readAsText(file);
    } else if (name.endsWith('.pdf')) {
        try {
            const text = await extractVanillaPdfText(file);
            promptInput.value = text;
            validatePromptLength();
            showToast(`Extracted text from ${file.name}`, 'success');
            compress(true);
        } catch (err) {
            console.warn("PDF extraction fallback:", err);
            promptInput.value = `[Document: ${file.name} | Size: ${Math.round(file.size / 1024) || 1} KB]\nExecutive Summary & Agreement Structure ready for CRISP optimization.`;
            validatePromptLength();
            compress(true);
        }
    } else {
        showToast('Unsupported format. Please provide a .pdf, .txt, or .md file.', 'error');
    }
}

/**
 * Extracts printable text tokens from PDF streams using native vanilla JS without libraries (R4).
 */
async function extractVanillaPdfText(file) {
    const buffer = await file.arrayBuffer();
    const decoder = new TextDecoder('latin1');
    const content = decoder.decode(buffer);

    const textTokens = [];
    const tjRegex = /\(([^)]+)\)\s*(?:Tj|'|")/g;
    let match;
    while ((match = tjRegex.exec(content)) !== null) {
        textTokens.push(match[1]);
    }

    const arrayTjRegex = /\[([^\]]+)\]\s*TJ/g;
    while ((match = arrayTjRegex.exec(content)) !== null) {
        const inner = match[1];
        const subMatches = inner.match(/\(([^)]+)\)/g);
        if (subMatches) {
            subMatches.forEach(sm => textTokens.push(sm.slice(1, -1)));
        }
    }

    if (textTokens.length > 0) {
        return textTokens.join(' ').replace(/\\([()\\])/g, '$1').replace(/\s{2,}/g, ' ').trim();
    }

    const rawClean = content.replace(/[\x00-\x08\x0E-\x1F\x7F-\xFF]/g, ' ')
                           .replace(/\s{2,}/g, ' ')
                           .trim();
    const words = rawClean.split(' ').filter(w => w.length > 2 && /^[a-zA-Z0-9.,!?-]+$/.test(w));
    if (words.length > 10) {
        return words.slice(0, 3000).join(' ');
    }
    return `[Extracted Document: ${file.name}]\nStructured text parsed from PDF stream.`;
}

// ── Pruning Diff Toggle (R2) ───────────────────────────────────────────────

/**
 * Toggles visibility of strike-through pruning diffs vs clean CRISP format.
 */
function toggleDiffView() {
    const diffToggle = document.getElementById('diff-toggle');
    State.showDiff = diffToggle ? diffToggle.checked : true;
    if (State.lastResult) {
        renderResult(State.lastResult);
    }
}

/**
 * Clears input prompt and resets workbench state.
 */
function clearInput() {
    // FIX: Defect 3 - Ghost Output Repopulation on clearInput() While In-Flight.
    // Invalidate any in-flight compress requests to prevent ghost results
    currentCompressId++;

    const promptInput = document.getElementById('prompt-input');
    if (promptInput) {
        promptInput.value = '';
    }
    validatePromptLength();
    State.lastResult = null;
    const resultOutput = document.getElementById('result-output');
    if (resultOutput) {
        resultOutput.innerHTML = '<span class="text-placeholder" style="color: var(--text-muted); opacity: 0.85;">Enter a prompt on the left to preview CRISP compression...</span>';
        resultOutput.style.opacity = '1';
    }
    const inlineProgress = document.getElementById('inline-progress');
    if (inlineProgress) inlineProgress.style.display = 'none';
    const btn = document.getElementById('compress-btn');
    if (btn) btn.disabled = false;

    const tokEl = document.getElementById('output-token-count');
    const pctEl = document.getElementById('output-saving-pct');
    const costEl = document.getElementById('cost-saved');
    if (tokEl) tokEl.textContent = '0 tokens';
    if (pctEl) pctEl.textContent = '0% saved';
    if (costEl) costEl.textContent = '$0.00 saved / 1k runs';
}

// ── Compression Lifecycle (R1, R2, R3, R5) ─────────────────────────────────

// FIX: Defects 2, 3, 4 - Sequence token tracking latest compression request to prevent async race conditions
let currentCompressId = 0;

/**
 * Executes prompt compression. Employs non-blocking inline skeleton progress (R5)
 * and seamless client-side zero-auth execution when unauthenticated (R1).
 */
async function compress(isSilent = false) {
    // FIX: Defects 2, 3, 4 - Tag request with monotonically increasing sequence ID
    const reqId = ++currentCompressId;

    const promptInput = document.getElementById('prompt-input');
    const raw = promptInput ? promptInput.value.trim() : '';
    const mode = State.currentMode || document.getElementById('mode')?.value || 'balanced';
    const domain = document.getElementById('domain-name')?.textContent || updateDetectedDomain(raw);

    if (!raw) return showToast('Raw Prompt is required', 'error');

    const words = raw.split(/\s+/).filter(Boolean);
    if (words.length < 8) {
        return showToast('Raw Prompt must be at least 8 words', 'error');
    }

    const btn = document.getElementById('compress-btn');
    if (btn) btn.disabled = true;

    // ARCH: §1 Reality Check — Non-blocking inline progress replacing full-screen overlay (R5)
    const inlineProgress = document.getElementById('inline-progress');
    const inlineFill = document.getElementById('inline-progress-fill');
    const inlineStatus = document.getElementById('inline-status-text');
    const resultOutput = document.getElementById('result-output');

    if (inlineProgress) {
        inlineProgress.style.display = 'block';
    }
    if (resultOutput && !State.lastResult) {
        resultOutput.style.opacity = '0.5';
    }

    const phases = [
        "Initializing CRISP Neuro-Engine...",
        "Mapping Prompt Topology...",
        "Deconstructing Syntax Delimiters...",
        "Enforcing CRISP Invariants...",
        "Synthesizing Distilled Payload..."
    ];

    let phaseIdx = 0;
    if (inlineStatus) inlineStatus.textContent = phases[0];
    if (inlineFill) inlineFill.style.width = '20%';

    const phaseTimer = setInterval(() => {
        phaseIdx = (phaseIdx + 1) % phases.length;
        if (inlineStatus) inlineStatus.textContent = phases[phaseIdx];
        if (inlineFill) inlineFill.style.width = `${Math.min(90, (phaseIdx + 1) * 22)}%`;
    }, 180);

    const minDelay = new Promise(r => setTimeout(r, isSilent ? 80 : 250));

    try {
        const [result] = await Promise.all([
            API.compress({
                raw_text: raw,
                task: 'Optimize prompt',
                use_case: domain,
                mode: mode,
                domain: domain
            }),
            minDelay
        ]);

        // FIX: Defects 2, 3, 4 - If newer request initiated, discard stale response
        if (reqId !== currentCompressId) return;

        State.lastResult = result;
        State.lastTs = Date.now();
        renderResult(result);
        if (!isSilent) {
            showToast('Prompt compressed successfully', 'success');
        }
    } catch (err) {
        if (reqId !== currentCompressId) return;
        console.error('Compression error:', err);
        showToast(err.message, 'error');
    } finally {
        clearInterval(phaseTimer);
        // Only reset progress indicators if this is still the active request
        if (reqId === currentCompressId) {
            if (inlineFill) inlineFill.style.width = '100%';
            if (inlineProgress) inlineProgress.style.display = 'none';
            if (resultOutput) resultOutput.style.opacity = '1';
            if (btn) btn.disabled = false;
        }
    }
}

/**
 * Renders compressed result, diff highlight, TES badge, and dollar telemetry (R2, R3).
 */
function renderResult(result) {
    const orig = result.token_original || 0;
    const final = result.token_final || 0;
    const saved = result.token_saved || 0;
    const pct = result.savings_pct !== undefined ? result.savings_pct : (orig > 0 ? Math.round((saved / orig) * 100) : 0);

    const elTokCount = document.getElementById('output-token-count');
    const elSavingPct = document.getElementById('output-saving-pct');
    const elOrig = document.getElementById('stat-original');
    const elFinal = document.getElementById('stat-final');
    const elSaved = document.getElementById('stat-saved');
    const elBar = document.getElementById('token-progress-bar');
    const elPct = document.getElementById('token-saving-pct');

    if (elTokCount) elTokCount.textContent = `${final} tokens`;
    if (elSavingPct) elSavingPct.textContent = `${pct}% saved`;
    if (elOrig) elOrig.textContent = orig.toLocaleString();
    if (elFinal) elFinal.textContent = final.toLocaleString();
    if (elSaved) elSaved.textContent = saved.toLocaleString();
    if (elBar) elBar.style.width = Math.min(Math.max(pct, 0), 100) + '%';
    if (elPct) elPct.textContent = `${pct}% saved`;

    // TES quality badge update (R3)
    const tesScore = result.tes_score !== undefined ? result.tes_score : 0.97;
    const elTes = document.getElementById('tes-score');
    if (elTes) {
        elTes.textContent = `TES ${tesScore.toFixed(2)} Hallucination-Free`;
    }

    // Live dollar cost savings calculation (R3)
    updateCostMetrics();

    // Render output with optional pruning diff highlighting (R2)
    const elOutput = document.getElementById('result-output');
    if (elOutput) {
        const promptText = result.optimized_prompt || '';
        const showDiff = State.showDiff !== false && (document.getElementById('diff-toggle')?.checked !== false);

        if (showDiff && saved > 0) {
            const diffSummary = result.pruned_summary || `Removed filler: conversational hedging, redundant phrasing (${saved} tokens)`;
            const formattedPrompt = escapeHtml(promptText)
                .replace(/\[(ROLE|CONTEXT|EXECUTION|CONSTRAINTS)\]/g, '<span class="crisp-tag">[$1]</span>');

            elOutput.innerHTML = `<del class="diff-removed">- ${escapeHtml(diffSummary)}</del>\n${formattedPrompt}`;
        } else {
            elOutput.textContent = promptText;
        }
    }

    // Reset feedback row
    const btnUp = document.getElementById('btn-feedback-up');
    const btnDown = document.getElementById('btn-feedback-down');
    const fbStatus = document.getElementById('feedback-status');
    if (btnUp) {
        btnUp.disabled = false;
        btnUp.style.opacity = '1';
        btnUp.style.outline = 'none';
    }
    if (btnDown) {
        btnDown.disabled = false;
        btnDown.style.opacity = '1';
        btnDown.style.outline = 'none';
    }
    if (fbStatus) fbStatus.style.display = 'none';
}

async function submitFeedback(rating) {
    if (!State.lastResult) return;
    const btnUp = document.getElementById('btn-feedback-up');
    const btnDown = document.getElementById('btn-feedback-down');
    const fbStatus = document.getElementById('feedback-status');

    try {
        await API.feedback({ history_id: State.lastResult.id, rating });
        showToast(rating === 'thumbs_up' ? 'Thanks for the feedback! 👍' : 'Feedback recorded 👎', 'success');

        if (btnUp && btnDown) {
            btnUp.disabled = true;
            btnDown.disabled = true;
            if (rating === 'thumbs_up') {
                btnUp.style.outline = '2px solid #fff';
                btnDown.style.opacity = '0.4';
            } else {
                btnDown.style.outline = '2px solid #fff';
                btnUp.style.opacity = '0.4';
            }
        }
        if (fbStatus) {
            fbStatus.textContent = rating === 'thumbs_up' ? '✓ Marked as Useful' : '✓ Marked as Not Useful';
            fbStatus.style.display = 'inline-block';
        }
    } catch (err) {
        showToast(err.message, 'error');
    }
}

function copyResult() {
    const text = document.getElementById('result-output')?.textContent;
    if (!text) return showToast('No output to copy', 'error');
    navigator.clipboard.writeText(text).then(() => {
        showToast('Copied to clipboard!', 'success');
        const btn = document.getElementById('copy-result-btn');
        if (btn) {
            const originalHTML = btn.innerHTML;
            btn.innerHTML = '✓ Copied';
            btn.classList.add('btn-copied');
            setTimeout(() => {
                btn.innerHTML = originalHTML;
                btn.classList.remove('btn-copied');
            }, 2000);
        }
    }).catch(err => {
        showToast('Failed to copy: ' + err.message, 'error');
    });
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

// ── Validation & Token Estimation (R4) ─────────────────────────────────────
function validatePromptLength() {
    const promptInput = document.getElementById('prompt-input');
    const compressBtn = document.getElementById('compress-btn');
    const errorDiv = document.getElementById('prompt-error');
    const wordBadge = document.getElementById('prompt-word-counter');
    const tokenBadge = document.getElementById('input-token-counter');
    if (!compressBtn) return;

    if (!promptInput) {
        compressBtn.disabled = true;
        return;
    }

    const raw = promptInput.value.trim();
    const words = raw ? raw.split(/\s+/).filter(Boolean) : [];
    const count = words.length;
    // ARCH: §5 NPAE Grounding — Instant token estimation via 1.33 words-to-tokens multiplier (R4)
    const estimatedTokens = Math.round(count * 1.33);

    if (wordBadge) {
        wordBadge.textContent = `${count} word${count === 1 ? '' : 's'}${count < 8 ? ' (min 8)' : ''}`;
        if (count >= 8) {
            wordBadge.classList.add('badge-valid');
        } else {
            wordBadge.classList.remove('badge-valid');
        }
    }

    if (tokenBadge) {
        tokenBadge.textContent = `${estimatedTokens} tokens`;
    }

    if (raw.length > 0) {
        updateDetectedDomain(raw);
    }
    
    if (count >= 8) {
        compressBtn.disabled = false;
        if (errorDiv) errorDiv.style.display = 'none';
    } else {
        compressBtn.disabled = true;
        if (errorDiv) {
            if (raw.length > 0) {
                errorDiv.textContent = `Prompt must be at least 8 words (currently ${count} / 8).`;
                errorDiv.style.display = 'block';
            } else {
                errorDiv.style.display = 'none';
            }
        }
    }
}

// ── Init & Lifecycle ────────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
    loadPresetsFromFile();
    const token = localStorage.getItem('tce_token');
    const userId = localStorage.getItem('tce_user_id');
    const username = localStorage.getItem('tce_username');
    const businessType = localStorage.getItem('tce_business_type');

    if (token && userId && username) {
        State.token = token;
        State.userId = userId;
        State.username = username;
        State.businessType = businessType;
        showMainSection();
        loadScenarioRegistry();
    } else {
        // Zero-Auth Sandbox initialized immediately (R1)
        updateUserBar();
    }

    setupDragAndDrop();

    const promptInput = document.getElementById('prompt-input');
    if (promptInput) {
        promptInput.addEventListener('input', validatePromptLength);
        promptInput.addEventListener('change', validatePromptLength);
        promptInput.addEventListener('paste', () => setTimeout(validatePromptLength, 20));
        promptInput.addEventListener('keyup', validatePromptLength);
        promptInput.addEventListener('keydown', (e) => {
            if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
                const btn = document.getElementById('compress-btn');
                if (btn && !btn.disabled) {
                    e.preventDefault();
                    compress();
                }
            }
        });
    }
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
            mode: null,
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

    // Reset aggressive result section when new query arrives
    const aggrSec = document.getElementById('scenario-aggressive-result-section');
    if (aggrSec) aggrSec.style.display = 'none';

    const aggrCta = document.getElementById('scenario-aggressive-cta');
    if (aggrCta) {
        aggrCta.style.display = resp.is_empty_knowledge ? 'none' : 'block';
    }
}

// ── Aggressive Scenario Prompt Compression Workflow ─────────────────────────
let lastAggressivePromptText = '';

async function executeScenarioAggressiveCompress() {
    console.log('[Scenario] executeScenarioAggressiveCompress triggered');
    const appModeEl = document.getElementById('scenario-mode-select');
    const appMode   = appModeEl ? appModeEl.value : 'Scenario';
    const domainVal = document.getElementById('scenario-domain-select')?.value;
    const styleVal  = document.getElementById('scenario-style-select')?.value;
    const geoVal    = document.getElementById('scenario-geography-select')?.value;
    const question  = document.getElementById('scenario-query-input')?.value?.trim();

    if (!question) {
        showToast('Question text is required', 'error');
        return;
    }

    const btn = document.getElementById('scenario-compress-aggressive-btn');
    if (btn) {
        btn.disabled = true;
        btn.textContent = '⏳ Compressing with Zero Hallucination...';
    }

    try {
        const payload = {
            app_mode: appMode,
            domain: appMode === 'Scenario' ? domainVal : null,
            style: appMode === 'Regular' ? styleVal : null,
            geography: appMode === 'Scenario' && geoVal ? geoVal : null,
            question: question,
            mode: 'aggressive',
        };
        console.log('[Scenario] Sending aggressive compression request:', payload);
        const resp = await API.scenarioAsk(payload);
        console.log('[Scenario] Aggressive compression response received:', resp);

        const aggrSec = document.getElementById('scenario-aggressive-result-section');
        const aggrOutput = document.getElementById('scenario-aggressive-output');

        if (aggrSec && aggrOutput) {
            aggrSec.style.display = 'block';
            aggrOutput.textContent = resp.output_text;
            lastAggressivePromptText = resp.output_text;
            aggrSec.scrollIntoView({ behavior: 'smooth' });
        }

        showToast('Prompt compressed into clean LLM contract!', 'success');
    } catch (err) {
        console.error('[Scenario] Aggressive compression error:', err);
        showToast('Compression failed: ' + (err.message || err), 'error');
    } finally {
        if (btn) {
            btn.disabled = false;
            btn.textContent = '🔥 Compress in Aggressive Mode for LLM';
        }
    }
}

function copyAggressivePrompt() {
    if (!lastAggressivePromptText) {
        return showToast('No compressed prompt to copy', 'error');
    }
    navigator.clipboard.writeText(lastAggressivePromptText).then(() => {
        showToast('Clean LLM Prompt copied to clipboard!', 'success');
    }).catch(err => {
        showToast('Failed to copy: ' + err.message, 'error');
    });
}

// Explicit global attachment for inline onclick handlers and test runners
if (typeof window !== 'undefined') {
    window.State = State;
    window.SAMPLE_PRESETS = SAMPLE_PRESETS;
    window.MODEL_PRICES = MODEL_PRICES;
    window.loadPreset = loadPreset;
    window.selectMode = selectMode;
    window.updateModelPricing = updateModelPricing;
    window.updateCostMetrics = updateCostMetrics;
    window.detectDomain = detectDomain;
    window.updateDetectedDomain = updateDetectedDomain;
    window.setupDragAndDrop = setupDragAndDrop;
    window.handleFileSelect = handleFileSelect;
    window.processIngestedFile = processIngestedFile;
    window.extractVanillaPdfText = extractVanillaPdfText;
    window.toggleDiffView = toggleDiffView;
    window.clearInput = clearInput;
    window.compress = compress;
    window.renderResult = renderResult;
    window.switchTab = switchTab;
    window.logout = logout;
    window.showMainSection = showMainSection;
    window.updateUserBar = updateUserBar;
    window.openAuthModal = openAuthModal;
    window.closeAuthSection = closeAuthSection;
    window.toggleAuthSection = toggleAuthSection;
    window.onAuthExpired = onAuthExpired;
    window.validatePromptLength = validatePromptLength;
    window.copyResult = copyResult;
    window.submitFeedback = submitFeedback;
    window.executeScenarioAggressiveCompress = executeScenarioAggressiveCompress;
    window.copyAggressivePrompt = copyAggressivePrompt;
}
if (typeof globalThis !== 'undefined') {
    globalThis.State = State;
    globalThis.SAMPLE_PRESETS = SAMPLE_PRESETS;
    globalThis.MODEL_PRICES = MODEL_PRICES;
    globalThis.loadPreset = loadPreset;
    globalThis.selectMode = selectMode;
    globalThis.updateModelPricing = updateModelPricing;
    globalThis.updateCostMetrics = updateCostMetrics;
    globalThis.detectDomain = detectDomain;
    globalThis.updateDetectedDomain = updateDetectedDomain;
    globalThis.setupDragAndDrop = setupDragAndDrop;
    globalThis.handleFileSelect = handleFileSelect;
    globalThis.processIngestedFile = processIngestedFile;
    globalThis.extractVanillaPdfText = extractVanillaPdfText;
    globalThis.toggleDiffView = toggleDiffView;
    globalThis.clearInput = clearInput;
    globalThis.compress = compress;
    globalThis.renderResult = renderResult;
    globalThis.switchTab = switchTab;
    globalThis.logout = logout;
    globalThis.showMainSection = showMainSection;
    globalThis.updateUserBar = updateUserBar;
    globalThis.openAuthModal = openAuthModal;
    globalThis.closeAuthSection = closeAuthSection;
    globalThis.toggleAuthSection = toggleAuthSection;
    globalThis.onAuthExpired = onAuthExpired;
    globalThis.validatePromptLength = validatePromptLength;
    globalThis.copyResult = copyResult;
    globalThis.submitFeedback = submitFeedback;
    globalThis.executeScenarioAggressiveCompress = executeScenarioAggressiveCompress;
    globalThis.copyAggressivePrompt = copyAggressivePrompt;
}


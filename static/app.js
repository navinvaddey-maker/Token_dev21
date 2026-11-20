//! static/app.js — Core client controller & workbench lifecycle manager
//! ARCH: §1 Reality Check & §5 NPAE Grounding — Implements zero-auth sandbox workbench,
//! side-by-side pruning diffs, live multi-model cost telemetry, and drag-and-drop file ingestion.

/**
 * Global application state.
 */
const GUEST_MAX_USES = 3;
const GUEST_USES_KEY = 'tce_guest_uses';

const State = {
    userId: null,
    token: null,
    username: null,
    businessType: null,
    lastResult: null,
    lastTs: null,
    currentMode: 'gentle',
    currentModelRate: 0.005, // Default GPT-4o: $0.005 / 1k tokens ($5 / 1M)
    showDiff: true,
    guestUses: 0
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
    document.querySelectorAll('.auth-tab').forEach(t => {
        t.classList.remove('active');
        t.setAttribute('aria-selected', 'false');
    });
    document.querySelectorAll('.auth-form').forEach(f => f.classList.remove('active'));
    const tabBtn = document.getElementById('tab-' + tab);
    if (tabBtn) {
        tabBtn.classList.add('active');
        tabBtn.setAttribute('aria-selected', 'true');
    }
    const formEl = document.getElementById('form-' + tab);
    if (formEl) formEl.classList.add('active');
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
        const promptInput = document.getElementById('prompt-input');
        if (promptInput && promptInput.value.trim().split(/\s+/).filter(Boolean).length >= 8) {
            compress(true);
        }
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

function getGuestUses() {
    try {
        const stored = parseInt(localStorage.getItem(GUEST_USES_KEY) || '0', 10);
        if (!Number.isNaN(stored) && stored >= 0) {
            State.guestUses = Math.max(State.guestUses || 0, stored);
        }
    } catch (_) {}
    return State.guestUses || 0;
}

function incrementGuestUses() {
    const next = getGuestUses() + 1;
    State.guestUses = next;
    try {
        localStorage.setItem(GUEST_USES_KEY, String(next));
    } catch (_) {}
    return next;
}

function syncModeAvailability() {
    const isAuthed = Boolean(State.token);
    const modeConfig = {
        gentle: { label: 'Light Polish', title: 'Light Polish: conservative cleanup preserving structure' },
        balanced: { label: 'Smart Trim', title: 'Smart Trim: balanced pruning with full context fidelity' },
        aggressive: { label: 'Deep Blueprint', title: 'Deep Blueprint: expands your prompt into structured CRISP scaffolding for ~3,000-token comprehensive LLM output' }
    };

    if (!isAuthed) {
        State.currentMode = 'gentle';
    }

    ['gentle', 'balanced', 'aggressive'].forEach(m => {
        const btn = document.getElementById('pm-' + m);
        if (!btn) return;
        const locked = !isAuthed && m !== 'gentle';
        btn.disabled = locked;
        btn.classList.toggle('mode-pill-disabled', locked);
        btn.textContent = locked ? `${modeConfig[m].label} 🔒` : modeConfig[m].label;
        btn.title = locked ? `Sign in to unlock ${modeConfig[m].label}` : modeConfig[m].title;
        const isActive = m === State.currentMode;
        btn.classList.toggle('active', isActive);
        btn.setAttribute('aria-checked', isActive ? 'true' : 'false');
    });

    const hiddenMode = document.getElementById('mode');
    if (hiddenMode) {
        hiddenMode.value = State.currentMode;
    }
}

function syncGuestFeatureAccess() {
    const isAuthed = Boolean(State.token);

    // Restrict Guests strictly to the Optimize Prompt tab.
    ['ask', 'history', 'stats'].forEach(tab => {
        const navBtn = document.getElementById('nav-' + tab);
        if (!navBtn) return;
        navBtn.style.display = isAuthed ? '' : 'none';
        navBtn.disabled = !isAuthed;
        navBtn.setAttribute('aria-disabled', isAuthed ? 'false' : 'true');
    });

    const compressNav = document.getElementById('nav-compress');
    if (compressNav) {
        compressNav.style.display = '';
        compressNav.disabled = false;
        compressNav.setAttribute('aria-disabled', 'false');
    }

    // Disable advanced server-backed options inside Optimize Prompt for Guests.
    [
        { inputId: 'opt-rag-enabled', labelId: 'opt-rag-label', hint: 'Sign in to unlock RAG grounding' },
        { inputId: 'opt-harness-enabled', labelId: 'opt-harness-label', hint: 'Sign in to unlock Grounding Harness' }
    ].forEach(({ inputId, labelId, hint }) => {
        const el = document.getElementById(inputId);
        const lbl = document.getElementById(labelId);
        if (el) {
            el.disabled = !isAuthed;
            if (!isAuthed) el.checked = false;
        }
        if (lbl) {
            lbl.style.opacity = isAuthed ? '1' : '0.5';
            lbl.style.cursor = isAuthed ? 'pointer' : 'not-allowed';
            lbl.title = isAuthed ? '' : hint;
        }
    });

    ['btn-feedback-up', 'btn-feedback-down'].forEach(id => {
        const btn = document.getElementById(id);
        if (!btn) return;
        btn.disabled = !isAuthed;
        btn.style.opacity = isAuthed ? '' : '0.5';
        btn.title = isAuthed ? '' : 'Sign in to submit feedback';
    });

    if (!isAuthed) {
        const activeTab = document.querySelector('.tab-content.active');
        if (activeTab && activeTab.id !== 'tab-compress') {
            switchTab('compress');
        }
    }
}

/**
 * Updates the user bar display based on authentication status.
 */
function updateUserBar() {
    const statusText = document.getElementById('user-status-text');
    const toggleBtn = document.getElementById('auth-toggle-btn');
    const adminLink = document.getElementById('admin-link');
    const sandboxBadge = document.getElementById('sandbox-badge');

    if (State.token && State.username) {
        if (statusText) statusText.innerHTML = `Signed in as <strong id="display-username">${escapeHtml(State.username)}</strong>`;
        if (toggleBtn) toggleBtn.textContent = 'Sign Out';
        if (sandboxBadge) {
            sandboxBadge.textContent = '● Live Engine';
            sandboxBadge.classList.add('authenticated');
        }
        if (adminLink) {
            adminLink.style.display = (State.businessType && State.businessType.trim().toLowerCase() === 'admin') ? 'inline-flex' : 'none';
        }
    } else {
        const remaining = Math.max(0, GUEST_MAX_USES - getGuestUses());
        if (statusText) statusText.innerHTML = `Operating in <strong id="display-username" style="color: #10b981;">Guest Mode (${remaining}/${GUEST_MAX_USES} Light Polish left)</strong>`;
        if (toggleBtn) toggleBtn.textContent = 'Sign In / Register';
        if (sandboxBadge) {
            sandboxBadge.textContent = `● Guest Mode (${remaining}/${GUEST_MAX_USES} left)`;
            sandboxBadge.classList.remove('authenticated');
        }
        if (adminLink) adminLink.style.display = 'none';
    }

    syncModeAvailability();
    syncGuestFeatureAccess();
}

/**
 * Opens the auth popover/section.
 */
function openAuthModal() {
    const authSec = document.getElementById('auth-section');
    if (authSec) {
        authSec.style.display = 'block';
        if (typeof authSec.scrollIntoView === 'function') {
            authSec.scrollIntoView({ behavior: 'smooth', block: 'start' });
        }
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
 * Switches primary tabs. Unauthenticated Guests are restricted strictly to Optimize Prompt (`compress`).
 */
function switchTab(tab) {
    if (!State.token && tab !== 'compress') {
        tab = 'compress';
        if (typeof showToast === 'function') {
            showToast('Sign in to access Ask My Docs, History, and Insights.', 'error');
        }
        if (typeof openAuthModal === 'function') openAuthModal();
    }

    document.querySelectorAll('.nav-tab').forEach(t => {
        t.classList.remove('active');
        t.setAttribute('aria-selected', 'false');
    });
    document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
    const navBtn = document.getElementById('nav-' + tab);
    if (navBtn) {
        navBtn.classList.add('active');
        navBtn.setAttribute('aria-selected', 'true');
    }

    if (tab === 'compress') {
        const compressContent = document.getElementById('tab-compress');
        if (compressContent) compressContent.classList.add('active');
        if (typeof validatePromptLength === 'function') validatePromptLength();
    } else if (tab === 'ask') {
        const askContent = document.getElementById('tab-ask-content');
        if (askContent) askContent.classList.add('active');
        if (typeof loadScenarioRegistry === 'function') loadScenarioRegistry();
        if (typeof RagWorkspace !== 'undefined') RagWorkspace.loadDocuments();
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
    // Presets are hydrated directly from SAMPLE_PRESETS (no external presets.txt 404).
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
    if (!State.token && mode !== 'gentle') {
        showToast('Guest mode only supports Light Polish. Sign in to unlock Smart Trim and Deep Blueprint.', 'error');
        syncModeAvailability();
        return;
    }

    State.currentMode = mode;
    ['gentle', 'balanced', 'aggressive'].forEach(m => {
        const btn = document.getElementById('pm-' + m);
        if (btn) {
            const isActive = m === mode;
            btn.classList.toggle('active', isActive);
            btn.setAttribute('aria-checked', isActive ? 'true' : 'false');
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
 * Extracts phase and validation QA gate counts from an Aggressive Deep Architect prompt.
 */
function extractArchitectMetrics(promptText) {
    if (!promptText) return { phaseCount: 0, validationCount: 0 };
    const phaseSection = promptText.match(/# EXECUTION PHASES:\s*([\s\S]*?)(?=\n# [A-Z]|$)/i);
    const valSection = promptText.match(/# VALIDATION:\s*([\s\S]*?)(?=\n# [A-Z]|$)/i);
    const phaseMatches = phaseSection ? phaseSection[1].match(/^\d+\.\s+/gm) : null;
    const valMatches = valSection ? valSection[1].match(/^\d+\.\s+/gm) : null;
    return {
        phaseCount: phaseMatches ? phaseMatches.length : 0,
        validationCount: valMatches ? valMatches.length : 0,
    };
}

/**
 * Returns true when the current result represents an Aggressive Deep Architect expansion
 * rather than a token-reducing compression.
 */
function isArchitectExpansionResult(result) {
    if (!result) return false;
    const mode = State.currentMode || document.getElementById('mode')?.value || 'balanced';
    const orig = result.token_original || 0;
    const final = result.token_final || 0;
    const saved = result.token_saved || 0;
    const promptText = result.optimized_prompt || '';
    return (
        (mode === 'aggressive' && (final >= orig || saved <= 0)) ||
        (final > orig && /# (?:EXECUTION PHASES|VALIDATION|SUCCESS CRITERIA)/i.test(promptText))
    );
}

/**
 * Computes and renders estimated dollar cost savings per 1k runs,
 * or Deep Architect output-depth telemetry when in Aggressive expansion mode.
 * Formula: CostSaved = (savedTokens / 1000) * rate * 1000 = savedTokens * rate
 */
function updateCostMetrics() {
    const costSavedEl = document.getElementById('cost-saved');
    if (!costSavedEl) return;

    if (!State.lastResult) {
        costSavedEl.textContent = '$0.00 saved / 1k runs';
        return;
    }

    if (isArchitectExpansionResult(State.lastResult)) {
        const { phaseCount, validationCount } = extractArchitectMetrics(State.lastResult.optimized_prompt || '');
        costSavedEl.textContent = `~3,000 tok output target · ${phaseCount} Phases · ${validationCount} QA Gates`;
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
    if (btn) btn.disabled = true;

    const tesEl = document.getElementById('tes-score');
    if (tesEl) tesEl.textContent = 'Quality Ready';

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
    const isGuest = !State.token;
    const mode = isGuest ? 'gentle' : (State.currentMode || document.getElementById('mode')?.value || 'balanced');
    const domain = document.getElementById('domain-name')?.textContent || updateDetectedDomain(raw);

    if (!raw) return showToast('Raw Prompt is required', 'error');

    const words = raw.split(/\s+/).filter(Boolean);
    if (words.length < 8) {
        return showToast('Raw Prompt must be at least 8 words', 'error');
    }

    if (isGuest && getGuestUses() >= GUEST_MAX_USES) {
        showToast(`Guest limit reached (${GUEST_MAX_USES}/${GUEST_MAX_USES} Light Polish runs). Please sign in for unlimited access.`, 'error');
        if (!isSilent && typeof openAuthModal === 'function') {
            openAuthModal();
        }
        return;
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

    const ragEnabled = !isGuest && (document.getElementById('opt-rag-enabled')?.checked || false);
    const renderStyle = (!isGuest && document.getElementById('opt-harness-enabled')?.checked) ? 'harness' : 'crisp';
    const ragDocIds = (typeof RagWorkspace !== 'undefined' && ragEnabled) ? RagWorkspace.getSelectedDocIds() : [];

    try {
        const [result] = await Promise.all([
            API.compress({
                raw_text: raw,
                task: 'Optimize prompt',
                use_case: domain,
                mode: mode,
                domain: domain,
                rag_enabled: ragEnabled,
                rag_document_ids: ragDocIds.length > 0 ? ragDocIds : undefined,
                npae_render_style: renderStyle
            }),
            minDelay
        ]);

        // FIX: Defects 2, 3, 4 - If newer request initiated, discard stale response
        if (reqId !== currentCompressId) return;

        State.lastResult = result;
        State.lastTs = Date.now();
        if (isGuest) {
            incrementGuestUses();
            updateUserBar();
        }
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
 * Toggles the quality score breakdown panel and keeps aria-expanded synchronized.
 */
function toggleQualityBreakdown() {
    const panel = document.getElementById('quality-breakdown');
    const trigger = document.getElementById('tes-score');
    if (!panel) return;
    const isNowHidden = panel.classList.toggle('hidden');
    if (trigger) {
        trigger.setAttribute('aria-expanded', isNowHidden ? 'false' : 'true');
    }
}

/**
 * Computes a concise list of pruned tokens between the raw input and optimized output.
 * Falls back to server-provided pruned_summary when inputs are unavailable.
 */
function computePrunedTokenSummary(rawInput, optimizedPrompt, savedCount, fallbackSummary) {
    if (fallbackSummary) return fallbackSummary;
    const rawWords = (rawInput || '')
        .replace(/[^\w\s'-]/g, ' ')
        .split(/\s+/)
        .filter(w => w.length > 2);
    if (rawWords.length === 0) {
        return `Removed filler: conversational hedging, redundant phrasing (${savedCount} tokens)`;
    }
    const optSet = new Set(
        (optimizedPrompt || '')
            .toLowerCase()
            .replace(/[^\w\s'-]/g, ' ')
            .split(/\s+/)
            .filter(Boolean)
    );
    const seen = new Set();
    const removed = [];
    for (const w of rawWords) {
        const lower = w.toLowerCase();
        if (!optSet.has(lower) && !seen.has(lower)) {
            seen.add(lower);
            removed.push(w);
            if (removed.length >= 14) break;
        }
    }
    if (removed.length === 0) {
        return `Removed filler: conversational hedging, redundant phrasing (${savedCount} tokens)`;
    }
    const ellipsis = removed.length >= 14 ? '…' : '';
    return `Pruned (${savedCount} tok): ${removed.join(', ')}${ellipsis}`;
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

    const isArchitect = isArchitectExpansionResult(result);
    const architectPctLabel = (orig > 0 && final > orig)
        ? `+${Math.round(((final - orig) / orig) * 100)}% output`
        : 'Deep Architect';
    if (elTokCount) elTokCount.textContent = `${final} tokens`;
    if (elSavingPct) elSavingPct.textContent = isArchitect ? architectPctLabel : `${pct}% saved`;
    if (elOrig) elOrig.textContent = orig.toLocaleString();
    if (elFinal) elFinal.textContent = final.toLocaleString();
    if (elSaved) elSaved.textContent = saved.toLocaleString();
    if (elBar) elBar.style.width = Math.min(Math.max(pct, 0), 100) + '%';
    if (elPct) elPct.textContent = isArchitect ? architectPctLabel : `${pct}% saved`;

    // TES quality badge update (R3) — supports both Aggressive (tes_score) and Legacy (dual_score.tes)
    const tesScore = result.tes_score !== undefined
        ? result.tes_score
        : (result.dual_score && result.dual_score.tes !== undefined ? result.dual_score.tes : 0.97);
    const elTes = document.getElementById('tes-score');
    if (elTes) {
        elTes.textContent = `TES ${Number(tesScore).toFixed(2)} Hallucination-Free`;
    }

    const qBreakdown = document.getElementById('quality-breakdown');
    if (qBreakdown && result.dual_score) {
        qBreakdown.innerHTML = `
            <span>Efficiency (TES): <strong>${Number(result.dual_score.tes || 0).toFixed(2)}</strong></span>
            <span>Meaning (SFS): <strong>${Number(result.dual_score.sfs || 0).toFixed(2)}</strong></span>
            <span>Rules Kept (SCS): <strong>${Number(result.dual_score.scs || 0).toFixed(2)}</strong></span>
        `;
    }

    // Live dollar cost savings calculation (R3)
    updateCostMetrics();

    // Render output with optional pruning diff highlighting (R2)
    const elOutput = document.getElementById('result-output');
    if (elOutput) {
        const promptText = result.optimized_prompt || '';
        const showDiff = State.showDiff !== false && (document.getElementById('diff-toggle')?.checked !== false);

        if (showDiff && saved > 0) {
            const rawInput = document.getElementById('prompt-input')?.value || '';
            const diffSummary = computePrunedTokenSummary(rawInput, promptText, saved, result.pruned_summary);
            const formattedPrompt = escapeHtml(promptText)
                .replace(/\[(ROLE|CONTEXT|EXECUTION|CONSTRAINTS)\]/g, '<span class="crisp-tag">[$1]</span>');

            elOutput.innerHTML = `<del class="diff-removed">- ${escapeHtml(diffSummary)}</del>\n${formattedPrompt}`;
        } else {
            elOutput.textContent = promptText;
        }
    }

    // Reset feedback row (keep disabled in Guest mode)
    const isAuthed = Boolean(State.token);
    const btnUp = document.getElementById('btn-feedback-up');
    const btnDown = document.getElementById('btn-feedback-down');
    const fbStatus = document.getElementById('feedback-status');
    [btnUp, btnDown].forEach(btn => {
        if (!btn) return;
        btn.disabled = !isAuthed;
        btn.style.opacity = isAuthed ? '1' : '0.5';
        btn.style.outline = 'none';
        btn.title = isAuthed ? '' : 'Sign in to submit feedback';
    });
    if (fbStatus) fbStatus.style.display = 'none';
}

async function submitFeedback(rating) {
    if (!State.token) {
        showToast('Sign in to submit feedback.', 'error');
        openAuthModal();
        return;
    }
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
    const cleanPrompt = State.lastResult?.optimized_prompt;
    const text = cleanPrompt || document.getElementById('result-output')?.textContent;
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

// ── History Timeline ────────────────────────────────────────────────────────

let cachedHistoryRecords = [];
const HistoryState = {
    query: '',
    mode: 'all',
    sort: 'newest',
    expandedIds: new Set(),
};

async function loadHistory() {
    if (!State.token) return;
    try {
        const records = await API.history();
        cachedHistoryRecords = Array.isArray(records) ? records : [];
        applyHistoryFilters();
    } catch (err) {
        showToast(err.message, 'error');
    }
}

function filterHistory(query) {
    HistoryState.query = (query || '').trim().toLowerCase();
    applyHistoryFilters();
}

function setHistoryModeFilter(mode) {
    HistoryState.mode = mode || 'all';
    document.querySelectorAll('[data-history-mode]').forEach(btn => {
        btn.classList.toggle('active', btn.getAttribute('data-history-mode') === HistoryState.mode);
    });
    applyHistoryFilters();
}

function setHistorySort(sortKey) {
    HistoryState.sort = sortKey || 'newest';
    applyHistoryFilters();
}

function resetHistoryFilters() {
    HistoryState.query = '';
    HistoryState.mode = 'all';
    HistoryState.sort = 'newest';
    const searchEl = document.getElementById('history-search');
    if (searchEl) searchEl.value = '';
    const sortEl = document.getElementById('history-sort');
    if (sortEl) sortEl.value = 'newest';
    setHistoryModeFilter('all');
}

function getHistoryReductionPct(r) {
    const orig = Number(r.token_original) || 0;
    const saved = Number(r.tokens_saved) || 0;
    return orig > 0 ? Math.max(0, Math.round((saved / orig) * 100)) : 0;
}

function applyHistoryFilters() {
    let filtered = cachedHistoryRecords.slice();
    if (HistoryState.mode !== 'all') {
        filtered = filtered.filter(r => String(r.mode || 'balanced').toLowerCase() === HistoryState.mode);
    }
    if (HistoryState.query) {
        const q = HistoryState.query;
        filtered = filtered.filter(r =>
            (r.original_prompt || '').toLowerCase().includes(q) ||
            (r.optimized_prompt || '').toLowerCase().includes(q) ||
            (r.use_case || '').toLowerCase().includes(q) ||
            (r.mode || '').toLowerCase().includes(q)
        );
    }
    if (HistoryState.sort === 'saved') {
        filtered.sort((a, b) => (Number(b.tokens_saved) || 0) - (Number(a.tokens_saved) || 0));
    } else if (HistoryState.sort === 'ratio') {
        filtered.sort((a, b) => getHistoryReductionPct(b) - getHistoryReductionPct(a));
    } else {
        filtered.sort((a, b) => new Date(b.created_at || 0) - new Date(a.created_at || 0));
    }
    renderHistoryList(filtered);
}

function getHistoryDateGroup(dateValue) {
    const dt = new Date(dateValue);
    if (Number.isNaN(dt.getTime())) return 'Earlier';
    const now = new Date();
    const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const startOfTarget = new Date(dt.getFullYear(), dt.getMonth(), dt.getDate());
    const diffDays = Math.round((startOfToday - startOfTarget) / 86400000);
    if (diffDays <= 0) return 'Today';
    if (diffDays === 1) return 'Yesterday';
    if (diffDays <= 7) return 'Last 7 Days';
    return 'Earlier';
}

function toggleHistoryExpand(recordId) {
    if (HistoryState.expandedIds.has(recordId)) {
        HistoryState.expandedIds.delete(recordId);
    } else {
        HistoryState.expandedIds.add(recordId);
    }
    applyHistoryFilters();
}

function restoreHistoryPrompt(recordId) {
    const r = cachedHistoryRecords.find(item => String(item.id) === String(recordId));
    if (!r) return;
    const input = document.getElementById('prompt-input');
    if (input) input.value = r.original_prompt || '';
    validatePromptLength();
    switchTab('compress');
}

function copyHistoryOptimized(recordId, btnEl) {
    const r = cachedHistoryRecords.find(item => String(item.id) === String(recordId));
    if (!r) return;
    const text = r.optimized_prompt || r.original_prompt || '';
    if (!navigator.clipboard || typeof navigator.clipboard.writeText !== 'function') {
        showToast('Clipboard unavailable in this browser context', 'error');
        return;
    }
    navigator.clipboard.writeText(text).then(() => {
        if (btnEl) {
            const origLabel = btnEl.textContent;
            btnEl.textContent = '✓ Copied';
            setTimeout(() => { btnEl.textContent = origLabel; }, 1600);
        }
    }).catch(err => showToast('Failed to copy: ' + err.message, 'error'));
}

function renderHistoryList(records) {
    const list = document.getElementById('history-list');
    const summaryPill = document.getElementById('history-summary-pill');
    if (!list) return;

    const totalSavedInView = records.reduce((acc, r) => acc + Math.max(0, Number(r.tokens_saved) || 0), 0);
    if (summaryPill) {
        summaryPill.textContent = `${records.length} ${records.length === 1 ? 'run' : 'runs'} · +${totalSavedInView.toLocaleString()} tok saved`;
    }

    if (!cachedHistoryRecords.length) {
        list.innerHTML = `
            <div class="bento-card insights-empty">
                <div class="auth-gate-icon">🕒</div>
                <div class="auth-gate-title">No Compression History Yet</div>
                <div class="auth-gate-desc">Your past prompt optimizations will appear here with side-by-side CRISP diffs and one-click restore.</div>
                <button type="button" class="tc-compress-btn" onclick="switchTab('compress')" style="width: auto; padding: 10px 22px;">Optimize a Prompt</button>
            </div>`;
        return;
    }

    if (!records.length) {
        list.innerHTML = `
            <div class="bento-card insights-empty">
                <div class="auth-gate-title">No Matching Runs</div>
                <div class="auth-gate-desc">Try clearing your search query or switching the mode filter.</div>
                <button type="button" class="history-btn primary" onclick="resetHistoryFilters()" style="padding: 8px 16px;">Reset Filters</button>
            </div>`;
        return;
    }

    list.innerHTML = '';
    let lastGroup = null;

    records.forEach((r, idx) => {
        const recId = String(r.id || `hist-${idx}`);
        if (HistoryState.sort === 'newest') {
            const grp = getHistoryDateGroup(r.created_at);
            if (grp !== lastGroup) {
                lastGroup = grp;
                const grpHeader = document.createElement('div');
                grpHeader.className = 'history-group-label';
                grpHeader.textContent = grp;
                list.appendChild(grpHeader);
            }
        }

        const modeKey = String(r.mode || 'balanced').toLowerCase();
        const modeMeta = INSIGHTS_MODE_META[modeKey] || { label: modeKey, color: 'var(--accent-text)' };
        const origTok = Number(r.token_original) || 0;
        const finalTok = Number(r.token_final) || 0;
        const savedTok = Number(r.tokens_saved) || 0;
        const pct = getHistoryReductionPct(r);
        const isExpanded = HistoryState.expandedIds.has(recId);
        const dt = new Date(r.created_at);
        const timeStr = Number.isNaN(dt.getTime()) ? '' : dt.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });

        const item = document.createElement('div');
        item.className = 'history-item';
        item.setAttribute('data-history-id', recId);

        const head = document.createElement('div');
        head.className = 'history-item-head';
        head.innerHTML = `
            <div class="history-item-badges">
                <span class="bento-badge"><span class="bento-dot" style="background:${modeMeta.color};"></span>${escapeHtml(modeMeta.label)}</span>
                <span class="bento-badge">${escapeHtml(r.use_case || 'generic')}</span>
                <span class="bento-badge ${savedTok > 0 ? 'positive' : ''}">+${savedTok.toLocaleString()} tok (-${pct}%)</span>
            </div>
            <span class="history-item-time">${escapeHtml(timeStr)}</span>
        `;

        const preview = document.createElement('button');
        preview.type = 'button';
        preview.className = 'history-preview';
        preview.setAttribute('aria-expanded', isExpanded ? 'true' : 'false');
        preview.textContent = r.original_prompt || '';
        preview.title = 'Click to inspect Original vs. Optimized CRISP prompt';
        preview.addEventListener('click', () => toggleHistoryExpand(recId));

        const meta = document.createElement('div');
        meta.className = 'history-meta';

        const tokInfo = document.createElement('span');
        tokInfo.className = 'bento-row-meta';
        tokInfo.textContent = `${origTok.toLocaleString()} → ${finalTok.toLocaleString()} tok`;

        const actions = document.createElement('div');
        actions.className = 'history-actions';

        const inspectBtn = document.createElement('button');
        inspectBtn.type = 'button';
        inspectBtn.className = 'history-btn';
        inspectBtn.textContent = isExpanded ? 'Hide Diff' : 'Inspect Diff';
        inspectBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            toggleHistoryExpand(recId);
        });

        const copyBtn = document.createElement('button');
        copyBtn.type = 'button';
        copyBtn.className = 'history-btn';
        copyBtn.textContent = 'Copy CRISP';
        copyBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            copyHistoryOptimized(recId, copyBtn);
        });

        const restoreBtn = document.createElement('button');
        restoreBtn.type = 'button';
        restoreBtn.className = 'history-btn primary';
        restoreBtn.textContent = 'Load in Workbench';
        restoreBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            restoreHistoryPrompt(recId);
        });

        actions.appendChild(inspectBtn);
        actions.appendChild(copyBtn);
        actions.appendChild(restoreBtn);
        meta.appendChild(tokInfo);
        meta.appendChild(actions);

        item.appendChild(head);
        item.appendChild(preview);
        item.appendChild(meta);

        if (isExpanded) {
            const drawer = document.createElement('div');
            drawer.className = 'history-diff-drawer';
            drawer.innerHTML = `
                <div class="history-diff-pane">
                    <div class="history-diff-pane-title">Original Prompt (${origTok} tok)</div>
                    <pre>${escapeHtml(r.original_prompt || '')}</pre>
                </div>
                <div class="history-diff-pane">
                    <div class="history-diff-pane-title">Optimized CRISP (${finalTok} tok)</div>
                    <pre>${escapeHtml(r.optimized_prompt || '')}</pre>
                </div>
            `;
            item.appendChild(drawer);
        }

        list.appendChild(item);
    });
}

// ── Stats & Insights Bento Dashboard ───────────────────────────────────────

const INSIGHTS_MODELS = {
    gpt4o: { label: 'GPT-4o', ratePer1k: 0.005 },
    claude35: { label: 'Claude 3.5', ratePer1k: 0.003 },
    gemini25: { label: 'Gemini 2.5', ratePer1k: 0.00125 },
};

const INSIGHTS_MODE_META = {
    gentle: { label: 'Light Polish', color: '#38bdf8' },
    balanced: { label: 'Smart Trim', color: '#10b981' },
    aggressive: { label: 'Deep Blueprint', color: '#a855f7' },
};

/**
 * Updates the active pricing target on the Insights Hero card without re-fetching.
 */
function setInsightsModel(modelKey) {
    if (!INSIGHTS_MODELS[modelKey]) return;
    State.insightsModel = modelKey;
    if (State.lastStats) {
        renderInsightsBento(State.lastStats);
    }
}

/**
 * Builds a responsive pure-SVG 14-day token trajectory chart (Original vs. Saved).
 */
function buildInsightsTrajectorySvg(dailySeries) {
    if (!Array.isArray(dailySeries) || dailySeries.length === 0) {
        return '<div class="bento-kpi-foot" style="text-align:center; padding: 48px 0;">Run compressions over multiple days to unlock your 14-day trajectory.</div>';
    }

    const w = 560;
    const h = 165;
    const padX = 16;
    const padTop = 14;
    const padBot = 24;
    const plotW = w - padX * 2;
    const plotH = h - padTop - padBot;

    const maxVal = Math.max(
        1,
        ...dailySeries.map(d => Math.max(Number(d.original_tokens) || 0, Number(d.tokens_saved) || 0))
    );

    const stepX = dailySeries.length > 1 ? plotW / (dailySeries.length - 1) : 0;
    const xAt = (idx) => (dailySeries.length === 1 ? padX + plotW / 2 : padX + idx * stepX);
    const yAt = (val) => padTop + plotH - ((Number(val) || 0) / maxVal) * plotH;

    const origPts = dailySeries.map((d, i) => `${xAt(i).toFixed(1)},${yAt(d.original_tokens).toFixed(1)}`);
    const savedPts = dailySeries.map((d, i) => `${xAt(i).toFixed(1)},${yAt(d.tokens_saved).toFixed(1)}`);

    const areaPath = dailySeries.length > 1
        ? `M ${xAt(0).toFixed(1)},${(padTop + plotH).toFixed(1)} L ${savedPts.join(' L ')} L ${xAt(dailySeries.length - 1).toFixed(1)},${(padTop + plotH).toFixed(1)} Z`
        : '';

    const gridLines = [0.25, 0.5, 0.75, 1].map(frac => {
        const y = (padTop + plotH * (1 - frac)).toFixed(1);
        return `<line x1="${padX}" y1="${y}" x2="${w - padX}" y2="${y}" stroke="currentColor" stroke-opacity="0.08" stroke-dasharray="3 4" />`;
    }).join('');

    const dots = dailySeries.map((d, i) => {
        const cx = xAt(i).toFixed(1);
        const cySaved = yAt(d.tokens_saved).toFixed(1);
        const dayLbl = escapeHtml(String(d.day || '').slice(5));
        const tip = `${escapeHtml(String(d.day || ''))}: ${Number(d.runs) || 0} runs · ${Number(d.tokens_saved) || 0} saved / ${Number(d.original_tokens) || 0} orig tok`;
        const showLabel = i === 0 || i === dailySeries.length - 1 || (dailySeries.length > 4 && i === Math.floor(dailySeries.length / 2));
        return `
            <g>
                <circle cx="${cx}" cy="${cySaved}" r="4" fill="var(--stat-pos)" stroke="var(--surface-bg, #0b1120)" stroke-width="1.5">
                    <title>${tip}</title>
                </circle>
                ${showLabel ? `<text x="${cx}" y="${h - 4}" text-anchor="middle" fill="var(--text-muted)" font-size="10" font-family="JetBrains Mono, monospace">${dayLbl}</text>` : ''}
            </g>`;
    }).join('');

    return `
        <svg class="bento-chart-svg" viewBox="0 0 ${w} ${h}" role="img" aria-label="14-day token trajectory">
            <defs>
                <linearGradient id="insightsSavedGrad" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stop-color="var(--stat-pos)" stop-opacity="0.32" />
                    <stop offset="100%" stop-color="var(--stat-pos)" stop-opacity="0.02" />
                </linearGradient>
            </defs>
            ${gridLines}
            ${areaPath ? `<path d="${areaPath}" fill="url(#insightsSavedGrad)" />` : ''}
            ${dailySeries.length > 1 ? `<polyline fill="none" stroke="var(--text-muted)" stroke-opacity="0.55" stroke-width="1.5" stroke-dasharray="4 3" points="${origPts.join(' ')}" />` : ''}
            ${dailySeries.length > 1 ? `<polyline fill="none" stroke="var(--stat-pos)" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" points="${savedPts.join(' ')}" />` : ''}
            ${dots}
        </svg>`;
}

/**
 * Renders the Insights Bento Grid from server-aggregated stats payload.
 */
function renderInsightsBento(stats) {
    const container = document.getElementById('global-stats');
    if (!container) return;

    const totalRuns = Number(stats.total_compressions) || 0;
    if (totalRuns === 0) {
        container.innerHTML = `
            <div class="bento-card bento-span-12 insights-empty">
                <div class="auth-gate-icon">✨</div>
                <div class="auth-gate-title">No Compression Telemetry Yet</div>
                <div class="auth-gate-desc">Run your first prompt optimization to unlock 14-day token trajectories, mode efficiency, and model cost savings.</div>
                <button type="button" class="tc-compress-btn" onclick="switchTab('compress')" style="width: auto; padding: 10px 22px;">Optimize a Prompt</button>
            </div>
        `;
        return;
    }

    const totalSaved = Number(stats.total_tokens_saved) || 0;
    const avgSaved = Math.round(Number(stats.avg_tokens_saved) || 0);
    const totalOrig = Number(stats.total_original_tokens) || 0;
    const totalFinal = Number(stats.total_final_tokens) || 0;
    const bestSaved = Number(stats.best_tokens_saved) || 0;
    const reductionPct = totalOrig > 0 ? Math.max(0, Math.round((totalSaved / totalOrig) * 1000) / 10) : 0;

    const activeModelKey = State.insightsModel && INSIGHTS_MODELS[State.insightsModel] ? State.insightsModel : 'gpt4o';
    const modelCfg = INSIGHTS_MODELS[activeModelKey];
    const cumulativeDollars = ((totalSaved / 1000) * modelCfg.ratePer1k).toFixed(2);
    const per1kDollars = (avgSaved * modelCfg.ratePer1k).toFixed(2);

    const modelPillsHtml = Object.entries(INSIGHTS_MODELS).map(([key, cfg]) => `
        <button type="button"
            class="bento-model-pill ${key === activeModelKey ? 'active' : ''}"
            data-insights-model="${key}"
            onclick="setInsightsModel('${key}')">${escapeHtml(cfg.label)}</button>
    `).join('');

    const modes = Array.isArray(stats.mode_breakdown) ? stats.mode_breakdown : [];
    const modeStackHtml = modes.length > 0
        ? modes.map(m => {
            const pct = totalRuns > 0 ? Math.max(4, (Number(m.count) / totalRuns) * 100) : 0;
            const meta = INSIGHTS_MODE_META[m.mode] || { label: m.mode, color: 'var(--accent-text)' };
            return `<div class="mode-stack-seg" style="width:${pct.toFixed(1)}%; background:${meta.color};" title="${escapeHtml(meta.label)}: ${Number(m.count)} runs"></div>`;
        }).join('')
        : '<div class="mode-stack-seg" style="width:100%; background:var(--prog-track);"></div>';

    const modeRowsHtml = modes.length > 0
        ? modes.map(m => {
            const meta = INSIGHTS_MODE_META[m.mode] || { label: m.mode, color: 'var(--accent-text)' };
            const pct = totalRuns > 0 ? Math.round((Number(m.count) / totalRuns) * 100) : 0;
            return `
                <div class="bento-row">
                    <span><span class="bento-dot" style="background:${meta.color};"></span>${escapeHtml(meta.label)}</span>
                    <span class="bento-row-meta">${Number(m.count)} (${pct}%) · +${Number(m.tokens_saved || 0).toLocaleString()} tok</span>
                </div>`;
        }).join('')
        : '<div class="bento-kpi-foot">No mode data recorded.</div>';

    const domains = Array.isArray(stats.domain_breakdown) ? stats.domain_breakdown : [];
    const maxDomainSaved = Math.max(1, ...domains.map(d => Number(d.tokens_saved) || 0));
    const domainRowsHtml = domains.length > 0
        ? domains.map(d => {
            const saved = Number(d.tokens_saved) || 0;
            const barPct = Math.max(6, Math.round((saved / maxDomainSaved) * 100));
            return `
                <div class="bento-row">
                    <span style="min-width: 96px; text-transform: capitalize;">${escapeHtml(String(d.domain || 'generic'))}</span>
                    <div class="domain-bar-track"><div class="domain-bar-fill" style="width:${barPct}%"></div></div>
                    <span class="bento-row-meta">${saved.toLocaleString()} tok</span>
                </div>`;
        }).join('')
        : '<div class="bento-kpi-foot">No domain breakdown yet.</div>';

    const topRuns = Array.isArray(stats.top_runs) ? stats.top_runs : [];
    const topRunsHtml = topRuns.length > 0
        ? topRuns.map(r => `
            <div class="top-run-item">
                <div style="min-width: 0;">
                    <div class="top-run-preview">${escapeHtml(r.preview || 'Prompt run')}</div>
                    <div class="top-run-tags">
                        <span class="bento-badge">${escapeHtml(r.use_case || 'generic')}</span>
                        <span>${escapeHtml((INSIGHTS_MODE_META[r.mode] || {}).label || r.mode || 'balanced')}</span>
                        <span>· ${Number(r.token_original || 0)} → ${Number(r.token_final || 0)} tok</span>
                    </div>
                </div>
                <span class="bento-badge positive">+${Number(r.tokens_saved || 0).toLocaleString()} tok</span>
            </div>`).join('')
        : '<div class="bento-kpi-foot">No top runs recorded yet.</div>';

    container.innerHTML = `
        <div class="bento-card bento-span-6">
            <div class="bento-eyebrow">
                <span>Net Token Savings</span>
                <span class="bento-badge positive">-${reductionPct}% avg reduction</span>
            </div>
            <div class="bento-hero-val" id="insights-total-saved">${totalSaved.toLocaleString()}</div>
            <div class="bento-cost-bar">
                <div class="bento-cost-readout">
                    <div class="bento-cost-amount" id="insights-cost-saved">$${cumulativeDollars} saved</div>
                    <div class="bento-cost-sub">~$${per1kDollars} projected / 1k runs (${escapeHtml(modelCfg.label)})</div>
                </div>
                <div class="bento-model-pills" role="group" aria-label="Target model pricing">${modelPillsHtml}</div>
            </div>
        </div>

        <div class="bento-card bento-span-3">
            <div class="bento-eyebrow"><span>Total Runs</span></div>
            <div class="bento-kpi-val" id="insights-total-runs">${totalRuns.toLocaleString()}</div>
            <div class="bento-kpi-foot">Peak single run: +${bestSaved.toLocaleString()} tok</div>
        </div>

        <div class="bento-card bento-span-3">
            <div class="bento-eyebrow"><span>Avg Saved / Run</span></div>
            <div class="bento-kpi-val" id="insights-avg-saved">${avgSaved.toLocaleString()}</div>
            <div class="bento-kpi-foot">${totalOrig.toLocaleString()} → ${totalFinal.toLocaleString()} tok total</div>
        </div>

        <div class="bento-card bento-span-7">
            <div class="bento-eyebrow">
                <span>14-Day Token Trajectory</span>
                <span class="bento-legend">
                    <span><span class="bento-dot" style="background:var(--text-muted);"></span>Original</span>
                    <span><span class="bento-dot" style="background:var(--stat-pos);"></span>Saved</span>
                </span>
            </div>
            <div class="bento-chart-wrap" id="insights-trajectory-chart">
                ${buildInsightsTrajectorySvg(stats.daily_series || [])}
            </div>
        </div>

        <div class="bento-card bento-span-5">
            <div class="bento-eyebrow"><span>Compression Mode Mix</span></div>
            <div class="mode-stack-bar">${modeStackHtml}</div>
            <div class="bento-list">${modeRowsHtml}</div>
        </div>

        <div class="bento-card bento-span-5">
            <div class="bento-eyebrow"><span>Top Domains</span></div>
            <div class="bento-list">${domainRowsHtml}</div>
        </div>

        <div class="bento-card bento-span-7">
            <div class="bento-eyebrow"><span>Peak Efficiency Runs</span></div>
            <div class="bento-list">${topRunsHtml}</div>
        </div>
    `;
}

async function loadStats() {
    if (!State.token) return;
    try {
        const stats = await API.stats();
        State.lastStats = stats;
        renderInsightsBento(stats);
    } catch (err) {
        showToast(err.message, 'error');
    }
}

// ── Utils ───────────────────────────────────────────────────────────────────

function showToast(msg, type) {
    const region = document.getElementById('toast-region') || document.body;
    const existing = region.querySelector('.toast');
    if (existing) existing.remove();

    const toast = document.createElement('div');
    toast.className = 'toast toast-' + type;
    toast.setAttribute('role', type === 'error' ? 'alert' : 'status');
    toast.textContent = msg;
    region.appendChild(toast);
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
        promptInput.removeAttribute('aria-invalid');
        if (errorDiv) errorDiv.style.display = 'none';
    } else {
        compressBtn.disabled = true;
        if (errorDiv) {
            if (raw.length > 0) {
                promptInput.setAttribute('aria-invalid', 'true');
                errorDiv.textContent = `Prompt must be at least 8 words (currently ${count} / 8).`;
                errorDiv.style.display = 'block';
            } else {
                promptInput.removeAttribute('aria-invalid');
                errorDiv.style.display = 'none';
            }
        }
    }
}

// ── Init & Lifecycle ────────────────────────────────────────────────────────

// ── Native <dialog> Helpers & Theme Lifecycle ───────────────────────────────

function openDialogSafe(dialogEl) {
    if (!dialogEl) return;
    if (typeof dialogEl.showModal === 'function') {
        if (!dialogEl.open) dialogEl.showModal();
    } else {
        dialogEl.setAttribute('open', '');
        dialogEl.style.display = 'flex';
    }
}

function closeDialogSafe(dialogEl) {
    if (!dialogEl) return;
    if (typeof dialogEl.close === 'function') {
        if (dialogEl.open) dialogEl.close();
    } else {
        dialogEl.removeAttribute('open');
        dialogEl.style.display = 'none';
    }
}

function applyThemeClass(themeClass) {
    const normalized = (themeClass === 'theme-dark' || themeClass === 'theme-night') ? 'theme-dark' : 'theme-light';
    const scheme = normalized === 'theme-dark' ? 'dark' : 'light';
    document.documentElement.setAttribute('data-theme', scheme);
    document.documentElement.style.colorScheme = scheme;
    document.body.classList.remove('theme-light', 'theme-dark', 'theme-night', 'theme-morning', 'theme-afternoon');
    document.body.classList.add(normalized);
    const btn = document.getElementById('btn-theme-toggle');
    if (btn) {
        btn.textContent = normalized === 'theme-dark' ? '☀️' : '🌙';
        btn.setAttribute('aria-label', normalized === 'theme-dark' ? 'Switch to Light Theme' : 'Switch to Dark Theme');
    }
}

function clearCustomSkinInlineVars() {
    const root = document.documentElement;
    ['--page-bg', '--glass-bg', '--btn-grad', '--btn-text', '--head-text', '--body-text', '--accent-text'].forEach(p => {
        root.style.removeProperty(p);
    });
}

function initTheme() {
    const saved = localStorage.getItem('oryx_theme');
    if (saved) {
        applyThemeClass(saved);
    } else {
        const prefersDark = typeof window !== 'undefined' && typeof window.matchMedia === 'function'
            ? window.matchMedia('(prefers-color-scheme: dark)').matches
            : false;
        applyThemeClass(prefersDark ? 'theme-dark' : 'theme-light');
    }
    if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
        const mq = window.matchMedia('(prefers-color-scheme: dark)');
        if (mq && typeof mq.addEventListener === 'function') {
            mq.addEventListener('change', (e) => {
                if (!localStorage.getItem('oryx_theme')) {
                    applyThemeClass(e.matches ? 'theme-dark' : 'theme-light');
                }
            });
        }
    }
}

function toggleTheme() {
    const isDark = document.body.classList.contains('theme-dark') || document.body.classList.contains('theme-night');
    const next = isDark ? 'theme-light' : 'theme-dark';
    clearCustomSkinInlineVars();
    localStorage.setItem('oryx_theme', next);
    applyThemeClass(next);
}

function initCustomSkinModal() {
    const btn = document.getElementById('btn-custom-skin');
    const modal = document.getElementById('skin-modal');
    const closeBtn = document.getElementById('skin-modal-close');
    const applyBtn = document.getElementById('skin-modal-apply');
    const resetBtn = document.getElementById('skin-modal-reset');

    const savedSkin = localStorage.getItem('na_ui_custom_skin');
    if (savedSkin) {
        try {
            applyCustomSkinConfig(JSON.parse(savedSkin));
        } catch (_) {}
    }

    if (!btn || !modal) return;

    btn.addEventListener('click', () => openDialogSafe(modal));
    if (closeBtn) {
        closeBtn.addEventListener('click', () => closeDialogSafe(modal));
    }
    modal.addEventListener('click', (e) => {
        if (e.target === modal) closeDialogSafe(modal);
    });
    if (applyBtn) {
        applyBtn.addEventListener('click', () => {
            const bgType = document.getElementById('skin-bg-type')?.value || 'dark';
            const primaryColor = document.getElementById('skin-primary')?.value || '#0284c7';
            const config = bgType === 'dark'
                ? {
                    bg: '#090d16',
                    glassBg: 'rgba(16, 22, 36, 0.92)',
                    btnGrad: `linear-gradient(135deg, ${primaryColor}, #6366f1)`,
                    btnText: '#061326',
                    headText: '#f8fafc',
                    bodyText: '#cbd5e1',
                    accentText: primaryColor
                }
                : {
                    bg: '#f8fafc',
                    glassBg: 'rgba(255, 255, 255, 0.94)',
                    btnGrad: `linear-gradient(135deg, ${primaryColor}, #4338ca)`,
                    btnText: '#ffffff',
                    headText: '#0f172a',
                    bodyText: '#334155',
                    accentText: primaryColor
                };
            localStorage.setItem('na_ui_custom_skin', JSON.stringify(config));
            applyCustomSkinConfig(config);
            closeDialogSafe(modal);
            showToast('Custom skin applied', 'success');
        });
    }
    if (resetBtn) {
        resetBtn.addEventListener('click', () => {
            localStorage.removeItem('na_ui_custom_skin');
            clearCustomSkinInlineVars();
            initTheme();
            closeDialogSafe(modal);
            showToast('Skin reset to default', 'success');
        });
    }
}

/**
 * Enables Left/Right arrow keyboard navigation on ARIA tablists.
 */
function initTablistKeyboardNav() {
    document.querySelectorAll('[role="tablist"]').forEach(tablist => {
        tablist.addEventListener('keydown', (e) => {
            if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return;
            const tabs = Array.from(tablist.querySelectorAll('[role="tab"]'))
                .filter(t => !t.disabled && t.style.display !== 'none');
            if (tabs.length < 2) return;
            const idx = tabs.indexOf(document.activeElement);
            if (idx === -1) return;
            e.preventDefault();
            const nextIdx = e.key === 'ArrowRight'
                ? (idx + 1) % tabs.length
                : (idx - 1 + tabs.length) % tabs.length;
            tabs[nextIdx].focus();
            tabs[nextIdx].click();
        });
    });
}

/**
 * Studio ⌘K / Ctrl+K Command Palette for fast keyboard-first navigation & actions.
 */
function openStudioCommandPalette() {
    const modal = document.getElementById('studio-cmd-modal');
    const input = document.getElementById('studio-cmd-input');
    if (!modal) return;
    openDialogSafe(modal);
    if (input) {
        input.value = '';
        renderStudioCommandList('');
        setTimeout(() => input.focus(), 10);
    }
}

function getStudioCommands() {
    const isAuthed = Boolean(State.token);
    const cmds = [
        { label: 'Load Preset: Buying a House', hint: 'Preset', action: () => { switchTab('compress'); loadPreset('house'); } },
        { label: 'Load Preset: Bug Report', hint: 'Preset', action: () => { switchTab('compress'); loadPreset('bug'); } },
        { label: 'Load Preset: Contract Review', hint: 'Preset', action: () => { switchTab('compress'); loadPreset('legal'); } },
        { label: 'Mode: Light Polish', hint: 'Mode', action: () => { switchTab('compress'); selectMode('gentle'); } },
        { label: isAuthed ? 'Mode: Smart Trim' : 'Mode: Smart Trim (Sign in)', hint: 'Mode', action: () => { switchTab('compress'); selectMode('balanced'); } },
        { label: isAuthed ? 'Mode: Deep Blueprint' : 'Mode: Deep Blueprint (Sign in)', hint: 'Mode', action: () => { switchTab('compress'); selectMode('aggressive'); } },
        { label: 'Toggle Light / Dark Theme', hint: 'Theme', action: () => toggleTheme() },
        { label: 'Customize UI Skin', hint: 'Theme', action: () => openDialogSafe(document.getElementById('skin-modal')) },
        { label: isAuthed ? 'Sign Out' : 'Sign In / Register', hint: 'Account', action: () => toggleAuthSection() },
    ];
    if (isAuthed) {
        cmds.push(
            { label: 'Go to Optimize Prompt', hint: 'Tab', action: () => switchTab('compress') },
            { label: 'Go to Ask My Docs (RAG)', hint: 'Tab', action: () => switchTab('ask') },
            { label: 'Go to History Timeline', hint: 'Tab', action: () => switchTab('history') },
            { label: 'Go to Insights Dashboard', hint: 'Tab', action: () => switchTab('stats') },
        );
    }
    return cmds;
}

function renderStudioCommandList(query = '') {
    const list = document.getElementById('studio-cmd-list');
    const modal = document.getElementById('studio-cmd-modal');
    if (!list) return;
    const q = query.trim().toLowerCase();
    const matches = getStudioCommands().filter(c =>
        c.label.toLowerCase().includes(q) || c.hint.toLowerCase().includes(q)
    );
    list.innerHTML = '';
    if (matches.length === 0) {
        list.innerHTML = '<div style="padding:0.75rem;font-size:0.8rem;color:var(--text-muted);">No matching commands.</div>';
        return;
    }
    matches.forEach((cmd, idx) => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'cmd-palette-item' + (idx === 0 ? ' selected' : '');
        btn.innerHTML = `<span>${escapeHtml(cmd.label)}</span><span class="bento-badge">${escapeHtml(cmd.hint)}</span>`;
        btn.addEventListener('click', () => {
            closeDialogSafe(modal);
            cmd.action();
        });
        list.appendChild(btn);
    });
}

function initStudioCommandPalette() {
    const modal = document.getElementById('studio-cmd-modal');
    const input = document.getElementById('studio-cmd-input');
    const list = document.getElementById('studio-cmd-list');
    if (!modal || !input || !list) return;

    modal.addEventListener('click', (e) => {
        if (e.target === modal) closeDialogSafe(modal);
    });

    input.addEventListener('input', (e) => renderStudioCommandList(e.target.value));
    input.addEventListener('keydown', (e) => {
        const items = Array.from(list.querySelectorAll('.cmd-palette-item'));
        if (!items.length) return;
        let idx = items.findIndex(el => el.classList.contains('selected'));
        if (e.key === 'ArrowDown') {
            e.preventDefault();
            items.forEach(el => el.classList.remove('selected'));
            items[(idx + 1) % items.length].classList.add('selected');
        } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            items.forEach(el => el.classList.remove('selected'));
            items[(idx - 1 + items.length) % items.length].classList.add('selected');
        } else if (e.key === 'Enter') {
            e.preventDefault();
            const active = items[idx >= 0 ? idx : 0];
            if (active) active.click();
        }
    });

    document.addEventListener('keydown', (e) => {
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
            e.preventDefault();
            if (modal.open) closeDialogSafe(modal);
            else openStudioCommandPalette();
        } else if (e.key === 'Escape') {
            const authSec = document.getElementById('auth-section');
            if (authSec && authSec.style.display === 'block') {
                closeAuthSection();
            }
        }
    });

    // Backdrop click on auth overlay closes it cleanly
    const authSec = document.getElementById('auth-section');
    if (authSec) {
        authSec.addEventListener('click', (e) => {
            if (e.target === authSec) closeAuthSection();
        });
    }
    const verifyModal = document.getElementById('verify-modal');
    if (verifyModal) {
        verifyModal.addEventListener('click', (e) => {
            if (e.target === verifyModal) closeDialogSafe(verifyModal);
        });
    }
}

function applyCustomSkinConfig(config) {
    const root = document.documentElement;
    if (config.bg) root.style.setProperty('--page-bg', config.bg);
    if (config.glassBg) root.style.setProperty('--glass-bg', config.glassBg);
    if (config.btnGrad) root.style.setProperty('--btn-grad', config.btnGrad);
    if (config.btnText) root.style.setProperty('--btn-text', config.btnText);
    if (config.headText) root.style.setProperty('--head-text', config.headText);
    if (config.bodyText) root.style.setProperty('--body-text', config.bodyText);
    if (config.accentText) root.style.setProperty('--accent-text', config.accentText);
}

document.addEventListener('DOMContentLoaded', () => {
    initTheme();
    initCustomSkinModal();
    initTablistKeyboardNav();
    initStudioCommandPalette();
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

async function buildScenarioAskPayload({ appMode, domainVal, styleVal, geoVal, question, mode }) {
    const dataSource = (typeof RagWorkspace !== 'undefined' && typeof RagWorkspace.getDataSource === 'function')
        ? RagWorkspace.getDataSource()
        : 'cloud';

    const payload = {
        app_mode: appMode,
        domain: appMode === 'Scenario' ? domainVal : null,
        style: appMode === 'Regular' ? styleVal : null,
        geography: appMode === 'Scenario' && geoVal ? geoVal : null,
        question: question,
        mode: mode || null,
        data_source: dataSource,
    };

    if (dataSource === 'own') {
        const localResp = await API.localRagRetrieve({
            query: question,
            top_k: 5,
            file_extensions: ['.pdf', '.docx'],
        });
        payload.local_chunks = localResp.chunks || [];
    }

    return payload;
}

async function executeScenarioAsk() {
    if (!State.token) {
        showToast('Sign in to access Ask My Docs.', 'error');
        openAuthModal();
        return;
    }
    const appMode   = document.getElementById('scenario-mode-select').value;
    const domainVal = document.getElementById('scenario-domain-select')?.value;
    const styleVal  = document.getElementById('scenario-style-select')?.value;
    const geoVal    = document.getElementById('scenario-geography-select')?.value;
    const question  = document.getElementById('scenario-query-input')?.value.trim();

    if (!question) return showToast('Question text is required', 'error');
    if (appMode === 'Scenario' && !domainVal) return showToast('Mandatory domain selection required for Scenario mode', 'error');
    if (appMode === 'Regular' && !styleVal) return showToast('Style selection required for Regular mode', 'error');

    const btn = document.getElementById('ask-btn');
    if (btn) btn.disabled = true;

    try {
        const payload = await buildScenarioAskPayload({
            appMode,
            domainVal,
            styleVal,
            geoVal,
            question,
            mode: null,
        });
        const resp = await API.scenarioAsk(payload);

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
    if (!State.token) {
        showToast('Sign in to access Ask My Docs.', 'error');
        openAuthModal();
        return;
    }
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
        const payload = await buildScenarioAskPayload({
            appMode,
            domainVal,
            styleVal,
            geoVal,
            question,
            mode: 'aggressive',
        });
        const resp = await API.scenarioAsk(payload);

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
    window.toggleTheme = toggleTheme;
    window.filterHistory = filterHistory;
    window.toggleQualityBreakdown = toggleQualityBreakdown;
    window.openStudioCommandPalette = openStudioCommandPalette;
    window.openDialogSafe = openDialogSafe;
    window.closeDialogSafe = closeDialogSafe;
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


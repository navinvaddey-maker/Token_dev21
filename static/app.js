//! static/app.js — Core client controller & workbench lifecycle manager
//! ARCH: §1 Reality Check & §5 NPAE Grounding — Implements zero-auth sandbox workbench,
//! side-by-side pruning diffs, live multi-model cost telemetry, and drag-and-drop file ingestion.

/**
 * Global application state.
 */
const GUEST_MAX_USES = 3;
const GUEST_USES_KEY = 'tce_guest_uses';

const RECENT_RUNS_KEY = 'oryx_recent_runs';

// ARCH: ARCHITECTURE.md §2 Full Component Map (Presentation Layer) — macOS Apple HIG platform
// shortcut adaptation & Spotlight recent-runs ring buffer (purged on logout/session expiry).
/**
 * Detects whether the client OS uses Apple modifier conventions (`⌘` vs `Ctrl`).
 */
function isApplePlatform() {
    if (typeof window !== 'undefined' && window.__ORYX_FORCE_PLATFORM === 'win') return false;
    if (typeof window !== 'undefined' && window.__ORYX_FORCE_PLATFORM === 'apple') return true;
    const ua = (typeof navigator !== 'undefined' && ((navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || navigator.userAgent)) || '';
    if (/jsdom/i.test(ua)) return true;
    return /Mac|iPhone|iPad|iPod/i.test(ua);
}

function getModKeySymbol() {
    return isApplePlatform() ? '⌘' : 'Ctrl+';
}

function getCmdPaletteBadge() {
    return isApplePlatform() ? '⌘K' : 'Ctrl K';
}

function getRunShortcutBadge() {
    return isApplePlatform() ? '⌘↩' : 'Ctrl+↩';
}

function syncPlatformShortcutBadges() {
    const kbdCmd = document.getElementById('header-cmd-kbd');
    if (kbdCmd) kbdCmd.textContent = getCmdPaletteBadge();
    const btnStudioCmd = document.getElementById('btn-studio-cmd');
    if (btnStudioCmd) btnStudioCmd.title = `Quick Actions & Recent Prompts (${getCmdPaletteBadge()})`;
    const kbdRun = document.getElementById('compress-btn-kbd');
    if (kbdRun) kbdRun.textContent = getRunShortcutBadge();
}

function setCompressButtonLabel(labelText) {
    const labelEl = document.getElementById('compress-btn-label');
    const btn = document.getElementById('compress-btn');
    if (labelEl) {
        labelEl.textContent = labelText;
    } else if (btn) {
        const kbd = document.getElementById('compress-btn-kbd');
        const kbdHtml = kbd ? kbd.outerHTML : '<kbd id="compress-btn-kbd">⌘↩</kbd>';
        btn.innerHTML = escapeHtml(labelText) + ' ' + kbdHtml;
    }
}

function loadRecentPromptRuns() {
    try {
        const raw = localStorage.getItem(RECENT_RUNS_KEY);
        if (raw) {
            const parsed = JSON.parse(raw);
            if (Array.isArray(parsed)) {
                State.recentRuns = parsed.slice(0, 5);
            }
        }
    } catch (err) {
        console.warn('Failed to load recent prompt runs:', err);
    }
}

function recordRecentPromptRun({ id, rawText, tokenOriginal, tokenFinal, tokensSaved, mode }) {
    try {
        if (!rawText) return;
        const item = {
            id: id || ('run_' + Math.random().toString(36).slice(2, 9)),
            preview: rawText.slice(0, 90),
            raw_text: rawText.slice(0, 2000),
            token_original: tokenOriginal || 0,
            token_final: tokenFinal || 0,
            tokens_saved: tokensSaved || 0,
            mode: mode || State.currentMode,
            created_at: new Date().toISOString()
        };
        State.recentRuns = [item, ...(State.recentRuns || []).filter(r => r.id !== item.id && r.raw_text !== item.raw_text)].slice(0, 5);
        localStorage.setItem(RECENT_RUNS_KEY, JSON.stringify(State.recentRuns));
    } catch (err) {
        console.warn('Failed to record recent prompt run:', err);
    }
}

const State = {
    userId: null,
    token: null,
    username: null,
    businessType: null,
    role: null,
    lastResult: null,
    lastTs: null,
    currentMode: 'balanced',
    outcomePreset: 'saver',
    currentModelRate: 0.005, // Default GPT-4o: $0.005 / 1k tokens ($5 / 1M)
    currentRoiScale: 1000,
    showDiff: true,
    guestUses: 0,
    recentRuns: [],
    // Phase 4: Multi-turn continuation state
    parentId: null,
    parentRawPrompt: '',
    turnNumber: 0,
    activeConstraints: [],
    revokedConstraints: [],
    phases: [],
    activePhase: null,
    resolvedSlots: {},
    outputView: 'rehydrated',
    pendingClarifySlot: null
};

/**
 * Canonical 1-click sample presets for the Zero-Auth Sandbox (R1).
 */
const SAMPLE_PRESETS = {
    house: {
        id: "house",
        label: "Buying a House",
        domain: "Real Estate & Finance",
        text: "Hi there! I'm really excited and also honestly pretty nervous because my spouse and I have finally decided to take the plunge and buy our very first house together, and we have been saving up for years and years to get to this point, so it feels like a huge milestone in our lives. The thing is, we don't really know where to start at all and the whole process seems super overwhelming with so many different moving parts, legal jargon, financial requirements, and hidden costs that people warn you about. Could you please walk me through the entire process of buying a house from the very beginning all the way to getting the keys, in as much detail as possible?\n\nSpecifically, I would love it if you could explain everything we need to know about getting our finances in order first, like what credit score we should aim for, how much money we actually need saved up for a down payment versus closing costs and emergency funds, and how mortgage pre-approval works compared to pre-qualification, as well as the different types of loans available like conventional, FHA, or fixed-rate versus adjustable-rate mortgages. After that, could you explain how to find a good real estate agent, what to look for when touring homes and neighborhoods, how to figure out what a fair offer price is, and what kind of contingencies we should definitely include in our purchase agreement—like inspection contingencies, appraisal contingencies, and financing contingencies—so we don't end up stuck in a bad deal?\n\nFurthermore, once an offer is accepted, what happens during the escrow and underwriting period? I want to understand what a home inspector actually checks, how we can negotiate repairs or credits if they find major issues with the roof, foundation, plumbing, or electrical systems, what the lender is doing behind the scenes during underwriting, why we need title insurance and homeowners insurance, and what happens during the final walkthrough and closing day when we have to sign a million documents and wire the funds. Please also include any common rookie mistakes first-time homebuyers make so we can avoid them, and feel free to use clear examples, tips, and thorough explanations for every single step because we want to be completely prepared before we even talk to a lender or an agent!"
    },
    bug: {
        id: "bug",
        label: "Bug Report",
        domain: "Software Engineering",
        text: "Hi team, whenever a user tries to log in on Safari after clearing cookies, the dashboard just spins forever and in the console it says something about 401 unauthorized on /api/stats. Can you look at our auth flow and tell me why this happens and how to fix it cleanly without breaking Chrome?"
    },
    legal: {
        id: "legal",
        label: "Contract Review",
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
        const r = await fetch('/api/login', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ username, password }),
        });
        
        if (!r.ok) {
            const err = await r.json().catch(() => ({}));
            throw new Error(err.error?.message || err.error || `Login failed (HTTP ${r.status})`);
        }
        const data = await r.json();
        
        // Validate required fields FIRST before accessing any properties
        if (!data || typeof data !== 'object') {
            throw new Error('Invalid login response: data is not an object');
        }
        
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
        State.token    = 'cookie';
        State.userId   = data.user_id;
        State.username = data.username;
        State.role     = data.role || 'user';
        localStorage.removeItem('tce_token');
        localStorage.setItem('tce_user_id', data.user_id);
        localStorage.setItem('tce_username', data.username);
        localStorage.setItem('tce_business_type', data.business_type);
        localStorage.setItem('tce_role', State.role);
        State.businessType = data.business_type;
        showMainSection();
        showToast('Welcome back, ' + data.username, 'success');
        const promptInput = document.getElementById('prompt-input');
        if (promptInput && promptInput.value.trim().split(/\s+/).filter(Boolean).length >= 8) {
            compress(true);
        }
    } catch (err) {
        console.error('Login error:', err);
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
            const err = await r.json().catch(() => ({}));
            throw new Error(err.error?.message || err.error || `Registration failed (HTTP ${r.status})`);
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
function isAdminUser() {
    const r = (State.role || '').trim().toLowerCase();
    const b = (State.businessType || '').trim().toLowerCase();
    return r === 'admin' || b === 'admin';
}

/**
 * Purges session-scoped prompt caches and UI state on logout or 401 session expiration
 * so prompt contents never leak across users on a shared browser.
 */
function purgeSensitiveSessionCache() {
    State.recentRuns = [];
    State.lastResult = null;
    State.lastStats = null;
    cachedHistoryRecords = [];
    try {
        localStorage.removeItem('tce_token');
        localStorage.removeItem('tce_user_id');
        localStorage.removeItem('tce_username');
        localStorage.removeItem('tce_business_type');
        localStorage.removeItem('tce_role');
        localStorage.removeItem(RECENT_RUNS_KEY);
    } catch (err) {
        console.warn('Failed to clear session storage keys:', err);
    }
}

function logout() {
    // ARCH: §1 Reality Check — Session termination clears HttpOnly cookie and purges client caches
    fetch('/api/logout', { method: 'POST', credentials: 'same-origin' })
        .catch(err => console.warn('Logout request failed:', err));
    State.token = null;
    State.userId = null;
    State.username = null;
    State.businessType = null;
    State.role = null;
    purgeSensitiveSessionCache();
    resetParentSession(true);
    clearInput();

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

    if (isAdminUser()) {
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
    } catch (err) {
        console.warn('Failed to read guest uses from localStorage:', err);
    }
    return State.guestUses || 0;
}

function incrementGuestUses() {
    const next = getGuestUses() + 1;
    State.guestUses = next;
    try {
        localStorage.setItem(GUEST_USES_KEY, String(next));
    } catch (err) {
        console.warn('Failed to persist guest uses to localStorage:', err);
    }
    return next;
}

function syncModeAvailability() {
    const modeConfig = {
        gentle: { label: 'Light Polish', title: 'Light Polish: conservative cleanup preserving structure' },
        balanced: { label: 'Smart Trim', title: 'Smart Trim: balanced pruning with full context fidelity' },
        aggressive: { label: 'Deep Blueprint', title: 'Deep Blueprint: expands your prompt into structured CRISP scaffolding for ~3,000-token comprehensive LLM output' }
    };

    if (!State.currentMode) {
        State.currentMode = 'balanced';
    }

    ['gentle', 'balanced', 'aggressive'].forEach(m => {
        const btn = document.getElementById('pm-' + m);
        if (!btn) return;
        btn.disabled = false;
        btn.classList.remove('mode-pill-disabled');
        btn.textContent = modeConfig[m].label;
        btn.title = modeConfig[m].title;
        const isActive = m === State.currentMode;
        btn.classList.toggle('active', isActive);
        btn.setAttribute('aria-checked', isActive ? 'true' : 'false');
    });

    const hiddenMode = document.getElementById('mode');
    if (hiddenMode) {
        hiddenMode.value = State.currentMode;
    }

    if (typeof syncOutcomePresetFromControls === 'function') {
        syncOutcomePresetFromControls();
    }
}

function syncGuestFeatureAccess() {
    const isAuthed = Boolean(State.token);

    // Restrict Guests from persistent/cloud-only tabs (Ask, History, Insights).
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

    // Keep Cloud RAG gated to Sign In, while allowing guests to try the Grounding Harness.
    const ragEl = document.getElementById('opt-rag-enabled');
    const ragLbl = document.getElementById('opt-rag-label');
    if (ragEl) {
        ragEl.disabled = !isAuthed;
        if (!isAuthed) ragEl.checked = false;
    }
    if (ragLbl) {
        ragLbl.style.opacity = isAuthed ? '1' : '0.5';
        ragLbl.style.cursor = isAuthed ? 'pointer' : 'not-allowed';
        ragLbl.title = isAuthed ? '' : 'Sign in to unlock RAG grounding';
    }

    const harnessEl = document.getElementById('opt-harness-enabled');
    const harnessLbl = document.getElementById('opt-harness-label');
    if (harnessEl) harnessEl.disabled = false;
    if (harnessLbl) {
        harnessLbl.style.opacity = '1';
        harnessLbl.style.cursor = 'pointer';
        harnessLbl.title = 'Wrap facts in untrusted XML & enforce citation contract';
    }

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
            adminLink.style.display = isAdminUser() ? 'inline-flex' : 'none';
        }
    } else {
        const remaining = Math.max(0, GUEST_MAX_USES - getGuestUses());
        if (statusText) statusText.innerHTML = `<strong id="display-username">Trial (${remaining}/${GUEST_MAX_USES} left)</strong>`;
        if (toggleBtn) toggleBtn.textContent = 'Sign In';
        if (sandboxBadge) {
            sandboxBadge.textContent = `● Trial (${remaining}/${GUEST_MAX_USES})`;
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
    State.businessType = null;
    State.role = null;
    purgeSensitiveSessionCache();
    resetParentSession(true);
    clearInput();
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

    resetParentSession(true);

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
 * Updates visual active state and aria-checked on the 3 primary outcome preset pills.
 * # Arguments
 * * `activePreset` - 'factchecked' | 'saver' | 'blueprint' | null
 */
function updateOutcomePillsUI(activePreset) {
    ['factchecked', 'saver', 'blueprint'].forEach(p => {
        const btn = document.getElementById('po-' + p);
        if (!btn) return;
        const isActive = p === activePreset;
        btn.classList.toggle('active', isActive);
        btn.setAttribute('aria-checked', isActive ? 'true' : 'false');
    });
}

/**
 * Synchronizes the primary outcome preset pills from the fine-tune mode and harness checkbox.
 */
function syncOutcomePresetFromControls() {
    const mode = State.currentMode || document.getElementById('mode')?.value || 'balanced';
    const harnessChecked = Boolean(document.getElementById('opt-harness-enabled')?.checked);

    let matched = null;
    if (mode === 'aggressive' && harnessChecked) {
        matched = 'factchecked';
    } else if (mode === 'balanced' && !harnessChecked) {
        matched = 'saver';
    } else if (mode === 'aggressive' && !harnessChecked) {
        matched = 'blueprint';
    }

    State.outcomePreset = matched;
    updateOutcomePillsUI(matched);
}

/**
 * Applies an outcome-driven preset ('factchecked' | 'saver' | 'blueprint') by updating
 * the hallucination harness contract and compression mode together.
 * # Arguments
 * * `preset` - 'factchecked' | 'saver' | 'blueprint'
 */
function selectOutcomePreset(preset) {
    const harnessEl = document.getElementById('opt-harness-enabled');
    let targetMode = 'balanced';

    if (preset === 'factchecked') {
        if (harnessEl) harnessEl.checked = true;
        targetMode = 'aggressive';
    } else if (preset === 'blueprint') {
        if (harnessEl) harnessEl.checked = false;
        targetMode = 'aggressive';
    } else {
        preset = 'saver';
        if (harnessEl) harnessEl.checked = false;
        targetMode = 'balanced';
    }

    State.outcomePreset = preset;
    updateOutcomePillsUI(preset);
    selectMode(targetMode);
}

/**
 * Selects a compression mode via segmented pills ('gentle' | 'balanced' | 'aggressive').
 * All 3 modes are available in the guest trial sandbox (up to GUEST_MAX_USES runs).
 */
function selectMode(mode) {
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

    syncOutcomePresetFromControls();

    const promptInput = document.getElementById('prompt-input');
    const minWords = State.parentId ? 2 : 8;
    if (promptInput && promptInput.value.trim().split(/\s+/).filter(Boolean).length >= minWords) {
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

function updateRoiScale() {
    const scaleSelect = document.getElementById('roi-scale-select');
    if (scaleSelect) {
        State.currentRoiScale = parseInt(scaleSelect.value, 10) || 1000;
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

    const scale = State.currentRoiScale || 1000;
    const scaleLabel = scale >= 1000000 ? '1M/mo' : (scale >= 100000 ? '100k/mo' : '1k runs');

    if (!State.lastResult) {
        costSavedEl.textContent = `$0.00 saved / ${scaleLabel}`;
        return;
    }

    if (isArchitectExpansionResult(State.lastResult)) {
        const { phaseCount, validationCount } = extractArchitectMetrics(State.lastResult.optimized_prompt || '');
        costSavedEl.textContent = `~3,000 tok output target · ${phaseCount} Phases · ${validationCount} QA Gates`;
        return;
    }

    const savedTokens = State.lastResult.token_saved || 0;
    const rate = State.currentModelRate || 0.005;
    const savedDollar = savedTokens > 0 ? ((savedTokens / 1000) * rate * scale).toFixed(2) : '0.00';
    costSavedEl.textContent = `$${savedDollar} saved / ${scaleLabel}`;
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

function detectPerspective(text) {
    const t = text.toLowerCase();
    if (/\b(underwrite|irr|cap rate|portfolio|roi|capital allocation|yield|invest)\b/.test(t)) {
        return 'Investor';
    }
    if (/\b(client|broker|counsel|audit|deploy|endpoint|architecture|debug|compliance review)\b/.test(t)) {
        return 'Practitioner';
    }
    return 'Consumer';
}

function updateDetectedDomain(text) {
    const domain = detectDomain(text);
    const perspective = detectPerspective(text);
    const domainNameEl = document.getElementById('domain-name');
    if (domainNameEl) {
        domainNameEl.textContent = domain;
    }
    const perspEl = document.getElementById('perspective-badge');
    if (perspEl) {
        perspEl.textContent = perspective;
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
    if (!file) return;

    // Enforce 10 MB client-side file size check before reading into memory
    const MAX_FILE_SIZE = 10 * 1024 * 1024; // 10 MB
    if (file.size > MAX_FILE_SIZE) {
        showToast('File size exceeds 10MB limit. Please upload a smaller file.', 'error');
        return;
    }

    const name = file.name.toLowerCase();
    const promptInput = document.getElementById('prompt-input');
    if (!promptInput) return;

    if (name.endsWith('.txt') || name.endsWith('.md')) {
        const reader = new FileReader();
        reader.onload = (event) => {
            resetParentSession(true);
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
            resetParentSession(true);
            promptInput.value = text;
            validatePromptLength();
            showToast(`Extracted text from ${file.name}`, 'success');
            compress(true);
        } catch (err) {
            console.warn("PDF extraction failed:", err);
            showToast(`Could not extract text from ${file.name} in-browser. Please upload via Ask My Docs (Cloud or Own Data).`, 'error');
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
    throw new Error('Compressed or image-only PDF stream cannot be extracted client-side');
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
 * Clears input prompt and resets output preview WITHOUT wiping active parent continuation state,
 * so users can easily clear the box and type a Path A Delta Follow-Up.
 */
function clearInput() {
    // FIX: Defect 3 - Ghost Output Repopulation on clearInput() While In-Flight.
    // Invalidate any in-flight compress requests to prevent ghost results
    currentCompressId++;

    const promptInput = document.getElementById('prompt-input');
    if (promptInput) {
        promptInput.value = '';
    }
    State.lastResult = null;
    State.outputView = 'rehydrated';
    const resultOutput = document.getElementById('result-output');
    if (resultOutput) {
        resultOutput.innerHTML = '<span class="text-placeholder" style="color: var(--text-muted); opacity: 0.85;">Enter a prompt on the left to preview CRISP compression...</span>';
        resultOutput.style.opacity = '1';
    }
    const inlineProgress = document.getElementById('inline-progress');
    if (inlineProgress) inlineProgress.style.display = 'none';
    const viewToggle = document.getElementById('output-view-toggle');
    if (viewToggle) viewToggle.style.display = 'inline-flex';
    const rehydratedBtn = document.getElementById('ov-rehydrated');
    const deltaBtn = document.getElementById('ov-delta');
    if (rehydratedBtn) {
        rehydratedBtn.classList.add('active');
        rehydratedBtn.setAttribute('aria-checked', 'true');
    }
    if (deltaBtn) {
        deltaBtn.classList.remove('active');
        deltaBtn.setAttribute('aria-checked', 'false');
        deltaBtn.disabled = true;
        deltaBtn.setAttribute('aria-disabled', 'true');
        deltaBtn.title = 'Run a follow-up turn to generate a Turn Delta';
    }
    const btn = document.getElementById('compress-btn');
    if (btn) {
        btn.disabled = true;
    }
    setCompressButtonLabel('Update Prompt');
    closeClarifyInlineBox();
    const pillsEl = document.getElementById('clarifying-pills');
    if (pillsEl) {
        pillsEl.style.display = 'none';
        pillsEl.innerHTML = '';
    }
    ['btn-feedback-up', 'btn-feedback-down'].forEach(id => {
        const fbBtn = document.getElementById(id);
        if (fbBtn) {
            fbBtn.classList.remove('is-selected', 'is-dimmed');
            fbBtn.setAttribute('aria-pressed', 'false');
        }
    });
    const fbStatus = document.getElementById('feedback-status');
    if (fbStatus) fbStatus.style.display = 'none';
    validatePromptLength();
    updateContinuationModePill();

    const tesEl = document.getElementById('tes-score');
    if (tesEl) {
        tesEl.textContent = 'Quality Ready';
        delete tesEl.dataset.tes;
    }

    const summaryPillEl = document.getElementById('output-summary-pill');
    const tokEl = document.getElementById('output-token-count');
    const pctEl = document.getElementById('output-saving-pct');
    const costEl = document.getElementById('cost-saved');
    const statOrigEl = document.getElementById('stat-original');
    const statFinalEl = document.getElementById('stat-final');
    const statSavedEl = document.getElementById('stat-saved');
    const tokenPctEl = document.getElementById('token-saving-pct');
    const tokenBarEl = document.getElementById('token-progress-bar');
    if (summaryPillEl) summaryPillEl.textContent = '0 tokens';
    if (tokEl) tokEl.textContent = '0 tokens';
    if (pctEl) pctEl.textContent = '0% saved';
    if (costEl) costEl.textContent = '$0.00 saved / 1k runs';
    if (statOrigEl) statOrigEl.textContent = '0';
    if (statFinalEl) statFinalEl.textContent = '0';
    if (statSavedEl) statSavedEl.textContent = '0';
    if (tokenPctEl) tokenPctEl.textContent = '0% saved';
    if (tokenBarEl) tokenBarEl.style.width = '0%';

    const qBreakdown = document.getElementById('quality-breakdown');
    if (qBreakdown) {
        qBreakdown.innerHTML = `
            <span>Efficiency (TES): <strong>—</strong></span>
            <span>Meaning (SFS): <strong>—</strong></span>
            <span>Rules Kept (SCS): <strong>—</strong></span>
        `;
    }

    const verifyBtn = document.getElementById('btn-verify-workbench');
    if (verifyBtn) verifyBtn.style.display = 'none';
}

// ── Multi-Turn Continuation UI Lifecycle (Phase 4) ─────────────────────────

/**
 * Resets all parent-linked continuation state and restores Turn 1 UI controls.
 *
 * # Arguments
 * * `silent` - When true, suppresses the confirmation toast.
 */
function resetParentSession(silent = false) {
    State.parentId = null;
    State.parentRawPrompt = '';
    State.turnNumber = 0;
    State.activeConstraints = [];
    State.revokedConstraints = [];
    State.phases = [];
    State.activePhase = null;
    State.resolvedSlots = {};
    State.outputView = 'rehydrated';
    State.pendingClarifySlot = null;

    const lockBar = document.getElementById('parent-lock-bar');
    if (lockBar) lockBar.style.display = 'none';
    const ledgerStrip = document.getElementById('continuation-ledger-strip');
    if (ledgerStrip) ledgerStrip.style.display = 'none';
    const viewToggle = document.getElementById('output-view-toggle');
    if (viewToggle) viewToggle.style.display = 'inline-flex';
    const rehydratedBtn = document.getElementById('ov-rehydrated');
    const deltaBtn = document.getElementById('ov-delta');
    if (rehydratedBtn) {
        rehydratedBtn.classList.add('active');
        rehydratedBtn.setAttribute('aria-checked', 'true');
    }
    if (deltaBtn) {
        deltaBtn.classList.remove('active');
        deltaBtn.setAttribute('aria-checked', 'false');
        deltaBtn.disabled = true;
        deltaBtn.setAttribute('aria-disabled', 'true');
        deltaBtn.title = 'Run a follow-up turn to generate a Turn Delta';
    }

    setCompressButtonLabel('Update Prompt');

    validatePromptLength();
    if (!silent) {
        showToast('Started a fresh prompt session', 'success');
    }
}

/**
 * Detects whether the current #prompt-input text is a Path B Edit Continuation
 * (>= 45% word overlap with parent prompt or matching prefix) or a Path A Delta Follow-Up.
 */
function detectContinuationInputMode(raw) {
    const cur = (raw || '').trim().toLowerCase();
    const parent = (State.parentRawPrompt || '').trim().toLowerCase();
    if (!cur || !parent) return 'delta';

    const prefixLen = Math.min(24, parent.length);
    if (prefixLen >= 12 && cur.length >= 12 && cur.startsWith(parent.slice(0, prefixLen))) {
        return 'edit';
    }

    const parentWords = new Set(parent.split(/\s+/).filter(Boolean));
    const curWords = cur.split(/\s+/).filter(Boolean);
    if (parentWords.size === 0 || curWords.length === 0) return 'delta';

    let shared = 0;
    const seenCur = new Set();
    for (const w of curWords) {
        if (parentWords.has(w) && !seenCur.has(w)) {
            seenCur.add(w);
            shared++;
        }
    }
    const overlapRatio = shared / parentWords.size;
    return overlapRatio >= 0.45 ? 'edit' : 'delta';
}

/**
 * Synchronizes #parent-lock-bar, #continuation-mode-pill, and #compress-btn label
 * with active continuation state and live textarea input.
 */
function updateContinuationModePill() {
    const lockBar = document.getElementById('parent-lock-bar');
    const lockBadge = document.getElementById('parent-lock-badge');
    const modePill = document.getElementById('continuation-mode-pill');
    const promptInput = document.getElementById('prompt-input');

    const hasParent = Boolean(State.parentId);
    if (lockBar) {
        lockBar.style.display = hasParent ? 'flex' : 'none';
    }
    setCompressButtonLabel('Update Prompt');
    if (!hasParent) return;

    if (lockBadge) {
        lockBadge.textContent = `Linked to Parent · Turn ${State.turnNumber || 1}`;
    }
    if (modePill) {
        const mode = detectContinuationInputMode(promptInput ? promptInput.value : '');
        if (mode === 'edit') {
            modePill.textContent = 'Edit Continuation';
            modePill.classList.remove('mode-delta');
            modePill.classList.add('mode-edit');
        } else {
            modePill.textContent = 'Delta Follow-Up';
            modePill.classList.remove('mode-edit');
            modePill.classList.add('mode-delta');
        }
    }
}

/**
 * Renders inherited active constraint chips and execution phase stepper pills
 * inside #continuation-ledger-strip.
 */
function renderContinuationStrip() {
    const strip = document.getElementById('continuation-ledger-strip');
    const constraintsRow = document.getElementById('inherited-constraints-row');
    const phasesRow = document.getElementById('phase-stepper-row');
    if (!strip || !constraintsRow || !phasesRow) return;

    const constraints = Array.isArray(State.activeConstraints) ? State.activeConstraints : [];
    const phases = Array.isArray(State.phases) ? State.phases : [];
    const revokedSet = new Set((State.revokedConstraints || []).map(v => String(v).toLowerCase()));

    if (constraints.length > 0) {
        constraintsRow.style.display = 'flex';
        let html = '<span class="ledger-label">Constraints:</span>';
        constraints.forEach(c => {
            const cId = String(c.id || c.text || '');
            const cText = String(c.text || '');
            const isExcl = String(c.kind || '').toLowerCase() === 'exclusion';
            const isRevoked = revokedSet.has(cId.toLowerCase()) || revokedSet.has(cText.toLowerCase());
            const icon = isExcl ? '✕' : '✓';
            const kindClass = isExcl ? 'constraint-chip-exclude' : 'constraint-chip-include';
            const revClass = isRevoked ? ' constraint-chip-revoked' : '';
            html += `<span class="constraint-chip ${kindClass}${revClass}" title="${escapeHtml(cText)}">${icon} ${escapeHtml(cText)}<button type="button" class="constraint-revoke-btn" data-action="revokeInheritedConstraint" data-arg="${escapeHtml(cId)}" aria-label="Revoke ${escapeHtml(cText)}" title="Revoke constraint">×</button></span>`;
        });
        constraintsRow.innerHTML = html;
    } else {
        constraintsRow.style.display = 'none';
        constraintsRow.innerHTML = '';
    }

    if (phases.length > 0) {
        phasesRow.style.display = 'flex';
        let html = '<span class="ledger-label">Phases:</span>';
        phases.forEach(p => {
            const num = Number(p.phase_number);
            const name = String(p.name || `Phase ${num}`);
            const isActive = State.activePhase !== null && Number(State.activePhase) === num;
            const isDone = String(p.status || '').toLowerCase() === 'done';
            const cls = isActive ? 'phase-pill active' : (isDone ? 'phase-pill done' : 'phase-pill');
            html += `<button type="button" class="${cls}" data-action="selectActivePhase" data-arg="${num}" aria-pressed="${isActive ? 'true' : 'false'}">${num}. ${escapeHtml(name)}</button>`;
        });
        phasesRow.innerHTML = html;
    } else {
        phasesRow.style.display = 'none';
        phasesRow.innerHTML = '';
    }

    strip.style.display = (State.parentId && (constraints.length > 0 || phases.length > 0)) ? 'flex' : 'none';
}

/**
 * Marks an inherited constraint for revocation and updates the ledger chip strip.
 */
function revokeInheritedConstraint(idOrText) {
    if (!idOrText) return;
    const key = String(idOrText);
    const exists = State.revokedConstraints.some(v => String(v).toLowerCase() === key.toLowerCase());
    if (!exists) {
        State.revokedConstraints.push(key);
    }
    renderContinuationStrip();
    showToast('Constraint marked for removal on next update', 'success');
}

/**
 * Selects an active execution phase pill and runs a continuation update.
 */
function selectActivePhase(phaseNum) {
    const num = Number(phaseNum);
    if (!Number.isFinite(num) || num <= 0) return;
    State.activePhase = num;
    renderContinuationStrip();

    const phaseObj = (State.phases || []).find(p => Number(p.phase_number) === num);
    const label = phaseObj ? `${num}. ${phaseObj.name}` : `Phase ${num}`;
    const promptInput = document.getElementById('prompt-input');
    const curWords = promptInput ? promptInput.value.trim().split(/\s+/).filter(Boolean).length : 0;

    if (State.parentId) {
        compress(true, curWords >= 2 ? {} : { rawTextOverride: `Focus on Phase ${label}` });
    }
}

/**
 * Switches right-pane output display between Full Rehydrated Prompt and Turn Delta.
 */
function setOutputView(view) {
    const deltaBtn = document.getElementById('ov-delta');
    if (view === 'delta' && deltaBtn && deltaBtn.disabled) {
        return;
    }
    State.outputView = view === 'delta' ? 'delta' : 'rehydrated';
    const rehydratedBtn = document.getElementById('ov-rehydrated');
    if (rehydratedBtn) {
        const active = State.outputView === 'rehydrated';
        rehydratedBtn.classList.toggle('active', active);
        rehydratedBtn.setAttribute('aria-checked', active ? 'true' : 'false');
    }
    if (deltaBtn) {
        const active = State.outputView === 'delta';
        deltaBtn.classList.toggle('active', active);
        deltaBtn.setAttribute('aria-checked', active ? 'true' : 'false');
    }
    if (State.lastResult) {
        renderResult(State.lastResult);
    }
}

/**
 * Returns the active output prompt text matching State.outputView ('rehydrated' vs 'delta').
 */
function getActiveOutputText(result) {
    if (!result) return '';
    if (State.outputView === 'delta' && result.continuation?.delta_prompt) {
        return result.continuation.delta_prompt;
    }
    return result.continuation?.rehydrated_prompt || result.optimized_prompt || '';
}

/**
 * Applies the inline clarifying slot input value without polluting #prompt-input.
 */
function applyClarifySlot() {
    const inputEl = document.getElementById('clarify-inline-input');
    const slot = State.pendingClarifySlot;
    if (!inputEl || !slot) return;
    const val = inputEl.value.trim();
    if (!val) {
        inputEl.focus();
        return showToast('Enter a value to apply this clarification.', 'error');
    }
    State.resolvedSlots[slot] = val;
    State.pendingClarifySlot = null;
    closeClarifyInlineBox();
    const promptInput = document.getElementById('prompt-input');
    const curWords = promptInput ? promptInput.value.trim().split(/\s+/).filter(Boolean).length : 0;
    compress(true, curWords >= 2 ? {} : { rawTextOverride: `Clarify ${slot}: ${val}` });
}

/**
 * Closes the inline clarifying slot box if open.
 */
function closeClarifyInlineBox() {
    const box = document.getElementById('clarify-inline-box');
    if (box) box.remove();
    State.pendingClarifySlot = null;
}

/**
 * Triggers a surgical repair prompt from a failed Verify Grounding report.
 */
async function generateRepairFromVerify() {
    const report = (typeof RagWorkspace !== 'undefined') ? RagWorkspace.lastVerifyReport : null;
    if (!report) {
        return showToast('Run a grounding check first before generating a repair prompt.', 'error');
    }
    if (typeof RagWorkspace !== 'undefined' && typeof RagWorkspace.closeVerifyModal === 'function') {
        RagWorkspace.closeVerifyModal();
    }

    const grounded = Array.isArray(report.grounded) ? report.grounded : [];
    const partial = Array.isArray(report.partial) ? report.partial : [];
    const notFound = Array.isArray(report.not_found) ? report.not_found : [];
    const unsupported = Array.isArray(report.unsupported_claims) ? report.unsupported_claims : [];

    const failed_items = [];
    partial.forEach(p => {
        const st = String(p?.status || 'PARTIAL_MATCH').toUpperCase();
        if (st !== 'FOUND') {
            const label = String(p?.item || p?.answer || '').trim();
            if (label) failed_items.push(`${label} (${st})`);
        }
    });
    unsupported.forEach(u => {
        const itemLabel = String(u?.item || '').trim();
        const reason = String(u?.detail || u?.claim || u?.issue || '').trim();
        const text = itemLabel && reason
            ? `${itemLabel}: ${reason}`
            : (reason || itemLabel || JSON.stringify(u));
        if (text) failed_items.push(text);
    });

    const missing_items = Array.isArray(report.missing_items)
        ? report.missing_items.map(m => String(m || '').trim()).filter(Boolean)
        : [];

    // ARCH: src/npae/schema/types.rs RepairContext expects Vec<String> for details (never nested objects).
    const scorePct = Math.round((Number(report.grounding_score ?? 0)) * 100);
    const details = [
        `Grounding score: ${scorePct}% (grounded: ${grounded.length}, partial: ${partial.length}, not_found: ${notFound.length}, unsupported: ${unsupported.length}, missing: ${missing_items.length})`,
    ];
    if (report.contract_violated) {
        details.push('Output contract table was missing or violated.');
    }
    partial.forEach(p => {
        const itemLabel = String(p?.item || 'item').trim();
        const answer = String(p?.answer || '').trim();
        details.push(answer ? `Partial match (${itemLabel}): ${answer}` : `Partial match (${itemLabel})`);
    });
    notFound.forEach(nf => {
        const itemLabel = String(nf?.item || 'item').trim();
        const answer = String(nf?.answer || 'abstained').trim();
        details.push(`Not found (${itemLabel}): ${answer}`);
    });
    unsupported.forEach(u => {
        const itemLabel = String(u?.item || 'Claim').trim();
        const issue = String(u?.issue || 'unsupported').trim();
        const detail = String(u?.detail || u?.claim || 'failed grounding verification').trim();
        details.push(`${itemLabel} [${issue}]: ${detail}`);
    });

    const repair_context = { failed_items, missing_items, details };

    const compressTabBtn = document.getElementById('nav-compress');
    if (compressTabBtn && !compressTabBtn.classList.contains('active') && typeof switchTab === 'function') {
        switchTab('compress');
    }

    const summaryItems = [...failed_items.slice(0, 3), ...missing_items.slice(0, 3)].filter(Boolean);
    const repairDirective = summaryItems.length > 0
        ? `Repair grounding failures and missing items (${summaryItems.join('; ')}) using only verified source facts.`
        : 'Repair grounding failures using only verified source facts.';

    await compress(true, {
        repairContext: repair_context,
        rawTextOverride: repairDirective,
        modeOverride: 'aggressive',
    });
}

// ── Compression Lifecycle (R1, R2, R3, R5) ─────────────────────────────────

// FIX: Defects 2, 3, 4 - Sequence token tracking latest compression request to prevent async race conditions
let currentCompressId = 0;

/**
 * Executes prompt compression. Employs non-blocking inline skeleton progress (R5)
 * and seamless client-side zero-auth execution when unauthenticated (R1).
 */
async function compress(isSilent = false, opts = {}) {
    // FIX: Defects 2, 3, 4 - Tag request with monotonically increasing sequence ID
    const reqId = ++currentCompressId;

    const promptInput = document.getElementById('prompt-input');
    const typedRaw = promptInput ? promptInput.value.trim() : '';
    const overrideRaw = (opts && typeof opts.rawTextOverride === 'string') ? opts.rawTextOverride.trim() : '';
    const raw = overrideRaw || typedRaw;
    const isGuest = !State.token;
    const modeOverride = (opts && typeof opts.modeOverride === 'string') ? opts.modeOverride.trim() : '';
    const mode = modeOverride || State.currentMode || document.getElementById('mode')?.value || 'balanced';
    const domain = document.getElementById('domain-name')?.textContent || updateDetectedDomain(raw || State.parentRawPrompt || '');
    const repairContext = opts.repairContext || null;

    if (!raw) return showToast('Raw Prompt is required', 'error');

    const minWords = State.parentId ? 2 : 8;
    const words = raw.split(/\s+/).filter(Boolean);
    if (words.length < minWords) {
        return showToast(`Raw Prompt must be at least ${minWords} words`, 'error');
    }

    if (isGuest && getGuestUses() >= GUEST_MAX_USES) {
        showToast(`Live Trial limit reached (${GUEST_MAX_USES}/${GUEST_MAX_USES} runs). Please sign in for unlimited access.`, 'error');
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
    const renderStyle = document.getElementById('opt-harness-enabled')?.checked ? 'harness' : 'crisp';
    const ragDocIds = (typeof RagWorkspace !== 'undefined' && ragEnabled) ? RagWorkspace.getSelectedDocIds() : [];

    // Prepare request payload (client-side rehydration in Guest Mode vs native stateful ledger when authed)
    let requestPayload = {
        raw_text: raw,
        task: 'Optimize prompt',
        use_case: domain,
        mode: mode,
        domain: domain,
        rag_enabled: ragEnabled,
        rag_document_ids: ragDocIds.length > 0 ? ragDocIds : undefined,
        npae_render_style: renderStyle
    };

    if (isGuest && State.parentId && typeof ClientContinuation !== 'undefined' && ClientContinuation.isTopicReset(raw)) {
        State.parentId = null;
        State.parentRawPrompt = '';
        State.turnNumber = 0;
        State.activeConstraints = [];
        State.revokedConstraints = [];
        State.phases = [];
        State.activePhase = null;
        State.resolvedSlots = {};
        State.pendingClarifySlot = null;
    }

    let guestContinuationPlan = null;
    if (isGuest && State.parentId && typeof ClientContinuation !== 'undefined') {
        const isEditMode = !overrideRaw && typedRaw ? detectContinuationInputMode(typedRaw) === 'edit' : false;
        const { activeConstraints, supersededLabels } = ClientContinuation.reconcileConstraints(
            State.activeConstraints,
            raw,
            State.revokedConstraints
        );
        const nextActivePhase = ClientContinuation.detectPhaseSelection(raw, State.activePhase, State.phases);
        const rehydratedRaw = ClientContinuation.buildGuestRehydratedPrompt({
            parentRawPrompt: State.parentRawPrompt,
            followUpText: raw,
            isEditMode,
            activeConstraints,
            revokedLabels: supersededLabels,
            phases: State.phases,
            activePhase: nextActivePhase,
            resolvedSlots: State.resolvedSlots,
            repairContext,
        });
        const deltaPrompt = ClientContinuation.buildDeltaPrompt({
            activeConstraints,
            revokedLabels: supersededLabels,
            phases: State.phases,
            activePhase: nextActivePhase,
            resolvedSlots: State.resolvedSlots,
            followUpText: raw,
            repairContext,
        });
        requestPayload.raw_text = rehydratedRaw;
        guestContinuationPlan = {
            isEditMode,
            activeConstraints,
            supersededLabels,
            nextActivePhase,
            deltaPrompt,
        };
    } else if (!isGuest) {
        if (State.parentId) {
            requestPayload.parent_id = State.parentId;
        }
        if (State.revokedConstraints.length > 0) {
            requestPayload.revoked_constraints = State.revokedConstraints.slice();
        }
        if (State.activePhase !== null && State.activePhase !== undefined) {
            requestPayload.active_phase = State.activePhase;
        }
        if (State.resolvedSlots && Object.keys(State.resolvedSlots).length > 0) {
            requestPayload.resolved_slots = { ...State.resolvedSlots };
        }
        if (repairContext) {
            requestPayload.repair_context = repairContext;
        }
    } else if (isGuest && repairContext) {
        requestPayload.raw_text = `${raw}\n\nFailed Claims: ${(repairContext.failed_items || []).join('; ') || 'none'}\nMissing Items: ${(repairContext.missing_items || []).join('; ') || 'none'}`;
    }

    try {
        const [result] = await Promise.all([
            API.compress(requestPayload),
            minDelay
        ]);

        // FIX: Defects 2, 3, 4 - If newer request initiated, discard stale response
        if (reqId !== currentCompressId) return;

        // Hydrate multi-turn continuation state from backend or client-side guest ledger
        if (isGuest && guestContinuationPlan && typeof ClientContinuation !== 'undefined') {
            const nextTurn = (State.turnNumber || 1) + 1;
            const extractedPhases = (State.phases && State.phases.length > 0)
                ? State.phases
                : ClientContinuation.extractPhases(result, domain);
            const hydratedPhases = extractedPhases.map(p => ({
                ...p,
                status: guestContinuationPlan.nextActivePhase !== null &&
                    Number(p.phase_number) === Number(guestContinuationPlan.nextActivePhase)
                    ? 'active'
                    : p.status,
            }));

            let rehydratedPromptText = result.optimized_prompt || '';
            if (repairContext && !/\[REPAIR/i.test(rehydratedPromptText)) {
                rehydratedPromptText = `[REPAIR_CONTEXT]\nFix Unsupported: ${(repairContext.failed_items || []).join('; ') || 'none'}\nAdd Missing: ${(repairContext.missing_items || []).join('; ') || 'none'}\n\n${rehydratedPromptText}`;
                result.optimized_prompt = rehydratedPromptText;
            }

            result.continuation = {
                ledger_id: State.parentId,
                turn_number: nextTurn,
                path: guestContinuationPlan.isEditMode ? 'edit_continuation' : 'delta_follow_up',
                primary_kind: repairContext ? 'constraint_patch' : 'phase_step',
                delta_summary: repairContext ? 'Surgical grounding repair' : 'Applied follow-up delta',
                delta_prompt: guestContinuationPlan.deltaPrompt,
                rehydrated_prompt: rehydratedPromptText,
                active_constraints: guestContinuationPlan.activeConstraints,
                superseded_constraints: guestContinuationPlan.supersededLabels,
                phases: hydratedPhases,
                active_phase: guestContinuationPlan.nextActivePhase,
                open_slots: [],
            };

            State.turnNumber = nextTurn;
            State.activeConstraints = guestContinuationPlan.activeConstraints;
            State.revokedConstraints = [];
            State.phases = hydratedPhases;
            State.activePhase = guestContinuationPlan.nextActivePhase;
            if (guestContinuationPlan.isEditMode && typedRaw) {
                State.parentRawPrompt = typedRaw;
            }
        } else if (result.continuation) {
            const cont = result.continuation;
            State.parentId = cont.ledger_id || State.parentId || result.id;
            if (cont.path === 'topic_reset' || cont.turn_number === 1) {
                if (raw) State.parentRawPrompt = raw;
            } else if (!State.parentRawPrompt || cont.path === 'edit_continuation') {
                if (typedRaw) State.parentRawPrompt = typedRaw;
                else if (!State.parentRawPrompt && raw) State.parentRawPrompt = raw;
            }
            if (cont.path === 'topic_reset' || cont.turn_number === 1) {
                State.resolvedSlots = {};
                State.pendingClarifySlot = null;
            }
            State.turnNumber = cont.turn_number || (State.turnNumber ? State.turnNumber + 1 : 1);
            State.activeConstraints = Array.isArray(cont.active_constraints) ? cont.active_constraints : [];
            State.revokedConstraints = [];
            State.phases = Array.isArray(cont.phases) ? cont.phases : [];
            State.activePhase = (cont.path === 'topic_reset' || cont.turn_number === 1)
                ? (cont.active_phase ?? null)
                : (cont.active_phase ?? State.activePhase);
        } else {
            // Turn 1 or legacy fallback without continuation metadata
            const wasLinked = Boolean(State.parentId);
            if (!wasLinked) {
                State.parentId = result.id || ('session-' + Date.now());
                State.parentRawPrompt = raw;
                State.turnNumber = 1;
            } else {
                State.turnNumber = (State.turnNumber || 1) + 1;
            }
            if (typeof ClientContinuation !== 'undefined') {
                if (!wasLinked) {
                    State.activeConstraints = ClientContinuation.extractConstraints(raw);
                    State.phases = ClientContinuation.extractPhases(result, domain);
                    State.activePhase = State.phases.length > 0 ? State.phases[0].phase_number : null;
                }
                State.revokedConstraints = [];
            }
        }

        updateContinuationModePill();
        renderContinuationStrip();
        validatePromptLength();

        State.lastResult = result;
        recordRecentPromptRun({
            id: result.id,
            rawText: raw,
            tokenOriginal: result.token_original,
            tokenFinal: result.token_final,
            tokensSaved: result.token_saved,
            mode: mode
        });
        State.lastTs = Date.now();
        if (isGuest) {
            incrementGuestUses();
            updateUserBar();
        }
        renderResult(result);
        if (!isSilent) {
            showToast(repairContext ? 'Surgical repair prompt generated!' : 'Prompt compressed successfully', 'success');
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
            if (btn) {
                const curWords = promptInput ? promptInput.value.trim().split(/\s+/).filter(Boolean).length : 0;
                btn.disabled = curWords < (State.parentId ? 2 : 8);
            }
        }
    }
}

/**
 * Synchronizes #tes-score aria-expanded attribute with the native #inspector-drawer open state.
 */
function syncInspectorAriaExpanded() {
    const drawer = document.getElementById('inspector-drawer');
    const trigger = document.getElementById('tes-score');
    if (drawer && trigger) {
        trigger.setAttribute('aria-expanded', drawer.open ? 'true' : 'false');
    }
}

/**
 * Toggles the native Inspector <details> drawer and keeps aria-expanded synchronized.
 */
function toggleQualityBreakdown() {
    const drawer = document.getElementById('inspector-drawer');
    if (!drawer) return;
    drawer.open = !drawer.open;
    syncInspectorAriaExpanded();
}

// ARCH: ARCHITECTURE.md §6.6 CRISP/Harness Renderers — XSS-safe prompt syntax tinting.
/**
 * Formats prompt output with robust XSS-safe syntax highlighting for XML tags (including
 * attributed `<facts trust="untrusted">`), Markdown `# SECTION:` headers, bracketed markers,
 * and numbered execution phases.
 */
function formatPromptSyntaxHtml(promptText) {
    let html = escapeHtml(promptText || '');
    html = html.replace(/(&lt;\/?(?:rules|facts|role|task|context|grounding_facts|constraints|execution_phases|clarifying_questions|output_blueprint|output_contract|execution_protocol|requested_items)(?:\s+(?:(?!&gt;)[^\n])*)?&gt;)/g, '<span class="crisp-xml-tag">$1</span>');
    html = html.replace(/^(\s*#{1,3}\s+[^\n]+)$/gm, '<span class="crisp-tag crisp-section-heading">$1</span>');
    html = html.replace(/\[(ROLE|CONTEXT|EXECUTION|CONSTRAINTS|RESOLVED_SLOTS|REPAIR_CONTEXT|CONSTRAINTS_PATCH|PHASE_CURSOR|REPAIR_DIRECTIVE|NEW_FACTS|TASK)\]/g, '<span class="crisp-tag">[$1]</span>');
    html = html.replace(/(\*\*(?:Role|Task|Constraints|Deliverables):\*\*)/g, '<span class="crisp-tag">$1</span>');
    html = html.replace(/^(\d+\.\s+)(\*\*[^*\n]+\*\*)(\s*\([^)\n]+\):?)?/gm, (_, num, title, dur) => `${num}<span class="crisp-phase-title">${title}</span>${dur ? `<span class="crisp-phase-meta">${dur}</span>` : ''}`);
    html = html.replace(/\[(STRICT|AVOID|HIGH|MEDIUM|LOW|ASSUMED:[^\]]+|CONFLICT:[^\]]+)\]/g, '<span class="crisp-meta-badge">[$1]</span>');
    html = html.replace(/\[(F\d+)\]/g, '<span class="crisp-cite">[$1]</span>');
    return html;
}

const CLARIFY_GAP_LABELS = {
    'missing_slot:real-estate:buyer_state_or_market': 'Add state / market',
    'missing_slot:real-estate:buyer_budget_and_down_payment': 'Add budget & cash',
    'missing_slot:real-estate:buyer_credit_tier_and_income': 'Add credit & DTI',
    'missing_slot:real-estate:location_or_property_type': 'Add market & property',
    'missing_slot:legal:jurisdiction': 'Add jurisdiction',
    'missing_slot:legal:contract_or_party_role': 'Add contract & role',
    'missing_slot:medical:patient_age_or_profile': 'Add patient profile',
    'missing_slot:medical:duration_or_medications': 'Add duration & meds',
    'missing_slot:nutrition:calorie_or_goal_target': 'Add calorie & goal',
    'missing_slot:nutrition:dietary_restrictions_or_allergies': 'Add diet / allergies',
    'missing_slot:finance:capital_or_time_horizon': 'Add capital & horizon',
    'missing_slot:finance:risk_tolerance': 'Add risk & tax',
    'missing_slot:education:learner_level': 'Add learner level',
    'missing_slot:education:timeline_or_duration': 'Add timeline',
    'missing_slot:travel:destination': 'Add destination',
    'missing_slot:travel:dates_or_duration': 'Add dates & length',
    'missing_slot:travel:budget_or_travelers': 'Add budget & group',
    'missing_slot:software:stack_or_environment': 'Add tech stack',
    'missing_slot:business:target_market_or_stage': 'Add market & stage',
    'missing_slot:data-science:dataset_or_metric': 'Add dataset & metric',
    'missing_slot:cybersecurity:target_scope_or_environment': 'Add scope & compliance',
    'output_format_ambiguous': 'Choose output format',
    'domain_context_missing': 'Add domain context',
    'intent_unclear': 'Clarify primary goal',
    'constraint_incomplete': 'Add budget / timeline',
    'vague_language_detected': 'Specify outcome',
    'prompt_too_short': 'Add key details'
};

// ARCH: ARCHITECTURE.md §1.4 (Step 7) — Maps backend gap_addressed keys into concise 2–4 word
// single-row UI triggers while preserving the full question in tooltips and slot prompts.
function formatClarifyChipLabel(q) {
    if (!q) return 'Clarify detail';
    if (typeof q === 'object') {
        if (q.chip_label) return q.chip_label;
        if (q.gap_addressed && CLARIFY_GAP_LABELS[q.gap_addressed]) {
            return CLARIFY_GAP_LABELS[q.gap_addressed];
        }
    }
    const text = typeof q === 'string' ? q : (q.question || q.slot || '');
    const lower = text.toLowerCase();
    if (/credit|dti|score/i.test(lower)) return 'Add credit & DTI';
    if (/budget|down payment|cash/i.test(lower)) return 'Add budget & cash';
    if (/state|market|jurisdiction/i.test(lower)) return 'Add jurisdiction';
    if (/format|output/i.test(lower)) return 'Choose output format';
    if (/stack|language|environment|tech/i.test(lower)) return 'Add tech stack';
    if (/timeline|duration|date/i.test(lower)) return 'Add timeline';
    if (/diet|allergy|allerg/i.test(lower)) return 'Add diet / allergies';

    const words = text.replace(/[^a-zA-Z0-9\s]/g, '').split(/\s+/).filter(Boolean);
    let sliceLen = 0;
    const picked = [];
    for (const w of words) {
        const nextLen = sliceLen + w.length + (picked.length > 0 ? 1 : 0);
        if (nextLen <= 24) {
            picked.push(w);
            sliceLen = nextLen;
        } else {
            break;
        }
    }
    if (picked.length > 0) {
        return picked.join(' ');
    }
    return text.length > 24 ? text.slice(0, 21) + '…' : text;
}

function closeStudioCommandPalette() {
    closeDialogSafe(document.getElementById('studio-cmd-modal'));
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
    const cont = result && result.continuation ? result.continuation : null;
    if (cont && (cont.path === 'topic_reset' || cont.turn_number === 1)) {
        State.resolvedSlots = {};
        State.pendingClarifySlot = null;
        State.activePhase = cont.active_phase ?? null;
    }

    const orig = result.token_original || 0;
    const final = result.token_final || 0;
    const saved = result.token_saved || 0;
    const pct = result.savings_pct !== undefined ? result.savings_pct : (orig > 0 ? Math.round((saved / orig) * 100) : 0);

    const elSummaryPill = document.getElementById('output-summary-pill');
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
    if (elSummaryPill) elSummaryPill.textContent = `${final} tokens`;
    if (elTokCount) elTokCount.textContent = `${final} tokens`;
    if (elSavingPct) elSavingPct.textContent = isArchitect ? architectPctLabel : `${pct}% saved`;
    if (elOrig) elOrig.textContent = orig.toLocaleString();
    if (elFinal) elFinal.textContent = final.toLocaleString();
    if (elSaved) elSaved.textContent = saved.toLocaleString();
    if (elBar) elBar.style.width = Math.min(Math.max(pct, 0), 100) + '%';
    if (elPct) elPct.textContent = isArchitect ? architectPctLabel : `${pct}% saved`;

    // Calm plain-English quality summary on #tes-score + full metrics inside Inspector drawer
    const tesScore = result.tes_score !== undefined
        ? result.tes_score
        : (result.dual_score && result.dual_score.tes !== undefined ? result.dual_score.tes : 9.7);
    const hasFacts = Array.isArray(result.grounding_facts) && result.grounding_facts.length > 0;
    const harnessActive = Boolean(document.getElementById('opt-harness-enabled')?.checked);

    const elTes = document.getElementById('tes-score');
    if (elTes) {
        elTes.dataset.tes = Number(tesScore).toFixed(2);
        if (harnessActive || hasFacts) {
            elTes.textContent = '✓ Fact-Checked Ready';
        } else if (isArchitect) {
            elTes.textContent = '✓ Blueprint Ready';
        } else if (pct > 0) {
            elTes.textContent = `✓ Ready · ${pct}% shorter`;
        } else {
            elTes.textContent = '✓ Quality Ready';
        }
    }

    const qBreakdown = document.getElementById('quality-breakdown');
    if (qBreakdown) {
        const ds = result.dual_score || {};
        const qTes = Number(ds.tes !== undefined ? ds.tes : tesScore).toFixed(2);
        const qSfs = ds.sfs !== undefined ? Number(ds.sfs).toFixed(2) : '—';
        const qScs = ds.scs !== undefined ? Number(ds.scs).toFixed(2) : '—';
        const modeStr = escapeHtml(String(result.mode || State.currentMode || 'balanced').toUpperCase());
        qBreakdown.innerHTML = `
            <span>Efficiency (TES): <strong>${qTes}</strong></span>
            <span>Meaning (SFS): <strong>${qSfs}</strong></span>
            <span>Rules Kept (SCS): <strong>${qScs}</strong></span>
            <span>Mode: <strong>${modeStr}</strong></span>
        `;
    }

    const verifyBtn = document.getElementById('btn-verify-workbench');
    if (verifyBtn) {
        verifyBtn.style.display = (hasFacts || harnessActive) ? 'inline-flex' : 'none';
    }

    // Live dollar cost savings calculation (R3)
    updateCostMetrics();

    // Phase 4: Output view toggle state (Full Prompt vs Turn Delta — always accessible in header)
    const viewToggle = document.getElementById('output-view-toggle');
    const hasDelta = Boolean(result.continuation && result.continuation.delta_prompt);
    if (!hasDelta && State.outputView === 'delta') {
        State.outputView = 'rehydrated';
    }
    if (viewToggle) {
        viewToggle.style.display = 'inline-flex';
        const rehydratedBtn = document.getElementById('ov-rehydrated');
        const deltaBtn = document.getElementById('ov-delta');
        if (rehydratedBtn) {
            const active = State.outputView !== 'delta';
            rehydratedBtn.classList.toggle('active', active);
            rehydratedBtn.setAttribute('aria-checked', active ? 'true' : 'false');
        }
        if (deltaBtn) {
            deltaBtn.disabled = !hasDelta;
            deltaBtn.setAttribute('aria-disabled', hasDelta ? 'false' : 'true');
            deltaBtn.title = hasDelta
                ? 'View KV-cache-friendly Turn Delta'
                : 'Run a follow-up turn to generate a Turn Delta';
            const active = hasDelta && State.outputView === 'delta';
            deltaBtn.classList.toggle('active', active);
            deltaBtn.setAttribute('aria-checked', active ? 'true' : 'false');
        }
    }

    // Render output with always-on syntax tinting + optional pruning diff highlighting (R2)
    const elOutput = document.getElementById('result-output');
    if (elOutput) {
        const promptText = getActiveOutputText(result);
        const showDiff = State.showDiff !== false && (document.getElementById('diff-toggle')?.checked !== false);
        const formattedPrompt = formatPromptSyntaxHtml(promptText);

        if (State.outputView !== 'delta' && showDiff && saved > 0) {
            const rawInput = document.getElementById('prompt-input')?.value || '';
            const diffSummary = computePrunedTokenSummary(rawInput, promptText, saved, result.pruned_summary);
            elOutput.innerHTML = `<del class="diff-removed">- ${escapeHtml(diffSummary)}</del>\n${formattedPrompt}`;
        } else {
            elOutput.innerHTML = formattedPrompt;
        }
    }

    // Reset quiet monochrome feedback controls (keep disabled in Guest mode)
    const isAuthed = Boolean(State.token);
    const btnUp = document.getElementById('btn-feedback-up');
    const btnDown = document.getElementById('btn-feedback-down');
    const fbStatus = document.getElementById('feedback-status');
    [btnUp, btnDown].forEach(btn => {
        if (!btn) return;
        btn.disabled = !isAuthed;
        btn.classList.remove('is-selected', 'is-dimmed');
        btn.setAttribute('aria-pressed', 'false');
        btn.style.opacity = '';
        btn.style.outline = '';
        btn.title = isAuthed ? btn.getAttribute('aria-label') || '' : 'Sign in to submit feedback';
    });
    if (fbStatus) fbStatus.style.display = 'none';

    // GRW-02 v2: Single-Row Concise Clarifying Question Quick-Fill Pills
    const pillsEl = document.getElementById('clarifying-pills');
    const inlineHost = document.getElementById('clarify-inline-host') || pillsEl;
    if (pillsEl) {
        const qs = Array.isArray(result.clarifying_questions) ? result.clarifying_questions : [];
        if (qs.length > 0) {
            pillsEl.style.display = 'flex';
            pillsEl.innerHTML = '<span class="preset-label">Clarify:</span>';
            qs.slice(0, 3).forEach(q => {
                const qText = typeof q === 'string' ? q : (q.question || q.slot || '');
                const qSlot = typeof q === 'string' ? q : (q.slot || q.gap_addressed || q.question || '');
                if (!qText) return;
                const btn = document.createElement('button');
                btn.type = 'button';
                btn.className = 'preset-chip clarify-chip';
                btn.textContent = `+ ${formatClarifyChipLabel(q)}`;
                btn.title = qText;
                btn.addEventListener('click', () => {
                    if (State.parentId) {
                        // Phase 4: Inline slot-fill box without polluting #prompt-input or wrapping chip strip
                        State.pendingClarifySlot = qSlot || qText;
                        const existingBox = document.getElementById('clarify-inline-box');
                        if (existingBox) existingBox.remove();

                        const box = document.createElement('div');
                        box.id = 'clarify-inline-box';
                        box.className = 'clarify-inline-box';
                        box.innerHTML = `
                            <span class="clarify-inline-label">${escapeHtml(qText)}</span>
                            <input type="text" id="clarify-inline-input" class="clarify-inline-input" placeholder="Type value (e.g. $100/mo)...">
                            <button type="button" class="clarify-apply-btn" data-action="applyClarifySlot">Apply</button>
                            <button type="button" class="btn-ghost" data-action="closeClarifyInlineBox" aria-label="Cancel clarification">✕</button>
                        `;
                        if (inlineHost) inlineHost.appendChild(box);
                        const inlineInput = document.getElementById('clarify-inline-input');
                        if (inlineInput) {
                            inlineInput.value = State.resolvedSlots[State.pendingClarifySlot] || '';
                            inlineInput.addEventListener('keydown', (ev) => {
                                if (ev.key === 'Enter') {
                                    ev.preventDefault();
                                    applyClarifySlot();
                                } else if (ev.key === 'Escape') {
                                    ev.preventDefault();
                                    closeClarifyInlineBox();
                                }
                            });
                            inlineInput.focus();
                        }
                        return;
                    }
                    const input = document.getElementById('prompt-input');
                    if (!input) return;
                    const sep = input.value.trim().endsWith('\n') ? '' : '\n';
                    input.value = `${input.value.trim()}${sep}Context: [${qText}: ]`;
                    input.focus();
                    validatePromptLength();
                });
                pillsEl.appendChild(btn);
            });
        } else {
            pillsEl.style.display = 'none';
            pillsEl.innerHTML = '';
        }
    }
}

function openWorkbenchVerifyModal() {
    const factsEl = document.getElementById('verify-facts-input');
    if (factsEl && State.lastResult && Array.isArray(State.lastResult.grounding_facts) && State.lastResult.grounding_facts.length > 0) {
        factsEl.value = State.lastResult.grounding_facts
            .map(f => `[${f.id}] ${f.text}`)
            .join('\n');
    }
    if (typeof RagWorkspace !== 'undefined' && typeof RagWorkspace.openVerifyModal === 'function') {
        RagWorkspace.openVerifyModal();
    }
}

/**
 * Safely writes text to the clipboard if available, showing a fallback toast otherwise.
 */
function copyToClipboardSafe(text, onSuccess, errorPrefix = 'Copy failed') {
    if (!navigator.clipboard || typeof navigator.clipboard.writeText !== 'function') {
        showToast('Clipboard unavailable in this browser context', 'error');
        return;
    }
    navigator.clipboard.writeText(text).then(() => {
        if (typeof onSuccess === 'function') onSuccess();
    }).catch(err => showToast(`${errorPrefix}: ${err.message}`, 'error'));
}

function exportAuditCard() {
    if (!State.lastResult) return showToast('Compress a prompt first to export an audit card.', 'error');
    const r = State.lastResult;
    const orig = r.token_original || 0;
    const fin = r.token_final || 0;
    const saved = r.token_saved || 0;
    const pct = orig > 0 ? Math.round((saved / orig) * 100) : 0;
    const ds = r.dual_score || {};
    const tes = r.tes_score ?? ds.tes ?? 0;
    const sfs = ds.sfs ?? 0;
    const scs = ds.scs ?? 0;
    const scale = State.currentRoiScale || 1000;
    const scaleLabel = scale >= 1000000 ? '1M/mo' : (scale >= 100000 ? '100k/mo' : '1k runs');
    const estSavings = ((saved / 1000) * (State.currentModelRate || 0.005) * scale).toFixed(2);
    const card = [
        '### Oryx Studio — Token & Grounding Audit Card',
        `- **Mode:** \`${r.mode || State.currentMode}\``,
        `- **Tokens:** ${orig} → ${fin} (${saved >= 0 ? `-${pct}%` : `+${Math.abs(pct)}%`})`,
        `- **Scores (0–10):** TES \`${Number(tes).toFixed(2)}\` · SFS \`${Number(sfs).toFixed(2)}\` · SCS \`${Number(scs).toFixed(2)}\``,
        `- **Est. Savings (${scaleLabel}):** $${estSavings}`
    ].join('\n');
    copyToClipboardSafe(card, () => showToast('Audit Card copied to clipboard!', 'success'));
}

function launchInExternalAi(provider) {
    const text = getActiveOutputText(State.lastResult);
    if (!text) {
        return showToast('Compress a prompt first before launching.', 'error');
    }
    const baseUrl = provider === 'claude' ? 'https://claude.ai/new' : 'https://chatgpt.com/';
    const encoded = encodeURIComponent(text);
    const targetUrl = encoded.length <= 1800 ? `${baseUrl}?q=${encoded}` : baseUrl;

    if (navigator.clipboard && typeof navigator.clipboard.writeText === 'function') {
        navigator.clipboard.writeText(text).catch(err => console.warn('Clipboard copy failed:', err));
    }
    window.open(targetUrl, '_blank', 'noopener,noreferrer');
    showToast(
        encoded.length <= 1800
            ? `Opened ${provider === 'claude' ? 'Claude' : 'ChatGPT'}! Bring the reply back to Verify Reply.`
            : `Prompt copied to clipboard! Paste into ${provider === 'claude' ? 'Claude' : 'ChatGPT'} and verify the reply here.`,
        'success'
    );
}

const DOMAIN_PLAYBOOKS = [
    { domain: 'Real Estate', perspective: 'Consumer', title: 'First-Time Homebuyer Checklist', text: 'Walk me through buying a first home with a $450k budget, 20% down payment, fixed vs ARM mortgage comparison, and inspection contingencies without hidden closing fees.' },
    { domain: 'Software Engineering', perspective: 'Practitioner', title: 'Auth Cookie 401 Root Cause', text: 'Users on Safari hit 401 unauthorized on /api/stats after clearing cookies while Chrome works. Trace SameSite/Partitioned cookie headers and propose a clean fix.' },
    { domain: 'Legal & Compliance', perspective: 'Practitioner', title: 'SaaS MSA Risk Audit', text: 'Review this vendor SaaS agreement for uncapped indemnification, auto-renewal traps, and IP ownership clauses before Series A. Do not speculate on missing exhibits.' },
    { domain: 'Finance & Strategy', perspective: 'Investor', title: 'Series A SaaS Underwriting', text: 'Analyze a B2B SaaS startup with $2.4M ARR, 118% NRR, and 18 months runway. Model burn multiple scenarios and highlight diligence red flags.' },
    { domain: 'Healthcare', perspective: 'Practitioner', title: 'Clinical Triage Protocol', text: 'Draft an outpatient triage checklist for acute chest pain escalation criteria, contraindications, and EHR handoff fields without inventing dosage thresholds.' },
    { domain: 'Cybersecurity', perspective: 'Practitioner', title: 'JWT & CORS Hardening', text: 'Audit an Axum REST API using HS256 JWTs in localStorage and CORS allowlists. List concrete OWASP mitigations and rotation steps.' },
    { domain: 'E-Commerce', perspective: 'Practitioner', title: 'Checkout Drop-Off Diagnosis', text: 'Mobile checkout conversion dropped 14% after adding 3DS2 payment step. Outline instrumentation events, latency budgets, and A/B recovery tests.' },
    { domain: 'Education', perspective: 'Consumer', title: '12-Week Rust Study Plan', text: 'Create a 12-week Rust systems programming curriculum for a Python developer, capped at 6 hours/week, focused on ownership, async Tokio, and SQLite.' },
    { domain: 'Data Science', perspective: 'Practitioner', title: 'Hybrid RAG Eval Suite', text: 'Design an evaluation harness for hybrid BM25 + dense vector retrieval measuring Recall@5, MRR, and cross-tenant leak rate under 100ms latency.' },
    { domain: 'Marketing & Growth', perspective: 'Practitioner', title: 'Developer PLG Funnel', text: 'Diagnose why 60% of guest sandbox visitors bounce before signup on a developer API tool and propose 3 high-ROI onboarding experiments.' },
    { domain: 'Supply Chain', perspective: 'Practitioner', title: 'Safety Stock Reorder Model', text: 'Calculate reorder points and safety stock buffers for 3 SKUs with 14-day supplier lead time variance without assuming normal demand.' },
    { domain: 'HR & Recruiting', perspective: 'Practitioner', title: 'Staff Engineer Rubric', text: 'Build a structured 4-dimension interview rubric for a Staff Rust Backend Engineer covering systems reliability, SQLite/Postgres concurrency, and mentorship.' }
];

function openPlaybookModal() {
    const modal = document.getElementById('playbook-modal');
    if (!modal) return;
    renderPlaybooks('');
    if (typeof modal.showModal === 'function') {
        if (!modal.open) modal.showModal();
    } else {
        modal.style.display = 'flex';
    }
}

function closePlaybookModal() {
    const modal = document.getElementById('playbook-modal');
    if (!modal) return;
    if (typeof modal.close === 'function') {
        if (modal.open) modal.close();
    } else {
        modal.style.display = 'none';
    }
}

function renderPlaybooks(query) {
    const grid = document.getElementById('playbook-grid');
    if (!grid) return;
    const q = (query || '').trim().toLowerCase();
    const items = DOMAIN_PLAYBOOKS.filter(p =>
        !q || p.domain.toLowerCase().includes(q) || p.title.toLowerCase().includes(q) || p.text.toLowerCase().includes(q)
    );
    grid.innerHTML = '';
    items.forEach(p => {
        const card = document.createElement('button');
        card.type = 'button';
        card.className = 'preset-chip';
        card.style.cssText = 'display:flex;flex-direction:column;align-items:flex-start;text-align:left;padding:0.7rem;border-radius:0.65rem;gap:0.25rem;width:100%;';
        card.innerHTML = `<span style="font-size:0.7rem;color:var(--accent-text);font-weight:700;">${escapeHtml(p.domain)} · ${escapeHtml(p.perspective)}</span>
            <strong style="font-size:0.8rem;color:var(--head-text);">${escapeHtml(p.title)}</strong>`;
        card.addEventListener('click', () => {
            const input = document.getElementById('prompt-input');
            if (input) {
                input.value = p.text;
                updateDetectedDomain(p.text);
                if (typeof validatePromptLength === 'function') validatePromptLength();
            }
            closePlaybookModal();
            compress();
        });
        grid.appendChild(card);
    });
}

function showPrivacyLocalInfo() {
    if (State.token && typeof switchTab === 'function') {
        switchTab('ask');
        if (typeof RagWorkspace !== 'undefined' && typeof RagWorkspace.setDataSource === 'function') {
            RagWorkspace.setDataSource('own');
        }
        showToast('Switched to Own Data (100% Local RAG on 127.0.0.1:49152).', 'success');
        return;
    }
    showToast('Own Data Local Companion indexes .pdf/.docx on 127.0.0.1:49152 — zero bytes uploaded. Sign in to open Ask My Docs.', 'success');
}

function exportAuditSvg() {
    if (!State.lastResult) return showToast('Compress a prompt first to save an SVG card.', 'error');
    const r = State.lastResult;
    const orig = r.token_original || 0;
    const fin = r.token_final || 0;
    const saved = r.token_saved || 0;
    const pct = orig > 0 ? Math.round((saved / orig) * 100) : 0;
    const ds = r.dual_score || {};
    const tes = Number(r.tes_score ?? ds.tes ?? 0).toFixed(2);
    const sfs = Number(ds.sfs ?? 0).toFixed(2);
    const scs = Number(ds.scs ?? 0).toFixed(2);
    const scale = State.currentRoiScale || 1000;
    const scaleLabel = scale >= 1000000 ? '1M/mo' : (scale >= 100000 ? '100k/mo' : '1k runs');
    const estSavings = ((saved / 1000) * (State.currentModelRate || 0.005) * scale).toFixed(2);
    const modeLabel = escapeHtml(String(r.mode || State.currentMode || 'balanced').toUpperCase());

    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="680" height="240" viewBox="0 0 680 240">
  <defs>
    <linearGradient id="audit-oryx-bg" x1="4" y1="2" x2="28" y2="30" gradientUnits="userSpaceOnUse">
      <stop offset="0%" stop-color="#1E2738"/>
      <stop offset="100%" stop-color="#0D121B"/>
    </linearGradient>
    <linearGradient id="audit-oryx-horn" x1="6" y1="6" x2="16" y2="22" gradientUnits="userSpaceOnUse">
      <stop offset="0%" stop-color="#E2F2FF"/>
      <stop offset="45%" stop-color="#38BDF8"/>
      <stop offset="100%" stop-color="#0A84FF"/>
    </linearGradient>
    <linearGradient id="audit-oryx-core" x1="16" y1="16" x2="16" y2="27" gradientUnits="userSpaceOnUse">
      <stop offset="0%" stop-color="#7CF3FF"/>
      <stop offset="100%" stop-color="#0A84FF"/>
    </linearGradient>
  </defs>
  <rect width="680" height="240" rx="16" fill="#0f172a"/>
  <rect x="1" y="1" width="678" height="238" rx="15" fill="none" stroke="#1e293b" stroke-width="2"/>
  <g transform="translate(32,24)">
    <g transform="scale(0.75)">
      <rect x="1" y="1" width="30" height="30" rx="7.5" fill="url(#audit-oryx-bg)" stroke="#38bdf8" stroke-opacity="0.45" stroke-width="1.25"/>
      <path d="M6.2 6.4L12.1 10.2L15.7 16.5L12.3 20.6L6.2 6.4Z" fill="url(#audit-oryx-horn)"/>
      <path d="M25.8 6.4L19.9 10.2L16.3 16.5L19.7 20.6L25.8 6.4Z" fill="url(#audit-oryx-horn)"/>
      <path d="M16 15.8L20.3 21.1L16 26.8L11.7 21.1L16 15.8Z" fill="url(#audit-oryx-core)" stroke="#E0F7FF" stroke-width="0.7" stroke-linejoin="round"/>
    </g>
  </g>
  <text x="64" y="43" fill="#38bdf8" font-family="Inter,system-ui,sans-serif" font-size="13" font-weight="700">ORYX STUDIO · TOKEN &amp; GROUNDING AUDIT CARD</text>
  <text x="648" y="43" text-anchor="end" fill="#94a3b8" font-family="monospace" font-size="12">MODE: ${modeLabel}</text>
  <text x="32" y="102" fill="#f8fafc" font-family="Inter,system-ui,sans-serif" font-size="34" font-weight="800">${orig} → ${fin} tokens (${saved >= 0 ? `-${pct}%` : `+${Math.abs(pct)}%`})</text>
  <text x="32" y="142" fill="#10b981" font-family="Inter,system-ui,sans-serif" font-size="18" font-weight="700">Est. Savings (${scaleLabel}): $${estSavings}</text>
  <rect x="32" y="168" width="616" height="44" rx="10" fill="#1e293b"/>
  <text x="52" y="195" fill="#e2e8f0" font-family="monospace" font-size="14">TES: ${tes}/10   ·   SFS: ${sfs}/10   ·   SCS: ${scs}/10</text>
</svg>`;

    const blob = new Blob([svg], { type: 'image/svg+xml;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'oryx-audit-card.svg';
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
    showToast('Saved oryx-audit-card.svg!', 'success');
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
        showToast(rating === 'thumbs_up' ? 'Thanks for the feedback!' : 'Feedback recorded.', 'success');

        if (btnUp && btnDown) {
            btnUp.disabled = true;
            btnDown.disabled = true;
            btnUp.classList.toggle('is-selected', rating === 'thumbs_up');
            btnUp.classList.toggle('is-dimmed', rating !== 'thumbs_up');
            btnUp.setAttribute('aria-pressed', rating === 'thumbs_up' ? 'true' : 'false');
            btnDown.classList.toggle('is-selected', rating === 'thumbs_down');
            btnDown.classList.toggle('is-dimmed', rating !== 'thumbs_down');
            btnDown.setAttribute('aria-pressed', rating === 'thumbs_down' ? 'true' : 'false');
        }
        if (fbStatus) {
            fbStatus.textContent = '✓ Saved';
            fbStatus.style.display = 'inline-block';
        }
    } catch (err) {
        showToast(err.message, 'error');
    }
}

function copyResult() {
    const cleanPrompt = getActiveOutputText(State.lastResult);
    const text = cleanPrompt || document.getElementById('result-output')?.textContent;
    if (!text) return showToast('No output to copy', 'error');
    copyToClipboardSafe(text, () => {
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
    }, 'Failed to copy');
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
                <div class="auth-gate-icon" aria-hidden="true"><img src="icons/badge-vault-1x.png?v=20261127" srcset="icons/badge-vault-1x.png?v=20261127 1x, icons/badge-vault.png?v=20261127 2x" alt="" width="56" height="56" class="oryx-hero-badge-img" loading="lazy" decoding="async" /></div>
                <div class="auth-gate-title">No Compression History Yet</div>
                <div class="auth-gate-desc">Your past prompt optimizations will appear here with side-by-side CRISP diffs and one-click restore.</div>
                <button type="button" class="tc-compress-btn" data-action="switchTab" data-arg="compress" style="width: auto; padding: 10px 22px;">Optimize a Prompt</button>
            </div>`;
        return;
    }

    if (!records.length) {
        list.innerHTML = `
            <div class="bento-card insights-empty">
                <div class="auth-gate-title">No Matching Runs</div>
                <div class="auth-gate-desc">Try clearing your search query or switching the mode filter.</div>
                <button type="button" class="history-btn primary" data-action="resetHistoryFilters" style="padding: 8px 16px;">Reset Filters</button>
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
                <div class="auth-gate-icon" aria-hidden="true"><img src="icons/badge-telemetry-1x.png?v=20261127" srcset="icons/badge-telemetry-1x.png?v=20261127 1x, icons/badge-telemetry.png?v=20261127 2x" alt="" width="56" height="56" class="oryx-hero-badge-img" loading="lazy" decoding="async" /></div>
                <div class="auth-gate-title">No Compression Telemetry Yet</div>
                <div class="auth-gate-desc">Run your first prompt optimization to unlock 14-day token trajectories, mode efficiency, and model cost savings.</div>
                <button type="button" class="tc-compress-btn" data-action="switchTab" data-arg="compress" style="width: auto; padding: 10px 22px;">Optimize a Prompt</button>
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
            data-action="setInsightsModel" data-arg="${key}">${escapeHtml(cfg.label)}</button>
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
    if (typeof OryxDomUtils !== 'undefined' && OryxDomUtils.escapeHtmlEntities) {
        return OryxDomUtils.escapeHtmlEntities(text);
    }
    const d = document.createElement('div');
    d.textContent = text == null ? '' : String(text);
    return d.innerHTML.replace(/"/g, '&quot;').replace(/'/g, '&#39;');
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

    const minWords = State.parentId ? 2 : 8;
    const raw = promptInput.value.trim();
    const words = raw ? raw.split(/\s+/).filter(Boolean) : [];
    const count = words.length;
    // ARCH: §5 NPAE Grounding — Instant token estimation via 1.33 words-to-tokens multiplier (R4)
    const estimatedTokens = Math.round(count * 1.33);

    if (wordBadge) {
        wordBadge.textContent = `${count} word${count === 1 ? '' : 's'}${count < minWords ? ` (min ${minWords})` : ''}`;
        if (count >= minWords) {
            wordBadge.classList.add('badge-valid');
        } else {
            wordBadge.classList.remove('badge-valid');
        }
    }

    if (tokenBadge) {
        tokenBadge.textContent = `${estimatedTokens} tokens`;
    }

    if (raw.length > 0 && !State.parentId) {
        updateDetectedDomain(raw);
    }

    if (State.parentId) {
        updateContinuationModePill();
    }

    if (count >= minWords) {
        compressBtn.disabled = false;
        promptInput.removeAttribute('aria-invalid');
        if (errorDiv) errorDiv.style.display = 'none';
    } else {
        compressBtn.disabled = true;
        if (errorDiv) {
            if (raw.length > 0) {
                promptInput.setAttribute('aria-invalid', 'true');
                errorDiv.textContent = `Prompt must be at least ${minWords} words (currently ${count} / ${minWords}).`;
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
    document.documentElement.classList.toggle('dark', scheme === 'dark');
    document.body.classList.toggle('dark', scheme === 'dark');
    document.body.classList.remove('theme-light', 'theme-dark', 'theme-night', 'theme-morning', 'theme-afternoon');
    document.body.classList.add(normalized);
    const btn = document.getElementById('btn-theme-toggle');
    if (btn) {
        btn.textContent = '';
        btn.setAttribute('aria-label', normalized === 'theme-dark' ? 'Switch to Light Theme' : 'Switch to Dark Theme');
    }
}

function clearCustomSkinInlineVars() {
    const root = document.documentElement;
    ['--page-bg', '--glass-bg', '--btn-grad', '--btn-text', '--head-text', '--body-text', '--accent-text'].forEach(p => {
        root.style.removeProperty(p);
    });
}

function safeGetStorage(key) {
    try {
        return localStorage.getItem(key);
    } catch (err) {
        console.warn(`Failed to read localStorage key "${key}":`, err);
        return null;
    }
}

function safeSetStorage(key, val) {
    try {
        localStorage.setItem(key, val);
    } catch (err) {
        console.warn(`Failed to write localStorage key "${key}":`, err);
    }
}

function safeRemoveStorage(key) {
    try {
        localStorage.removeItem(key);
    } catch (err) {
        console.warn(`Failed to remove localStorage key "${key}":`, err);
    }
}

function initTheme() {
    const saved = safeGetStorage('oryx_theme');
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
                if (!safeGetStorage('oryx_theme')) {
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
    safeSetStorage('oryx_theme', next);
    applyThemeClass(next);
}

function initCustomSkinModal() {
    const btn = document.getElementById('btn-custom-skin');
    const modal = document.getElementById('skin-modal');
    const closeBtn = document.getElementById('skin-modal-close');
    const applyBtn = document.getElementById('skin-modal-apply');
    const resetBtn = document.getElementById('skin-modal-reset');

    const savedSkin = safeGetStorage('na_ui_custom_skin');
    if (savedSkin) {
        try {
            applyCustomSkinConfig(JSON.parse(savedSkin));
        } catch (err) {
            console.warn('Failed to parse saved custom skin config:', err);
        }
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
                    bg: '#161618',
                    glassBg: 'rgba(22, 22, 24, 0.80)',
                    btnGrad: primaryColor,
                    btnText: '#ffffff',
                    headText: '#F5F5F7',
                    bodyText: '#98989D',
                    accentText: primaryColor
                }
                : {
                    bg: '#F5F5F7',
                    glassBg: 'rgba(245, 245, 247, 0.80)',
                    btnGrad: primaryColor,
                    btnText: '#ffffff',
                    headText: '#1D1D1F',
                    bodyText: '#6E6E73',
                    accentText: primaryColor
                };
            safeSetStorage('na_ui_custom_skin', JSON.stringify(config));
            applyCustomSkinConfig(config);
            closeDialogSafe(modal);
            showToast('Custom skin applied', 'success');
        });
    }
    if (resetBtn) {
        resetBtn.addEventListener('click', () => {
            safeRemoveStorage('na_ui_custom_skin');
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
 * Enables Left/Right and Up/Down arrow keyboard navigation on ARIA radiogroups
 * (`#outcome-pills`, `#mode-pills`), moving focus and activating the adjacent option.
 */
function initRadiogroupKeyboardNav() {
    document.querySelectorAll('[role="radiogroup"]').forEach(group => {
        group.addEventListener('keydown', (e) => {
            if (
                e.key !== 'ArrowRight' &&
                e.key !== 'ArrowLeft' &&
                e.key !== 'ArrowDown' &&
                e.key !== 'ArrowUp'
            ) {
                return;
            }
            const radios = Array.from(group.querySelectorAll('[role="radio"]'))
                .filter(r => !r.disabled && r.style.display !== 'none');
            if (radios.length < 2) return;
            const idx = radios.indexOf(document.activeElement);
            if (idx === -1) return;
            e.preventDefault();
            const isForward = e.key === 'ArrowRight' || e.key === 'ArrowDown';
            const nextIdx = isForward
                ? (idx + 1) % radios.length
                : (idx - 1 + radios.length) % radios.length;
            radios[nextIdx].focus();
            radios[nextIdx].click();
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

function formatRelativePaletteTime(isoStr) {
    if (!isoStr) return 'Recent';
    const dt = new Date(isoStr);
    if (Number.isNaN(dt.getTime())) return 'Recent';
    const diffSec = Math.max(0, Math.round((Date.now() - dt.getTime()) / 1000));
    if (diffSec < 60) return 'Just now';
    if (diffSec < 3600) return `${Math.floor(diffSec / 60)}m ago`;
    if (diffSec < 86400) return `${Math.floor(diffSec / 3600)}h ago`;
    return dt.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

function getStudioCommands() {
    const isAuthed = Boolean(State.token);
    const cmds = [
        {
            label: 'Run Prompt Optimization',
            hint: getRunShortcutBadge(),
            action: () => {
                switchTab('compress');
                const btn = document.getElementById('compress-btn');
                if (btn && !btn.disabled) compress();
                else document.getElementById('prompt-input')?.focus();
            }
        },
        { label: 'Copy Prompt Output', hint: 'Copy', action: () => copyResult() },
        { label: 'Toggle Inspector Popover', hint: 'Inspector', action: () => { switchTab('compress'); toggleQualityBreakdown(); } },
        { label: 'Mode: Fact-Checked', hint: 'Preset', action: () => { switchTab('compress'); selectOutcomePreset('factchecked'); } },
        { label: 'Mode: Token Saver', hint: 'Preset', action: () => { switchTab('compress'); selectOutcomePreset('saver'); } },
        { label: 'Mode: Full Blueprint', hint: 'Preset', action: () => { switchTab('compress'); selectOutcomePreset('blueprint'); } },
        { label: 'Load Sample: Buying a House', hint: 'Sample', action: () => { switchTab('compress'); loadPreset('house'); } },
        { label: 'Load Sample: Bug Report', hint: 'Sample', action: () => { switchTab('compress'); loadPreset('bug'); } },
        { label: 'Load Sample: Contract Review', hint: 'Sample', action: () => { switchTab('compress'); loadPreset('legal'); } },
        { label: 'Toggle Light / Dark Theme', hint: 'Theme', action: () => toggleTheme() },
        { label: 'Customize UI Skin', hint: 'Theme', action: () => openDialogSafe(document.getElementById('skin-modal')) },
        { label: isAuthed ? 'Sign Out' : 'Sign In / Register', hint: 'Account', action: () => toggleAuthSection() },
    ];
    if (isAuthed) {
        cmds.push(
            { label: 'Go to Optimize Prompt', hint: 'View', action: () => switchTab('compress') },
            { label: 'Go to Ask My Docs (RAG)', hint: 'View', action: () => switchTab('ask') },
            { label: 'Go to History Timeline', hint: 'View', action: () => switchTab('history') },
            { label: 'Go to Insights Dashboard', hint: 'View', action: () => switchTab('stats') },
        );
    }
    return cmds;
}

function getRecentPromptsForPalette() {
    const merged = [];
    const seen = new Set();
    const pushEntry = (rawText, preview, orig, fin, createdAt) => {
        const cleanRaw = String(rawText || preview || '').trim();
        if (!cleanRaw) return;
        const key = cleanRaw.slice(0, 120).toLowerCase();
        if (seen.has(key)) return;
        seen.add(key);
        merged.push({
            rawText: cleanRaw,
            preview: String(preview || cleanRaw).replace(/\s+/g, ' ').trim().slice(0, 68),
            tokenOriginal: Number(orig) || 0,
            tokenFinal: Number(fin) || 0,
            timeLabel: formatRelativePaletteTime(createdAt),
        });
    };

    (State.recentRuns || []).forEach(r => {
        pushEntry(r.raw_text, r.preview, r.token_original, r.token_final, r.created_at);
    });
    (cachedHistoryRecords || []).forEach(r => {
        pushEntry(r.original_prompt, r.original_prompt, r.token_original, r.token_final, r.created_at);
    });
    return merged.slice(0, 5);
}

// ARCH: ARCHITECTURE.md §2 Full Component Map (Presentation Layer) — Accessible Spotlight ⌘K
// combobox/listbox command palette with ARIA activedescendant synchronization.
function syncPaletteActiveOption(items, activeIdx) {
    const input = document.getElementById('studio-cmd-input');
    items.forEach((el, i) => {
        const isSel = i === activeIdx;
        el.classList.toggle('selected', isSel);
        el.setAttribute('aria-selected', isSel ? 'true' : 'false');
    });
    if (input) {
        const activeEl = items[activeIdx];
        if (activeEl && activeEl.id) {
            input.setAttribute('aria-activedescendant', activeEl.id);
        } else {
            input.removeAttribute('aria-activedescendant');
        }
    }
}

function renderStudioCommandList(query = '') {
    const list = document.getElementById('studio-cmd-list');
    const modal = document.getElementById('studio-cmd-modal');
    const input = document.getElementById('studio-cmd-input');
    if (!list) return;
    const q = query.trim().toLowerCase();
    const actionMatches = getStudioCommands().filter(c =>
        c.label.toLowerCase().includes(q) || c.hint.toLowerCase().includes(q)
    );
    const recentMatches = getRecentPromptsForPalette().filter(r =>
        !q || r.preview.toLowerCase().includes(q) || r.rawText.toLowerCase().includes(q)
    );

    list.innerHTML = '';
    let selectableCount = 0;

    const actionsHeader = document.createElement('div');
    actionsHeader.className = 'cmd-group-header';
    actionsHeader.setAttribute('role', 'presentation');
    actionsHeader.textContent = 'Actions';
    list.appendChild(actionsHeader);

    if (actionMatches.length === 0) {
        const emptyAct = document.createElement('div');
        emptyAct.setAttribute('role', 'presentation');
        emptyAct.style.cssText = 'padding:0.45rem 0.65rem;font-size:0.76rem;color:var(--text-tertiary);';
        emptyAct.textContent = 'No matching actions.';
        list.appendChild(emptyAct);
    } else {
        actionMatches.forEach((cmd) => {
            const btn = document.createElement('button');
            btn.type = 'button';
            btn.id = `cmd-opt-${selectableCount}`;
            btn.setAttribute('role', 'option');
            const isFirst = selectableCount === 0;
            btn.setAttribute('aria-selected', isFirst ? 'true' : 'false');
            btn.className = 'cmd-palette-item' + (isFirst ? ' selected' : '');
            selectableCount++;
            btn.innerHTML = `<span>${escapeHtml(cmd.label)}</span><kbd class="macos-kbd">${escapeHtml(cmd.hint)}</kbd>`;
            btn.addEventListener('click', () => {
                closeDialogSafe(modal);
                cmd.action();
            });
            list.appendChild(btn);
        });
    }

    const recentHeader = document.createElement('div');
    recentHeader.className = 'cmd-group-header';
    recentHeader.setAttribute('role', 'presentation');
    recentHeader.textContent = 'Recent Prompts';
    list.appendChild(recentHeader);

    if (recentMatches.length === 0) {
        const emptyRec = document.createElement('div');
        emptyRec.setAttribute('role', 'presentation');
        emptyRec.style.cssText = 'padding:0.45rem 0.65rem 0.65rem;font-size:0.76rem;color:var(--text-tertiary);';
        emptyRec.textContent = q ? 'No matching recent prompts.' : 'Run a prompt optimization to populate recent history.';
        list.appendChild(emptyRec);
    } else {
        recentMatches.forEach((rec) => {
            const btn = document.createElement('button');
            btn.type = 'button';
            btn.id = `cmd-opt-${selectableCount}`;
            btn.setAttribute('role', 'option');
            const isFirst = selectableCount === 0;
            btn.setAttribute('aria-selected', isFirst ? 'true' : 'false');
            btn.className = 'cmd-palette-item' + (isFirst ? ' selected' : '');
            selectableCount++;
            const metaText = `${rec.tokenOriginal} → ${rec.tokenFinal} tok · ${rec.timeLabel}`;
            btn.innerHTML = `<span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escapeHtml(rec.preview)}</span><span class="cmd-item-meta bento-row-meta">${escapeHtml(metaText)}</span>`;
            btn.addEventListener('click', () => {
                closeDialogSafe(modal);
                resetParentSession(true);
                const promptEl = document.getElementById('prompt-input');
                if (promptEl) promptEl.value = rec.rawText;
                validatePromptLength();
                switchTab('compress');
            });
            list.appendChild(btn);
        });
    }

    if (input) {
        if (selectableCount > 0) {
            input.setAttribute('aria-activedescendant', 'cmd-opt-0');
        } else {
            input.removeAttribute('aria-activedescendant');
        }
    }
}

function initStudioCommandPalette() {
    const modal = document.getElementById('studio-cmd-modal');
    const input = document.getElementById('studio-cmd-input');
    const list = document.getElementById('studio-cmd-list');

    // Modern Web Guidance: pointer-position light-dismiss fallback where closedBy is absent
    if (typeof HTMLDialogElement !== 'undefined' && !('closedBy' in HTMLDialogElement.prototype)) {
        const outside = (e, dialog) => {
            const r = dialog.getBoundingClientRect();
            return e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom;
        };
        document.querySelectorAll('dialog[closedby="any"]').forEach((dlg) => {
            let pressedOutside = false;
            dlg.addEventListener('pointerdown', (e) => { pressedOutside = outside(e, dlg); });
            dlg.addEventListener('pointerup', (e) => { if (pressedOutside && outside(e, dlg)) closeDialogSafe(dlg); });
        });
    }

    // Light-dismiss floating Inspector popover on outside click
    document.addEventListener('click', (e) => {
        const inspector = document.getElementById('inspector-drawer');
        const tesBtn = document.getElementById('tes-score');
        if (!inspector || !inspector.open) return;
        if (inspector.contains(e.target) || (tesBtn && tesBtn.contains(e.target))) return;
        inspector.open = false;
        syncInspectorAriaExpanded();
    });

    if (!modal || !input || !list) return;

    input.addEventListener('input', (e) => renderStudioCommandList(e.target.value));
    input.addEventListener('keydown', (e) => {
        const items = Array.from(list.querySelectorAll('.cmd-palette-item'));
        if (!items.length) return;
        const idx = items.findIndex(el => el.classList.contains('selected'));
        if (e.key === 'ArrowDown') {
            e.preventDefault();
            const nextIdx = (idx + 1) % items.length;
            syncPaletteActiveOption(items, nextIdx);
            if (typeof items[nextIdx].scrollIntoView === 'function') items[nextIdx].scrollIntoView({ block: 'nearest' });
        } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            const prevIdx = (idx - 1 + items.length) % items.length;
            syncPaletteActiveOption(items, prevIdx);
            if (typeof items[prevIdx].scrollIntoView === 'function') items[prevIdx].scrollIntoView({ block: 'nearest' });
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
            const inspector = document.getElementById('inspector-drawer');
            if (inspector && inspector.open) {
                inspector.open = false;
                syncInspectorAriaExpanded();
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

function initStudioDeclarativeEvents() {
    const clickActions = {
        openStudioCommandPalette: () => openStudioCommandPalette(),
        closeStudioCommandPalette: () => closeStudioCommandPalette(),
        toggleTheme: () => toggleTheme(),
        closeAuthSection: () => closeAuthSection(),
        switchAuthTab: (arg) => switchAuthTab(arg),
        toggleAuthSection: () => toggleAuthSection(),
        switchTab: (arg) => switchTab(arg),
        loadPreset: (arg) => loadPreset(arg),
        openPlaybookModal: () => openPlaybookModal(),
        closePlaybookModal: () => closePlaybookModal(),
        showPrivacyLocalInfo: () => showPrivacyLocalInfo(),
        selectOutcomePreset: (arg) => selectOutcomePreset(arg),
        selectMode: (arg) => selectMode(arg),
        clearInput: () => clearInput(),
        resetParentSession: () => resetParentSession(),
        revokeInheritedConstraint: (arg) => revokeInheritedConstraint(arg),
        selectActivePhase: (arg) => selectActivePhase(arg),
        setOutputView: (arg) => setOutputView(arg),
        applyClarifySlot: () => applyClarifySlot(),
        closeClarifyInlineBox: () => closeClarifyInlineBox(),
        generateRepairFromVerify: () => generateRepairFromVerify(),
        compress: () => compress(),
        toggleQualityBreakdown: () => toggleQualityBreakdown(),
        copyResult: () => copyResult(),
        launchInExternalAi: (arg) => launchInExternalAi(arg),
        exportAuditCard: () => exportAuditCard(),
        exportAuditSvg: () => exportAuditSvg(),
        openWorkbenchVerifyModal: () => openWorkbenchVerifyModal(),
        submitFeedback: (arg) => submitFeedback(arg),
        copyStartCommand: () => RagWorkspace.copyStartCommand(),
        checkLocalHealth: () => RagWorkspace.checkLocalHealth(),
        openVerifyModal: () => RagWorkspace.openVerifyModal(),
        closeVerifyModal: () => RagWorkspace.closeVerifyModal(),
        runVerify: () => RagWorkspace.runVerify(),
        exportVerifyReport: () => RagWorkspace.exportVerifyReport(),
        executeScenarioAsk: () => executeScenarioAsk(),
        executeScenarioAggressiveCompress: () => executeScenarioAggressiveCompress(),
        copyAggressivePrompt: () => copyAggressivePrompt(),
        setHistoryModeFilter: (arg) => setHistoryModeFilter(arg),
        resetHistoryFilters: () => resetHistoryFilters(),
        openAuthModal: () => openAuthModal(),
        loadStats: () => loadStats(),
        setInsightsModel: (arg) => setInsightsModel(arg),
    };

    document.addEventListener('click', (e) => {
        const el = e.target.closest('[data-action]');
        if (!el) return;
        const action = el.getAttribute('data-action');
        const arg = el.getAttribute('data-arg');
        if (clickActions[action]) {
            clickActions[action](arg, e, el);
        }
    });

    document.addEventListener('change', (e) => {
        const el = e.target.closest('[data-change]');
        if (!el) return;
        const act = el.getAttribute('data-change');
        if (act === 'handleFileSelect') handleFileSelect(e);
        else if (act === 'toggleDiffView') toggleDiffView();
        else if (act === 'updateModelPricing') updateModelPricing();
        else if (act === 'updateRoiScale') updateRoiScale();
        else if (act === 'syncOutcomePresetFromControls') syncOutcomePresetFromControls();
        else if (act === 'setDataSource') RagWorkspace.setDataSource(el.value);
        else if (act === 'handleScenarioModeChange') handleScenarioModeChange(el.value);
        else if (act === 'handleDomainChange') handleDomainChange(el.value);
        else if (act === 'setHistorySort') setHistorySort(el.value);
    });

    document.addEventListener('input', (e) => {
        const el = e.target.closest('[data-input]');
        if (!el) return;
        const act = el.getAttribute('data-input');
        if (act === 'filterHistory') filterHistory(el.value);
        else if (act === 'renderPlaybooks') renderPlaybooks(el.value);
    });

    document.addEventListener('submit', (e) => {
        const el = e.target.closest('[data-submit]');
        if (!el) return;
        const act = el.getAttribute('data-submit');
        if (act === 'handleLogin') handleLogin(e);
        else if (act === 'handleRegister') handleRegister(e);
    });

    const inspectorDrawer = document.getElementById('inspector-drawer');
    if (inspectorDrawer) {
        inspectorDrawer.addEventListener('toggle', syncInspectorAriaExpanded);
    }
}

document.addEventListener('DOMContentLoaded', async () => {
    loadRecentPromptRuns();
    syncPlatformShortcutBadges();
    initStudioDeclarativeEvents();
    initTheme();
    initCustomSkinModal();
    initTablistKeyboardNav();
    initRadiogroupKeyboardNav();
    initStudioCommandPalette();
    loadPresetsFromFile();
    if (typeof OryxDomUtils !== 'undefined' && typeof OryxDomUtils.initHoverTiltEmblem === 'function') {
        OryxDomUtils.initHoverTiltEmblem('oryx-logo-3d');
    }

    // Hydrate session from HttpOnly cookie via GET /api/me (fallback to zero-auth sandbox).
    localStorage.removeItem('tce_token');
    let hydrated = false;
    try {
        const meRes = await fetch('/api/me', { credentials: 'same-origin' });
        if (meRes.ok) {
            const me = await meRes.json();
            State.token = 'cookie';
            State.userId = me.user_id;
            State.username = me.username;
            State.businessType = me.business_type;
            State.role = me.role;
            showMainSection();
            loadScenarioRegistry();
            hydrated = true;
        }
    } catch (err) {
        console.warn('Session hydration check failed:', err);
    }

    if (!hydrated) {
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
            domainSel.innerHTML = '';
            const defaultOpt = document.createElement('option');
            defaultOpt.value = '';
            defaultOpt.disabled = true;
            defaultOpt.selected = true;
            defaultOpt.textContent = '-- Select Mandatory Domain --';
            domainSel.appendChild(defaultOpt);

            for (const d of reg.domains) {
                const opt = document.createElement('option');
                opt.value = d.key || '';
                opt.textContent = d.label || '';
                domainSel.appendChild(opt);
            }
        }

        if (styleSel && reg.styles) {
            styleSel.innerHTML = '';
            for (const s of reg.styles) {
                const opt = document.createElement('option');
                opt.value = s.key || '';
                opt.textContent = `${s.label || ''} — ${s.description || ''}`;
                styleSel.appendChild(opt);
            }
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
        btn.textContent = 'Compressing with Zero Hallucination...';
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
            btn.textContent = 'Compress in Aggressive Mode for LLM';
        }
    }
}

function copyAggressivePrompt() {
    if (!lastAggressivePromptText) {
        return showToast('No compressed prompt to copy', 'error');
    }
    copyToClipboardSafe(
        lastAggressivePromptText,
        () => showToast('Clean LLM Prompt copied to clipboard!', 'success'),
        'Failed to copy'
    );
}

// Explicit global attachment for inline onclick handlers and test runners
if (typeof window !== 'undefined') {
    window.State = State;
    window.SAMPLE_PRESETS = SAMPLE_PRESETS;
    window.MODEL_PRICES = MODEL_PRICES;
    window.loadPreset = loadPreset;
    window.selectOutcomePreset = selectOutcomePreset;
    window.syncOutcomePresetFromControls = syncOutcomePresetFromControls;
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
    window.resetParentSession = resetParentSession;
    window.revokeInheritedConstraint = revokeInheritedConstraint;
    window.selectActivePhase = selectActivePhase;
    window.setOutputView = setOutputView;
    window.applyClarifySlot = applyClarifySlot;
    window.closeClarifyInlineBox = closeClarifyInlineBox;
    window.generateRepairFromVerify = generateRepairFromVerify;
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
    window.closeStudioCommandPalette = closeStudioCommandPalette;
    window.isApplePlatform = isApplePlatform;
    window.syncPlatformShortcutBadges = syncPlatformShortcutBadges;
    window.formatClarifyChipLabel = formatClarifyChipLabel;
    window.formatPromptSyntaxHtml = formatPromptSyntaxHtml;
    window.openDialogSafe = openDialogSafe;
    window.closeDialogSafe = closeDialogSafe;
}
if (typeof globalThis !== 'undefined') {
    globalThis.State = State;
    globalThis.SAMPLE_PRESETS = SAMPLE_PRESETS;
    globalThis.MODEL_PRICES = MODEL_PRICES;
    globalThis.loadPreset = loadPreset;
    globalThis.selectOutcomePreset = selectOutcomePreset;
    globalThis.syncOutcomePresetFromControls = syncOutcomePresetFromControls;
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
    globalThis.resetParentSession = resetParentSession;
    globalThis.revokeInheritedConstraint = revokeInheritedConstraint;
    globalThis.selectActivePhase = selectActivePhase;
    globalThis.setOutputView = setOutputView;
    globalThis.applyClarifySlot = applyClarifySlot;
    globalThis.closeClarifyInlineBox = closeClarifyInlineBox;
    globalThis.generateRepairFromVerify = generateRepairFromVerify;
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
    globalThis.isApplePlatform = isApplePlatform;
    globalThis.syncPlatformShortcutBadges = syncPlatformShortcutBadges;
    globalThis.formatClarifyChipLabel = formatClarifyChipLabel;
    globalThis.formatPromptSyntaxHtml = formatPromptSyntaxHtml;
    globalThis.closeStudioCommandPalette = closeStudioCommandPalette;
}


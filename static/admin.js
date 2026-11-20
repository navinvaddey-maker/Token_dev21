const State = {
    token: localStorage.getItem('tce_token'),
    userId: localStorage.getItem('tce_user_id'),
    username: localStorage.getItem('tce_username'),
    businessType: localStorage.getItem('tce_business_type'),
    currentSection: 'overview',
    users: [],
    history: [],
    ragDocs: [],
    selectedFile: null
};

// ── Init ────────────────────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
    if (!State.token || !State.businessType || State.businessType.toLowerCase() !== 'admin') {
        window.location.href = '/'; // Redirect to login if no token or not admin
        return;
    }

    document.getElementById('admin-username').textContent = State.username;
    const avatarEl = document.getElementById('admin-avatar');
    if (avatarEl && State.username) {
        avatarEl.textContent = State.username.charAt(0).toUpperCase();
    }
    initAdminTheme();
    initEventListeners();
    initHudClock();
    initCommandPalette();
    initHollywoodCanvases();
    initAutoSync();
    loadOverview();
});

function applyAdminTheme(themeClass) {
    const normalized = (themeClass === 'theme-light') ? 'theme-light' : 'theme-dark';
    const scheme = normalized === 'theme-dark' ? 'dark' : 'light';
    document.documentElement.setAttribute('data-theme', scheme);
    document.documentElement.style.colorScheme = scheme;
    document.body.classList.remove('theme-light', 'theme-dark', 'theme-night');
    document.body.classList.add(normalized);
    const btn = document.getElementById('btn-admin-theme');
    if (btn) {
        btn.textContent = normalized === 'theme-dark' ? '☀️' : '🌙';
        btn.setAttribute('aria-label', normalized === 'theme-dark' ? 'Switch to Light Theme' : 'Switch to Dark Theme');
    }
}

function initAdminTheme() {
    const saved = localStorage.getItem('oryx_theme');
    if (saved) {
        applyAdminTheme(saved);
    } else {
        const prefersDark = typeof window.matchMedia === 'function'
            ? window.matchMedia('(prefers-color-scheme: dark)').matches
            : true;
        applyAdminTheme(prefersDark ? 'theme-dark' : 'theme-light');
    }
}

function toggleAdminTheme() {
    const isDark = document.body.classList.contains('theme-dark');
    const next = isDark ? 'theme-light' : 'theme-dark';
    localStorage.setItem('oryx_theme', next);
    applyAdminTheme(next);
}

function initEventListeners() {
    // Nav links
    document.querySelectorAll('.nav-links li').forEach(li => {
        li.addEventListener('click', (e) => {
            e.preventDefault();
            const section = li.getAttribute('data-section');
            switchSection(section);
        });
    });

    // Logout
    document.getElementById('logout-btn').addEventListener('click', () => {
        localStorage.removeItem('tce_token');
        localStorage.removeItem('tce_user_id');
        localStorage.removeItem('tce_username');
        localStorage.removeItem('tce_business_type');
        window.location.href = '/';
    });

    // Mobile sidebar toggle
    const mobileBtn = document.getElementById('mobile-menu-btn');
    const sidebar = document.getElementById('admin-sidebar');
    if (mobileBtn && sidebar) {
        mobileBtn.addEventListener('click', () => sidebar.classList.toggle('open'));
    }

    // Shared Light/Dark Theme toggle
    const themeBtn = document.getElementById('btn-admin-theme');
    if (themeBtn) {
        themeBtn.addEventListener('click', toggleAdminTheme);
    }

    // Admin Ask (RAG)
    const adminAskBtn = document.getElementById('admin-ask-btn');
    if (adminAskBtn) {
        adminAskBtn.addEventListener('click', handleAdminAsk);
    }

    // Search
    document.getElementById('user-search').addEventListener('input', (e) => {
        renderUsers(e.target.value);
    });

    // Modal close
    document.getElementById('close-modal').addEventListener('click', () => {
        document.getElementById('edit-user-modal').classList.remove('active');
    });

    // Edit form submit
    document.getElementById('edit-user-form').addEventListener('submit', handleEditUser);

    // RAG listeners
    initRagEventListeners();
}

// ── Navigation ──────────────────────────────────────────────────────────────

function switchSection(sectionId) {
    State.currentSection = sectionId;

    // Update navUI
    document.querySelectorAll('.nav-links li').forEach(li => {
        li.classList.toggle('active', li.getAttribute('data-section') === sectionId);
    });

    // Update content UI
    document.querySelectorAll('main section').forEach(section => {
        section.classList.toggle('active', section.id === `${sectionId}-section`);
    });

    if (sectionId === 'rag') {
        document.getElementById('section-title').textContent = 'RAG Knowledge Base';
    } else if (sectionId === 'ask') {
        document.getElementById('section-title').textContent = 'Ask (RAG)';
    } else if (sectionId === 'feedback') {
        document.getElementById('section-title').textContent = 'Feedback Signals';
    } else {
        document.getElementById('section-title').textContent = sectionId.charAt(0).toUpperCase() + sectionId.slice(1);
    }

    // Load data for section
    if (sectionId === 'overview') loadOverview();
    else if (sectionId === 'users') loadUsers();
    else if (sectionId === 'history') loadHistory();
    else if (sectionId === 'feedback') loadFeedback();
    else if (sectionId === 'rag') loadRag();
}

// ── Data Fetching ──────────────────────────────────────────────────────────

async function apiFetch(url, options = {}) {
    const defaultOptions = {
        headers: {
            'Authorization': `Bearer ${State.token}`,
            'Content-Type': 'application/json'
        }
    };
    const response = await fetch(url, { ...defaultOptions, ...options });

    if (response.status === 401 || response.status === 403) {
        window.location.href = '/';
        return;
    }

    if (!response.ok) {
        const err = await response.json();
        throw new Error(err.error?.message || 'API request failed');
    }

    if (response.status === 204) return null;
    return response.json();
}

// ── Overview Section (Mission Control HUD) ──────────────────────────────────

function initHudClock() {
    const clockEl = document.getElementById('hud-utc-clock');
    if (!clockEl) return;
    const tick = () => {
        const now = new Date();
        clockEl.textContent = now.toISOString().slice(11, 19) + ' UTC';
    };
    tick();
    setInterval(tick, 1000);
}

function initCommandPalette() {
    const modal = document.getElementById('cmd-palette-modal');
    const input = document.getElementById('cmd-palette-input');
    const list = document.getElementById('cmd-palette-list');
    const btn = document.getElementById('btn-cmd-palette');
    if (!modal || !input || !list) return;

    const commands = [
        { label: 'Overview — Mission Control HUD', hint: 'Section', action: () => switchSection('overview') },
        { label: 'User Management — Accounts & Roles', hint: 'Section', action: () => switchSection('users') },
        { label: 'Global History — Pipeline Runs', hint: 'Section', action: () => switchSection('history') },
        { label: 'Feedback Signals — Quality & Sentiment', hint: 'Section', action: () => switchSection('feedback') },
        { label: 'RAG Knowledge Base — Vector Docs', hint: 'Section', action: () => switchSection('rag') },
        { label: 'Ask (RAG) — Internal Query Console', hint: 'Section', action: () => switchSection('ask') },
        { label: 'Sync HUD Telemetry Now', hint: 'Action', action: () => loadOverview() },
        { label: 'Return to Oryx Studio', hint: 'Link', action: () => { window.location.href = '/'; } },
    ];

    const renderList = (q = '') => {
        const query = q.trim().toLowerCase();
        const matches = commands.filter(c => c.label.toLowerCase().includes(query) || c.hint.toLowerCase().includes(query));
        list.innerHTML = '';
        matches.forEach((cmd, idx) => {
            const row = document.createElement('div');
            row.className = 'cmd-palette-item' + (idx === 0 ? ' selected' : '');
            row.innerHTML = `<span>${cmd.label}</span><span class="hud-panel-tag">${cmd.hint}</span>`;
            row.addEventListener('click', () => {
                close();
                cmd.action();
            });
            list.appendChild(row);
        });
    };

    const open = () => {
        modal.classList.add('open');
        input.value = '';
        renderList('');
        setTimeout(() => input.focus(), 10);
    };
    const close = () => modal.classList.remove('open');

    if (btn) btn.addEventListener('click', open);
    modal.addEventListener('click', (e) => { if (e.target === modal) close(); });
    input.addEventListener('input', (e) => renderList(e.target.value));
    input.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') close();
        if (e.key === 'Enter') {
            const first = list.querySelector('.cmd-palette-item');
            if (first) first.click();
        }
    });
    document.addEventListener('keydown', (e) => {
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
            e.preventDefault();
            if (modal.classList.contains('open')) close();
            else open();
        }
    });
}

function formatHudDate(val) {
    if (!val) return '--';
    const raw = String(val).trim();
    // Normalize SQLite "YYYY-MM-DD HH:MM:SS" into ISO UTC so Safari/WebKit parses it reliably.
    const normalized = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}/.test(raw)
        ? raw.replace(' ', 'T') + (raw.endsWith('Z') ? '' : 'Z')
        : raw;
    const dt = new Date(normalized);
    return Number.isNaN(dt.getTime()) ? raw : dt.toLocaleString();
}

function copyHudValue(text, el) {
    if (!text || !navigator.clipboard || typeof navigator.clipboard.writeText !== 'function') return;
    navigator.clipboard.writeText(text).then(() => {
        if (el) {
            const prev = el.textContent;
            el.textContent = '✓ Copied';
            setTimeout(() => { el.textContent = prev; }, 1200);
        }
    }).catch(() => { });
}

function renderHudPerformance(sysPerf, engagement, errMetrics) {
    const panel = document.getElementById('hud-perf-panel');
    if (!panel) return;

    const avgLatencyMs = sysPerf?.avg_latency_ms !== undefined
        ? Number(sysPerf.avg_latency_ms)
        : Number(sysPerf?.avg_processing_time_seconds ?? 0) * 1000;
    const activeUsers = Number(
        engagement?.active_users_24h ??
        sysPerf?.active_users_last_hour ??
        engagement?.total_active_users ??
        0
    );
    const totalUsers = Number((document.getElementById('stat-total-users')?.textContent || '0').replace(/,/g, '')) || 0;
    const engPct = totalUsers > 0 ? Math.min(100, Math.round((activeUsers / totalUsers) * 100)) : 0;
    const errRate = Number(errMetrics?.error_rate ?? errMetrics?.percentage_with_warnings ?? 0);
    const healthPct = Math.max(0, Math.min(100, Math.round(100 - errRate)));
    const latencyPct = Math.min(100, Math.max(8, Math.round((avgLatencyMs / 500) * 100)));

    panel.innerHTML = `
        <div class="hud-meter-row">
            <div class="hud-meter-label"><span>Pipeline Health</span><span>${healthPct}%</span></div>
            <div class="hud-meter-track"><div class="hud-meter-fill" style="width:${healthPct}%; background: linear-gradient(90deg, #10b981, #22d3ee);"></div></div>
        </div>
        <div class="hud-meter-row">
            <div class="hud-meter-label"><span>Avg Engine Latency</span><span>${avgLatencyMs > 0 ? `${avgLatencyMs.toFixed(0)} ms` : 'Nominal'}</span></div>
            <div class="hud-meter-track"><div class="hud-meter-fill" style="width:${latencyPct}%;"></div></div>
        </div>
        <div class="hud-meter-row">
            <div class="hud-meter-label"><span>Active User Engagement</span><span>${activeUsers} active (${engPct}%)</span></div>
            <div class="hud-meter-track"><div class="hud-meter-fill" style="width:${Math.max(6, engPct)}%; background: linear-gradient(90deg, #a855f7, #3b82f6);"></div></div>
        </div>
    `;
}

function initAutoSync() {
    const btn = document.getElementById('btn-auto-sync');
    State.autoSync = true;
    setInterval(() => {
        if (State.autoSync && State.currentSection === 'overview') {
            loadOverview();
        }
    }, 10000);
    if (btn) {
        btn.addEventListener('click', () => {
            State.autoSync = !State.autoSync;
            btn.textContent = State.autoSync ? 'AUTO-SYNC: ON' : 'AUTO-SYNC: OFF';
            btn.style.opacity = State.autoSync ? '1' : '0.6';
        });
    }
}

function initHollywoodCanvases() {
    const reduced = typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    const radar = document.getElementById('hud-radar-canvas');
    const wave = document.getElementById('hud-wave-canvas');

    let angle = 0;
    let phase = 0;
    const stageNodes = ['S0', 'S1', 'S2', 'NPAE', 'ORY', 'GUARD'];

    const drawRadar = () => {
        if (!radar || !radar.getContext) return;
        const ctx = radar.getContext('2d');
        if (!ctx) return;
        const w = radar.width;
        const h = radar.height;
        const cx = w / 2;
        const cy = h / 2;
        const maxR = Math.min(cx, cy) - 18;

        ctx.clearRect(0, 0, w, h);

        // Concentric rings
        [0.33, 0.66, 1].forEach(frac => {
            ctx.beginPath();
            ctx.arc(cx, cy, maxR * frac, 0, Math.PI * 2);
            ctx.strokeStyle = 'rgba(34, 211, 238, 0.16)';
            ctx.lineWidth = 1;
            ctx.stroke();
        });

        // Crosshairs
        ctx.beginPath();
        ctx.moveTo(cx - maxR, cy);
        ctx.lineTo(cx + maxR, cy);
        ctx.moveTo(cx, cy - maxR);
        ctx.lineTo(cx, cy + maxR);
        ctx.strokeStyle = 'rgba(34, 211, 238, 0.1)';
        ctx.stroke();

        // Sweep sector
        ctx.save();
        ctx.translate(cx, cy);
        ctx.rotate(angle);
        const grad = ctx.createLinearGradient(0, 0, maxR, 0);
        grad.addColorStop(0, 'rgba(34, 211, 238, 0.35)');
        grad.addColorStop(1, 'rgba(34, 211, 238, 0.02)');
        ctx.beginPath();
        ctx.moveTo(0, 0);
        ctx.arc(0, 0, maxR, -0.45, 0);
        ctx.closePath();
        ctx.fillStyle = grad;
        ctx.fill();
        ctx.restore();

        // Pipeline stage nodes around perimeter
        stageNodes.forEach((lbl, idx) => {
            const a = (idx / stageNodes.length) * Math.PI * 2 - Math.PI / 2;
            const nx = cx + Math.cos(a) * (maxR * 0.78);
            const ny = cy + Math.sin(a) * (maxR * 0.78);
            const dist = Math.abs(((angle % (Math.PI * 2)) - (a + Math.PI * 2) % (Math.PI * 2)));
            const active = dist < 0.55;

            ctx.beginPath();
            ctx.arc(nx, ny, active ? 4.5 : 3, 0, Math.PI * 2);
            ctx.fillStyle = active ? '#34d399' : '#22d3ee';
            ctx.fill();

            ctx.fillStyle = active ? '#f8fafc' : '#94a3b8';
            ctx.font = '600 9px "JetBrains Mono", monospace';
            ctx.textAlign = 'center';
            ctx.fillText(lbl, nx, ny - 8);
        });

        // Core pulse
        ctx.beginPath();
        ctx.arc(cx, cy, 4, 0, Math.PI * 2);
        ctx.fillStyle = '#22d3ee';
        ctx.fill();
    };

    const drawWave = () => {
        if (!wave || !wave.getContext) return;
        const ctx = wave.getContext('2d');
        if (!ctx) return;
        const w = wave.width;
        const h = wave.height;
        ctx.clearRect(0, 0, w, h);

        // Center baseline
        ctx.beginPath();
        ctx.moveTo(0, h / 2);
        ctx.lineTo(w, h / 2);
        ctx.strokeStyle = 'rgba(255, 255, 255, 0.06)';
        ctx.lineWidth = 1;
        ctx.stroke();

        // Primary cyan carrier + secondary emerald savings harmonic
        [
            { amp: 20, freq: 0.025, speed: 1, color: '#22d3ee' },
            { amp: 13, freq: 0.04, speed: 1.6, color: '#34d399' },
        ].forEach(ch => {
            ctx.beginPath();
            for (let x = 0; x <= w; x += 4) {
                const env = Math.sin((x / w) * Math.PI);
                const y = h / 2 + Math.sin(x * ch.freq + phase * ch.speed) * ch.amp * env;
                if (x === 0) ctx.moveTo(x, y);
                else ctx.lineTo(x, y);
            }
            ctx.strokeStyle = ch.color;
            ctx.lineWidth = 1.6;
            ctx.stroke();
        });
    };

    drawRadar();
    drawWave();
    if (!reduced && typeof requestAnimationFrame === 'function') {
        const tick = () => {
            angle += 0.025;
            phase += 0.08;
            drawRadar();
            drawWave();
            requestAnimationFrame(tick);
        };
        requestAnimationFrame(tick);
    }
}

function renderHudLiveFeed(records) {
    const feed = document.getElementById('hud-live-feed');
    if (!feed) return;
    if (!Array.isArray(records) || records.length === 0) {
        feed.innerHTML = '<div class="hud-kpi-sub">Awaiting pipeline executions... <span class="hud-cursor"></span></div>';
        return;
    }
    feed.innerHTML = records.slice(0, 6).map(r => {
        const uid = String(r.user_id || '').slice(0, 8);
        const orig = Number(r.token_original) || 0;
        const fin = Number(r.token_final) || 0;
        const saved = Number(r.tokens_saved) || 0;
        const mode = String(r.mode || 'balanced').toUpperCase();
        const t = r.created_at ? new Date(r.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' }) : '--:--';
        return `
            <div class="hud-ops-line">
                <div class="hud-ops-top">
                    <span>[OK] USR:${uid}</span>
                    <span class="hud-panel-tag">${mode}</span>
                </div>
                <div class="hud-ops-detail">
                    <span>${orig} → ${fin} tok <strong style="color:#34d399;">(+${saved})</strong></span>
                    <span>${t}</span>
                </div>
            </div>`;
    }).join('') + '<div class="hud-kpi-sub" style="margin-top:4px;">STREAMING LIVE <span class="hud-cursor"></span></div>';
}

async function loadOverview() {
    const t0 = performance.now();
    try {
        const [stats, analytics, learning, errMetrics, sysPerf, engagement, recentHist] = await Promise.all([
            apiFetch('/api/admin/stats'),
            apiFetch('/api/admin/compression-analytics').catch(() => null),
            apiFetch('/api/admin/learning-engine-stats').catch(() => null),
            apiFetch('/api/admin/error-metrics').catch(() => null),
            apiFetch('/api/admin/system-performance').catch(() => null),
            apiFetch('/api/admin/user-engagement').catch(() => null),
            apiFetch('/api/admin/history?limit=6').catch(() => [])
        ]);

        const rtt = Math.max(1, Math.round(performance.now() - t0));
        const latEl = document.getElementById('hud-latency');
        if (latEl) latEl.textContent = `RTT ${rtt} ms`;

        document.getElementById('stat-total-users').textContent = Number(stats.total_users || 0).toLocaleString();
        document.getElementById('stat-total-compressions').textContent = Number(stats.total_compressions || 0).toLocaleString();
        const totalSaved = Number(stats.total_tokens_saved || 0);
        document.getElementById('stat-total-tokens-saved').textContent = totalSaved.toLocaleString();

        const costSub = document.getElementById('hud-cost-sub');
        if (costSub) {
            costSub.textContent = `~$${((totalSaved / 1000) * 0.005).toFixed(2)} est. GPT-4o savings`;
        }

        const learningEl = document.getElementById('stat-learning-updates');
        if (learningEl && learning) {
            const clusterHits = Array.isArray(learning.cluster_distribution)
                ? learning.cluster_distribution.reduce((sum, c) => sum + Number(c.total_hits || 0), 0)
                : 0;
            const updatesVal = learning.total_updates ?? learning.active_patterns ?? (clusterHits || learning.total_clusters || 0);
            learningEl.textContent = Number(updatesVal).toLocaleString();
        }
        const errEl = document.getElementById('stat-error-rate');
        const errSub = document.getElementById('hud-error-sub');
        if (errEl && errMetrics) {
            const rateNum = Number(errMetrics.error_rate ?? errMetrics.percentage_with_warnings ?? 0);
            errEl.textContent = `${rateNum.toFixed(1)}%`;
            if (errSub) errSub.textContent = rateNum < 2 ? 'NOMINAL' : 'ELEVATED';
        }

        renderUsageChart(analytics);
        renderHudPerformance(sysPerf, engagement, errMetrics);
        renderHudLiveFeed(recentHist);
    } catch (err) {
        console.error('Failed to load stats:', err);
    }
}

function renderUsageChart(analytics) {
    const ctx = document.getElementById('usageChart').getContext('2d');
    if (window.myChart) window.myChart.destroy();

    let series = [];
    if (Array.isArray(analytics?.daily_counts) && analytics.daily_counts.length > 0) {
        series = analytics.daily_counts.map(s => ({
            label: s.label || s.date || 'Day',
            count: s.count ?? s.compressions ?? 0
        }));
    } else if (Array.isArray(analytics?.by_use_case_and_mode) && analytics.by_use_case_and_mode.length > 0) {
        series = analytics.by_use_case_and_mode.slice(0, 8).map(item => ({
            label: `${item.use_case || 'general'} (${item.mode || 'balanced'})`,
            count: Number(item.compression_count || 0)
        }));
    } else {
        const total = Number((document.getElementById('stat-total-compressions')?.textContent || '0').replace(/,/g, '')) || 0;
        series = [{ label: 'Total', count: total }];
    }

    window.myChart = new Chart(ctx, {
        type: 'line',
        data: {
            labels: series.map(s => s.label),
            datasets: [{
                label: 'Compressions',
                data: series.map(s => s.count),
                borderColor: '#22d3ee',
                backgroundColor: 'rgba(34, 211, 238, 0.12)',
                fill: true,
                tension: 0.4
            }]
        },
        options: {
            responsive: true,
            maintainAspectRatio: false,
            plugins: {
                legend: { display: false }
            },
            scales: {
                y: { grid: { color: 'rgba(255, 255, 255, 0.05)' }, border: { display: false } },
                x: { grid: { display: false }, border: { display: false } }
            }
        }
    });
}

// ── User Management Section ──────────────────────────────────────────────────

async function loadUsers() {
    const tbody = document.getElementById('users-tbody');
    try {
        if (tbody) tbody.innerHTML = '<tr><td colspan="6" class="empty-row">Loading users...</td></tr>';
        const data = await apiFetch('/api/admin/users');
        State.users = Array.isArray(data) ? data : [];
        renderUsers(document.getElementById('user-search')?.value || '');
    } catch (err) {
        console.error('Failed to load users:', err);
        if (tbody) tbody.innerHTML = `<tr><td colspan="6" class="empty-row" style="color:#f87171;">Failed to load users: ${escapeHtml(err.message)}</td></tr>`;
    }
}

function renderUsers(filter = '') {
    const tbody = document.getElementById('users-tbody');
    const q = String(filter || '').trim().toLowerCase();
    const filtered = (State.users || []).filter(u =>
        String(u.id || '').toLowerCase().includes(q) ||
        String(u.username || '').toLowerCase().includes(q) ||
        String(u.email || '').toLowerCase().includes(q) ||
        String(u.business_type || '').toLowerCase().includes(q)
    );

    if (!filtered.length) {
        tbody.innerHTML = '<tr><td colspan="6" class="empty-row">No matching users found.</td></tr>';
        return;
    }

    tbody.innerHTML = filtered.map(u => {
        const uid = String(u.id || '');
        const shortId = uid.length > 10 ? `${uid.substring(0, 8)}…` : uid;
        return `
        <tr>
            <td><span class="hud-mono-chip" title="${escapeHtml(uid)} (click to copy)" style="cursor:pointer;" onclick="copyHudValue('${escapeHtml(uid)}', this)">${escapeHtml(shortId)}</span></td>
            <td>${escapeHtml(u.username || '')}</td>
            <td>${escapeHtml(u.email || '')}</td>
            <td><span class="badge">${escapeHtml(u.business_type || 'Developer')}</span></td>
            <td style="font-family:'JetBrains Mono',monospace; font-size:12px;">${escapeHtml(formatHudDate(u.created_at))}</td>
            <td>
                <button class="action-btn edit-btn" onclick="openEditModal('${escapeHtml(uid)}', '${escapeHtml(u.business_type || '')}')">Edit</button>
                <button class="action-btn delete-btn" onclick="deleteUser('${escapeHtml(uid)}')">Delete</button>
            </td>
        </tr>`;
    }).join('');
}

function openEditModal(id, type) {
    document.getElementById('edit-user-id').value = id;
    const sel = document.getElementById('edit-business-type');
    if (sel) {
        const lower = String(type || '').toLowerCase();
        const match = Array.from(sel.options).find(opt => opt.value.toLowerCase() === lower);
        sel.value = match ? match.value : 'Developer';
    }
    document.getElementById('edit-user-modal').classList.add('active');
}

async function handleEditUser(e) {
    e.preventDefault();
    const id = document.getElementById('edit-user-id').value;
    const type = document.getElementById('edit-business-type').value;

    try {
        await apiFetch(`/api/admin/users/${id}`, {
            method: 'PUT',
            body: JSON.stringify({ business_type: type })
        });
        document.getElementById('edit-user-modal').classList.remove('active');
        loadUsers(); // Refresh
    } catch (err) {
        alert('Failed to update user: ' + err.message);
    }
}

async function deleteUser(id) {
    if (!confirm('Are you sure you want to delete this user? This action cannot be undone.')) return;

    try {
        await apiFetch(`/api/admin/users/${id}`, { method: 'DELETE' });
        loadUsers(); // Refresh
    } catch (err) {
        alert('Failed to delete user: ' + err.message);
    }
}

// ── Global History Section ───────────────────────────────────────────────────

async function loadHistory() {
    const tbody = document.getElementById('history-tbody');
    try {
        if (tbody) tbody.innerHTML = '<tr><td colspan="7" class="empty-row">Loading global history...</td></tr>';
        const data = await apiFetch('/api/admin/history');
        State.history = Array.isArray(data) ? data : [];
        renderHistory();
    } catch (err) {
        console.error('Failed to load history:', err);
        if (tbody) tbody.innerHTML = `<tr><td colspan="7" class="empty-row" style="color:#f87171;">Failed to load history: ${escapeHtml(err.message)}</td></tr>`;
    }
}

function renderHistory() {
    const tbody = document.getElementById('history-tbody');
    const q = (document.getElementById('admin-history-search')?.value || '').trim().toLowerCase();
    const modeFilter = (document.getElementById('admin-history-mode')?.value || 'all').toLowerCase();

    const rows = (State.history || []).filter(h => {
        const m = String(h.mode || 'balanced').toLowerCase();
        const uc = String(h.use_case || 'generic').toLowerCase();
        if (modeFilter !== 'all' && m !== modeFilter) return false;
        if (q && !String(h.user_id || '').toLowerCase().includes(q) && !m.includes(q) && !uc.includes(q)) return false;
        return true;
    });

    if (!rows.length) {
        tbody.innerHTML = '<tr><td colspan="7" class="empty-row">No matching global history records.</td></tr>';
        return;
    }

    tbody.innerHTML = rows.map(h => {
        const uid = String(h.user_id || '');
        const shortUid = uid.length > 10 ? `${uid.substring(0, 8)}…` : uid;
        return `
        <tr>
            <td><span class="hud-mono-chip" title="${escapeHtml(uid)} (click to copy)" style="cursor:pointer;" onclick="copyHudValue('${escapeHtml(uid)}', this)">${escapeHtml(shortUid)}</span></td>
            <td><span class="badge">${escapeHtml(h.use_case || 'generic')}</span></td>
            <td style="font-variant-numeric:tabular-nums;">${Number(h.token_original || 0).toLocaleString()}</td>
            <td style="font-variant-numeric:tabular-nums;">${Number(h.token_final || 0).toLocaleString()}</td>
            <td style="color: #34d399; font-weight:600; font-variant-numeric:tabular-nums;">+${Number(h.tokens_saved || 0).toLocaleString()}</td>
            <td><span class="badge">${escapeHtml(h.mode || 'balanced')}</span></td>
            <td style="font-family:'JetBrains Mono',monospace; font-size:12px;">${escapeHtml(formatHudDate(h.created_at))}</td>
        </tr>`;
    }).join('');
}

// ── Feedback Signals Section ─────────────────────────────────────────────────

async function loadFeedback() {
    const tbodyType = document.getElementById('feedback-type-tbody');
    const tbodyUseCase = document.getElementById('feedback-usecase-tbody');
    try {
        if (tbodyType) tbodyType.innerHTML = '<tr><td colspan="3" class="empty-row">Loading feedback signals...</td></tr>';
        if (tbodyUseCase) tbodyUseCase.innerHTML = '<tr><td colspan="4" class="empty-row">Loading feedback by use case...</td></tr>';

        const data = await apiFetch('/api/admin/feedback-analysis');
        renderFeedback(data);
    } catch (err) {
        console.error('Failed to load feedback analysis:', err);
        if (tbodyType) tbodyType.innerHTML = `<tr><td colspan="3" class="empty-row" style="color: #f87171;">Failed: ${err.message}</td></tr>`;
        if (tbodyUseCase) tbodyUseCase.innerHTML = `<tr><td colspan="4" class="empty-row" style="color: #f87171;">Failed: ${err.message}</td></tr>`;
    }
}

function renderFeedback(data) {
    if (!data) return;

    const totalEl = document.getElementById('stat-total-feedback');
    if (totalEl) totalEl.textContent = Number(data.total_feedback_signals || 0).toLocaleString();

    const tbodyType = document.getElementById('feedback-type-tbody');
    if (tbodyType) {
        if (!Array.isArray(data.feedback_by_type) || data.feedback_by_type.length === 0) {
            tbodyType.innerHTML = '<tr><td colspan="3" class="empty-row">No feedback signals recorded yet.</td></tr>';
        } else {
            tbodyType.innerHTML = data.feedback_by_type.map(item => {
                const avgNum = Number(item.avg_value || 0);
                const avg = avgNum.toFixed(2);
                const avgColor = avgNum > 0 ? '#4ade80' : (avgNum < 0 ? '#f87171' : '#94a3b8');
                return `
                    <tr>
                        <td><span class="badge" style="background: rgba(56, 189, 248, 0.15); color: #38bdf8;">${escapeHtml(item.signal_type || 'unknown')}</span></td>
                        <td style="font-variant-numeric:tabular-nums;">${Number(item.signal_count || 0).toLocaleString()}</td>
                        <td style="color: ${avgColor}; font-weight: 600; font-variant-numeric:tabular-nums;">${avgNum > 0 ? '+' + avg : avg}</td>
                    </tr>
                `;
            }).join('');
        }
    }

    const tbodyUseCase = document.getElementById('feedback-usecase-tbody');
    if (tbodyUseCase) {
        if (!Array.isArray(data.feedback_by_use_case) || data.feedback_by_use_case.length === 0) {
            tbodyUseCase.innerHTML = '<tr><td colspan="4" class="empty-row">No use case feedback recorded yet.</td></tr>';
        } else {
            tbodyUseCase.innerHTML = data.feedback_by_use_case.map(item => {
                const avgNum = Number(item.avg_value || 0);
                const avg = avgNum.toFixed(2);
                const avgColor = avgNum > 0 ? '#4ade80' : (avgNum < 0 ? '#f87171' : '#94a3b8');
                return `
                    <tr>
                        <td><strong>${escapeHtml(item.use_case || 'general')}</strong></td>
                        <td><span class="badge">${escapeHtml(item.signal_type || 'unknown')}</span></td>
                        <td style="font-variant-numeric:tabular-nums;">${Number(item.signal_count || 0).toLocaleString()}</td>
                        <td style="color: ${avgColor}; font-weight: 600; font-variant-numeric:tabular-nums;">${avgNum > 0 ? '+' + avg : avg}</td>
                    </tr>
                `;
            }).join('');
        }
    }
}

// ── RAG Knowledge Base Section ───────────────────────────────────────────────

function initRagEventListeners() {
    const dropZone = document.getElementById('drop-zone');
    const fileInput = document.getElementById('pdf-file-input');
    const browseBtn = document.getElementById('browse-btn');
    const chipRemove = document.getElementById('chip-remove');
    const uploadBtn = document.getElementById('upload-btn');
    const refreshBtn = document.getElementById('rag-refresh-btn');

    if (!dropZone) return;

    // Trigger file dialog
    browseBtn.addEventListener('click', () => {
        fileInput.click();
    });

    // File input change
    fileInput.addEventListener('change', (e) => {
        if (e.target.files.length > 0) {
            handleFileSelect(e.target.files[0]);
        }
    });

    // Drag & Drop
    ['dragenter', 'dragover'].forEach(eventName => {
        dropZone.addEventListener(eventName, (e) => {
            e.preventDefault();
            e.stopPropagation();
            dropZone.classList.add('drag-over');
        }, false);
    });

    ['dragleave', 'drop'].forEach(eventName => {
        dropZone.addEventListener(eventName, (e) => {
            e.preventDefault();
            e.stopPropagation();
            dropZone.classList.remove('drag-over');
        }, false);
    });

    dropZone.addEventListener('drop', (e) => {
        const dt = e.dataTransfer;
        const files = dt.files;
        if (files.length > 0) {
            handleFileSelect(files[0]);
        }
    });

    // Remove selected file
    chipRemove.addEventListener('click', (e) => {
        e.stopPropagation();
        clearFileSelect();
    });

    // Support clicking drop zone to choose file
    dropZone.addEventListener('click', (e) => {
        if (!State.selectedFile && e.target !== chipRemove && !chipRemove.contains(e.target) && e.target !== browseBtn) {
            fileInput.click();
        }
    });

    // Upload button
    uploadBtn.addEventListener('click', handleRagUpload);

    // Refresh button
    refreshBtn.addEventListener('click', loadRag);
}

function handleFileSelect(file) {
    if (!file) return;

    if (!file.name.toLowerCase().endsWith('.pdf')) {
        showRagToast('Only PDF files are supported.', 'error');
        clearFileSelect();
        return;
    }

    const maxSize = 50 * 1024 * 1024;
    if (file.size > maxSize) {
        showRagToast('File exceeds 50 MB limit.', 'error');
        clearFileSelect();
        return;
    }

    State.selectedFile = file;
    document.getElementById('chip-name').textContent = file.name;
    document.getElementById('file-chip').classList.remove('hidden');
    document.querySelector('.drop-zone-inner').classList.add('hidden');
    document.getElementById('upload-btn').removeAttribute('disabled');
    clearRagToast();
}

function clearFileSelect() {
    State.selectedFile = null;
    document.getElementById('pdf-file-input').value = '';
    document.getElementById('file-chip').classList.add('hidden');
    document.querySelector('.drop-zone-inner').classList.remove('hidden');
    document.getElementById('upload-btn').setAttribute('disabled', 'true');
    document.getElementById('rag-progress').classList.add('hidden');
}

function showRagToast(message, type = 'success') {
    const toast = document.getElementById('rag-toast');
    toast.textContent = message;
    toast.className = `rag-toast ${type}`;
    toast.classList.remove('hidden');
}

function clearRagToast() {
    const toast = document.getElementById('rag-toast');
    toast.classList.add('hidden');
    toast.textContent = '';
}

async function handleRagUpload() {
    if (!State.selectedFile) return;

    const file = State.selectedFile;
    const targetUserId = document.getElementById('rag-target-user').value.trim();

    const formData = new FormData();
    formData.append('file', file);

    const uploadBtn = document.getElementById('upload-btn');
    uploadBtn.setAttribute('disabled', 'true');

    const progressDiv = document.getElementById('rag-progress');
    const progressFill = document.getElementById('progress-fill');
    const progressLabel = document.getElementById('progress-label');

    progressDiv.classList.remove('hidden');
    progressFill.style.width = '10%';
    progressLabel.textContent = 'Preparing file upload...';
    clearRagToast();

    try {
        let url = '/api/admin/rag/upload';
        if (targetUserId) {
            url += `?target_user_id=${encodeURIComponent(targetUserId)}`;
        }

        progressFill.style.width = '30%';
        progressLabel.textContent = 'Uploading to server...';

        const response = await fetch(url, {
            method: 'POST',
            headers: {
                'Authorization': `Bearer ${State.token}`
            },
            body: formData
        });

        if (response.status === 401 || response.status === 403) {
            window.location.href = '/';
            return;
        }

        progressFill.style.width = '70%';
        progressLabel.textContent = 'Processing PDF chunks...';

        if (!response.ok) {
            const err = await response.json();
            throw new Error(err.error?.message || 'Upload failed');
        }

        const data = await response.json();

        progressFill.style.width = '100%';
        progressLabel.textContent = 'Ingestion initiated!';

        showRagToast(data.message || 'PDF accepted. Processing runs in the background.', 'success');

        setTimeout(() => {
            clearFileSelect();
            loadRag();
        }, 1500);

    } catch (err) {
        console.error('Failed to upload PDF:', err);
        showRagToast('Upload failed: ' + err.message, 'error');
        uploadBtn.removeAttribute('disabled');
        progressDiv.classList.add('hidden');
    }
}

async function loadRag() {
    const tbody = document.getElementById('rag-docs-tbody');
    try {
        if (tbody.innerHTML === '' || tbody.innerHTML.includes('empty-row')) {
            tbody.innerHTML = '<tr><td colspan="8" class="empty-row">Loading documents…</td></tr>';
        }

        const docs = await apiFetch('/api/admin/rag/documents');
        State.ragDocs = docs;
        renderRagDocs();
    } catch (err) {
        console.error('Failed to load RAG documents:', err);
        tbody.innerHTML = `<tr><td colspan="8" class="empty-row" style="color: #f87171;">Failed to load documents: ${err.message}</td></tr>`;
    }
}

function renderRagDocs() {
    const tbody = document.getElementById('rag-docs-tbody');
    if (!State.ragDocs || State.ragDocs.length === 0) {
        tbody.innerHTML = '<tr><td colspan="8" class="empty-row">No documents found.</td></tr>';
        return;
    }

    tbody.innerHTML = State.ragDocs.map(doc => {
        let statusClass = 'status-processing';
        let statusText = 'Processing';
        if (doc.status === 'ready') {
            statusClass = 'status-ready';
            statusText = 'Ready';
        } else if (doc.status === 'failed') {
            statusClass = 'status-failed';
            statusText = 'Failed';
        }

        const domainVal = doc.domain || 'N/A';
        const pageCountVal = doc.page_count !== null && doc.page_count !== undefined ? doc.page_count : '-';
        const chunkCountVal = doc.chunk_count !== null && doc.chunk_count !== undefined ? doc.chunk_count : '-';
        const uploadedDate = new Date(doc.created_at).toLocaleString();

        return `
            <tr>
                <td title="${doc.filename}">${doc.filename}</td>
                <td title="${doc.user_id}">${doc.user_id.substring(0, 8)}...</td>
                <td><span class="badge" style="background: rgba(129, 140, 248, 0.15); color: #818cf8; border: 1px solid rgba(129, 140, 248, 0.3);">${domainVal}</span></td>
                <td>${pageCountVal}</td>
                <td>${chunkCountVal}</td>
                <td><span class="status-badge ${statusClass}">${statusText}</span></td>
                <td>${uploadedDate}</td>
                <td>
                    <button class="action-btn delete-btn" onclick="deleteRagDoc('${doc.id}')">Delete</button>
                </td>
            </tr>
        `;
    }).join('');
}

async function deleteRagDoc(id) {
    if (!confirm('Are you sure you want to delete this RAG document? All associated chunks and embeddings will be removed.')) return;

    try {
        await apiFetch(`/api/admin/rag/documents/${id}`, { method: 'DELETE' });
        loadRag();
    } catch (err) {
        alert('Failed to delete document: ' + err.message);
    }
}

function escapeHtml(str) {
    const d = document.createElement('div');
    d.textContent = str == null ? '' : String(str);
    return d.innerHTML;
}

async function handleAdminAsk() {
    const appMode = document.getElementById('admin-scenario-mode-select')?.value || 'Regular';
    const domain = document.getElementById('admin-scenario-domain-select')?.value || null;
    const style = document.getElementById('admin-scenario-style-select')?.value || 'balanced';
    const question = document.getElementById('admin-scenario-query-input')?.value.trim();
    const outSec = document.getElementById('admin-scenario-result-section');
    const outEl = document.getElementById('admin-scenario-result-output');
    const btn = document.getElementById('admin-ask-btn');

    if (!question) return;
    if (btn) btn.disabled = true;

    try {
        const resp = await apiFetch('/api/scenario/ask', {
            method: 'POST',
            body: JSON.stringify({
                app_mode: appMode,
                domain: appMode === 'Scenario' ? domain : null,
                style: appMode === 'Regular' ? style : null,
                question
            })
        });
        if (outSec) outSec.style.display = 'block';
        if (outEl) outEl.textContent = resp?.output_text || 'No response.';
    } catch (err) {
        if (outSec) outSec.style.display = 'block';
        if (outEl) outEl.textContent = 'Query failed: ' + err.message;
    } finally {
        if (btn) btn.disabled = false;
    }
}

// Expose functions called by HTML onclick handlers
window.loadFeedback = loadFeedback;
window.openEditModal = openEditModal;
window.deleteUser = deleteUser;
window.deleteRagDoc = deleteRagDoc;

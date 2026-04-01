const State = {
    token: localStorage.getItem('tce_token'),
    userId: localStorage.getItem('tce_user_id'),
    username: localStorage.getItem('tce_username'),
    currentSection: 'overview',
    users: [],
    history: []
};

// ── Init ────────────────────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
    if (!State.token) {
        window.location.href = '/'; // Redirect to login if no token
        return;
    }
    
    document.getElementById('admin-username').textContent = State.username;
    initEventListeners();
    loadOverview();
});

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
        window.location.href = '/';
    });

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

    document.getElementById('section-title').textContent = sectionId.charAt(0).toUpperCase() + sectionId.slice(1);

    // Load data for section
    if (sectionId === 'overview') loadOverview();
    else if (sectionId === 'users') loadUsers();
    else if (sectionId === 'history') loadHistory();
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

// ── Overview Section ────────────────────────────────────────────────────────

async function loadOverview() {
    try {
        const stats = await apiFetch('/api/admin/stats');
        document.getElementById('stat-total-users').textContent = stats.total_users;
        document.getElementById('stat-total-compressions').textContent = stats.total_compressions;
        document.getElementById('stat-total-tokens-saved').textContent = stats.total_tokens_saved;
        
        // Mock data for usage chart since we don't have a time-series endpoint yet
        // In a real app, I'd fetch /api/admin/usage-over-time
        renderUsageChart();
    } catch (err) {
        console.error('Failed to load stats:', err);
    }
}

function renderUsageChart() {
    const ctx = document.getElementById('usageChart').getContext('2d');
    
    // Check if chart already exists
    if (window.myChart) window.myChart.destroy();

    window.myChart = new Chart(ctx, {
        type: 'line',
        data: {
            labels: ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'],
            datasets: [{
                label: 'Compressions',
                data: [12, 19, 3, 5, 2, 3, 9],
                borderColor: '#38bdf8',
                backgroundColor: 'rgba(56, 189, 248, 0.1)',
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
    try {
        State.users = await apiFetch('/api/admin/users');
        renderUsers();
    } catch (err) {
        console.error('Failed to load users:', err);
    }
}

function renderUsers(filter = '') {
    const tbody = document.getElementById('users-tbody');
    const filtered = State.users.filter(u => 
        u.username.toLowerCase().includes(filter.toLowerCase()) || 
        u.email.toLowerCase().includes(filter.toLowerCase())
    );

    tbody.innerHTML = filtered.map(u => `
        <tr>
            <td>${u.username}</td>
            <td>${u.email}</td>
            <td><span class="badge">${u.business_type}</span></td>
            <td>${new Date(u.created_at).toLocaleDateString()}</td>
            <td>
                <button class="action-btn edit-btn" onclick="openEditModal('${u.id}', '${u.business_type}')">Edit</button>
                <button class="action-btn delete-btn" onclick="deleteUser('${u.id}')">Delete</button>
            </td>
        </tr>
    `).join('');
}

function openEditModal(id, type) {
    document.getElementById('edit-user-id').value = id;
    document.getElementById('edit-business-type').value = type;
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
    try {
        State.history = await apiFetch('/api/admin/history');
        renderHistory();
    } catch (err) {
        console.error('Failed to load history:', err);
    }
}

function renderHistory() {
    const tbody = document.getElementById('history-tbody');
    tbody.innerHTML = State.history.map(h => `
        <tr>
            <td title="${h.user_id}">${h.user_id.substring(0, 8)}...</td>
            <td>${h.token_original}</td>
            <td>${h.token_final}</td>
            <td style="color: #4ade80">+${h.tokens_saved}</td>
            <td><span class="badge">${h.mode}</span></td>
            <td>${new Date(h.created_at).toLocaleString()}</td>
        </tr>
    `).join('');
}

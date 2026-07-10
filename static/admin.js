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
    } else {
        document.getElementById('section-title').textContent = sectionId.charAt(0).toUpperCase() + sectionId.slice(1);
    }

    // Load data for section
    if (sectionId === 'overview') loadOverview();
    else if (sectionId === 'users') loadUsers();
    else if (sectionId === 'history') loadHistory();
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

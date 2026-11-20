//! static/rag_workspace.js — Self-service User RAG Knowledge Base & Grounding Verifier

const RagWorkspace = {
    docs: [],
    selectedDocIds: new Set(),
    pollTimer: null,
    dataSource: 'cloud', // 'cloud' | 'own' (mutually exclusive)
    localConnected: false,

    init() {
        const fileInput = document.getElementById('user-rag-file-input');
        const uploadBtn = document.getElementById('user-rag-upload-btn');
        if (uploadBtn && fileInput) {
            uploadBtn.addEventListener('click', () => {
                if (this.dataSource !== 'cloud') return;
                if (!State.token) {
                    showToast('Please sign in to upload PDFs to your Knowledge Base.', 'error');
                    if (typeof openAuthModal === 'function') openAuthModal();
                    return;
                }
                fileInput.click();
            });
            fileInput.addEventListener('change', async (e) => {
                const file = e.target.files?.[0];
                if (!file) return;
                await this.uploadPdf(file);
                fileInput.value = '';
            });
        }
        this.setDataSource('cloud');
    },

    setDataSource(source) {
        const normalized = source === 'own' ? 'own' : 'cloud';
        this.dataSource = normalized;

        const cloudPanel = document.getElementById('rag-cloud-panel');
        const ownPanel = document.getElementById('rag-own-panel');
        const cloudOpt = document.getElementById('rag-opt-cloud');
        const ownOpt = document.getElementById('rag-opt-own');
        const askBtn = document.getElementById('ask-btn');

        const cloudRadio = document.querySelector('input[name="ask-data-source"][value="cloud"]');
        const ownRadio = document.querySelector('input[name="ask-data-source"][value="own"]');
        if (cloudRadio) cloudRadio.checked = normalized === 'cloud';
        if (ownRadio) ownRadio.checked = normalized === 'own';

        if (cloudPanel) cloudPanel.style.display = normalized === 'cloud' ? 'flex' : 'none';
        if (ownPanel) ownPanel.style.display = normalized === 'own' ? 'flex' : 'none';
        if (cloudOpt) cloudOpt.classList.toggle('active', normalized === 'cloud');
        if (ownOpt) ownOpt.classList.toggle('active', normalized === 'own');

        if (askBtn) {
            askBtn.textContent = normalized === 'own'
                ? '💻 Query Own Local Folder'
                : '🔍 Query Internal Knowledge Base';
        }

        if (normalized === 'own') {
            this.checkLocalHealth();
        }
    },

    getDataSource() {
        return this.dataSource === 'own' ? 'own' : 'cloud';
    },

    async checkLocalHealth() {
        const pill = document.getElementById('local-rag-status-pill');
        const watchEl = document.getElementById('local-rag-watch-dir');
        const countsEl = document.getElementById('local-rag-counts');

        if (pill) {
            pill.className = 'rag-status-pill status-processing';
            pill.textContent = 'checking';
        }

        try {
            const health = await API.localRagHealth();
            this.localConnected = true;
            if (pill) {
                pill.className = 'rag-status-pill status-ready';
                pill.textContent = 'connected';
            }
            if (watchEl) {
                watchEl.textContent = `Folder: ${health.watched_dir || '(default)'}`;
            }
            if (countsEl) {
                const mode = health.neural_embeddings ? 'ONNX MiniLM' : 'Hash Fallback';
                countsEl.textContent = `${health.indexed_files ?? 0} files · ${health.indexed_chunks ?? 0} chunks · ${mode}`;
            }
            const cmdWrap = document.getElementById('local-rag-cmd-wrap');
            if (cmdWrap) cmdWrap.classList.add('hidden');
            return health;
        } catch (err) {
            this.localConnected = false;
            if (pill) {
                pill.className = 'rag-status-pill status-failed';
                pill.textContent = 'offline';
            }
            if (watchEl) {
                watchEl.textContent = 'Endpoint: http://127.0.0.1:49152';
            }
            if (countsEl) {
                countsEl.textContent = 'Local companion is offline. Run this command to start it:';
            }
            const cmdWrap = document.getElementById('local-rag-cmd-wrap');
            if (cmdWrap) cmdWrap.classList.remove('hidden');
            return null;
        }
    },

    copyStartCommand() {
        const cmd = document.getElementById('local-rag-start-cmd')?.textContent?.trim() || 'cargo run --bin rag -- --watch ./docs';
        if (!navigator.clipboard || typeof navigator.clipboard.writeText !== 'function') {
            showToast('Clipboard unavailable in this browser context', 'error');
            return;
        }
        navigator.clipboard.writeText(cmd).then(() => {
            const btn = document.getElementById('local-rag-copy-cmd-btn');
            if (btn) {
                const prev = btn.textContent;
                btn.textContent = '✓ Command Copied';
                btn.classList.add('btn-copied');
                setTimeout(() => {
                    btn.textContent = prev;
                    btn.classList.remove('btn-copied');
                }, 1800);
            }
            showToast('Local companion start command copied.', 'success');
        }).catch(err => showToast('Copy failed: ' + err.message, 'error'));
    },

    async loadDocuments() {
        const listEl = document.getElementById('user-rag-docs-list');
        if (!listEl) return;

        if (!State.token) {
            listEl.innerHTML = '<div class="rag-empty-note">Sign in to upload &amp; manage your own PDFs.</div>';
            return;
        }

        try {
            const docs = await API.ragList();
            this.docs = Array.isArray(docs) ? docs : [];
            this.renderDocuments();

            const hasProcessing = this.docs.some(d => d.status === 'processing');
            if (hasProcessing && !this.pollTimer) {
                this.pollTimer = setTimeout(() => {
                    this.pollTimer = null;
                    this.loadDocuments();
                }, 3000);
            }
        } catch (err) {
            listEl.innerHTML = `<div class="rag-empty-note">Could not load documents: ${escapeHtml(err.message)}</div>`;
        }
    },

    renderDocuments() {
        const listEl = document.getElementById('user-rag-docs-list');
        if (!listEl) return;

        if (this.docs.length === 0) {
            listEl.innerHTML = '<div class="rag-empty-note">No PDFs uploaded yet. Upload a PDF to ground your prompts and answers.</div>';
            return;
        }

        listEl.innerHTML = '';
        this.docs.forEach(doc => {
            const item = document.createElement('div');
            item.className = 'rag-doc-item';

            const label = document.createElement('label');
            label.className = 'rag-doc-label';

            const cb = document.createElement('input');
            cb.type = 'checkbox';
            cb.checked = this.selectedDocIds.has(doc.id);
            cb.addEventListener('change', () => {
                if (cb.checked) this.selectedDocIds.add(doc.id);
                else this.selectedDocIds.delete(doc.id);
            });

            const meta = document.createElement('div');
            meta.className = 'rag-doc-meta';
            const title = document.createElement('div');
            title.className = 'rag-doc-name';
            title.textContent = doc.filename || 'Document.pdf';

            const sub = document.createElement('div');
            sub.className = 'rag-doc-sub';
            const pages = doc.page_count != null ? `${doc.page_count}p` : '-';
            const chunks = doc.chunk_count != null ? `${doc.chunk_count} chunks` : '-';
            sub.textContent = `${doc.domain || 'general'} · ${pages} · ${chunks}`;

            meta.appendChild(title);
            meta.appendChild(sub);

            const status = document.createElement('span');
            status.className = `rag-status-pill status-${doc.status || 'processing'}`;
            status.textContent = doc.status || 'processing';

            const delBtn = document.createElement('button');
            delBtn.type = 'button';
            delBtn.className = 'rag-del-btn';
            delBtn.title = 'Delete document';
            delBtn.textContent = '✕';
            delBtn.addEventListener('click', (e) => {
                e.preventDefault();
                e.stopPropagation();
                this.deleteDoc(doc.id);
            });

            label.appendChild(cb);
            label.appendChild(meta);
            label.appendChild(status);
            label.appendChild(delBtn);
            item.appendChild(label);
            listEl.appendChild(item);
        });
    },

    async uploadPdf(file) {
        if (!file.name.toLowerCase().endsWith('.pdf')) {
            return showToast('Only .pdf files are supported in Knowledge Base.', 'error');
        }
        try {
            showToast(`Uploading ${file.name}...`, 'success');
            await API.ragUpload(file);
            showToast(`${file.name} queued for indexing.`, 'success');
            await this.loadDocuments();
        } catch (err) {
            showToast(err.message || 'Upload failed', 'error');
        }
    },

    async deleteDoc(id) {
        try {
            await API.ragDelete(id);
            this.selectedDocIds.delete(id);
            showToast('Document removed.', 'success');
            await this.loadDocuments();
        } catch (err) {
            showToast(err.message || 'Delete failed', 'error');
        }
    },

    getSelectedDocIds() {
        return Array.from(this.selectedDocIds);
    },

    openVerifyModal() {
        const modal = document.getElementById('verify-modal');
        if (!modal) return;
        if (typeof openDialogSafe === 'function') {
            openDialogSafe(modal);
        } else if (typeof modal.showModal === 'function') {
            if (!modal.open) modal.showModal();
        } else {
            modal.style.display = 'flex';
        }
    },

    closeVerifyModal() {
        const modal = document.getElementById('verify-modal');
        if (!modal) return;
        if (typeof closeDialogSafe === 'function') {
            closeDialogSafe(modal);
        } else if (typeof modal.close === 'function') {
            if (modal.open) modal.close();
        } else {
            modal.style.display = 'none';
        }
    },

    async runVerify() {
        const factsRaw = document.getElementById('verify-facts-input')?.value.trim() || '';
        const answerRaw = document.getElementById('verify-answer-input')?.value.trim() || '';
        const outEl = document.getElementById('verify-result-box');

        if (!answerRaw) {
            return showToast('Paste the LLM reply to verify.', 'error');
        }

        const facts = factsRaw
            .split(/\r?\n/)
            .map(l => l.trim())
            .filter(Boolean)
            .map((content, idx) => ({ id: `F${idx + 1}`, content }));

        try {
            const report = await API.verifyGrounding({
                llm_answer: answerRaw,
                facts,
                requested_items: []
            });
            if (outEl) {
                outEl.style.display = 'block';
                outEl.textContent = JSON.stringify(report, null, 2);
            }
            showToast('Verification complete.', 'success');
        } catch (err) {
            showToast(err.message || 'Verification failed', 'error');
        }
    }
};

document.addEventListener('DOMContentLoaded', () => {
    RagWorkspace.init();
});

if (typeof window !== 'undefined') {
    window.RagWorkspace = RagWorkspace;
}

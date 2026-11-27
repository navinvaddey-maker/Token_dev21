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
            askBtn.dataset.icon = normalized === 'own' ? 'laptop' : 'search';
            askBtn.textContent = normalized === 'own'
                ? 'Query Own Local Folder'
                : 'Query Internal Knowledge Base';
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

    lastVerifyReport: null,

    renderVerifyReport(report) {
        const outEl = document.getElementById('verify-result-box');
        if (!outEl) return;
        this.lastVerifyReport = report;
        const esc = typeof OryxDomUtils !== 'undefined' && OryxDomUtils.escapeHtmlEntities
            ? OryxDomUtils.escapeHtmlEntities
            : (typeof escapeHtml === 'function'
                ? escapeHtml
                : (s) => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c])));

        const passed = Boolean(report.passed);
        const scorePct = Math.round((Number(report.grounding_score || 0)) * 100);
        const unsupported = Array.isArray(report.unsupported_claims) ? report.unsupported_claims : [];
        const missing = Array.isArray(report.missing_items) ? report.missing_items : [];
        const grounded = Array.isArray(report.grounded) ? report.grounded : [];
        const partial = Array.isArray(report.partial) ? report.partial : [];
        const notFound = Array.isArray(report.not_found) ? report.not_found : [];
        const verifiedRows = [
            ...grounded.map(c => ({ ...c, status: c.status || 'FOUND' })),
            ...partial.map(c => ({ ...c, status: c.status || 'PARTIAL_MATCH' })),
            ...notFound.map(c => ({ ...c, status: c.status || 'NOT_FOUND' })),
        ];

        const badgeBg = passed ? 'rgba(16,185,129,0.16)' : 'rgba(239,68,68,0.16)';
        const badgeFg = passed ? '#10b981' : '#ef4444';

        let html = `
            <div style="display:flex;align-items:center;justify-content:space-between;gap:0.5rem;flex-wrap:wrap;margin-bottom:0.65rem;">
                <div style="display:flex;align-items:center;gap:0.5rem;flex-wrap:wrap;">
                    <span style="padding:0.2rem 0.55rem;border-radius:999px;font-weight:700;font-size:0.72rem;background:${badgeBg};color:${badgeFg};">
                        ${passed ? '✓ GROUNDED' : '✕ ISSUES FOUND'}
                    </span>
                    <span style="font-size:0.76rem;color:var(--head-text);font-weight:600;">Score: ${scorePct}%</span>
                    <span style="font-size:0.72rem;color:var(--text-muted);">Grounded: ${grounded.length} · Partial: ${partial.length} · Not Found: ${notFound.length} · Unsupported: ${unsupported.length} · Missing: ${missing.length}</span>
                </div>
                <div style="display:flex;align-items:center;gap:0.4rem;flex-wrap:wrap;">
                    ${!passed ? '<button type="button" class="copy-btn repair-trigger-btn oryx-icon-start" data-icon="zap" data-action="generateRepairFromVerify" style="padding:0.22rem 0.55rem;font-size:0.72rem;">Generate Repair Prompt</button>' : ''}
                    <button type="button" class="copy-btn oryx-icon-start" data-icon="clipboard" data-action="exportVerifyReport" style="padding:0.22rem 0.55rem;font-size:0.72rem;">Export Report</button>
                </div>
            </div>
        `;

        if (verifiedRows.length > 0) {
            html += '<div style="display:flex;flex-direction:column;gap:0.35rem;margin-bottom:0.5rem;">';
            verifiedRows.forEach(c => {
                const st = String(c.status || '').toUpperCase();
                const color = st === 'FOUND' ? '#10b981' : (st === 'PARTIAL_MATCH' ? '#f59e0b' : '#ef4444');
                const bg = st === 'FOUND' ? 'rgba(16,185,129,0.08)' : (st === 'PARTIAL_MATCH' ? 'rgba(245,158,11,0.08)' : 'rgba(239,68,68,0.08)');
                const label = c.item || c.claim || '';
                const answerText = c.answer ? ` — ${c.answer}` : '';
                const cited = Array.isArray(c.cited_facts) && c.cited_facts.length > 0
                    ? ` [${c.cited_facts.join(', ')}]`
                    : '';
                const body = label ? `${label}${answerText}${cited}` : (c.answer || JSON.stringify(c));
                html += `<div style="padding:0.4rem 0.55rem;border-radius:0.45rem;background:${bg};border-left:3px solid ${color};font-size:0.74rem;">
                    <strong style="color:${color};">${esc(st)}</strong> · ${esc(body)}
                </div>`;
            });
            html += '</div>';
        }

        if (unsupported.length > 0) {
            html += '<div style="display:flex;flex-direction:column;gap:0.35rem;margin-bottom:0.5rem;">';
            unsupported.forEach(u => {
                html += `<div style="padding:0.4rem 0.55rem;border-radius:0.45rem;background:rgba(239,68,68,0.1);border-left:3px solid #ef4444;font-size:0.74rem;">
                    <strong style="color:#ef4444;">${esc(u.issue || 'UNSUPPORTED')}</strong>: ${esc(u.detail || u.claim || JSON.stringify(u))}
                </div>`;
            });
            html += '</div>';
        }

        if (missing.length > 0) {
            html += `<div style="padding:0.4rem 0.55rem;border-radius:0.45rem;background:rgba(245,158,11,0.1);border-left:3px solid #f59e0b;font-size:0.74rem;">
                <strong style="color:#f59e0b;">MISSING ITEMS:</strong> ${missing.map(esc).join(', ')}
            </div>`;
        }

        outEl.style.display = 'block';
        outEl.innerHTML = html;
    },

    exportVerifyReport() {
        if (!this.lastVerifyReport) return;
        const r = this.lastVerifyReport;
        const md = [
            '### Oryx Studio — Grounding Verification Report',
            `- **Status:** ${r.passed ? 'PASSED' : 'FAILED'}`,
            `- **Grounding Score:** ${Math.round((Number(r.grounding_score || 0)) * 100)}%`,
            `- **Unsupported Claims:** ${(r.unsupported_claims || []).length}`,
            `- **Missing Items:** ${(r.missing_items || []).length}`
        ].join('\n');
        if (!navigator.clipboard || typeof navigator.clipboard.writeText !== 'function') {
            showToast('Clipboard unavailable in this browser context', 'error');
            return;
        }
        navigator.clipboard.writeText(md).then(() => {
            showToast('Verification report copied!', 'success');
        }).catch(err => showToast('Copy failed: ' + err.message, 'error'));
    },

    async runVerify() {
        const factsRaw = document.getElementById('verify-facts-input')?.value.trim() || '';
        const answerRaw = document.getElementById('verify-answer-input')?.value.trim() || '';

        if (!answerRaw) {
            return showToast('Paste the LLM reply to verify.', 'error');
        }

        const numbered_facts = factsRaw
            .split(/\r?\n/)
            .map(l => l.trim())
            .filter(Boolean)
            .map((text, idx) => ({ id: `F${idx + 1}`, text }));

        try {
            const report = await API.verifyGrounding({
                llm_answer: answerRaw,
                numbered_facts,
                requested_items: []
            });
            this.renderVerifyReport(report);
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

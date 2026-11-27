//! static/mock_api.js — Client-side mock compression engine & API communication layer
//! ARCH: §1 Reality Check & §5 NPAE Grounding — Provides deterministic, zero-auth client-side
//! sandbox execution when unauthenticated, and communicates with Axum backend when signed in.

/**
 * Shared DOM utilities fallback when `oryx_dom_utils.js` is not pre-loaded.
 */
if (typeof globalThis.OryxDomUtils === 'undefined') {
    class OryxDomUtils {
        /**
         * Escapes HTML special characters in untrusted strings.
         * Matches app.js apostrophe entity (&#39;) for 100% test compatibility.
         *
         * # Arguments
         * * `value` - Any input value (string, number, null, undefined)
         */
        static escapeHtmlEntities(value) {
            if (value === null || value === undefined) return '';
            return String(value)
                .replace(/&/g, '&amp;')
                .replace(/</g, '&lt;')
                .replace(/>/g, '&gt;')
                .replace(/"/g, '&quot;')
                .replace(/'/g, '&#39;');
        }

        /**
         * Initializes calm hover-only 3D tilt & specular glint on a `.oryx-3d-emblem` container.
         * Zero `requestAnimationFrame` idle loop and zero `<canvas>` pixel shaders.
         *
         * # Arguments
         * * `containerId` - DOM ID of the `.tc-logo-icon` / `.logo-icon` wrapper containing `.oryx-3d-emblem`.
         *
         * # Panics
         * Never throws; catches DOM/matchMedia errors and logs a structured warning.
         */
        static initHoverTiltEmblem(containerId) {
            try {
                const container = document.getElementById(containerId);
                const emblem = container?.querySelector('.oryx-3d-emblem');
                if (!container || !emblem || container.dataset.oryx3dInit === '1') {
                    return;
                }
                container.dataset.oryx3dInit = '1';

                const resetRestingPose = () => {
                    emblem.style.setProperty('--oryx-rx', '0deg');
                    emblem.style.setProperty('--oryx-ry', '0deg');
                    emblem.style.setProperty('--oryx-glint-x', '30%');
                    emblem.style.setProperty('--oryx-glint-y', '24%');
                };

                resetRestingPose();

                const targetHost = container.closest('.tc-logo, .logo') || container;
                const prefersReducedMotion = () => Boolean(
                    typeof window !== 'undefined' &&
                    typeof window.matchMedia === 'function' &&
                    window.matchMedia('(prefers-reduced-motion: reduce)').matches
                );

                targetHost.addEventListener('pointermove', (e) => {
                    if (prefersReducedMotion()) {
                        resetRestingPose();
                        return;
                    }
                    const rect = container.getBoundingClientRect();
                    const halfW = Math.max(1, (rect.width || 36) * 0.5);
                    const halfH = Math.max(1, (rect.height || 36) * 0.5);
                    const cx = rect.left + (rect.width || 36) * 0.5;
                    const cy = rect.top + (rect.height || 36) * 0.5;
                    const dx = Math.max(-1, Math.min(1, (e.clientX - cx) / halfW));
                    const dy = Math.max(-1, Math.min(1, (e.clientY - cy) / halfH));

                    emblem.style.setProperty('--oryx-ry', `${(dx * 14).toFixed(2)}deg`);
                    emblem.style.setProperty('--oryx-rx', `${(-dy * 12).toFixed(2)}deg`);
                    emblem.style.setProperty('--oryx-glint-x', `${Math.round(50 - dx * 34)}%`);
                    emblem.style.setProperty('--oryx-glint-y', `${Math.round(50 - dy * 34)}%`);
                }, { passive: true });

                targetHost.addEventListener('pointerleave', () => {
                    resetRestingPose();
                }, { passive: true });

                targetHost.addEventListener('pointercancel', () => {
                    resetRestingPose();
                }, { passive: true });
            } catch (err) {
                console.warn('[Oryx3D] Hover tilt init failed:', err);
            }
        }
    }
    if (typeof window !== 'undefined') {
        window.OryxDomUtils = OryxDomUtils;
    }
    if (typeof globalThis !== 'undefined') {
        globalThis.OryxDomUtils = OryxDomUtils;
    }
}

/**
 * Client-side continuation & constraint/phase reconciliation helpers (Phase 4).
 * Provides deterministic Turn 1 extraction and Guest Mode multi-turn rehydration
 * with zero backend state.
 */
const ClientContinuation = {
    /**
     * Extracts inclusion and exclusion constraints from prompt text.
     */
    extractConstraints(text, startCounter = 1) {
        if (!text || typeof text !== 'string') return [];
        const items = [];
        const seen = new Set();
        let idx = startCounter;

        const addItem = (kind, rawLabel) => {
            const cleaned = String(rawLabel || '')
                .replace(/^[,.\s:;]+|[,.\s:;]+$/g, '')
                .replace(/\s+/g, ' ')
                .trim();
            if (!cleaned || cleaned.length < 2 || cleaned.length > 48) return;
            const key = `${kind}:${cleaned.toLowerCase()}`;
            if (seen.has(key)) return;
            seen.add(key);
            items.push({ id: `C${idx++}`, kind, text: cleaned, status: 'active' });
        };

        // Exclusions: "no Redis", "without Redis", "avoid X", "drop X", "remove X"
        const exclRegex = /\b(?:no|without|avoid|exclude|drop|remove)\s+([A-Za-z0-9_+#.-]+(?:\s+[A-Za-z0-9_+#.-]+){0,2})/gi;
        let m;
        while ((m = exclRegex.exec(text)) !== null) {
            const candidate = m[1].split(/\b(?:and|or|because|since|so|for|with|phase)\b/i)[0].trim();
            if (candidate && !/^(speculation|hidden|issues|more|other|new)$/i.test(candidate)) {
                addItem('exclusion', candidate);
            }
            if (items.length >= 8) break;
        }

        // Inclusions: "use X", "using X", "in Python", "with FastAPI", "under $100/mo"
        const inclRegex = /\b(?:use|using|with|keep|require|requiring)\s+([A-Za-z0-9_+#.$/-]+(?:\s+[A-Za-z0-9_+#.$/-]+){0,2})/gi;
        while ((m = inclRegex.exec(text)) !== null) {
            const candidate = m[1].split(/\b(?:and|or|for|to|in|on|with|using|without|no|avoid|because|since|phase)\b/i)[0].trim();
            if (candidate && !/^(a|an|the|clear|this|that|our|my|your)$/i.test(candidate)) {
                addItem('inclusion', candidate);
            }
            if (items.length >= 10) break;
        }

        const budgetMatch = text.match(/(?:under|max|budget(?:\s+of)?)\s+(\$[0-9,.kKmM]+(?:\/mo|\/month|\/yr)?)/i);
        if (budgetMatch) {
            addItem('inclusion', `Budget <= ${budgetMatch[1]}`);
        }

        return items;
    },

    /**
     * Extracts execution phases from a structured response or rendered prompt text.
     */
    extractPhases(result, fallbackDomain = 'General') {
        const spPhases = result?.structured_prompt?.execution_phases;
        if (Array.isArray(spPhases) && spPhases.length > 0) {
            return spPhases.map((p, i) => ({
                phase_number: Number(p.phase_number || i + 1),
                name: String(p.name || `Phase ${i + 1}`),
                status: i === 0 ? 'active' : 'pending',
            }));
        }

        const text = String(result?.optimized_prompt || '');
        const sectionMatch = text.match(/# EXECUTION PHASES:\s*([\s\S]*?)(?=\n# [A-Z]|$)/i);
        if (sectionMatch) {
            const parsed = [];
            const lines = sectionMatch[1].split(/\r?\n/);
            for (const line of lines) {
                const lm = line.trim().match(/^(\d+)\.\s+(?:\*\*)?([^:\n*]+?)(?:\*\*)?(?::|$)/);
                if (lm) {
                    parsed.push({
                        phase_number: Number(lm[1]),
                        name: lm[2].trim(),
                        status: parsed.length === 0 ? 'active' : 'pending',
                    });
                }
            }
            if (parsed.length > 0) return parsed;
        }

        const execMatch = text.match(/\[EXECUTION\]\s*([^\n]+)/i);
        if (execMatch) {
            const parts = execMatch[1].split(/\s*·\s*/).filter(Boolean);
            const parsed = parts.map((part, idx) => {
                const cleaned = part.replace(/^\d+\.\s*/, '').trim();
                return {
                    phase_number: idx + 1,
                    name: cleaned.length > 36 ? cleaned.slice(0, 33) + '…' : cleaned,
                    status: idx === 0 ? 'active' : 'pending',
                };
            });
            if (parsed.length > 0) return parsed;
        }

        if (fallbackDomain === 'Software Engineering') {
            return [
                { phase_number: 1, name: 'Schema & Contracts', status: 'active' },
                { phase_number: 2, name: 'API & Logic', status: 'pending' },
                { phase_number: 3, name: 'Verify & Deploy', status: 'pending' },
            ];
        }
        return [];
    },

    /**
     * Reconciles active constraints across turns, applying manual revocations and new delta patches.
     */
    reconcileConstraints(existingConstraints, newText, revokedIds = []) {
        const revokedSet = new Set((revokedIds || []).map(v => String(v).toLowerCase()));
        const supersededLabels = [];
        const active = [];

        for (const c of (existingConstraints || [])) {
            const idKey = String(c.id || '').toLowerCase();
            const txtKey = String(c.text || '').toLowerCase();
            if (revokedSet.has(idKey) || revokedSet.has(txtKey)) {
                supersededLabels.push(c.text);
                continue;
            }
            active.push({ ...c, status: 'active' });
        }

        const nextCounter = active.reduce((mx, c) => {
            const n = parseInt(String(c.id || '').replace(/\D+/g, ''), 10);
            return Number.isFinite(n) && n >= mx ? n + 1 : mx;
        }, 1);

        const incoming = this.extractConstraints(newText, nextCounter);
        for (const inc of incoming) {
            const incTextLower = inc.text.toLowerCase();
            // Remove conflicting constraint of opposite kind with matching or prefix-overlapping label
            for (let i = active.length - 1; i >= 0; i--) {
                const curLower = active[i].text.toLowerCase();
                const overlaps = curLower === incTextLower ||
                    curLower.startsWith(incTextLower + ' ') ||
                    incTextLower.startsWith(curLower + ' ');
                if (overlaps && active[i].kind !== inc.kind) {
                    supersededLabels.push(active[i].text);
                    active.splice(i, 1);
                }
            }
            const alreadyExists = active.some(
                c => c.kind === inc.kind && c.text.toLowerCase() === incTextLower
            );
            if (!alreadyExists) {
                active.push(inc);
            }
        }

        return { activeConstraints: active, supersededLabels };
    },

    /**
     * Detects explicit phase step directives (e.g., "phase 2", "step 1") in follow-up text.
     */
    detectPhaseSelection(text, currentActivePhase, phases) {
        const m = String(text || '').match(/\b(?:phase|step)\s+(\d+)\b/i);
        if (m) {
            const num = Number(m[1]);
            if (!Array.isArray(phases) || phases.length === 0 || phases.some(p => Number(p.phase_number) === num)) {
                return num;
            }
        }
        return currentActivePhase ?? null;
    },

    /**
     * Removes superseded constraint phrases from prompt text and cleans up dangling
     * prepositions/punctuation, matching backend `scrub_superseded_terms` behavior.
     */
    scrubSupersededTerms(text, superseded = []) {
        const raw = String(text || '');
        if (!raw.trim() || !Array.isArray(superseded) || superseded.length === 0) {
            return raw;
        }
        const directivePrefixes = [
            'must include ',
            'include ',
            'require ',
            'using ',
            'use ',
            'with ',
            'avoid ',
            'exclude ',
            'without ',
            'no ',
        ];
        const targets = [];
        const seenLower = new Set();
        const pushTarget = (val) => {
            const trimmed = String(val || '').trim();
            if (!trimmed) return;
            const lower = trimmed.toLowerCase();
            if (!seenLower.has(lower)) {
                seenLower.add(lower);
                targets.push(trimmed);
            }
        };

        for (const rawTerm of superseded) {
            const trimmed = String(rawTerm || '').trim();
            if (!trimmed) continue;
            pushTarget(trimmed);
            const lower = trimmed.toLowerCase();
            for (const prefix of directivePrefixes) {
                if (lower.startsWith(prefix)) {
                    const core = trimmed.slice(prefix.length).trim();
                    if (core.length >= 3) pushTarget(core);
                }
            }
        }

        targets.sort((a, b) => b.length - a.length);
        let current = raw;
        for (const target of targets) {
            const escaped = target.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
            const re = new RegExp(`(^|[^A-Za-z0-9_])${escaped}(?=$|[^A-Za-z0-9_])`, 'gi');
            current = current.replace(re, '$1');
        }

        // Clean up dangling prepositions, conjunctions, and punctuation artifacts
        current = current.replace(/\s+/g, ' ').trim();
        const cleanupRules = [
            [/\b(using|with|via)\s*,\s*(?:and|or)\s+/gi, '$1 '],
            [/\b(using|with|via)\s+(?:and|or)\s+/gi, '$1 '],
            [/\s*,\s*(?:and|or)\s*([.!?;])/gi, '$1'],
            [/\s*\b(?:instead of|rather than|using|with|via)\s*([.!?;]|$)/gi, '$1'],
            [/\s*,\s*,+/g, ','],
            [/\s*,\s*([.!?;])/g, '$1'],
            [/\s+([.!?;,])/g, '$1'],
            [/\b(and|or)\s+\1\b/gi, '$1'],
            [/\s{2,}/g, ' '],
        ];
        for (const [pat, rep] of cleanupRules) {
            current = current.replace(pat, rep);
        }
        return current.trim().replace(/^[,;]+/, '').trim();
    },

    /**
     * Returns true when prompt text contains an explicit topic-reset cue matching backend RESET_CUES.
     */
    isTopicReset(text) {
        const lower = String(text || '').trim().toLowerCase();
        if (!lower) return false;
        const resetCues = [
            'new prompt:',
            'new topic:',
            'start over',
            'forget previous',
            'reset prompt',
            'scratch that',
            'ignore previous prompt',
            'disregard previous prompt',
        ];
        return resetCues.some(cue => lower.includes(cue));
    },

    /**
     * Merges parent prompt, reconciled constraints, active phase, resolved slots, and follow-up
     * into a clean rehydrated prompt string for stateless Guest Mode execution.
     */
    buildGuestRehydratedPrompt({
        parentRawPrompt = '',
        followUpText = '',
        isEditMode = false,
        activeConstraints = [],
        revokedLabels = [],
        phases = [],
        activePhase = null,
        resolvedSlots = {},
        repairContext = null,
    }) {
        const parts = [];
        const scrubbedParent = this.scrubSupersededTerms(parentRawPrompt.trim(), revokedLabels);
        const base = isEditMode
            ? this.scrubSupersededTerms(followUpText.trim(), revokedLabels)
            : scrubbedParent;
        if (base) parts.push(base);

        if (activeConstraints.length > 0) {
            const formatted = activeConstraints
                .map(c => (c.kind === 'exclusion' ? `no ${c.text}` : c.text))
                .join('; ');
            parts.push(`Active Constraints: ${formatted}.`);
        }
        if (revokedLabels.length > 0) {
            parts.push(`Disregard superseded/removed constraints: ${revokedLabels.join(', ')}.`);
        }
        if (activePhase !== null && activePhase !== undefined) {
            const phaseObj = (phases || []).find(p => Number(p.phase_number) === Number(activePhase));
            parts.push(`Focus on Phase ${activePhase}${phaseObj ? `: ${phaseObj.name}` : ''}.`);
        }
        const slotEntries = Object.entries(resolvedSlots || {}).filter(([, v]) => String(v || '').trim() !== '');
        if (slotEntries.length > 0) {
            parts.push(`Clarified Context: ${slotEntries.map(([k, v]) => `${k}=${v}`).join('; ')}.`);
        }
        if (repairContext) {
            const failed = (repairContext.failed_items || []).slice(0, 6).join('; ');
            const missing = (repairContext.missing_items || []).slice(0, 6).join('; ');
            parts.push(`Surgical Grounding Repair: fix unsupported claims [${failed || 'none'}] and include missing items [${missing || 'none'}] strictly using verified [F#] facts.`);
        } else if (!isEditMode && followUpText.trim()) {
            parts.push(`Follow-Up Update: ${followUpText.trim()}`);
        }
        return parts.join('\n\n');
    },

    /**
     * Renders a KV-cache-friendly Turn Delta prompt summarizing only changes on this turn.
     */
    buildDeltaPrompt({
        activeConstraints = [],
        revokedLabels = [],
        phases = [],
        activePhase = null,
        resolvedSlots = {},
        followUpText = '',
        repairContext = null,
    }) {
        const lines = [];
        if (activeConstraints.length > 0 || revokedLabels.length > 0) {
            lines.push('[CONSTRAINTS_PATCH]');
            if (activeConstraints.length > 0) {
                lines.push(`Active: ${activeConstraints.map(c => `${c.kind === 'exclusion' ? '✕' : '✓'} ${c.text}`).join(' · ')}`);
            }
            if (revokedLabels.length > 0) {
                lines.push(`Revoked: ${revokedLabels.join(', ')}`);
            }
        }
        if (activePhase !== null && activePhase !== undefined) {
            const phaseObj = (phases || []).find(p => Number(p.phase_number) === Number(activePhase));
            lines.push('[PHASE_CURSOR]');
            lines.push(`Phase ${activePhase}${phaseObj ? `: ${phaseObj.name}` : ''}`);
        }
        const slotEntries = Object.entries(resolvedSlots || {}).filter(([, v]) => String(v || '').trim() !== '');
        if (slotEntries.length > 0) {
            lines.push('[RESOLVED_SLOTS]');
            slotEntries.forEach(([k, v]) => lines.push(`- ${k}: ${v}`));
        }
        if (repairContext) {
            lines.push('[REPAIR_DIRECTIVE]');
            const failed = (repairContext.failed_items || []).slice(0, 6);
            const missing = (repairContext.missing_items || []).slice(0, 6);
            if (failed.length > 0) lines.push(`Fix unsupported: ${failed.join('; ')}`);
            if (missing.length > 0) lines.push(`Add missing: ${missing.join('; ')}`);
        }
        lines.push('[TASK]');
        lines.push(followUpText ? followUpText.trim() : 'Apply continuation updates above.');
        return lines.join('\n');
    },
};

/**
 * MockAPI provides deterministic, high-fidelity CRISP prompt compression
 * entirely in browser memory for zero-auth sandbox visitors without hitting the backend.
 */
const MockAPI = {
    /**
     * Executes mock prompt compression adhering to CRISP v3 specifications.
     * 
     * # Arguments
     * * `req.raw_text` - Raw input prompt string
     * * `req.task` - Optional task intent
     * * `req.use_case` - Detected or selected use case domain
     * * `req.mode` - 'gentle' | 'balanced' | 'aggressive'
     * * `req.domain` - Domain name for role/context specialization
     * 
     * # Returns
     * Resolves to a structured CRISP response with token counts, savings, TES score, and clean prompt.
     */
    compress({
        raw_text = '',
        task = 'Optimize prompt',
        use_case = 'generic',
        mode = 'balanced',
        domain = 'General',
        parent_id = null,
        revoked_constraints = [],
        active_phase = null,
        resolved_slots = null,
        repair_context = null,
    } = {}) {
        // ARCH: §5 NPAE Grounding — Calculate tokens using standard 1.33 words-to-tokens ratio
        const words = raw_text.trim().split(/\s+/).filter(Boolean);
        const inTokens = Math.round(words.length * 1.33);

        // Compression ratios per canonical mode: Gentle = 78%, Balanced = 58%, Aggressive = 42%
        const ratioMap = {
            aggressive: 0.42,
            balanced: 0.58,
            gentle: 0.78
        };
        const currentMode = (mode || 'balanced').toLowerCase();
        const ratio = ratioMap[currentMode] || 0.58;
        const rawOut = Math.round(inTokens * ratio);
        const outTokens = Math.min(inTokens, Math.max(inTokens > 0 ? 1 : 0, rawOut));
        const savedTokens = Math.max(0, inTokens - outTokens);
        const savedPct = inTokens > 0 ? Math.round((savedTokens / inTokens) * 100) : 0;

        const executionPlans = {
            'Real Estate & Finance': '1. Audit financial thresholds & debt ratios · 2. Model rate vs. down payment scenarios · 3. 3-phase execution roadmap',
            'Software Engineering': '1. Trace auth token lifecycle · 2. Inspect Safari SameSite/Partitioned cookie policy · 3. Patch 401 interception',
            'Legal & Compliance': '1. Audit indemnification & liability caps · 2. Flag auto-renewal notice windows · 3. Remediate IP ownership clauses',
            'Finance & Strategy': '1. Audit EBITDA benchmarks · 2. Model capital allocation scenarios · 3. Actionable governance roadmap',
            'General': '1. Audit core constraints · 2. Quantify risk vectors · 3. Actionable structured execution'
        };

        const activeDomain = domain || (use_case && use_case !== 'generic' ? use_case : 'General');
        const plan = executionPlans[activeDomain] || executionPlans['General'];
        const isResetCue = ClientContinuation.isTopicReset(raw_text);
        const effectiveRevoked = isResetCue ? [] : (revoked_constraints || []);
        const effectiveSlots = isResetCue ? null : resolved_slots;
        const effectiveActivePhase = isResetCue ? null : active_phase;

        const extracted = ClientContinuation.extractConstraints(raw_text);
        const { activeConstraints, supersededLabels } = ClientContinuation.reconcileConstraints(
            extracted,
            '',
            effectiveRevoked
        );
        const scrubbedRawText = ClientContinuation.scrubSupersededTerms(raw_text, supersededLabels);
        const trimmedSnippet = scrubbedRawText.length > 140 ? scrubbedRawText.slice(0, 137) + '...' : scrubbedRawText;
        const constraintLine = activeConstraints.length > 0
            ? activeConstraints.map(c => (c.kind === 'exclusion' ? `NO ${c.text}` : c.text)).join(' · ')
            : 'Zero speculation · Structured machine-actionable output';

        const slotEntries = Object.entries(effectiveSlots || {}).filter(([, v]) => String(v || '').trim() !== '');
        const slotBlock = slotEntries.length > 0
            ? `\n[RESOLVED_SLOTS] ${slotEntries.map(([k, v]) => `${k}: ${v}`).join(' · ')}`
            : '';

        const repairBlock = repair_context
            ? `\n[REPAIR_CONTEXT] Fix failed claims (${(repair_context.failed_items || []).join('; ') || 'none'}) & cover missing items (${(repair_context.missing_items || []).join('; ') || 'none'}) using verified [F#] facts only.`
            : '';

        const crispOutput =
`[ROLE] Principal ${activeDomain} Strategist
[CONTEXT] ${trimmedSnippet}${slotBlock}${repairBlock}
[EXECUTION] ${plan}
[CONSTRAINTS] Mode=${currentMode.toUpperCase()} · ${constraintLine}`;

        const phases = ClientContinuation.extractPhases({ optimized_prompt: crispOutput }, activeDomain);
        const resolvedPhase = effectiveActivePhase ?? (phases.length > 0 ? phases[0].phase_number : null);
        const hydratedPhases = phases.map(p => ({
            ...p,
            status: resolvedPhase !== null && p.phase_number === Number(resolvedPhase) ? 'active' : p.status,
        }));

        const continuation = (parent_id && !isResetCue)
            ? {
                ledger_id: parent_id,
                turn_number: 2,
                path: 'delta_follow_up',
                primary_kind: repair_context ? 'constraint_patch' : 'phase_step',
                delta_summary: repair_context ? 'Surgical grounding repair' : 'Applied client continuation delta',
                delta_prompt: ClientContinuation.buildDeltaPrompt({
                    activeConstraints,
                    revokedLabels: supersededLabels,
                    phases: hydratedPhases,
                    activePhase: resolvedPhase,
                    resolvedSlots: effectiveSlots || {},
                    followUpText: raw_text,
                    repairContext: repair_context,
                }),
                rehydrated_prompt: crispOutput,
                active_constraints: activeConstraints,
                superseded_constraints: supersededLabels,
                phases: hydratedPhases,
                active_phase: resolvedPhase,
                open_slots: [],
            }
            : {
                ledger_id: 'mock-ledger-' + Date.now(),
                turn_number: 1,
                path: isResetCue ? 'topic_reset' : 'initial',
                primary_kind: isResetCue ? 'topic_reset' : 'initial',
                delta_summary: isResetCue ? 'Started new topic branch (reset cue)' : 'Initialized parent prompt state',
                active_constraints: activeConstraints,
                superseded_constraints: [],
                phases: hydratedPhases,
                active_phase: resolvedPhase,
                open_slots: [],
            };

        return Promise.resolve({
            id: 'mock-' + Date.now(),
            optimized_prompt: crispOutput,
            token_original: inTokens,
            token_final: outTokens,
            token_saved: savedTokens,
            savings_pct: savedPct,
            tes_score: 9.7,
            domain: activeDomain,
            pruned_summary: `Removed filler: conversational hedging, redundant phrasing (${savedTokens} tokens)`,
            warnings: [],
            engine_version: '3.5.0-sandbox',
            continuation,
        });
    }
};

/**
 * Primary API communication gateway.
 * Intercepts unauthenticated sandbox compression to preserve zero-auth workflow.
 */
const API = {
    async call(path, options = {}) {
        const headers = {
            'Content-Type': 'application/json',
            ...options.headers
        };

        const r = await fetch(path, { ...options, headers, credentials: 'same-origin' });
        
        if (r.status === 401) {
            console.warn('Endpoint 401 unauthorized:', path);
            if (typeof onAuthExpired === 'function') {
                onAuthExpired();
            } else if (typeof logout === 'function') {
                logout();
            }
            throw new Error('Session expired or authentication required. Please sign in.');
        }

        if (!r.ok) {
            const err = await r.json().catch(() => ({}));
            throw new Error(err.error?.message || err.error || 'Request failed');
        }

        return r.status === 204 ? null : r.json();
    },

    /**
     * Dispatches compression. Routes unauthenticated guests to the stateless live demo endpoint
     * (`/api/demo/compress`), falling back to `MockAPI` only when offline.
     */
    async compress(req) {
        if (typeof State === 'undefined' || !State.token) {
            try {
                const r = await fetch('/api/demo/compress', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(req),
                });
                if (!r.ok) {
                    const err = await r.json().catch(() => ({}));
                    throw new Error(err.error?.message || err.error || 'Demo compression failed');
                }
                return await r.json();
            } catch (err) {
                if (err instanceof TypeError) {
                    return MockAPI.compress(req);
                }
                throw err;
            }
        }

        return await this.call('/api/compress', {
            method: 'POST',
            body: JSON.stringify(req),
        });
    },

    async feedback(req) {
        return this.call('/api/feedback', {
            method: 'POST',
            body: JSON.stringify(req),
        });
    },

    async history() {
        return this.call('/api/history');
    },

    async stats() {
        return this.call('/api/stats');
    },

    async devConfig() {
        return this.call('/api/dev/config');
    },

    async scenarioRegistry() {
        return this.call('/api/scenario/registry');
    },

    async scenarioAsk(req) {
        return this.call('/api/scenario/ask', {
            method: 'POST',
            body: JSON.stringify(req),
        });
    },

    async ragUpload(file) {
        const formData = new FormData();
        formData.append('file', file);
        const r = await fetch('/api/rag/upload', { method: 'POST', credentials: 'same-origin', body: formData });
        if (!r.ok) {
            const err = await r.json().catch(() => ({}));
            throw new Error(err.error?.message || err.error || 'Upload failed');
        }
        return r.json();
    },

    async ragList() {
        return this.call('/api/rag/documents');
    },

    async ragDelete(id) {
        return this.call(`/api/rag/documents/${encodeURIComponent(id)}`, { method: 'DELETE' });
    },

    async localRagHealth(timeoutMs = 2000) {
        const ctrl = typeof AbortController !== 'undefined' ? new AbortController() : null;
        const timer = ctrl ? setTimeout(() => ctrl.abort(), timeoutMs) : null;
        try {
            const r = await fetch('http://127.0.0.1:49152/health', {
                method: 'GET',
                signal: ctrl ? ctrl.signal : undefined,
            });
            if (!r.ok) throw new Error(`Local companion health HTTP ${r.status}`);
            return await r.json();
        } finally {
            if (timer) clearTimeout(timer);
        }
    },

    async localRagRetrieve({ query, top_k = 5, file_extensions = ['.pdf', '.docx'] }) {
        // ARCH: §2.1 — Strict JSON IPC contract with local offline companion (127.0.0.1:49152).
        const r = await fetch('http://127.0.0.1:49152/retrieve', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                query: String(query || ''),
                top_k: Number(top_k) || 5,
                file_extensions: Array.isArray(file_extensions) ? file_extensions : ['.pdf', '.docx'],
            }),
        });
        if (!r.ok) {
            const err = await r.json().catch(() => ({}));
            throw new Error(err.error || `Local retrieval failed (HTTP ${r.status})`);
        }
        const payload = await r.json();
        if (!payload || payload.status !== 'ok' || !Array.isArray(payload.chunks)) {
            throw new Error('Local companion returned an invalid response schema.');
        }
        return payload;
    },

    async verifyGrounding(req) {
        return this.call('/api/npae/verify', {
            method: 'POST',
            body: JSON.stringify(req),
        });
    }
};

if (typeof window !== 'undefined') {
    window.ClientContinuation = ClientContinuation;
    window.MockAPI = MockAPI;
    window.API = API;
}
if (typeof globalThis !== 'undefined') {
    globalThis.ClientContinuation = ClientContinuation;
    globalThis.MockAPI = MockAPI;
    globalThis.API = API;
}

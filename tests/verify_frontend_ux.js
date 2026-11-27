/**
 * tests/verify_frontend_ux.js
 * Automated JSDOM Verification Suite for Oryx Intelligence Engine Frontend Overhaul.
 * 
 * Verifies Milestone 1 (Requirements R1 through R5):
 *   R1: Zero-Auth Sandbox & Sample Presets (Frontend Only)
 *   R2: Side-by-Side Diff Workbench
 *   R3: Dollar Cost Savings & TES Fidelity Metrics
 *   R4: Multi-Format Drag-and-Drop Ingestion
 *   R5: Non-Blocking Progress & Streamlined Controls
 * 
 * Execution:
 *   NODE_PATH=./scratch/jsd/node_modules node tests/verify_frontend_ux.js
 */

const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');

let passed = 0;
let failed = 0;
const failures = [];

function assert(condition, message) {
    if (condition) {
        console.log(`  ✅ PASS: ${message}`);
        passed++;
    } else {
        console.error(`  ❌ FAIL: ${message}`);
        failed++;
        failures.push(message);
    }
}

async function runTestSuite() {
    console.log('\n======================================================');
    console.log(' Oryx Intelligence Engine — Frontend UX Test Suite');
    console.log(' Requirements Verification: R1 through R5');
    console.log('======================================================\n');

    const htmlPath = path.resolve(__dirname, '../static/index.html');
    const appJsPath = path.resolve(__dirname, '../static/app.js');
    const oryxDomUtilsPath = path.resolve(__dirname, '../static/oryx_dom_utils.js');
    const mockJsPath = path.resolve(__dirname, '../static/mock_api.js');
    const optJsPath = path.resolve(__dirname, '../static/prompt_optimizer.js');
    const devJsPath = path.resolve(__dirname, '../static/dev_engine.js');

    const html = fs.readFileSync(htmlPath, 'utf8');
    const appJs = fs.readFileSync(appJsPath, 'utf8');
    const oryxDomUtilsJs = fs.readFileSync(oryxDomUtilsPath, 'utf8');
    const mockJs = fs.readFileSync(mockJsPath, 'utf8');
    const optJs = fs.readFileSync(optJsPath, 'utf8');
    const devJs = fs.readFileSync(devJsPath, 'utf8');

    // =========================================================================
    // Suite 1: Static HTML & CSS Structural Checks (R1, R2, R3, R4, R5)
    // =========================================================================
    console.log('--- Suite 1: Layout & Design Structure ---');

    // 1.1 Responsive @media queries
    const mediaMatches = html.match(/@media\s*\([^{]+\)\s*\{/g) || [];
    assert(mediaMatches.length >= 2, `R5: Responsive @media breakpoints defined (found ${mediaMatches.length} queries)`);
    assert(html.includes('max-width: 900px') && html.includes('max-width: 640px'), 'R5: Breakpoints for desktop (900px) and mobile (640px) present');

    // 1.2 Glass card desktop width
    assert(html.includes('max-width: 1160px'), 'R2: .glass-card widened to 1160px for desktop split workbench');

    // 1.3 Instantiate DOM without external network resource loading
    const dom = new JSDOM(html, {
        runScripts: 'dangerously',
        url: 'http://localhost:8081/'
    });
    const doc = dom.window.document;
    // Offline JSDOM fetch mock: guest /api/demo/compress hits TypeError fallback to MockAPI.compress;
    // signed-in /api/compress returns MockAPI.compress response (or simulated HTTP status) without network calls.
    let mockCompressStatus = 200;
    dom.window.fetch = async (url, opts = {}) => {
        if (url === '/api/compress' && opts.body && dom.window.MockAPI) {
            if (mockCompressStatus !== 200) {
                return {
                    ok: false,
                    status: mockCompressStatus,
                    json: async () => ({ error: { message: `HTTP ${mockCompressStatus} failure` } })
                };
            }
            const payload = JSON.parse(opts.body);
            return {
                ok: true,
                status: 200,
                json: async () => dom.window.MockAPI.compress(payload)
            };
        }
        throw new dom.window.TypeError('Offline JSDOM');
    };

    // 1.4 R1: Zero-Auth Sandbox elements
    const mainSec = doc.getElementById('main-section');
    assert(mainSec && mainSec.style.display !== 'none', 'R1: #main-section is rendered and unblocked by default');

    const authSec = doc.getElementById('auth-section');
    assert(authSec && authSec.style.display === 'none', 'R1: #auth-section does not block initial load');

    const presets = doc.querySelectorAll('.preset-chip, [data-preset]');
    assert(presets.length >= 3, `R1: At least 3 sample preset buttons exist (found ${presets.length})`);

    const presetHouse = doc.getElementById('preset-house') || doc.querySelector('[data-preset="house"]');
    const presetBug = doc.getElementById('preset-bug') || doc.querySelector('[data-preset="bug"]');
    const presetLegal = doc.getElementById('preset-legal') || doc.querySelector('[data-preset="legal"]');
    assert(!!presetHouse && !!presetBug && !!presetLegal, 'R1: All 3 presets (House, Bug, Legal) exist in DOM');

    // 1.5 R2: Split Workbench & Diff Toggle
    const splitWorkbench = doc.querySelector('.workbench-split, [data-workbench-split]');
    assert(!!splitWorkbench, 'R2: 2-column split workbench container exists');

    const inputPane = doc.getElementById('input-pane') || doc.querySelector('.pane-left');
    const outputPane = doc.getElementById('output-pane') || doc.querySelector('.pane-right');
    assert(!!inputPane && !!outputPane, 'R2: Left (Input) and Right (CRISP Output) panes exist');

    const diffToggle = doc.getElementById('diff-toggle');
    assert(!!diffToggle && diffToggle.type === 'checkbox', 'R2: Pruning diff toggle checkbox exists');

    // 1.6 R3: Dollar Savings & TES Metrics
    const modelSelect = doc.getElementById('model-select');
    assert(!!modelSelect, 'R3: Model pricing target selector exists');
    if (modelSelect) {
        const options = Array.from(modelSelect.options).map(o => o.text);
        assert(options.some(t => t.includes('GPT-4o')), 'R3: Model selector contains GPT-4o');
        assert(options.some(t => t.includes('Claude 3.5')), 'R3: Model selector contains Claude 3.5 Sonnet');
        assert(options.some(t => t.includes('Gemini 2.5')), 'R3: Model selector contains Gemini 2.5 Pro');
    }

    const costSaved = doc.getElementById('cost-saved');
    assert(!!costSaved, 'R3: Dollar cost savings display element (#cost-saved) exists');

    const tesBadge = doc.getElementById('tes-score') || doc.querySelector('.tes-badge');
    assert(!!tesBadge, 'R3: TES fidelity score badge element exists');

    // 1.7 R4: Drag-and-Drop Ingestion
    const dropzone = doc.getElementById('dropzone') || doc.querySelector('[data-dropzone]');
    assert(!!dropzone, 'R4: Drag-and-drop dropzone element exists');

    const fileInput = doc.getElementById('file-input');
    assert(!!fileInput && fileInput.type === 'file', 'R4: File input exists');
    assert(fileInput.accept.includes('.pdf') && fileInput.accept.includes('.txt') && fileInput.accept.includes('.md'), 'R4: File input accepts .pdf, .txt, .md');

    const tokenCounter = doc.getElementById('input-token-counter');
    assert(!!tokenCounter, 'R4: Input token counter pill exists for instant estimation');

    // 1.8 R5: Non-Blocking Controls & Streamlined UI
    const overlay = doc.getElementById('neuro-progress-overlay');
    assert(!overlay || overlay.style.display.includes('none'), 'R5: Blocking full-screen overlay is suppressed on load');

    const inlineProgress = doc.getElementById('inline-progress');
    assert(!!inlineProgress, 'R5: Non-blocking inline skeleton progress element exists');

    const modePills = doc.querySelectorAll('.mode-pill, [data-mode]');
    assert(modePills.length >= 3, `R5: Segmented mode selection pills exist (found ${modePills.length})`);

    // Option 3: Outcome Presets & Collapsed Disclosures
    const outcomePills = doc.getElementById('outcome-pills');
    const poFact = doc.getElementById('po-factchecked');
    const poSaver = doc.getElementById('po-saver');
    const poBlueprint = doc.getElementById('po-blueprint');
    assert(!!outcomePills && !!poFact && !!poSaver && !!poBlueprint, 'Option 3: Primary Outcome Presets (Fact-Checked, Token Saver, Full Blueprint) exist');
    assert(poSaver.classList.contains('active') && poSaver.getAttribute('aria-checked') === 'true', 'Option 3: Token Saver preset is active by default');

    const advDisclosure = doc.getElementById('advanced-options-disclosure');
    assert(
        advDisclosure && advDisclosure.tagName === 'DETAILS' && !advDisclosure.open &&
        !!advDisclosure.querySelector('#mode-pills') &&
        !!advDisclosure.querySelector('#pm-gentle') &&
        !!advDisclosure.querySelector('#pm-balanced') &&
        !!advDisclosure.querySelector('#pm-aggressive') &&
        !!advDisclosure.querySelector('#opt-rag-enabled') &&
        !!advDisclosure.querySelector('#opt-harness-enabled'),
        'Option 3: #advanced-options-disclosure is collapsed <details> containing mode pills and source toggles'
    );

    const inspectorDrawerInit = doc.getElementById('inspector-drawer');
    assert(
        inspectorDrawerInit && inspectorDrawerInit.tagName === 'DETAILS' && !inspectorDrawerInit.open &&
        !!inspectorDrawerInit.querySelector('#quality-breakdown') &&
        !!inspectorDrawerInit.querySelector('#cost-saved') &&
        !!inspectorDrawerInit.querySelector('#model-select') &&
        !!inspectorDrawerInit.querySelector('#export-audit-btn') &&
        !!inspectorDrawerInit.querySelector('#export-svg-btn'),
        'Option 3: #inspector-drawer is collapsed <details> containing quality breakdown, cost telemetry, and export buttons'
    );

    const domainChip = doc.getElementById('domain-chip') || doc.getElementById('domain-name');
    assert(!!domainChip, 'R5: Auto-detected domain chip element exists');

    const skinInNav = doc.querySelector('.nav-bar #btn-custom-skin');
    const skinInHeader = doc.querySelector('.tc-header #btn-custom-skin');
    assert(!skinInNav, 'R5: Skin button moved out of primary .nav-bar');
    assert(!!skinInHeader, 'R5: Skin button placed in .tc-header retaining id="btn-custom-skin"');

    // =========================================================================
    // Suite 2: Functional Logic & Behavioral Execution (R1 through R5)
    // =========================================================================
    console.log('\n--- Suite 2: Functional Logic & Behavioral Execution ---');

    // Inject scripts into window context via script elements
    function injectScript(content) {
        const scriptEl = doc.createElement('script');
        scriptEl.textContent = content;
        doc.body.appendChild(scriptEl);
    }

    injectScript(oryxDomUtilsJs);
    injectScript(mockJs);
    injectScript(optJs);
    injectScript(devJs);
    injectScript(appJs);

    const win = dom.window;

    // 2.1 R1: Preset loading and zero-auth execution
    console.log('Testing R1: Sample Presets...');
    win.loadPreset('house');
    const promptInput = doc.getElementById('prompt-input');
    assert(promptInput.value.includes('buy our very first house'), 'R1: "Buying a House" preset populated prompt textarea');
    assert(doc.getElementById('domain-name').textContent.includes('Real Estate'), 'R1: House preset set domain to Real Estate & Finance');

    // Wait microtask for mock compression execution
    await new Promise(r => setTimeout(r, 200));
    const outputBox = doc.getElementById('result-output');
    assert(outputBox.textContent.includes('ROLE') || outputBox.innerHTML.includes('crisp-tag'), 'R1: Auto-preview compression executed and populated output');
    assert(win.State.lastResult !== null, 'R1: State.lastResult is populated without authentication');

    // Test Bug Report Preset
    win.loadPreset('bug');
    assert(promptInput.value.includes('Safari after clearing cookies'), 'R1: "Bug Report" preset populated prompt textarea');
    assert(doc.getElementById('domain-name').textContent.includes('Software'), 'R1: Bug preset set domain to Software Engineering');

    // Test Legal Preset
    win.loadPreset('legal');
    assert(promptInput.value.includes('vendor SaaS agreement'), 'R1: "Contract Review" preset populated prompt textarea');
    assert(doc.getElementById('domain-name').textContent.includes('Legal'), 'R1: Legal preset set domain to Legal & Compliance');

    // 2.2 R1: Guest Tab & Feature Gating (Optimize Prompt Only)
    console.log('Testing R1: Guest Tab & Feature Gating...');
    assert(doc.getElementById('nav-compress').style.display !== 'none' && !doc.getElementById('nav-compress').disabled, 'R1: Optimize Prompt tab is visible and enabled for Guests');
    ['nav-ask', 'nav-history', 'nav-stats'].forEach(id => {
        const el = doc.getElementById(id);
        assert(el && el.style.display === 'none' && el.disabled === true, `R1: ${id} tab is hidden and disabled for Guests`);
    });
    assert(doc.getElementById('opt-rag-enabled').disabled === true && doc.getElementById('opt-harness-enabled').disabled === false, 'R1: Cloud RAG checkbox disabled for Guests while Grounding Harness remains enabled');
    assert(doc.getElementById('btn-feedback-up').disabled === true && doc.getElementById('btn-feedback-down').disabled === true, 'R1: Feedback buttons are disabled for Guests');

    win.switchTab('ask');
    assert(doc.getElementById('tab-compress').classList.contains('active') && !doc.getElementById('tab-ask-content').classList.contains('active'), 'R1: Guest cannot switch to Ask tab');
    win.switchTab('history');
    assert(doc.getElementById('tab-compress').classList.contains('active') && !doc.getElementById('tab-history-content').classList.contains('active'), 'R1: Guest cannot switch to History tab');
    win.switchTab('stats');
    assert(doc.getElementById('tab-compress').classList.contains('active') && !doc.getElementById('tab-stats-content').classList.contains('active'), 'R1: Guest cannot switch to Stats tab');
    win.closeAuthSection();

    win.switchTab('compress');
    assert(doc.getElementById('tab-compress').classList.contains('active'), 'R1: Compress tab remains open and unblocked');

    // 2.3 R2: Diff view toggle
    console.log('Testing R2: Diff View Toggle...');
    await win.compress(true);
    await new Promise(r => setTimeout(r, 200));

    // When diff is toggled ON
    diffToggle.checked = true;
    win.toggleDiffView();
    assert(outputBox.innerHTML.includes('diff-removed'), 'R2: Diff toggle ON renders .diff-removed strike-through text');

    // When diff is toggled OFF
    diffToggle.checked = false;
    win.toggleDiffView();
    assert(!outputBox.innerHTML.includes('diff-removed'), 'R2: Diff toggle OFF strips .diff-removed formatting');

    // 2.4 R3: Dollar savings calculation across models
    console.log('Testing R3: Dollar Savings Calculation...');
    win.State.lastResult = {
        token_original: 500,
        token_final: 250,
        token_saved: 250,
        savings_pct: 50,
        tes_score: 0.98,
        optimized_prompt: '[ROLE] Principal Strategist\n[EXECUTION] Distilled plan'
    };

    // GPT-4o @ $0.005 / 1k tokens: 250 saved * 0.005 = $1.25 saved / 1k runs
    modelSelect.value = '0.005';
    win.updateModelPricing();
    assert(costSaved.textContent.includes('$1.25 saved / 1k runs'), `R3 Math: GPT-4o calculated $1.25 saved (got ${costSaved.textContent})`);

    // Claude 3.5 Sonnet @ $0.003 / 1k tokens: 250 saved * 0.003 = $0.75 saved / 1k runs
    modelSelect.value = '0.003';
    win.updateModelPricing();
    assert(costSaved.textContent.includes('$0.75 saved / 1k runs'), `R3 Math: Claude 3.5 Sonnet calculated $0.75 saved (got ${costSaved.textContent})`);

    // Gemini 2.5 Pro @ $0.00125 / 1k tokens: 250 saved * 0.00125 = $0.31 saved / 1k runs
    modelSelect.value = '0.00125';
    win.updateModelPricing();
    assert(costSaved.textContent.includes('$0.31 saved / 1k runs'), `R3 Math: Gemini 2.5 Pro calculated $0.31 saved (got ${costSaved.textContent})`);

    // TES fidelity score check (calm badge on header + detailed TES/SFS/SCS inside Inspector)
    win.renderResult({
        ...win.State.lastResult,
        dual_score: { tes: 0.98, sfs: 0.96, scs: 0.99 }
    });
    assert(tesBadge.textContent.includes('✓ Ready · 50% shorter'), `R3 TES: Calm header summary rendered (got "${tesBadge.textContent}")`);
    const qBreakdownText = doc.getElementById('quality-breakdown').textContent;
    assert(
        qBreakdownText.includes('0.98') && qBreakdownText.includes('0.96') && qBreakdownText.includes('0.99'),
        `R3 Inspector: Quality breakdown populated TES/SFS/SCS inside #inspector-drawer (got "${qBreakdownText.trim()}")`
    );
    assert(doc.getElementById('output-summary-pill').textContent.includes('250 tokens'), 'Option 3: #output-summary-pill shows compressed token count');

    // 2.5 R4: Multi-Format Drag & Drop and Instant Token Estimation
    console.log('Testing R4: File Ingestion & Token Estimation...');
    promptInput.value = 'Word '.repeat(60).trim();
    win.validatePromptLength();
    assert(tokenCounter.textContent.includes('80 tokens'), `R4 Token Calc: 60 words * 1.33 = ~80 tokens (got ${tokenCounter.textContent})`);

    // Test text file ingestion
    const fakeTxtFile = {
        name: 'test_document.txt',
        size: 2048,
        arrayBuffer: async () => Buffer.from('Here is an ingested test document with ample content for semantic compression.'),
    };
    win.FileReader = class MockFileReader {
        readAsText(file) {
            setTimeout(() => {
                this.onload({ target: { result: 'Extracted text content from dropped file for prompt optimization.' } });
            }, 10);
        }
    };
    await win.processIngestedFile(fakeTxtFile);
    await new Promise(r => setTimeout(r, 50));
    assert(promptInput.value.includes('Extracted text content'), 'R4: Ingested .txt file populated prompt input');

    // Test vanilla PDF extraction function
    const samplePdfRaw = '%PDF-1.4\nstream\n(This is a test invoice agreement for software services) Tj\nendstream\n%%EOF';
    const fakePdfFile = {
        name: 'agreement.pdf',
        size: 4096,
        arrayBuffer: async () => Buffer.from(samplePdfRaw, 'binary')
    };
    const extractedPdfText = await win.extractVanillaPdfText(fakePdfFile);
    assert(extractedPdfText.includes('test invoice agreement'), `R4 PDF: Extracted text from raw PDF stream (got: "${extractedPdfText}")`);

    // 2.6 R5: Guest Mode Sandbox (all 3 modes + outcome presets up to 3 runs) & Authenticated Access
    console.log('Testing R5: Mode Pills, Outcome Presets & Domain Detection...');
    win.updateUserBar();
    assert(
        doc.getElementById('pm-gentle').disabled === false &&
        doc.getElementById('pm-balanced').disabled === false &&
        doc.getElementById('pm-aggressive').disabled === false &&
        doc.getElementById('pm-balanced').classList.contains('active'),
        'Guest Mode: All 3 modes available in sandbox with Smart Trim active by default'
    );

    // Outcome Preset mapping checks
    win.selectOutcomePreset('factchecked');
    assert(
        win.State.currentMode === 'aggressive' &&
        doc.getElementById('opt-harness-enabled').checked === true &&
        doc.getElementById('po-factchecked').classList.contains('active') &&
        doc.getElementById('po-factchecked').getAttribute('aria-checked') === 'true' &&
        doc.getElementById('po-saver').getAttribute('aria-checked') === 'false',
        'Option 3: Fact-Checked preset maps to aggressive + harness and syncs aria-checked'
    );

    win.selectOutcomePreset('blueprint');
    assert(
        win.State.currentMode === 'aggressive' &&
        doc.getElementById('opt-harness-enabled').checked === false &&
        doc.getElementById('po-blueprint').classList.contains('active') &&
        doc.getElementById('po-blueprint').getAttribute('aria-checked') === 'true',
        'Option 3: Full Blueprint preset maps to aggressive + crisp and syncs aria-checked'
    );

    win.selectOutcomePreset('saver');
    assert(
        win.State.currentMode === 'balanced' &&
        doc.getElementById('opt-harness-enabled').checked === false &&
        doc.getElementById('po-saver').classList.contains('active') &&
        doc.getElementById('po-saver').getAttribute('aria-checked') === 'true',
        'Option 3: Token Saver preset maps to balanced + crisp and syncs aria-checked'
    );

    win.selectMode('gentle');
    assert(
        win.State.currentMode === 'gentle' &&
        !doc.getElementById('po-factchecked').classList.contains('active') &&
        !doc.getElementById('po-saver').classList.contains('active') &&
        !doc.getElementById('po-blueprint').classList.contains('active'),
        'Option 3: Fine-tune Light Polish clears outcome preset active highlight'
    );

    // Verify 4th guest compression is blocked after 3 completed runs
    promptInput.value = 'one two three four five six seven eight nine ten';
    win.validatePromptLength();
    while (win.State.guestUses < 3) {
        await win.compress(true);
    }
    const prevResult = win.State.lastResult;
    await win.compress(false);
    assert(win.State.lastResult === prevResult && doc.getElementById('auth-section').style.display === 'block', 'Guest Mode: 4th compression blocked after 3 runs and opens sign-in');
    win.closeAuthSection();

    // Sign in to unlock all modes and unlimited compressions
    win.State.token = 'test-token';
    win.State.username = 'tester';
    win.updateUserBar();
    assert(doc.getElementById('sandbox-badge').textContent.includes('Live Engine'), 'Auth: Sandbox badge updates to Live Engine on sign-in');
    assert(doc.getElementById('pm-balanced').disabled === false && doc.getElementById('pm-aggressive').disabled === false, 'Auth: Smart Trim and Deep Blueprint pills unlocked on sign-in');
    assert(['nav-ask', 'nav-history', 'nav-stats'].every(id => doc.getElementById(id).style.display !== 'none' && doc.getElementById(id).disabled === false), 'Auth: Ask, History, and Insights tabs unlocked on sign-in');
    assert(doc.getElementById('opt-rag-enabled').disabled === false && doc.getElementById('opt-harness-enabled').disabled === false, 'Auth: RAG and Harness checkboxes unlocked on sign-in');
    assert(doc.getElementById('btn-feedback-up').disabled === false && doc.getElementById('btn-feedback-down').disabled === false, 'Auth: Feedback buttons unlocked on sign-in');

    win.selectMode('aggressive');
    assert(doc.getElementById('pm-aggressive').classList.contains('active'), 'R5: Aggressive mode pill marked active');
    assert(doc.getElementById('mode').value === 'aggressive', 'R5: Hidden #mode input updated to aggressive');

    win.selectMode('gentle');
    assert(doc.getElementById('pm-gentle').classList.contains('active'), 'R5: Gentle mode pill marked active');
    assert(doc.getElementById('mode').value === 'gentle', 'R5: Hidden #mode input updated to gentle');

    // AUD-02: Signed-in API.compress must reject on HTTP 500 instead of falling back to MockAPI.compress
    mockCompressStatus = 500;
    let apiRejectedOn500 = false;
    try {
        await win.API.compress({
            raw_text: 'Verify signed-in HTTP 500 error propagation without mock fallback.',
            mode: 'balanced',
            domain: 'General'
        });
    } catch (err) {
        apiRejectedOn500 = err && err.message.includes('500');
    } finally {
        mockCompressStatus = 200;
    }
    assert(apiRejectedOn500, 'AUD-02: Signed-in API.compress rejects on HTTP 500 rather than falling back to MockAPI');

    // Test domain keyword heuristic
    assert(win.detectDomain('mortgage interest rate and down payment') === 'Real Estate & Finance', 'R5: Domain detected Real Estate & Finance');
    assert(win.detectDomain('safari console bug 401 error crash') === 'Software Engineering', 'R5: Domain detected Software Engineering');
    assert(win.detectDomain('indemnification clause contract liability') === 'Legal & Compliance', 'R5: Domain detected Legal & Compliance');
    assert(win.detectDomain('ebitda revenue investor runway') === 'Finance & Strategy', 'R5: Domain detected Finance & Strategy');
    assert(win.detectDomain('how do I make a cake') === 'General', 'R5: Default domain is General');

    // 2.7 Tier 2: Boundary & Corner Cases
    console.log('\n--- Suite 3: Boundary & Corner Cases ---');
    // Start fresh Turn-1 prompt session before testing initial < 8 words boundary
    win.resetParentSession(true);
    promptInput.value = 'Too short';
    win.validatePromptLength();
    const compressBtn = doc.getElementById('compress-btn');
    assert(compressBtn.disabled === true, 'Boundary: Prompt < 8 words disables #compress-btn');
    assert(doc.getElementById('prompt-error').style.display !== 'none', 'Boundary: Prompt < 8 words shows error notice');

    // Unsupported file format rejection
    const fakeDocxFile = {
        name: 'report.docx',
        size: 10240,
    };
    await win.processIngestedFile(fakeDocxFile);
    const toast = doc.querySelector('.toast');
    assert(toast && toast.textContent.includes('Unsupported format'), 'Boundary: Unsupported file format (.docx) triggers error toast');

    // Clear input
    win.clearInput();
    assert(promptInput.value === '', 'Boundary: clearInput() resets textarea');
    assert(doc.getElementById('output-token-count').textContent.includes('0 tokens'), 'Boundary: clearInput() resets output tokens');
    assert(doc.getElementById('cost-saved').textContent.includes('$0.00'), 'Boundary: clearInput() resets dollar savings to $0.00');

    // 3.4 Minimum 8-Word Prompt Boundary (Defect 1 remediation)
    promptInput.value = 'one two three four five six seven eight';
    win.validatePromptLength();
    await win.compress(true);
    assert(win.State.lastResult && win.State.lastResult.token_saved >= 0, `Boundary 8-word: token_saved >= 0 (got ${win.State.lastResult.token_saved})`);
    assert(win.State.lastResult && win.State.lastResult.savings_pct >= 0, `Boundary 8-word: savings_pct >= 0 (got ${win.State.lastResult.savings_pct}%)`);
    const boundaryCostText = doc.getElementById('cost-saved').textContent;
    assert(!boundaryCostText.includes('$-') && (boundaryCostText.includes('$0.') || !boundaryCostText.includes('-')), `Boundary 8-word: cost display is non-negative (got "${boundaryCostText}")`);

    // 3.5 Async Race Condition on Mode Switching (Defect 2 remediation)
    promptInput.value = 'We need an in-depth audit of the vendor SaaS indemnification terms.';
    win.validatePromptLength();
    win.selectMode('balanced');
    const slowModePromise = win.compress(false); // 250ms delay
    await new Promise(r => setTimeout(r, 50));
    win.selectMode('aggressive'); // 80ms delay silent compress
    await slowModePromise;
    await new Promise(r => setTimeout(r, 150));
    assert(win.State.currentMode === 'aggressive', 'Race Condition: State currentMode remains aggressive');
    assert(win.State.lastResult.optimized_prompt.includes('Mode=AGGRESSIVE'), 'Race Condition: Aggressive constraints rendered without being clobbered by slower Balanced response');

    // 3.6 In-Flight Compression Invalidation on clearInput() (Defect 3 remediation)
    promptInput.value = 'We need an in-depth audit of the vendor SaaS indemnification terms.';
    win.validatePromptLength();
    const slowInFlightPromise = win.compress(false); // 250ms delay
    await new Promise(r => setTimeout(r, 50));
    win.clearInput();
    await slowInFlightPromise;
    await new Promise(r => setTimeout(r, 300));
    assert(win.State.lastResult === null, 'In-Flight Cancel: State.lastResult remains null after in-flight compress resolves');
    assert(outputBox.innerHTML.includes('text-placeholder'), 'In-Flight Cancel: Output pane preserves placeholder without ghost population');
    assert(doc.getElementById('cost-saved').textContent.includes('$0.00'), 'In-Flight Cancel: Cost display remains $0.00');

    // 3.7 In-Flight Compression Invalidation on loadPreset() (Defect 4 remediation)
    promptInput.value = 'Custom prompt about machine learning inference latency and batch sizes.';
    win.validatePromptLength();
    const customInFlightPromise = win.compress(false); // 250ms delay
    await new Promise(r => setTimeout(r, 50));
    win.loadPreset('house');
    await customInFlightPromise;
    await new Promise(r => setTimeout(r, 300));
    assert(win.State.lastResult && win.State.lastResult.optimized_prompt.includes('Real Estate & Finance'), 'Preset Invalidation: Preset output preserved without being clobbered by earlier custom compression');

    // =========================================================================
    // Suite 4: Tier 3 Cross-Feature Pairwise Combinations
    // =========================================================================
    console.log('\n--- Suite 4: Tier 3 Cross-Feature Combinations ---');

    // Combination A: House Preset + Aggressive Mode + Gemini 2.5 Pro + Diff Toggle
    win.loadPreset('house');
    win.selectMode('aggressive');
    modelSelect.value = '0.00125';
    win.updateModelPricing();
    diffToggle.checked = true;
    win.toggleDiffView();
    await new Promise(r => setTimeout(r, 200));

    assert(win.State.currentMode === 'aggressive', 'Pairwise A: Mode set to aggressive');
    assert(win.State.currentModelRate === 0.00125, 'Pairwise A: Model rate set to Gemini 2.5 Pro');
    assert(outputBox.innerHTML.includes('diff-removed'), 'Pairwise A: Pruning diff visible in output');
    assert(costSaved.textContent.includes('$') && costSaved.textContent.includes('/ 1k runs'), 'Pairwise A: Cost telemetry computed');

    // Combination B: Legal Preset + Gentle Mode + Claude 3.5 Sonnet + Diff Toggle Off
    win.loadPreset('legal');
    win.selectMode('gentle');
    modelSelect.value = '0.003';
    win.updateModelPricing();
    diffToggle.checked = false;
    win.toggleDiffView();
    await new Promise(r => setTimeout(r, 200));

    assert(win.State.currentMode === 'gentle', 'Pairwise B: Mode set to gentle');
    assert(win.State.currentModelRate === 0.003, 'Pairwise B: Model rate set to Claude 3.5 Sonnet');
    assert(!outputBox.innerHTML.includes('diff-removed'), 'Pairwise B: Diff cleanly toggled off');
    assert(outputBox.textContent.includes('Legal & Compliance'), 'Pairwise B: Output specialized for Legal domain');

    // =========================================================================
    // Suite 5: Tier 4 Real-World End-to-End Guest Workflow
    // =========================================================================
    console.log('\n--- Suite 5: Tier 4 End-to-End Workload Scenario ---');

    // 1. Visitor drops markdown technical requirements document
    win.clearInput();
    const markdownDoc = {
        name: 'architecture_spec.md',
        size: 5120
    };
    win.FileReader = class MockMarkdownReader {
        readAsText(file) {
            setTimeout(() => {
                this.onload({
                    target: {
                        result: '# System Architecture & Backend Contract\n\n' +
                                'Please review the database schema and API auth middleware carefully. ' +
                                'Whenever the Safari browser clears cookies, the auth session crashes with 401 unauthorized. ' +
                                'We really just want to figure out how to resolve this bug cleanly before launch.'
                    }
                });
            }, 10);
        }
    };
    await win.processIngestedFile(markdownDoc);
    await new Promise(r => setTimeout(r, 100));

    assert(promptInput.value.includes('System Architecture & Backend Contract'), 'E2E: Markdown file ingested into input pane');
    assert(doc.getElementById('domain-name').textContent.includes('Software'), 'E2E: Domain auto-detected as Software Engineering');

    // 2. Selects Balanced compression mode
    win.selectMode('balanced');
    assert(doc.getElementById('mode').value === 'balanced', 'E2E: Balanced compression mode selected');

    // 3. Executes compression
    await win.compress(true);
    await new Promise(r => setTimeout(r, 200));

    // 4. Inspects CRISP structured payload
    assert(outputBox.textContent.includes('[ROLE] Principal Software Engineering'), 'E2E: CRISP prompt contains role tag');
    assert(outputBox.textContent.includes('[CONTEXT]'), 'E2E: CRISP prompt contains context tag');
    assert(outputBox.textContent.includes('[EXECUTION]'), 'E2E: CRISP prompt contains execution plan');
    assert(outputBox.textContent.includes('[CONSTRAINTS]'), 'E2E: CRISP prompt contains constraints tag');

    // 5. Verifies calm quality badge & Inspector TES/SFS/SCS metrics
    assert(tesBadge.textContent.startsWith('✓'), `E2E: Calm quality badge displayed (got "${tesBadge.textContent}")`);
    assert(doc.getElementById('quality-breakdown').textContent.includes('Efficiency (TES):'), 'E2E: Inspector quality breakdown populated');

    // 6. Evaluates cost savings on GPT-4o
    modelSelect.value = '0.005';
    win.updateModelPricing();
    assert(costSaved.textContent.includes('saved / 1k runs'), 'E2E: Dollar savings calculated per 1k runs');

    // 7. Copies to clipboard
    let clipboardWrittenText = '';
    win.navigator.clipboard = {
        writeText: async (t) => { clipboardWrittenText = t; }
    };
    win.copyResult();
    await new Promise(r => setTimeout(r, 50));
    assert(clipboardWrittenText.length > 0, 'E2E: Optimized CRISP prompt copied to clipboard');

    // 8. Toggles pruning diff
    diffToggle.checked = true;
    win.toggleDiffView();
    assert(outputBox.innerHTML.includes('diff-removed'), 'E2E: Pruning diff visible on final output');

    // 9. Aggressive Deep Architect expansion telemetry (outputTokens > inputTokens)
    win.State.currentMode = 'aggressive';
    win.State.lastResult = {
        id: 'aggr-deep-architect-1',
        token_original: 512,
        token_final: 974,
        token_saved: 0,
        savings_pct: 0,
        tes_score: 0.96,
        optimized_prompt: '# ROLE: REAL ESTATE & MORTGAGE GUIDE\n\n# EXECUTION PHASES:\n1. **Financial Readiness** (1-2 weeks): Prep\n2. **Pre-Approval** (1 week): Lender\n3. **Home Search** (2-6 weeks): Tour\n4. **Escrow & Closing** (30 days): Close\n\n# VALIDATION:\n1. DTI check — Verify ratio (calc)\n2. Inspection check — Review report (audit)\n3. Closing disclosure — Confirm fees (verify)\n\n# SUCCESS CRITERIA:\n- Conceptually clear explanation of residential home-buying and mortgage process at the target audience level.'
    };
    win.renderResult(win.State.lastResult);
    assert(
        doc.getElementById('output-saving-pct').textContent === '+90% output',
        `Deep Architect: Saving pill shows "+90% output" (got "${doc.getElementById('output-saving-pct').textContent}")`
    );
    assert(
        costSaved.textContent === '~3,000 tok output target · 4 Phases · 3 QA Gates',
        `Deep Architect: Footer shows depth & structure telemetry (got "${costSaved.textContent}")`
    );
    // Switching model pricing must not clobber Deep Architect telemetry
    modelSelect.value = '0.003';
    win.updateModelPricing();
    assert(
        costSaved.textContent === '~3,000 tok output target · 4 Phases · 3 QA Gates',
        `Deep Architect: Model pricing switch preserves depth telemetry (got "${costSaved.textContent}")`
    );

    // =========================================================================
    // Suite 6: Insights Bento Dashboard Telemetry & Interactive Cost Switcher
    // =========================================================================
    console.log('\n--- Suite 6: Insights Bento Dashboard ---');
    win.renderInsightsBento({ total_compressions: 0, total_tokens_saved: 0, avg_tokens_saved: 0 });
    assert(
        doc.querySelector('#global-stats .insights-empty') !== null,
        'Insights: Empty state rendered when total_compressions is 0'
    );

    const mockStatsPayload = {
        total_compressions: 12,
        total_tokens_saved: 4000,
        avg_tokens_saved: 333.3,
        total_original_tokens: 10000,
        total_final_tokens: 6000,
        best_tokens_saved: 820,
        mode_breakdown: [
            { mode: 'balanced', count: 7, tokens_saved: 2400 },
            { mode: 'aggressive', count: 3, tokens_saved: 1100 },
            { mode: 'gentle', count: 2, tokens_saved: 500 },
        ],
        domain_breakdown: [
            { domain: 'software', count: 6, tokens_saved: 2200 },
            { domain: 'legal', count: 4, tokens_saved: 1200 },
            { domain: 'real-estate', count: 2, tokens_saved: 600 },
        ],
        daily_series: [
            { day: '2026-11-16', runs: 4, original_tokens: 3200, final_tokens: 1900, tokens_saved: 1300 },
            { day: '2026-11-17', runs: 5, original_tokens: 4100, final_tokens: 2400, tokens_saved: 1700 },
            { day: '2026-11-18', runs: 3, original_tokens: 2700, final_tokens: 1700, tokens_saved: 1000 },
        ],
        top_runs: [
            { id: 'r1', use_case: 'software', mode: 'balanced', token_original: 1200, token_final: 380, tokens_saved: 820, preview: 'Refactor auth middleware...' },
        ],
    };
    win.State.lastStats = mockStatsPayload;
    win.renderInsightsBento(mockStatsPayload);

    assert(
        doc.getElementById('insights-total-saved').textContent.replace(/,/g, '') === '4000',
        `Insights: Hero total saved rendered (got "${doc.getElementById('insights-total-saved').textContent}")`
    );
    assert(
        doc.getElementById('insights-cost-saved').textContent === '$0.02 saved',
        `Insights: GPT-4o default cumulative cost computed (got "${doc.getElementById('insights-cost-saved').textContent}")`
    );
    assert(
        doc.querySelector('#insights-trajectory-chart svg.bento-chart-svg') !== null,
        'Insights: 14-day SVG trajectory chart rendered'
    );
    assert(
        doc.querySelectorAll('#global-stats .mode-stack-seg').length === 3,
        'Insights: Segmented mode mix bar rendered 3 segments'
    );
    assert(
        doc.querySelectorAll('#global-stats .top-run-item').length === 1,
        'Insights: Top efficiency runs list rendered'
    );

    win.setInsightsModel('claude35');
    assert(
        doc.getElementById('insights-cost-saved').textContent === '$0.01 saved',
        `Insights: Model switcher updates cumulative cost to Claude 3.5 ($0.01 saved, got "${doc.getElementById('insights-cost-saved').textContent}")`
    );

    // =========================================================================
    // Suite 7: History Timeline Filtering, Inline Diff & Workbench Restore
    // =========================================================================
    console.log('\n--- Suite 7: History Timeline ---');
    win.eval('cachedHistoryRecords = []; HistoryState.expandedIds.clear(); applyHistoryFilters();');
    assert(
        doc.querySelector('#history-list .insights-empty') !== null,
        'History: Empty state rendered when no history records exist'
    );

    const nowIso = new Date().toISOString();
    const mockHistory = [
        {
            id: 'h-1',
            original_prompt: 'Please review this SaaS NDA agreement for indemnification clauses.',
            optimized_prompt: '# ROLE: LEGAL COUNSEL\n# TASK: Audit NDA indemnification.',
            tokens_saved: 420,
            token_original: 800,
            token_final: 380,
            use_case: 'legal',
            mode: 'aggressive',
            created_at: nowIso,
        },
        {
            id: 'h-2',
            original_prompt: 'Fix Safari 401 cookie spin bug on /api/stats endpoint.',
            optimized_prompt: '# ROLE: AUTH ENGINEER\n# TASK: Diagnose Safari SameSite cookie.',
            tokens_saved: 150,
            token_original: 400,
            token_final: 250,
            use_case: 'software',
            mode: 'balanced',
            created_at: nowIso,
        },
    ];
    win.eval(`cachedHistoryRecords = ${JSON.stringify(mockHistory)}; HistoryState.expandedIds.clear(); resetHistoryFilters();`);

    assert(
        doc.querySelectorAll('#history-list .history-item').length === 2,
        'History: Both timeline cards rendered'
    );
    assert(
        doc.querySelector('#history-list .history-group-label')?.textContent === 'Today',
        'History: Date group header "Today" rendered'
    );
    assert(
        doc.getElementById('history-summary-pill').textContent.includes('2 runs'),
        `History: Summary pill shows run count (got "${doc.getElementById('history-summary-pill').textContent}")`
    );

    win.setHistoryModeFilter('aggressive');
    assert(
        doc.querySelectorAll('#history-list .history-item').length === 1,
        'History: Mode filter "aggressive" narrows list to 1 item'
    );

    win.setHistoryModeFilter('all');
    win.toggleHistoryExpand('h-1');
    assert(
        doc.querySelector('[data-history-id="h-1"] .history-diff-drawer') !== null,
        'History: Inline side-by-side Original vs. Optimized diff drawer opens on Inspect'
    );

    win.restoreHistoryPrompt('h-2');
    assert(
        doc.getElementById('prompt-input').value.includes('Safari 401 cookie'),
        'History: Load in Workbench restores original prompt into #prompt-input'
    );

    // =========================================================================
    // Suite 8: 2026 UI/UX, Accessibility & Modern Web Standards Verification
    // =========================================================================
    console.log('\n--- Suite 8: 2026 UI/UX & Accessibility ---');
    const adminHtml = fs.readFileSync(path.resolve(__dirname, '../static/admin.html'), 'utf8');
    const tokensCss = fs.readFileSync(path.resolve(__dirname, '../static/styles/tokens.css'), 'utf8');

    assert(html.includes('<meta name="color-scheme" content="light dark">'), 'A11y/Theme: index.html declares color-scheme meta');
    assert(adminHtml.includes('<meta name="color-scheme" content="light dark">'), 'A11y/Theme: admin.html declares color-scheme meta');
    assert(tokensCss.includes('color-scheme: light dark') && tokensCss.includes('light-dark('), 'Theme: tokens.css sets color-scheme on :root with light-dark()');
    assert(tokensCss.includes('prefers-reduced-motion: reduce') && tokensCss.includes('forced-colors: active'), 'A11y: tokens.css supports reduced-motion and forced-colors');

    assert(doc.querySelector('header.tc-header') !== null, 'A11y: Semantic <header> landmark present');
    assert(doc.querySelector('nav.nav-bar[role="tablist"]') !== null, 'A11y: Semantic <nav role="tablist"> present');
    assert(doc.querySelector('main#main-content') !== null, 'A11y: Semantic <main> landmark present');
    assert(doc.querySelector('a.skip-link[href="#main-content"]') !== null, 'A11y: Skip link targets #main-content');
    assert(doc.querySelector('#toast-region[aria-live="polite"]') !== null, 'A11y: Toast live region has aria-live="polite"');

    assert(doc.getElementById('skin-modal')?.tagName === 'DIALOG', 'Modern UI: #skin-modal uses native <dialog>');
    assert(doc.getElementById('verify-modal')?.tagName === 'DIALOG', 'Modern UI: #verify-modal uses native <dialog>');
    assert(doc.getElementById('studio-cmd-modal')?.tagName === 'DIALOG', 'Modern UI: #studio-cmd-modal (⌘K) uses native <dialog>');

    const tesBtn = doc.getElementById('tes-score');
    const inspectorDrawer = doc.getElementById('inspector-drawer');
    assert(tesBtn && tesBtn.tagName === 'BUTTON', 'A11y: #tes-score quality trigger is a native <button>');
    assert(inspectorDrawer && inspectorDrawer.tagName === 'DETAILS' && !inspectorDrawer.open, 'Modern UI: #inspector-drawer starts collapsed as native <details>');
    win.toggleQualityBreakdown();
    assert(inspectorDrawer.open === true && tesBtn.getAttribute('aria-expanded') === 'true', 'A11y: toggleQualityBreakdown() opens #inspector-drawer and updates aria-expanded="true"');
    win.toggleQualityBreakdown();
    assert(inspectorDrawer.open === false && tesBtn.getAttribute('aria-expanded') === 'false', 'A11y: toggleQualityBreakdown() closes #inspector-drawer and resets aria-expanded="false"');

    // Radiogroup keyboard navigation: ArrowRight on #outcome-pills moves from #po-factchecked -> #po-saver
    poFact.focus();
    outcomePills.dispatchEvent(new win.KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
    assert(
        doc.activeElement === poSaver &&
        poSaver.classList.contains('active') &&
        poSaver.getAttribute('aria-checked') === 'true' &&
        poFact.getAttribute('aria-checked') === 'false',
        'A11y: ArrowRight on #outcome-pills moves focus and selection from #po-factchecked to #po-saver'
    );

    win.openStudioCommandPalette();
    assert(doc.querySelectorAll('#studio-cmd-list .cmd-palette-item').length >= 5, 'Modern UI: Studio ⌘K Command Palette populates quick actions');

    // Suite 9: macOS Apple HIG UI & Layout Refinements
    console.log('\n--- Suite 9: macOS Apple HIG UI & Layout Refinements ---');
    const studioCss = fs.readFileSync(path.resolve(__dirname, '../static/styles/studio.css'), 'utf8');
    assert(
        tokensCss.includes('--bg-canvas') &&
        tokensCss.includes('--surface-card') &&
        tokensCss.includes('--surface-control') &&
        tokensCss.includes('--surface-popover') &&
        tokensCss.includes('--accent-primary') &&
        tokensCss.includes('--border-hairline'),
        'macOS HIG: tokens.css defines core design tokens (--bg-canvas, --surface-card, --surface-control, --surface-popover, --accent-primary, --border-hairline)'
    );
    assert(
        studioCss.includes('.tc-header.macos-toolbar') &&
        studioCss.includes('.inspector-popover-panel') &&
        studioCss.includes('.studio-disclosure.inspector-popover-wrap') &&
        studioCss.includes('.feedback-icon-btn') &&
        studioCss.includes('.crisp-section-heading') &&
        studioCss.includes('.clarify-strip'),
        'macOS HIG: studio.css implements sticky 52px toolbar, floating Inspector popover, quiet feedback icons, syntax tinting, and single-row Clarify strip'
    );

    assert(doc.querySelector('header.tc-header.macos-toolbar') !== null, 'macOS HIG: Top Toolbar has header.tc-header.macos-toolbar');
    assert(doc.querySelector('header.tc-header nav.nav-bar[role="tablist"]') !== null, 'macOS HIG: Top Toolbar nav has role="tablist"');
    assert(doc.getElementById('header-cmd-kbd') !== null, 'macOS HIG: #header-cmd-kbd present');

    win.__ORYX_FORCE_PLATFORM = 'win';
    win.syncPlatformShortcutBadges();
    assert(doc.getElementById('header-cmd-kbd').textContent.includes('Ctrl'), 'macOS HIG: Platform-adaptive shortcut badge shows Ctrl on Windows');

    win.__ORYX_FORCE_PLATFORM = 'apple';
    win.syncPlatformShortcutBadges();
    assert(doc.getElementById('header-cmd-kbd').textContent.includes('⌘'), 'macOS HIG: Platform-adaptive shortcut badge shows ⌘ on macOS');

    assert(doc.querySelector('#input-pane .pane-header #clear-input-btn') !== null, 'macOS HIG: #clear-input-btn is located inside #input-pane .pane-header');
    assert(doc.getElementById('compress-btn-kbd') !== null, 'macOS HIG: #compress-btn preserves #compress-btn-kbd');

    assert(doc.getElementById('output-view-toggle') !== null && doc.getElementById('ov-rehydrated') !== null && doc.getElementById('ov-delta') !== null, 'macOS HIG: Right Pane header keeps #output-view-toggle with #ov-rehydrated and #ov-delta');
    assert(
        doc.querySelector('#output-pane .pane-header #copy-result-btn') !== null &&
        doc.querySelector('#output-pane .pane-header #launch-chatgpt-btn') !== null &&
        doc.querySelector('#output-pane .pane-header #launch-claude-btn') !== null,
        'macOS HIG: Right Pane header keeps export actions (#copy-result-btn, #launch-chatgpt-btn, #launch-claude-btn) inside pane-header'
    );

    diffToggle.checked = false;
    win.State.showDiff = false;
    win.renderResult({
        id: 'hig-syntax-1',
        token_original: 400,
        token_final: 650,
        token_saved: 0,
        savings_pct: 0,
        tes_score: 0.97,
        optimized_prompt: '# ROLE: PRINCIPAL ARCHITECT\n<facts trust="untrusted">\n- [F1] Latency <= 50ms\n</facts>\n\n# EXECUTION PHASES:\n1. **Schema Design** (1 week): Model tables\n\n# CONSTRAINTS:\n- [STRICT] Zero downtime [F1]',
        clarifying_questions: [{ id: 1, gap_addressed: 'missing_slot:real-estate:buyer_credit_tier_and_income', question: 'What is your current credit score tier and approximate debt-to-income (DTI) profile?' }]
    });
    assert(
        outputBox.innerHTML.includes('crisp-section-heading') &&
        outputBox.innerHTML.includes('crisp-phase-title') &&
        outputBox.innerHTML.includes('crisp-cite') &&
        outputBox.innerHTML.includes('crisp-xml-tag'),
        'macOS HIG: Syntax Highlighting covers # SECTION:, phases, [F#] citations, and attributed <facts trust="untrusted"> XML tags when token_saved === 0'
    );

    const clarifyChip = doc.querySelector('#clarifying-pills button.clarify-chip');
    assert(
        clarifyChip !== null &&
        clarifyChip.textContent === '+ Add credit & DTI' &&
        clarifyChip.title.includes('credit score tier'),
        `macOS HIG: Clarify chip uses concise trigger "+ Add credit & DTI" while keeping full question in .title (got "${clarifyChip ? clarifyChip.textContent : 'null'}")`
    );

    win.clearInput();
    const clarifyingPills = doc.getElementById('clarifying-pills');
    assert(clarifyingPills && clarifyingPills.style.display === 'none' && clarifyingPills.innerHTML === '', 'macOS HIG: win.clearInput() hides and clears #clarifying-pills');

    win.openStudioCommandPalette();
    const cmdInput = doc.getElementById('studio-cmd-input');
    const cmdHeaders = Array.from(doc.querySelectorAll('#studio-cmd-list .cmd-group-header')).map(el => el.textContent);
    assert(
        cmdHeaders.includes('Actions') &&
        cmdHeaders.includes('Recent Prompts') &&
        cmdInput.getAttribute('aria-activedescendant') === 'cmd-opt-0',
        `macOS HIG: ⌘K Spotlight Command Palette renders grouped Actions & Recent Prompts with ARIA activedescendant (got ${JSON.stringify(cmdHeaders)})`
    );

    // Security: logout() purges recent prompt cache (oryx_recent_runs) and history state
    win.logout();
    assert(
        win.State.recentRuns.length === 0 &&
        win.localStorage.getItem('oryx_recent_runs') === null,
        'Security: logout() purges State.recentRuns and localStorage oryx_recent_runs'
    );

    win.toggleTheme();
    assert(doc.documentElement.classList.contains('dark') && doc.documentElement.getAttribute('data-theme') === 'dark', 'macOS HIG: .dark class and data-theme="dark" sync on <html>');
    win.toggleTheme();
    assert(!doc.documentElement.classList.contains('dark') && doc.documentElement.getAttribute('data-theme') === 'light', 'macOS HIG: toggleTheme() switches back to light mode');

    assert(!adminHtml.includes('hud-mode'), 'Admin P3: CRT scanline hud-mode class retired');
    assert(adminHtml.includes('id="btn-admin-theme"') && adminHtml.includes('<th scope="col">'), 'Admin P3: Shared theme toggle and accessible <th scope="col"> headers present');

    // Suite 10: Prismatic Oryx 3D Brand System, Hover-Only Motion & Monochrome HIG Icons
    console.log('\n--- Suite 10: Prismatic Oryx 3D Brand System, Hover-Only Motion & Monochrome HIG Icons ---');
    const adminJs = fs.readFileSync(path.resolve(__dirname, '../static/admin.js'), 'utf8');
    const ragJs = fs.readFileSync(path.resolve(__dirname, '../static/rag_workspace.js'), 'utf8');

    // 10.1 Favicon & multi-density .oryx-3d-emblem srcset in index.html & admin.html
    assert(
        html.includes('favicon.svg') &&
        adminHtml.includes('favicon.svg'),
        'Oryx 3D: index.html and admin.html link favicon.svg'
    );
    const indexEmblemImg = doc.querySelector('#oryx-logo-3d .oryx-3d-emblem img');
    assert(
        indexEmblemImg !== null &&
        (indexEmblemImg.getAttribute('srcset') || '').includes('logo-1x.png') &&
        (indexEmblemImg.getAttribute('srcset') || '').includes('1x') &&
        (indexEmblemImg.getAttribute('srcset') || '').includes('logo.png') &&
        (indexEmblemImg.getAttribute('srcset') || '').includes('2x'),
        'Oryx 3D: index.html .oryx-3d-emblem uses multi-density srcset (logo-1x.png 1x, logo.png 2x)'
    );
    assert(
        adminHtml.includes('class="oryx-3d-emblem"') &&
        /logo-1x\.png[^"']*1x,\s*logo\.png[^"']*2x/.test(adminHtml),
        'Oryx 3D: admin.html .oryx-3d-emblem uses multi-density srcset (logo-1x.png 1x, logo.png 2x)'
    );

    // 10.2 Legacy Sobel PBR owl artifacts removed
    const legacyOwlFiles = [html, adminHtml, studioCss, appJs, adminJs];
    const hasLegacyOwl = legacyOwlFiles.some(src =>
        src.includes('.owl-3d-rig') ||
        src.includes('oryx-owl-pbr-canvas') ||
        src.includes('initOryxOwl3dIcon')
    );
    assert(!hasLegacyOwl, 'Oryx 3D: Legacy .owl-3d-rig, #oryx-owl-pbr-canvas, and initOryxOwl3dIcon absent across HTML/CSS/JS');

    // 10.3 Hover-only tilt emblem behavior (calm at rest, idempotent, tilts on pointermove, resets on pointerleave & pointercancel, respects reduced motion)
    const logoContainer = doc.getElementById('oryx-logo-3d');
    const logoEmblem = logoContainer ? logoContainer.querySelector('.oryx-3d-emblem') : null;
    const logoHost = logoContainer ? (logoContainer.closest('.tc-logo, .logo') || logoContainer) : null;
    if (win.OryxDomUtils && typeof win.OryxDomUtils.initHoverTiltEmblem === 'function') {
        win.OryxDomUtils.initHoverTiltEmblem('oryx-logo-3d');
        // Idempotent second call must be a clean no-op
        win.OryxDomUtils.initHoverTiltEmblem('oryx-logo-3d');
    }
    assert(
        logoContainer !== null &&
        logoContainer.dataset.oryx3dInit === '1' &&
        logoEmblem !== null &&
        logoEmblem.style.getPropertyValue('--oryx-rx') === '0deg' &&
        logoEmblem.style.getPropertyValue('--oryx-ry') === '0deg',
        'Oryx 3D: initHoverTiltEmblem is idempotent, marks #oryx-logo-3d with data-oryx3d-init="1", and starts calm at 0deg rest pose'
    );
    if (logoHost && logoEmblem) {
        const PointerOrMouseEvent = win.PointerEvent || win.MouseEvent;
        logoHost.dispatchEvent(new PointerOrMouseEvent('pointermove', { clientX: 48, clientY: 6, bubbles: true }));
        const ryMoved = logoEmblem.style.getPropertyValue('--oryx-ry');
        assert(
            ryMoved !== '' && ryMoved !== '0deg' && ryMoved !== '0.00deg',
            `Oryx 3D: pointermove tilts .oryx-3d-emblem (--oryx-ry=${ryMoved})`
        );
        logoHost.dispatchEvent(new PointerOrMouseEvent('pointerleave', { bubbles: true }));
        assert(
            logoEmblem.style.getPropertyValue('--oryx-rx') === '0deg' &&
            logoEmblem.style.getPropertyValue('--oryx-ry') === '0deg',
            'Oryx 3D: pointerleave resets .oryx-3d-emblem back to 0deg'
        );

        // Verify pointercancel also resets pose back to 0deg
        logoHost.dispatchEvent(new PointerOrMouseEvent('pointermove', { clientX: 48, clientY: 6, bubbles: true }));
        const ryMovedAgain = logoEmblem.style.getPropertyValue('--oryx-ry');
        logoHost.dispatchEvent(new PointerOrMouseEvent('pointercancel', { bubbles: true }));
        assert(
            ryMovedAgain !== '0deg' &&
            logoEmblem.style.getPropertyValue('--oryx-rx') === '0deg' &&
            logoEmblem.style.getPropertyValue('--oryx-ry') === '0deg',
            'Oryx 3D: pointercancel resets .oryx-3d-emblem back to 0deg'
        );

        // Verify prefers-reduced-motion: reduce keeps emblem at 0deg during pointermove
        const origMatchMedia = win.matchMedia;
        win.matchMedia = (q) => ({ matches: q.includes('prefers-reduced-motion: reduce)'), media: q });
        logoHost.dispatchEvent(new PointerOrMouseEvent('pointermove', { clientX: 48, clientY: 6, bubbles: true }));
        const rxReduced = logoEmblem.style.getPropertyValue('--oryx-rx');
        const ryReduced = logoEmblem.style.getPropertyValue('--oryx-ry');
        win.matchMedia = origMatchMedia;
        assert(
            rxReduced === '0deg' && ryReduced === '0deg',
            `Oryx 3D: prefers-reduced-motion: reduce keeps emblem at 0deg (--oryx-rx=${rxReduced}, --oryx-ry=${ryReduced})`
        );
    }

    // 10.3b CSS mask icon architecture & DOM data-icon coverage across index.html + admin.html
    assert(
        !/@property\s+--icon(?:-start)?\s*\{[^}]*inherits:\s*false/i.test(tokensCss) &&
        tokensCss.includes('--oryx-glyph') &&
        tokensCss.includes('.oryx-icon-start:not(.oryx-icon):not(:empty)::before') &&
        !tokensCss.includes(':where(.oryx-icon-start') &&
        tokensCss.includes('-webkit-mask-image: var(--oryx-glyph)') &&
        tokensCss.includes('mask-image: var(--oryx-glyph)') &&
        tokensCss.includes('forced-color-adjust: none'),
        'HIG Icons: tokens.css uses inheriting --oryx-glyph, non-:where() selectors, mask-image longhands, and forced-color-adjust: none without non-inheriting @property'
    );

    const adminDom = new JSDOM(adminHtml, { url: 'http://localhost:8081/admin.html' });
    const adminDoc = adminDom.window.document;
    const customSkinBtn = doc.getElementById('btn-custom-skin');
    assert(
        customSkinBtn !== null &&
        customSkinBtn.getAttribute('data-icon') === 'palette' &&
        !customSkinBtn.hasAttribute('data-action'),
        'HIG Icons: #btn-custom-skin carries data-icon="palette" without spurious data-action="customSkin"'
    );

    const iconAuditFailures = [];
    for (const [pageName, pageDoc] of [['index.html', doc], ['admin.html', adminDoc]]) {
        const iconEls = pageDoc.querySelectorAll('.oryx-icon, .oryx-icon-start');
        iconEls.forEach((el) => {
            const id = el.id || '';
            if (id === 'btn-theme-toggle' || id === 'btn-admin-theme') {
                return;
            }
            const iconName = el.getAttribute('data-icon');
            if (!iconName) {
                iconAuditFailures.push(`${pageName}#${id || el.className}: missing data-icon`);
                return;
            }
            const tokenName = `--icon-${iconName}:`;
            const selectorName = `[data-icon="${iconName}"]`;
            if (!tokensCss.includes(tokenName) || !tokensCss.includes(selectorName)) {
                iconAuditFailures.push(`${pageName}#${id || iconName}: missing token/mapping for ${iconName}`);
            }
        });
    }
    assert(
        iconAuditFailures.length === 0,
        `HIG Icons: Every .oryx-icon / .oryx-icon-start across index.html and admin.html resolves to a defined tokens.css glyph (issues: ${iconAuditFailures.join('; ') || 'none'})`
    );

    // 10.3c On-disk asset verification (logos, favicon, 3D badges, shared OryxDomUtils module)
    const requiredAssets = [
        '../static/logo.png',
        '../static/logo-1x.png',
        '../static/favicon.svg',
        '../static/oryx_dom_utils.js',
        '../static/icons/badge-vault.png',
        '../static/icons/badge-vault-1x.png',
        '../static/icons/badge-telemetry.png',
        '../static/icons/badge-telemetry-1x.png',
        '../static/icons/badge-doc.png',
        '../static/icons/badge-doc-1x.png'
    ];
    const missingOrEmptyAssets = requiredAssets.filter((relPath) => {
        const fullPath = path.resolve(__dirname, relPath);
        return !fs.existsSync(fullPath) || fs.statSync(fullPath).size <= 0;
    });
    assert(
        missingOrEmptyAssets.length === 0,
        `Oryx 3D: All required brand assets and static/oryx_dom_utils.js exist on disk with size > 0 (missing: ${missingOrEmptyAssets.join(', ') || 'none'})`
    );

    // 10.4 3D Hero Badges in gates, drop overlay, and runtime empty states with alt=""
    const vaultGateImg = doc.querySelector('#history-auth-gate img.oryx-hero-badge-img');
    const telemetryGateImg = doc.querySelector('#stats-auth-gate img.oryx-hero-badge-img');
    const docDropImg = doc.querySelector('#drop-overlay img.oryx-hero-badge-img');
    assert(
        vaultGateImg !== null && (vaultGateImg.getAttribute('src') || '').includes('badge-vault') && vaultGateImg.getAttribute('alt') === '' &&
        telemetryGateImg !== null && (telemetryGateImg.getAttribute('src') || '').includes('badge-telemetry') && telemetryGateImg.getAttribute('alt') === '' &&
        docDropImg !== null && (docDropImg.getAttribute('src') || '').includes('badge-doc') && docDropImg.getAttribute('alt') === '',
        'Oryx 3D: #history-auth-gate, #stats-auth-gate, and #drop-overlay render 3D hero badges with alt=""'
    );

    if (typeof win.renderHistoryList === 'function') {
        win.renderHistoryList([]);
    }
    const emptyHistoryBadge = doc.querySelector('#history-list img.oryx-hero-badge-img');
    if (typeof win.renderInsightsBento === 'function') {
        win.renderInsightsBento({ total_compressions: 0 });
    }
    const emptyInsightsBadge = doc.querySelector('#global-stats img.oryx-hero-badge-img');
    assert(
        emptyHistoryBadge !== null && (emptyHistoryBadge.getAttribute('src') || '').includes('badge-vault') && emptyHistoryBadge.getAttribute('alt') === '' &&
        emptyInsightsBadge !== null && (emptyInsightsBadge.getAttribute('src') || '').includes('badge-telemetry') && emptyInsightsBadge.getAttribute('alt') === '',
        'Oryx 3D: Empty states for History and Insights render 3D hero badges (badge-vault, badge-telemetry) with alt=""'
    );

    // 10.5 Theme toggle empty textContent + ARIA label sync, and zero raw decorative emojis
    const themeBtn = doc.getElementById('btn-theme-toggle');
    win.toggleTheme();
    const darkAria = themeBtn ? themeBtn.getAttribute('aria-label') : '';
    const darkText = themeBtn ? themeBtn.textContent : null;
    win.toggleTheme();
    const lightAria = themeBtn ? themeBtn.getAttribute('aria-label') : '';
    const lightText = themeBtn ? themeBtn.textContent : null;
    assert(
        themeBtn !== null &&
        darkText === '' &&
        lightText === '' &&
        darkAria !== lightAria &&
        (darkAria === 'Switch to Light Theme' || darkAria === 'Switch to Dark Theme') &&
        (lightAria === 'Switch to Light Theme' || lightAria === 'Switch to Dark Theme'),
        `HIG Icons: #btn-theme-toggle keeps textContent === "" while updating aria-label (${darkAria} <-> ${lightAria})`
    );

    const emojiRegex = /[\u{1F300}-\u{1FAFF}]/gu;
    const emojiHits = [];
    for (const [name, src] of [
        ['index.html', html],
        ['admin.html', adminHtml],
        ['app.js', appJs],
        ['admin.js', adminJs],
        ['rag_workspace.js', ragJs]
    ]) {
        const matches = src.match(emojiRegex);
        if (matches && matches.length > 0) {
            emojiHits.push(`${name}: ${matches.join('')}`);
        }
    }
    assert(emojiHits.length === 0, `HIG Icons: Zero raw decorative emojis remain across frontend files (hits: ${emojiHits.join('; ') || 'none'})`);

    // Final Summary
    console.log('\n======================================================');
    console.log(` Results: ${passed} PASSED | ${failed} FAILED`);
    console.log('======================================================\n');

    if (failed > 0) {
        console.error('Failed Assertions:');
        failures.forEach((f, i) => console.error(`  ${i + 1}. ${f}`));
    }

    process.exit(failed > 0 ? 1 : 0);
}

runTestSuite().catch(err => {
    console.error('Test suite runner crashed:', err);
    process.exit(1);
});

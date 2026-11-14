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
    const mockJsPath = path.resolve(__dirname, '../static/mock_api.js');
    const optJsPath = path.resolve(__dirname, '../static/prompt_optimizer.js');
    const devJsPath = path.resolve(__dirname, '../static/dev_engine.js');

    const html = fs.readFileSync(htmlPath, 'utf8');
    const appJs = fs.readFileSync(appJsPath, 'utf8');
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

    // 2.2 R1: Selective Auth Gating
    console.log('Testing R1: Selective Auth Gating...');
    win.switchTab('history');
    const historyGate = doc.getElementById('history-auth-gate');
    assert(historyGate && historyGate.style.display !== 'none', 'R1: History tab displays auth gate for unauthenticated users');

    win.switchTab('stats');
    const statsGate = doc.getElementById('stats-auth-gate');
    assert(statsGate && statsGate.style.display !== 'none', 'R1: Stats tab displays auth gate for unauthenticated users');

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

    // TES fidelity score check
    win.renderResult(win.State.lastResult);
    assert(tesBadge.textContent.includes('TES 0.98'), `R3 TES: Badge renders score (got ${tesBadge.textContent})`);

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

    // 2.6 R5: Mode Pills & Domain Detection
    console.log('Testing R5: Mode Pills & Domain Detection...');
    win.selectMode('aggressive');
    assert(doc.getElementById('pm-aggressive').classList.contains('active'), 'R5: Aggressive mode pill marked active');
    assert(doc.getElementById('mode').value === 'aggressive', 'R5: Hidden #mode input updated to aggressive');

    win.selectMode('gentle');
    assert(doc.getElementById('pm-gentle').classList.contains('active'), 'R5: Gentle mode pill marked active');
    assert(doc.getElementById('mode').value === 'gentle', 'R5: Hidden #mode input updated to gentle');

    // Test domain keyword heuristic
    assert(win.detectDomain('mortgage interest rate and down payment') === 'Real Estate & Finance', 'R5: Domain detected Real Estate & Finance');
    assert(win.detectDomain('safari console bug 401 error crash') === 'Software Engineering', 'R5: Domain detected Software Engineering');
    assert(win.detectDomain('indemnification clause contract liability') === 'Legal & Compliance', 'R5: Domain detected Legal & Compliance');
    assert(win.detectDomain('ebitda revenue investor runway') === 'Finance & Strategy', 'R5: Domain detected Finance & Strategy');
    assert(win.detectDomain('how do I make a cake') === 'General', 'R5: Default domain is General');

    // 2.7 Tier 2: Boundary & Corner Cases
    console.log('\n--- Suite 3: Boundary & Corner Cases ---');
    // Prompt < 8 words disables compress button
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

    // 5. Verifies TES Fidelity score
    assert(tesBadge.textContent.includes('TES 0.97') || tesBadge.textContent.includes('TES 0.98'), 'E2E: TES score badge displayed');

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
        doc.getElementById('output-saving-pct').textContent === 'Deep Architect',
        `Deep Architect: Saving pill shows "Deep Architect" (got "${doc.getElementById('output-saving-pct').textContent}")`
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

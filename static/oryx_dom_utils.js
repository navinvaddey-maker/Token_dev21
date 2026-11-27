//! static/oryx_dom_utils.js — Shared DOM & Prismatic 3D Emblem utilities for Oryx Studio & Admin
//! ARCH: Presentation & Local Companion (§1) — Single source of truth for HTML entity escaping
//! and hover-only GPU 3D emblem tilt across `index.html` and `admin.html`.

/**
 * Shared DOM utilities for HTML entity escaping and hover-only 3D emblem interactions.
 */
class OryxDomUtils {
    /**
     * Escapes HTML special characters in untrusted strings.
     * Matches app.js apostrophe entity (&#39;) for 100% test compatibility.
     *
     * # Arguments
     * * `value` - Any input value (string, number, null, undefined)
     *
     * # Panics
     * Never throws; coerces non-null values safely via `String(value)`.
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

/**
 * NeuronAnimation (NA-UI Extension — High Efficiency, Low RAM)
 * Lightweight canvas particle animation designed for zero frame drops and minimal memory.
 * Architecture: UI / Web Gateway (§1 ARCHITECTURE_v3.md)
 */

class NeuronAnimation {
    constructor(canvasId) {
        this.canvas = document.getElementById(canvasId);
        if (!this.canvas) return;
        this.ctx = this.canvas.getContext('2d', { alpha: true });
        this.particles = [];
        // Low particle count (14 vs 80) reduces pairwise distance checks from 3,160 to 91 per frame
        this.numberOfParticles = 14;
        this.connectionDistance = 120;
        this.animationId = null;
        this.boundResize = () => this.resize();

        window.addEventListener('resize', this.boundResize, { passive: true });
        this.resize();
    }

    resize() {
        if (!this.canvas) return;
        const dpr = Math.min(window.devicePixelRatio || 1, 2);
        this.width = window.innerWidth;
        this.height = window.innerHeight;
        this.canvas.width = this.width * dpr;
        this.canvas.height = this.height * dpr;
        if (this.ctx) {
            this.ctx.resetTransform?.();
            this.ctx.scale(dpr, dpr);
        }
    }

    init() {
        if (!this.canvas) return;
        this.particles = [];
        const w = this.width || window.innerWidth;
        const h = this.height || window.innerHeight;

        for (let i = 0; i < this.numberOfParticles; i++) {
            this.particles.push({
                x: Math.random() * w,
                y: Math.random() * h,
                vx: (Math.random() - 0.5) * 0.5,
                vy: (Math.random() - 0.5) * 0.5,
                size: Math.random() * 1.5 + 1.2
            });
        }
    }

    draw() {
        if (!this.ctx || !this.canvas) return;
        const w = this.width || window.innerWidth;
        const h = this.height || window.innerHeight;

        this.ctx.clearRect(0, 0, w, h);

        const len = this.particles.length;
        for (let i = 0; i < len; i++) {
            const p = this.particles[i];

            p.x += p.vx;
            p.y += p.vy;

            if (p.x < 0 || p.x > w) p.vx *= -1;
            if (p.y < 0 || p.y > h) p.vy *= -1;

            // Draw Node
            this.ctx.beginPath();
            this.ctx.arc(p.x, p.y, p.size, 0, Math.PI * 2);
            this.ctx.fillStyle = 'rgba(0, 242, 254, 0.45)';
            this.ctx.fill();

            // Connections (low particle count = 91 iterations max)
            for (let j = i + 1; j < len; j++) {
                const p2 = this.particles[j];
                const dx = p.x - p2.x;
                const dy = p.y - p2.y;
                const distSq = dx * dx + dy * dy;
                const maxDistSq = this.connectionDistance * this.connectionDistance;

                if (distSq < maxDistSq) {
                    const opacity = (1 - Math.sqrt(distSq) / this.connectionDistance) * 0.22;
                    this.ctx.beginPath();
                    this.ctx.moveTo(p.x, p.y);
                    this.ctx.lineTo(p2.x, p2.y);
                    this.ctx.strokeStyle = `rgba(0, 242, 254, ${opacity})`;
                    this.ctx.lineWidth = 0.75;
                    this.ctx.stroke();
                }
            }
        }

        this.animationId = requestAnimationFrame(() => this.draw());
    }

    start() {
        if (this.animationId) return;
        this.init();
        this.draw();
    }

    stop() {
        if (this.animationId) {
            cancelAnimationFrame(this.animationId);
            this.animationId = null;
        }
        if (this.ctx && this.canvas) {
            const w = this.width || window.innerWidth;
            const h = this.height || window.innerHeight;
            this.ctx.clearRect(0, 0, w, h);
        }
        if (this.boundResize) {
            window.removeEventListener('resize', this.boundResize);
        }
    }
}

window.NeuronAnimation = NeuronAnimation;

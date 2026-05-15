/**
 * NeuroAmbient UI Architecture (NA-UI)
 * Handles Dynamic Time & Climate-based CSS Theme switching.
 */

class NeuroAmbientSystem {
    constructor() {
        this.timeState = 'morning';
        this.climateState = 'clear';
        this.isCustomActive = false;
        
        // Configuration
        this.updateIntervalMs = 60 * 1000 * 15; // 15 mins
        this.themeKeys = ['morning', 'afternoon', 'night', 'clear', 'rain', 'cloudy', 'snow'];
    }

    init() {
        console.log("NeuroAmbient UI initialized");
        
        // 1. Check for Custom Skin
        const customSkin = localStorage.getItem('na_ui_custom_skin');
        if (customSkin) {
            this.applyCustomSkin(JSON.parse(customSkin));
        } else {
            // 2. Otherwise start dynamic sensing
            this.performSensingCycle();
            setInterval(() => this.performSensingCycle(), this.updateIntervalMs);
        }

        this.bindCustomModal();
    }

    async performSensingCycle() {
        if (this.isCustomActive) return;

        this.detectTimeOfDay();
        
        try {
            await this.detectClimate();
        } catch (e) {
            console.warn("NeuroAmbient: Climate sensing failed or denied, relying on Time.", e);
        }

        this.applyTheme();
    }

    detectTimeOfDay() {
        const hour = new Date().getHours();
        
        if (hour >= 5 && hour < 12) {
            this.timeState = 'morning';
        } else if (hour >= 12 && hour < 18) {
            this.timeState = 'afternoon';
        } else {
            this.timeState = 'night';
        }
    }

    async detectClimate() {
        // We prompt for geolocation softly. If denied, we skip.
        return new Promise((resolve, reject) => {
            if (!navigator.geolocation) {
                reject("Geolocation not supported");
                return;
            }

            navigator.geolocation.getCurrentPosition(async (pos) => {
                const lat = pos.coords.latitude;
                const lon = pos.coords.longitude;
                
                try {
                    // Open-Meteo is free and requires no API key
                    const res = await fetch(`https://api.open-meteo.com/v1/forecast?latitude=${lat}&longitude=${lon}&current_weather=true`);
                    const data = await res.json();
                    
                    if (data && data.current_weather) {
                        const code = data.current_weather.weathercode;
                        this.climateState = this.mapWeatherCode(code);
                    }
                    resolve();
                } catch (e) {
                    reject(e);
                }
            }, (err) => {
                reject(err);
            });
        });
    }

    mapWeatherCode(code) {
        // WMO Weather interpretation codes (Open-Meteo)
        if (code === 0 || code === 1) return 'clear';
        if (code === 2 || code === 3) return 'cloudy';
        if (code >= 51 && code <= 67) return 'rain';
        if (code >= 71 && code <= 86) return 'snow';
        if (code >= 95) return 'rain'; // thunderstorm
        return 'clear';
    }

    applyTheme() {
        // Clean old themes
        this.themeKeys.forEach(t => document.body.classList.remove(`theme-${t}`));
        
        // Apply new composite theme
        document.body.classList.add(`theme-${this.timeState}`);
        
        // Add climate modifier if it's impactful (like rain/snow/cloudy)
        if (this.climateState !== 'clear') {
            document.body.classList.add(`theme-${this.climateState}`);
        }

        // Apply background bubbles specific styling
        this.updateBubbles();
    }

    updateBubbles() {
        const b1 = document.querySelector('.bubble-1');
        const b2 = document.querySelector('.bubble-2');
        if (!b1 || !b2) return;

        // Reset inline styles
        b1.style.background = '';
        b2.style.background = '';
    }

    /* ── Custom Skin Creator Logic ── */
    
    applyCustomSkin(config) {
        this.isCustomActive = true;
        
        // Remove dynamic classes
        this.themeKeys.forEach(t => document.body.classList.remove(`theme-${t}`));

        const root = document.documentElement;
        root.style.setProperty('--page-bg', config.bg);
        root.style.setProperty('--glass-bg', config.glassBg);
        root.style.setProperty('--btn-grad', config.btnGrad);
        root.style.setProperty('--btn-text', config.btnText);
        root.style.setProperty('--head-text', config.headText);
        root.style.setProperty('--body-text', config.bodyText);
        
        // Bubbles
        const b1 = document.querySelector('.bubble-1');
        const b2 = document.querySelector('.bubble-2');
        if(b1) b1.style.background = config.bubble1;
        if(b2) b2.style.background = config.bubble2;
    }

    resetToDynamic() {
        this.isCustomActive = false;
        localStorage.removeItem('na_ui_custom_skin');
        
        // Remove inline styles from root
        const root = document.documentElement;
        ['--page-bg', '--glass-bg', '--btn-grad', '--btn-text', '--head-text', '--body-text'].forEach(prop => {
            root.style.removeProperty(prop);
        });

        this.performSensingCycle();
    }

    bindCustomModal() {
        const btn = document.getElementById('btn-custom-skin');
        const modal = document.getElementById('skin-modal');
        const closeBtn = document.getElementById('skin-modal-close');
        const applyBtn = document.getElementById('skin-modal-apply');
        const resetBtn = document.getElementById('skin-modal-reset');

        if (!btn || !modal) return;

        btn.addEventListener('click', () => {
            modal.style.display = 'flex';
        });

        closeBtn.addEventListener('click', () => {
            modal.style.display = 'none';
        });

        applyBtn.addEventListener('click', () => {
            const bgType = document.getElementById('skin-bg-type').value;
            const primaryColor = document.getElementById('skin-primary').value;
            
            let bg, glassBg, btnGrad, bubble1, bubble2;

            if (bgType === 'dark') {
                bg = '#121212';
                glassBg = 'rgba(30, 30, 30, 0.7)';
                btnGrad = `linear-gradient(135deg, ${primaryColor}, #555)`;
                bubble1 = `radial-gradient(circle at 35% 35%, ${primaryColor}, transparent)`;
                bubble2 = `radial-gradient(circle at 40% 30%, #444, transparent)`;
            } else if (bgType === 'light') {
                bg = '#f8f9fa';
                glassBg = 'rgba(255, 255, 255, 0.7)';
                btnGrad = `linear-gradient(135deg, ${primaryColor}, #aaa)`;
                bubble1 = `radial-gradient(circle at 35% 35%, ${primaryColor}, transparent)`;
                bubble2 = `radial-gradient(circle at 40% 30%, #ddd, transparent)`;
            }

            const config = {
                bg,
                glassBg,
                btnGrad,
                btnText: '#fff',
                headText: bgType === 'dark' ? '#eee' : '#111',
                bodyText: bgType === 'dark' ? '#bbb' : '#666',
                bubble1,
                bubble2
            };

            localStorage.setItem('na_ui_custom_skin', JSON.stringify(config));
            this.applyCustomSkin(config);
            modal.style.display = 'none';
        });

        resetBtn.addEventListener('click', () => {
            this.resetToDynamic();
            modal.style.display = 'none';
        });
    }
}

// Initialize on load
document.addEventListener('DOMContentLoaded', () => {
    window.neuroAmbient = new NeuroAmbientSystem();
    window.neuroAmbient.init();
});

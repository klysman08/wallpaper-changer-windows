/* ─── NAVBAR SCROLL ─── */
const navbar = document.getElementById('navbar');
window.addEventListener('scroll', () => {
  navbar.classList.toggle('scrolled', window.scrollY > 20);
}, { passive: true });

/* ─── MOBILE NAV TOGGLE ─── */
const navToggle = document.getElementById('navToggle');
const navLinks  = document.querySelector('.nav-links');

navToggle.addEventListener('click', () => {
  navLinks.classList.toggle('open');
});

document.querySelectorAll('.nav-links a').forEach(link => {
  link.addEventListener('click', () => navLinks.classList.remove('open'));
});

/* ─── PREVIEW TABS ─── */
document.querySelectorAll('.tab-btn').forEach(btn => {
  btn.addEventListener('click', () => {
    const target = btn.dataset.tab;

    document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
    document.querySelectorAll('.preview-panel').forEach(p => p.classList.remove('active'));

    btn.classList.add('active');
    document.getElementById(`tab-${target}`).classList.add('active');
  });
});

/* ─── INSTALL TABS ─── */
document.querySelectorAll('.install-tab').forEach(btn => {
  btn.addEventListener('click', () => {
    const target = btn.dataset.install;

    document.querySelectorAll('.install-tab').forEach(b => b.classList.remove('active'));
    document.querySelectorAll('.install-panel').forEach(p => p.classList.remove('active'));

    btn.classList.add('active');
    document.getElementById(`install-${target}`).classList.add('active');
  });
});

/* ─── COPY BUTTONS ─── */
document.querySelectorAll('.copy-btn').forEach(btn => {
  btn.addEventListener('click', async () => {
    const targetId = btn.dataset.target;
    const pre = document.getElementById(targetId);
    if (!pre) return;

    const text = pre.innerText || pre.textContent;

    try {
      await navigator.clipboard.writeText(text);
      btn.textContent = 'Copied!';
      btn.classList.add('copied');
      setTimeout(() => {
        btn.textContent = 'Copy';
        btn.classList.remove('copied');
      }, 2000);
    } catch {
      // Fallback for older browsers
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.select();
      document.execCommand('copy');
      document.body.removeChild(ta);
      btn.textContent = 'Copied!';
      btn.classList.add('copied');
      setTimeout(() => {
        btn.textContent = 'Copy';
        btn.classList.remove('copied');
      }, 2000);
    }
  });
});

/* ─── SCROLL ANIMATIONS ─── */
const animateTargets = () => {
  document.querySelectorAll(
    '.feature-card, .comparison-table, .install-steps, ' +
    '.cli-card, .hotkey-row, .timeline-item, .cta-card, .mock-window'
  ).forEach(el => {
    el.setAttribute('data-animate', '');
  });
};

const observer = new IntersectionObserver((entries) => {
  entries.forEach((entry, i) => {
    if (entry.isIntersecting) {
      // Stagger children within the same parent
      const siblings = Array.from(entry.target.parentElement.children);
      const delay = siblings.indexOf(entry.target) * 80;
      setTimeout(() => {
        entry.target.classList.add('visible');
      }, delay);
      observer.unobserve(entry.target);
    }
  });
}, { threshold: 0.1, rootMargin: '0px 0px -40px 0px' });

animateTargets();
document.querySelectorAll('[data-animate]').forEach(el => observer.observe(el));

/* ─── SMOOTH ACTIVE NAV LINK ─── */
const sections = document.querySelectorAll('section[id]');
const navLinkItems = document.querySelectorAll('.nav-links a[href^="#"]');

const sectionObserver = new IntersectionObserver((entries) => {
  entries.forEach(entry => {
    if (entry.isIntersecting) {
      const id = entry.target.id;
      navLinkItems.forEach(link => {
        link.style.color = link.getAttribute('href') === `#${id}`
          ? 'var(--text)'
          : '';
      });
    }
  });
}, { threshold: 0.4 });

sections.forEach(s => sectionObserver.observe(s));

/* ─── SAFE STORAGE WRAPPER ─── */
const SafeStorage = {
  data: {},
  getItem(key) {
    try {
      return localStorage.getItem(key);
    } catch (e) {
      return this.data[key] || null;
    }
  },
  setItem(key, value) {
    try {
      localStorage.setItem(key, value);
    } catch (e) {
      this.data[key] = value;
    }
  }
};

/* ─── RETRO AUDIO ENGINE (WEB AUDIO API) ─── */
const RetroAudio = {
  ctx: null,
  enabled: true,
  volume: 0.1, // Adjusted for clear but pleasant sound

  init() {
    if (this.ctx) return;
    const AudioContextClass = window.AudioContext || window.webkitAudioContext;
    if (AudioContextClass) {
      try {
        this.ctx = new AudioContextClass();
      } catch (e) {
        console.warn("Failed to create AudioContext:", e);
      }
    }
  },

  resume() {
    try {
      this.init();
      if (this.ctx && this.ctx.state === 'suspended') {
        this.ctx.resume();
      }
    } catch (e) {
      console.warn("Failed to resume AudioContext:", e);
    }
  },

  playTone(freqs, duration, type = 'square', fadeOut = true) {
    if (!this.enabled) return;
    this.resume();
    if (!this.ctx) return;

    try {
      const now = this.ctx.currentTime;
      const osc = this.ctx.createOscillator();
      const gain = this.ctx.createGain();

      osc.type = type;
      osc.connect(gain);
      gain.connect(this.ctx.destination);

      // Volume envelope
      gain.gain.setValueAtTime(0, now);
      gain.gain.linearRampToValueAtTime(this.volume, now + 0.005);
      
      if (fadeOut) {
        gain.gain.setValueAtTime(this.volume, now + duration - 0.02);
        gain.gain.exponentialRampToValueAtTime(0.001, now + duration);
      } else {
        gain.gain.setValueAtTime(this.volume, now + duration);
      }

      // Frequency sweeps
      if (Array.isArray(freqs)) {
        if (freqs.length === 1) {
          osc.frequency.setValueAtTime(freqs[0], now);
        } else {
          osc.frequency.setValueAtTime(freqs[0], now);
          const step = duration / (freqs.length - 1);
          for (let i = 1; i < freqs.length; i++) {
            osc.frequency.setValueAtTime(freqs[i], now + i * step);
          }
        }
      } else {
        osc.frequency.setValueAtTime(freqs, now);
      }

      osc.start(now);
      osc.stop(now + duration + 0.05);
    } catch (e) {
      console.warn("Audio playback failed", e);
    }
  },

  playClick() {
    // Decaying triangular wave sound
    this.playTone([500, 250, 100], 0.08, 'triangle');
  },

  playHover() {
    // Extremely subtle high tick sound
    const oldVol = this.volume;
    this.volume = 0.02;
    this.playTone([800, 1000], 0.02, 'sine');
    this.volume = oldVol;
  },

  playCoin() {
    // Classic Mario-style coin chime (B5 -> E6)
    if (!this.enabled) return;
    this.resume();
    if (!this.ctx) return;

    try {
      const now = this.ctx.currentTime;
      
      const osc1 = this.ctx.createOscillator();
      const gain1 = this.ctx.createGain();
      osc1.type = 'square';
      osc1.frequency.setValueAtTime(987.77, now); // B5
      osc1.connect(gain1);
      gain1.connect(this.ctx.destination);
      gain1.gain.setValueAtTime(this.volume, now);
      gain1.gain.setValueAtTime(this.volume, now + 0.08);
      gain1.gain.exponentialRampToValueAtTime(0.001, now + 0.09);
      osc1.start(now);
      osc1.stop(now + 0.1);

      const osc2 = this.ctx.createOscillator();
      const gain2 = this.ctx.createGain();
      osc2.type = 'square';
      osc2.frequency.setValueAtTime(1318.51, now + 0.08); // E6
      osc2.connect(gain2);
      gain2.connect(this.ctx.destination);
      gain2.gain.setValueAtTime(0, now);
      gain2.gain.setValueAtTime(this.volume, now + 0.08);
      gain2.gain.setValueAtTime(this.volume, now + 0.25);
      gain2.gain.exponentialRampToValueAtTime(0.001, now + 0.3);
      osc2.start(now + 0.08);
      osc2.stop(now + 0.35);
    } catch (e) {
      console.warn(e);
    }
  },

  playToggle(on) {
    // Rising arpeggio for ON, falling for OFF
    if (on) {
      this.playTone([300, 450, 600, 800], 0.15, 'triangle');
    } else {
      this.playTone([800, 600, 450, 300], 0.15, 'triangle');
    }
  }
};

/* ─── INITIALIZE USER PREFERENCES (THEME & SOUND) ─── */
const htmlEl = document.documentElement;
const themeToggle = document.getElementById('themeToggle');
const soundToggle = document.getElementById('soundToggle');

if (themeToggle && soundToggle) {
  const sunIcon = themeToggle.querySelector('.sun-icon');
  const moonIcon = themeToggle.querySelector('.moon-icon');
  const soundOnIcon = soundToggle.querySelector('.sound-on-icon');
  const soundOffIcon = soundToggle.querySelector('.sound-off-icon');

  // 1. Theme Configuration
  const currentTheme = SafeStorage.getItem('theme') || 'light';
  if (currentTheme === 'dark') {
    htmlEl.setAttribute('data-theme', 'dark');
    if (sunIcon) sunIcon.style.display = 'none';
    if (moonIcon) moonIcon.style.display = 'block';
  }

  themeToggle.addEventListener('click', () => {
    const isDark = htmlEl.getAttribute('data-theme') === 'dark';
    if (isDark) {
      htmlEl.removeAttribute('data-theme');
      SafeStorage.setItem('theme', 'light');
      if (sunIcon) sunIcon.style.display = 'block';
      if (moonIcon) moonIcon.style.display = 'none';
      RetroAudio.playToggle(false);
    } else {
      htmlEl.setAttribute('data-theme', 'dark');
      SafeStorage.setItem('theme', 'dark');
      if (sunIcon) sunIcon.style.display = 'none';
      if (moonIcon) moonIcon.style.display = 'block';
      RetroAudio.playToggle(true);
    }
  });

  // 2. Sound Configuration
  const soundMuted = SafeStorage.getItem('sound-muted') === 'true';
  if (soundMuted) {
    RetroAudio.enabled = false;
    if (soundOnIcon) soundOnIcon.style.display = 'none';
    if (soundOffIcon) soundOffIcon.style.display = 'block';
  }

  soundToggle.addEventListener('click', () => {
    RetroAudio.enabled = !RetroAudio.enabled;
    SafeStorage.setItem('sound-muted', (!RetroAudio.enabled).toString());
    
    if (RetroAudio.enabled) {
      if (soundOnIcon) soundOnIcon.style.display = 'block';
      if (soundOffIcon) soundOffIcon.style.display = 'none';
      RetroAudio.playToggle(true);
    } else {
      // Play toggle-off sound right before disabling
      RetroAudio.enabled = true;
      RetroAudio.playToggle(false);
      // Tiny delay to allow audio buffer scheduling before disabling engine
      setTimeout(() => {
        RetroAudio.enabled = false;
        if (soundOnIcon) soundOnIcon.style.display = 'none';
        if (soundOffIcon) soundOffIcon.style.display = 'block';
      }, 150);
    }
  });

  // 3. Global Keyboard Shortcuts
  window.addEventListener('keydown', (e) => {
    if (document.activeElement.tagName === 'INPUT' || document.activeElement.tagName === 'TEXTAREA') {
      return;
    }
    const key = e.key.toLowerCase();
    if (key === 't') {
      themeToggle.click();
    } else if (key === 's') {
      soundToggle.click();
    }
  });
}

// 4. Attach Retro Audio Events to Interactive elements
const setupRetroAudioEvents = () => {
  try {
    const selectors = 'a, button, .tab-btn, .install-tab, .mock-btn, .mock-checkbox, .num-btns button, .mock-toggle-group span, .menu-item';
    document.querySelectorAll(selectors).forEach(el => {
      if (el.id === 'themeToggle' || el.id === 'soundToggle' || el.classList.contains('copy-btn')) {
        return;
      }
      el.addEventListener('click', () => {
        RetroAudio.playClick();
      });
    });

    // Attach hover sounds
    document.querySelectorAll(selectors).forEach(el => {
      el.addEventListener('mouseenter', () => {
        RetroAudio.playHover();
      });
    });

    // Special Coin sound for copy button clicks
    document.querySelectorAll('.copy-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        RetroAudio.playCoin();
      });
    });
  } catch (e) {
    console.warn("Failed to set up audio events:", e);
  }
};

// Resume Audio Context on initial user interaction to conform with modern browser policies
const resumeAudioOnInteraction = () => {
  RetroAudio.resume();
  ['click', 'keydown', 'touchstart'].forEach(evtType => {
    document.removeEventListener(evtType, resumeAudioOnInteraction);
  });
};
['click', 'keydown', 'touchstart'].forEach(evtType => {
  document.addEventListener(evtType, resumeAudioOnInteraction);
});

// Run layout setup
setupRetroAudioEvents();

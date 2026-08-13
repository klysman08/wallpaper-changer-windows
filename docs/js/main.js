/**
 * WallpaperChanger Documentation Website
 * Modern Interactive Logic: Smooth Acoustic UI Sound, Multi-Tab Mockup Engine, Real Image Shuffler & Collage Grid
 */

// ─── 1. SMOOTH MODERN WEB AUDIO ENGINE ───
const SoundEngine = {
  ctx: null,
  enabled: true,
  volume: 0.045, // Warm, gentle, unobtrusive

  init() {
    if (this.ctx) return;
    const AudioContextClass = window.AudioContext || window.webkitAudioContext;
    if (AudioContextClass) {
      this.ctx = new AudioContextClass();
    }
  },

  resume() {
    this.init();
    if (this.ctx && this.ctx.state === 'suspended') {
      this.ctx.resume();
    }
  },

  // Smooth tactile UI tap (like modern macOS / iOS / Linear haptic button tap)
  playClick() {
    if (!this.enabled) return;
    this.resume();
    if (!this.ctx) return;

    try {
      const now = this.ctx.currentTime;
      const osc = this.ctx.createOscillator();
      const gain = this.ctx.createGain();
      const filter = this.ctx.createBiquadFilter();

      // Low-pass filter for a smooth, warm organic tap
      filter.type = 'lowpass';
      filter.frequency.setValueAtTime(600, now);
      filter.frequency.exponentialRampToValueAtTime(100, now + 0.035);

      osc.type = 'sine';
      osc.frequency.setValueAtTime(220, now);
      osc.frequency.exponentialRampToValueAtTime(70, now + 0.035);

      gain.gain.setValueAtTime(this.volume, now);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.035);

      osc.connect(filter);
      filter.connect(gain);
      gain.connect(this.ctx.destination);

      osc.start(now);
      osc.stop(now + 0.04);
    } catch (e) {
      console.warn('Click audio error', e);
    }
  },

  // Soft harmonic chime for success / copy
  playSuccess() {
    if (!this.enabled) return;
    this.resume();
    if (!this.ctx) return;

    try {
      const now = this.ctx.currentTime;
      const chord = [523.25, 659.25, 783.99]; // C5, E5, G5 smooth triad

      chord.forEach((freq, i) => {
        const osc = this.ctx.createOscillator();
        const gain = this.ctx.createGain();

        osc.type = 'sine';
        osc.frequency.setValueAtTime(freq, now + i * 0.04);

        const startTime = now + i * 0.04;
        const duration = 0.22;

        gain.gain.setValueAtTime(0, startTime);
        gain.gain.linearRampToValueAtTime(this.volume * 0.6, startTime + 0.01);
        gain.gain.exponentialRampToValueAtTime(0.0001, startTime + duration);

        osc.connect(gain);
        gain.connect(this.ctx.destination);

        osc.start(startTime);
        osc.stop(startTime + duration + 0.02);
      });
    } catch (e) {
      console.warn('Success audio error', e);
    }
  },

  // Smooth switch toggle
  playToggle(on) {
    if (!this.enabled) return;
    this.resume();
    if (!this.ctx) return;

    try {
      const now = this.ctx.currentTime;
      const startFreq = on ? 260 : 360;
      const endFreq = on ? 360 : 260;

      const osc = this.ctx.createOscillator();
      const gain = this.ctx.createGain();

      osc.type = 'sine';
      osc.frequency.setValueAtTime(startFreq, now);
      osc.frequency.exponentialRampToValueAtTime(endFreq, now + 0.045);

      gain.gain.setValueAtTime(this.volume * 0.7, now);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.045);

      osc.connect(gain);
      gain.connect(this.ctx.destination);

      osc.start(now);
      osc.stop(now + 0.05);
    } catch (e) {
      console.warn('Toggle audio error', e);
    }
  },
};

document.addEventListener('DOMContentLoaded', () => {
  // ─── 2. SOUND TOGGLE INITIALIZATION ───
  const soundToggle = document.getElementById('soundToggle');
  const soundOnIcon = document.getElementById('soundOnIcon');
  const soundOffIcon = document.getElementById('soundOffIcon');

  const soundMuted = localStorage.getItem('sound-muted') === 'true';
  if (soundMuted) {
    SoundEngine.enabled = false;
    if (soundOnIcon) soundOnIcon.style.display = 'none';
    if (soundOffIcon) soundOffIcon.style.display = 'block';
  }

  if (soundToggle) {
    soundToggle.addEventListener('click', () => {
      SoundEngine.enabled = !SoundEngine.enabled;
      localStorage.setItem('sound-muted', (!SoundEngine.enabled).toString());

      if (SoundEngine.enabled) {
        if (soundOnIcon) soundOnIcon.style.display = 'block';
        if (soundOffIcon) soundOffIcon.style.display = 'none';
        SoundEngine.playToggle(true);
      } else {
        SoundEngine.enabled = true;
        SoundEngine.playToggle(false);
        setTimeout(() => {
          SoundEngine.enabled = false;
          if (soundOnIcon) soundOnIcon.style.display = 'none';
          if (soundOffIcon) soundOffIcon.style.display = 'block';
        }, 80);
      }
    });
  }

  // ─── 3. THEME TOGGLING ───
  const themeToggle = document.getElementById('themeToggle');
  const sunIcon = document.getElementById('sunIcon');
  const moonIcon = document.getElementById('moonIcon');

  function updateThemeIcons(isDark) {
    if (sunIcon && moonIcon) {
      sunIcon.style.display = isDark ? 'none' : 'block';
      moonIcon.style.display = isDark ? 'block' : 'none';
    }
  }

  function setTheme(theme) {
    const isDark = theme === 'dark';
    if (isDark) {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
    localStorage.setItem('theme', theme);
    updateThemeIcons(isDark);
    SoundEngine.playToggle(isDark);
  }

  const currentIsDark = document.documentElement.classList.contains('dark');
  updateThemeIcons(currentIsDark);

  if (themeToggle) {
    themeToggle.addEventListener('click', () => {
      const isCurrentlyDark = document.documentElement.classList.contains('dark');
      setTheme(isCurrentlyDark ? 'light' : 'dark');
    });
  }

  // ─── 4. GLOBAL SHORTCUTS (T = Theme, S = Sound) ───
  window.addEventListener('keydown', (e) => {
    if (
      document.activeElement &&
      (document.activeElement.tagName === 'INPUT' || document.activeElement.tagName === 'TEXTAREA')
    ) {
      return;
    }
    const key = e.key.toLowerCase();
    if (key === 't' && themeToggle) {
      themeToggle.click();
    } else if (key === 's' && soundToggle) {
      soundToggle.click();
    }
  });

  // ─── 5. MOBILE NAVIGATION DRAWER ───
  const navToggle = document.getElementById('navToggle');
  const mobileNavOverlay = document.getElementById('mobileNavOverlay');
  const mobileNavLinks = document.querySelectorAll('.mobile-nav-links a');

  function toggleMobileNav() {
    document.body.classList.toggle('mobile-nav-open');
    SoundEngine.playClick();
  }

  function closeMobileNav() {
    document.body.classList.remove('mobile-nav-open');
  }

  if (navToggle) navToggle.addEventListener('click', toggleMobileNav);
  if (mobileNavOverlay) mobileNavOverlay.addEventListener('click', closeMobileNav);
  mobileNavLinks.forEach((link) => link.addEventListener('click', closeMobileNav));

  // ─── 6. DESKTOP MOCKUP INTERNAL TAB SWITCHER ───
  const mockupNavBtns = document.querySelectorAll('.mockup-nav-btn');
  const mockupPanels = document.querySelectorAll('.mockup-tab-panel');

  function switchMockupPanel(targetTab) {
    SoundEngine.playClick();
    mockupNavBtns.forEach((btn) => {
      btn.classList.toggle('active', btn.dataset.mockTab === targetTab);
    });
    mockupPanels.forEach((panel) => {
      panel.classList.toggle('active', panel.id === `mock-panel-${targetTab}`);
    });
  }

  mockupNavBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      const tabId = btn.dataset.mockTab;
      if (tabId) {
        switchMockupPanel(tabId);
      }
    });
  });

  // ─── 7. PREVIEW SECTION TABS SWITCHER ───
  const previewTabBtns = document.querySelectorAll('.preview-tab-btn');
  const previewPanels = document.querySelectorAll('.preview-panel');

  function switchPreviewTab(targetTab) {
    SoundEngine.playClick();
    previewTabBtns.forEach((b) => {
      b.classList.toggle('active', b.dataset.tab === targetTab);
    });
    previewPanels.forEach((p) => {
      p.classList.toggle('active', p.id === `tab-${targetTab}`);
    });
  }

  previewTabBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      switchPreviewTab(btn.dataset.tab);
    });
  });

  // ─── 8. REAL WALLPAPERS ROTATION & STAGE INTERACTION ───
  const sampleImages = [
    'images/example 1 (1).jpg',
    'images/example 1 (2).jpg',
    'images/example 1 (3).jpg',
    'images/example 1 (4).jpg',
    'images/example 1 (5).jpg',
  ];

  let currentImageIndex = 0;
  const stageMonitors = document.querySelectorAll('.stage-monitor');
  const monitorChips = document.querySelectorAll('.mockup-chip[data-monitor]');
  const shuffleBtn = document.getElementById('mockShuffleBtn');
  const stageShuffleBtn = document.getElementById('mockStageShuffle');
  const applyBtn = document.getElementById('mockApplyBtn');
  const applyNowBtn = document.getElementById('mockApplyNowBtn');
  const prevBtn = document.getElementById('mockPrevBtn');
  const nextBtn = document.getElementById('mockNextBtn');

  function updateMonitorImages(offset = 0) {
    currentImageIndex = (currentImageIndex + offset + sampleImages.length) % sampleImages.length;
    stageMonitors.forEach((m, idx) => {
      const imgUrl = sampleImages[(currentImageIndex + idx) % sampleImages.length];
      m.style.backgroundImage = `url("${imgUrl}")`;
    });
  }

  // Initialize monitor backgrounds
  updateMonitorImages(0);

  function handleShuffle() {
    SoundEngine.playClick();
    updateMonitorImages(1);
  }

  if (shuffleBtn) shuffleBtn.addEventListener('click', handleShuffle);
  if (stageShuffleBtn) stageShuffleBtn.addEventListener('click', handleShuffle);
  if (nextBtn) nextBtn.addEventListener('click', handleShuffle);
  if (prevBtn) {
    prevBtn.addEventListener('click', () => {
      SoundEngine.playClick();
      updateMonitorImages(-1);
    });
  }

  function handleApply(btn) {
    if (!btn) return;
    SoundEngine.playSuccess();
    const originalText = btn.innerHTML;
    btn.innerHTML = `✓ Applied!`;
    setTimeout(() => {
      btn.innerHTML = originalText;
    }, 1500);
  }

  if (applyBtn) applyBtn.addEventListener('click', () => handleApply(applyBtn));
  if (applyNowBtn) applyNowBtn.addEventListener('click', () => handleApply(applyNowBtn));

  // Monitor focus click
  stageMonitors.forEach((m) => {
    m.addEventListener('click', () => {
      SoundEngine.playClick();
      const monId = m.dataset.monitor;
      monitorChips.forEach((c) => c.classList.toggle('active', c.dataset.monitor === monId));
      stageMonitors.forEach((s) => s.classList.toggle('is-focused', s.dataset.monitor === monId));
    });
  });

  monitorChips.forEach((chip) => {
    chip.addEventListener('click', () => {
      SoundEngine.playClick();
      const monId = chip.dataset.monitor;
      monitorChips.forEach((c) => c.classList.remove('active'));
      chip.classList.add('active');

      stageMonitors.forEach((m) => {
        if (monId === 'all') {
          m.classList.add('is-focused');
        } else {
          m.classList.toggle('is-focused', m.dataset.monitor === monId);
        }
      });
    });
  });

  // ─── 9. HERO COLLAGE GRID VIEW CONTROLS ───
  const heroCollageCounts = document.querySelectorAll('.hero-collage-count');
  const heroCollageGrid = document.getElementById('heroCollageGrid');
  const heroFitBtns = document.querySelectorAll('.hero-fit-btn');
  const heroCollageCountBadge = document.getElementById('heroCollageCountBadge');
  const heroFitBadge = document.getElementById('heroFitBadge');
  const mockCollageShuffleBtn = document.getElementById('mockCollageShuffleBtn');
  const mockCollageNextBtn = document.getElementById('mockCollageNextBtn');
  const mockCollagePrevBtn = document.getElementById('mockCollagePrevBtn');
  const mockCollageApplyBtn = document.getElementById('mockCollageApplyBtn');
  const mockCollageSaveBtn = document.getElementById('mockCollageSaveBtn');

  function updateHeroCollageGrid(count) {
    if (!heroCollageGrid) return;
    heroCollageGrid.className = `collage-demo-grid grid-${count}`;
    const cells = heroCollageGrid.querySelectorAll('.collage-cell-img');
    const num = parseInt(count, 10);
    cells.forEach((c, idx) => {
      c.style.display = idx < num ? 'block' : 'none';
    });
    if (heroCollageCountBadge) {
      heroCollageCountBadge.textContent = `${count} ${num === 1 ? 'image' : 'images'}`;
    }
  }

  heroCollageCounts.forEach((btn) => {
    btn.addEventListener('click', () => {
      SoundEngine.playClick();
      heroCollageCounts.forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
      updateHeroCollageGrid(btn.dataset.count);
    });
  });

  heroFitBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      SoundEngine.playClick();
      heroFitBtns.forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
      const fit = btn.dataset.fit;
      if (heroFitBadge) {
        heroFitBadge.textContent = fit.charAt(0).toUpperCase() + fit.slice(1);
      }
      if (heroCollageGrid) {
        const cells = heroCollageGrid.querySelectorAll('.collage-cell-img');
        cells.forEach((c) => {
          c.className = c.className.replace(/fit-\w+/, `fit-${fit}`);
        });
      }
    });
  });

  function shuffleCollageImages() {
    SoundEngine.playClick();
    if (!heroCollageGrid) return;
    const cells = heroCollageGrid.querySelectorAll('.collage-cell-img');
    cells.forEach((c, i) => {
      const randomIdx = (currentImageIndex + i + 1) % sampleImages.length;
      c.src = sampleImages[randomIdx];
    });
    currentImageIndex = (currentImageIndex + 1) % sampleImages.length;
  }

  if (mockCollageShuffleBtn) mockCollageShuffleBtn.addEventListener('click', shuffleCollageImages);
  if (mockCollageNextBtn) mockCollageNextBtn.addEventListener('click', shuffleCollageImages);
  if (mockCollagePrevBtn) mockCollagePrevBtn.addEventListener('click', shuffleCollageImages);
  if (mockCollageApplyBtn) mockCollageApplyBtn.addEventListener('click', () => handleApply(mockCollageApplyBtn));
  if (mockCollageSaveBtn) mockCollageSaveBtn.addEventListener('click', () => handleApply(mockCollageSaveBtn));

  // ─── 10. PREVIEW SECTION COLLAGE BUTTONS ───
  const collageNumBtns = document.querySelectorAll('.collage-num-btn');
  const collageGrid = document.getElementById('collageDemoGrid');
  const tabFitBtns = document.querySelectorAll('.tab-fit-btn');
  const tabCollageCountBadge = document.getElementById('tabCollageCountBadge');
  const tabFitBadge = document.getElementById('tabFitBadge');

  collageNumBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      SoundEngine.playClick();
      const count = btn.dataset.count;
      collageNumBtns.forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');

      if (tabCollageCountBadge) {
        tabCollageCountBadge.textContent = `${count} ${parseInt(count, 10) === 1 ? 'image' : 'images'}`;
      }

      if (collageGrid) {
        collageGrid.className = `collage-demo-grid grid-${count}`;
        const cells = collageGrid.querySelectorAll('.collage-cell-img');
        cells.forEach((c, idx) => {
          c.style.display = idx < parseInt(count, 10) ? 'block' : 'none';
        });
      }
    });
  });

  tabFitBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      SoundEngine.playClick();
      tabFitBtns.forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
      const fit = btn.dataset.fit;
      if (tabFitBadge) {
        tabFitBadge.textContent = fit.charAt(0).toUpperCase() + fit.slice(1);
      }
      if (collageGrid) {
        const cells = collageGrid.querySelectorAll('.collage-cell-img');
        cells.forEach((c) => {
          c.className = c.className.replace(/fit-\w+/, `fit-${fit}`);
        });
      }
    });
  });

  // ─── 11. TRANSPARENCY SLIDER INTERACTION ───
  const opacitySlider = document.getElementById('mockOpacitySlider');
  const opacityVal = document.getElementById('mockOpacityVal');
  if (opacitySlider && opacityVal) {
    opacitySlider.addEventListener('input', (e) => {
      opacityVal.textContent = `${e.target.value}%`;
    });
  }

  // ─── 12. INSTALLATION TABS ───
  const installTabs = document.querySelectorAll('.install-tab-btn');
  const installPanels = document.querySelectorAll('.install-panel');

  installTabs.forEach((btn) => {
    btn.addEventListener('click', () => {
      SoundEngine.playClick();
      const target = btn.dataset.install;
      installTabs.forEach((b) => b.classList.remove('active'));
      installPanels.forEach((p) => p.classList.remove('active'));

      btn.classList.add('active');
      const targetPanel = document.getElementById(`install-${target}`);
      if (targetPanel) {
        targetPanel.classList.add('active');
      }
    });
  });

  // ─── 13. COPY BUTTONS WITH AUDIO FEEDBACK ───
  const copyButtons = document.querySelectorAll('.copy-btn');

  copyButtons.forEach((btn) => {
    btn.addEventListener('click', async () => {
      const targetId = btn.dataset.target;
      const targetEl = document.getElementById(targetId);
      if (!targetEl) return;

      const codeText = targetEl.innerText || targetEl.textContent;

      try {
        await navigator.clipboard.writeText(codeText.trim());
        SoundEngine.playSuccess();

        const originalHtml = btn.innerHTML;
        btn.innerHTML = `
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <polyline points="20 6 9 17 4 12"></polyline>
          </svg>
          Copied!
        `;
        btn.classList.add('copied');

        setTimeout(() => {
          btn.innerHTML = originalHtml;
          btn.classList.remove('copied');
        }, 2000);
      } catch (err) {
        console.error('Clipboard copy failed:', err);
      }
    });
  });

  // ─── 14. CLICK SOUNDS FOR INTERACTIVE BUTTONS (NO HOVER SOUNDS) ───
  const interactiveButtons = document.querySelectorAll('button:not(#soundToggle):not(#themeToggle):not(.copy-btn), a.btn');
  interactiveButtons.forEach((btn) => {
    btn.addEventListener('click', () => {
      SoundEngine.playClick();
    });
  });

  // Unlock audio context on first interaction
  const unlockAudio = () => {
    SoundEngine.resume();
    ['click', 'keydown', 'touchstart'].forEach((evt) => {
      document.removeEventListener(evt, unlockAudio);
    });
  };
  ['click', 'keydown', 'touchstart'].forEach((evt) => {
    document.addEventListener(evt, unlockAudio);
  });
});

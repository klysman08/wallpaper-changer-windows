/**
 * KokoroPaper Documentation Website
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

// The hero's Wallpaper Studio is a module, which cannot see this script's consts.
window.SoundEngine = SoundEngine;

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

  // ─── 6. PREVIEW SECTION TABS SWITCHER ───
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

  // ─── 7. PREVIEW STAGE COLLAGE ENGINE (#tab-monitors) ───
  // The hero's Wallpaper Studio (js/studio/) composes real pixels; this stage is the
  // lighter CSS sketch of the app's window further down the page.
  const sampleImages = [
    'images/example 1 (1).jpg',
    'images/example 1 (2).jpg',
    'images/example 1 (3).jpg',
    'images/example 1 (4).jpg',
    'images/example 1 (5).jpg',
    'images/example 1 (1).jpeg',
    'images/example 1 (2).jpeg',
    'images/example 1 (3).jpeg',
    'images/example 1 (4).jpeg',
    'images/example 1 (5).jpeg',
  ];

  let currentImageIndex = 0;

  let previewTabCollageCount = 4;
  let previewTabFitMode = 'fill';
  let previewTabEffect = 'normal';

  const previewTabStage = document.getElementById('previewTabStage');
  const previewTabThumbs = document.getElementById('previewTabThumbs');
  const previewTabImagesUsedLabel = document.getElementById('previewTabImagesUsedLabel');
  const previewTabShuffleBtn = document.getElementById('previewTabShuffle');
  const previewTabApplyBtn = document.getElementById('previewTabApply');
  const previewTabApplyNowBtn = document.getElementById('previewTabApplyNow');
  const previewTabSpinIcon = document.getElementById('previewTabSpinIcon');
  const previewTabPrevBtn = document.getElementById('previewTabPrev');
  const previewTabNextBtn = document.getElementById('previewTabNext');
  const previewTabRotateBtn = document.getElementById('previewTabRotate');
  const previewTabSameSwitch = document.getElementById('previewTabSameSwitch');
  let isPreviewTabRotating = false;

  function renderStage(stageEl, count, fit, effect, sameImages, offset) {
    if (!stageEl) return [];
    const monitors = stageEl.querySelectorAll('.stage-monitor');
    const usedImages = [];

    monitors.forEach((m, monitorIdx) => {
      m.style.backgroundImage = 'none';

      let collage = m.querySelector('.stage-monitor-collage');
      if (!collage) {
        collage = document.createElement('div');
        m.prepend(collage);
      }

      collage.className = `stage-monitor-collage grid-${count} effect-${effect}`;

      let cellsHtml = '';
      for (let c = 0; c < count; c++) {
        const imgIdx = sameImages
          ? (offset + c) % sampleImages.length
          : (offset + monitorIdx * count + c) % sampleImages.length;
        const imgUrl = sampleImages[imgIdx];
        if (!usedImages.includes(imgUrl)) {
          usedImages.push(imgUrl);
        }
        const slotNum = sameImages ? c + 1 : monitorIdx * count + c + 1;

        cellsHtml += `
          <div class="stage-collage-cell" data-slot="${slotNum}" title="Click cell to cycle picture">
            <img src="${imgUrl}" alt="Wallpaper slot ${slotNum}" class="stage-cell-img fit-${fit}" />
            <span class="stage-cell-badge">${slotNum}</span>
          </div>
        `;
      }
      collage.innerHTML = cellsHtml;

      // Click cell to cycle that specific cell's picture
      collage.querySelectorAll('.stage-collage-cell').forEach((cell) => {
        cell.addEventListener('click', (e) => {
          e.stopPropagation();
          SoundEngine.playClick();
          const img = cell.querySelector('img');
          if (img) {
            const currentSrc = img.getAttribute('src');
            let idx = sampleImages.indexOf(currentSrc);
            if (idx === -1) idx = 0;
            const nextSrc = sampleImages[(idx + 1) % sampleImages.length];
            img.src = nextSrc;
            img.style.transform = 'scale(0.92)';
            setTimeout(() => { img.style.transform = ''; }, 160);
          }
        });
      });
    });

    return usedImages;
  }

  function renderAllStages(offset = 0) {
    currentImageIndex = (currentImageIndex + offset + sampleImages.length) % sampleImages.length;

    const previewSame = previewTabSameSwitch ? previewTabSameSwitch.checked : false;
    const previewUsed = renderStage(previewTabStage, previewTabCollageCount, previewTabFitMode, previewTabEffect, previewSame, currentImageIndex);

    if (previewTabThumbs) {
      const thumbs = previewUsed.length > 0 ? previewUsed.slice(0, 6) : sampleImages.slice(0, 4);
      previewTabThumbs.innerHTML = thumbs.map((url, i) => `
        <img src="${url}" alt="Used Image ${i + 1}" class="mockup-stage-thumb" title="Click to rotate collage" />
      `).join('');
      previewTabThumbs.querySelectorAll('.mockup-stage-thumb').forEach((thumb) => {
        thumb.addEventListener('click', () => {
          SoundEngine.playClick();
          renderAllStages(1);
        });
      });
    }
    if (previewTabImagesUsedLabel) {
      const total = previewTabCollageCount * (previewSame ? 1 : 3);
      previewTabImagesUsedLabel.textContent = `Images used in collage (${total})`;
    }
  }

  renderAllStages(0);

  // Monitor focus and zoom transition
  function setupStageZoom(stageEl, chipsContainer) {
    if (!stageEl || !chipsContainer) return;
    let focusedMon = 'all';

    function setFocus(monId) {
      focusedMon = monId;
      chipsContainer.querySelectorAll('.mockup-chip[data-monitor]').forEach((c) => {
        c.classList.toggle('active', c.dataset.monitor === monId);
      });
      stageEl.querySelectorAll('.stage-monitor').forEach((m) => {
        if (monId === 'all') {
          m.classList.add('is-focused');
        } else {
          m.classList.toggle('is-focused', m.dataset.monitor === monId);
        }
      });
      stageEl.classList.remove('zoomed-1', 'zoomed-2', 'zoomed-3');
      if (monId === '1' || monId === '2' || monId === '3') {
        stageEl.classList.add(`zoomed-${monId}`);
      }
    }

    stageEl.querySelectorAll('.stage-monitor').forEach((m) => {
      m.addEventListener('click', () => {
        SoundEngine.playClick();
        const monId = m.dataset.monitor;
        setFocus(focusedMon === monId ? 'all' : monId);
      });
    });

    chipsContainer.querySelectorAll('.mockup-chip[data-monitor]').forEach((chip) => {
      chip.addEventListener('click', () => {
        SoundEngine.playClick();
        setFocus(chip.dataset.monitor);
      });
    });
  }

  setupStageZoom(previewTabStage, document.getElementById('previewTabChips'));

  // ─── 8. PREVIEW STAGE CONTROLS (#tab-monitors) ───
  const previewTabCounts = document.querySelectorAll('.preview-tab-count');
  const previewTabCountBadge = document.getElementById('previewTabCountBadge');
  const previewTabFitBtns = document.querySelectorAll('.preview-tab-fit');
  const previewTabFitBadge = document.getElementById('previewTabFitBadge');
  const previewTabEffectBtns = document.querySelectorAll('.preview-tab-effect');
  const previewTabEffectBadge = document.getElementById('previewTabEffectBadge');

  previewTabCounts.forEach((btn) => {
    btn.addEventListener('click', () => {
      SoundEngine.playClick();
      previewTabCounts.forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
      previewTabCollageCount = parseInt(btn.dataset.count, 10) || 1;
      if (previewTabCountBadge) {
        previewTabCountBadge.textContent = `${previewTabCollageCount} ${previewTabCollageCount === 1 ? 'image' : 'images'}`;
      }
      renderAllStages(0);
    });
  });

  previewTabFitBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      SoundEngine.playClick();
      previewTabFitBtns.forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
      previewTabFitMode = btn.dataset.fit || 'fill';
      if (previewTabFitBadge) {
        previewTabFitBadge.textContent = previewTabFitMode.charAt(0).toUpperCase() + previewTabFitMode.slice(1);
      }
      renderAllStages(0);
    });
  });

  previewTabEffectBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      SoundEngine.playClick();
      previewTabEffectBtns.forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
      previewTabEffect = btn.dataset.effect || 'normal';
      if (previewTabEffectBadge) {
        previewTabEffectBadge.textContent = previewTabEffect.toUpperCase();
      }
      renderAllStages(0);
    });
  });

  if (previewTabSameSwitch) {
    previewTabSameSwitch.addEventListener('change', () => {
      SoundEngine.playToggle(previewTabSameSwitch.checked);
      renderAllStages(0);
    });
  }

  if (previewTabShuffleBtn) previewTabShuffleBtn.addEventListener('click', () => { SoundEngine.playClick(); renderAllStages(1); });
  if (previewTabNextBtn) previewTabNextBtn.addEventListener('click', () => { SoundEngine.playClick(); renderAllStages(1); });
  if (previewTabPrevBtn) previewTabPrevBtn.addEventListener('click', () => { SoundEngine.playClick(); renderAllStages(-1); });

  if (previewTabApplyBtn) {
    previewTabApplyBtn.addEventListener('click', () => {
      SoundEngine.playSuccess();
      const orig = previewTabApplyBtn.innerHTML;
      previewTabApplyBtn.innerHTML = `✓ Applied!`;
      setTimeout(() => { previewTabApplyBtn.innerHTML = orig; }, 1500);
    });
  }

  if (previewTabApplyNowBtn) {
    previewTabApplyNowBtn.addEventListener('click', () => {
      SoundEngine.playSuccess();
      if (previewTabSpinIcon) {
        previewTabSpinIcon.style.animation = 'spin 0.6s linear';
        setTimeout(() => { previewTabSpinIcon.style.animation = ''; }, 600);
      }
      renderAllStages(1);
    });
  }

  if (previewTabRotateBtn) {
    previewTabRotateBtn.addEventListener('click', () => {
      SoundEngine.playClick();
      isPreviewTabRotating = !isPreviewTabRotating;
      if (isPreviewTabRotating) {
        previewTabRotateBtn.classList.remove('btn-secondary');
        previewTabRotateBtn.classList.add('btn-destructive');
        previewTabRotateBtn.innerHTML = `
          <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><rect width="4" height="16" x="6" y="4" rx="1"/><rect width="4" height="16" x="14" y="4" rx="1"/></svg>
          <span>Stop rotation</span>
        `;
      } else {
        previewTabRotateBtn.classList.remove('btn-destructive');
        previewTabRotateBtn.classList.add('btn-secondary');
        previewTabRotateBtn.innerHTML = `
          <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>
          <span>Start rotation</span>
        `;
      }
    });
  }

  // ─── 9. COLLAGE TAB CONTROLS ───
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
          c.className = c.className.replace(/fit-\w+/, '').trim() + ` fit-${fit}`;
          c.style.objectFit = fit === 'fill' ? 'cover' : fit === 'fit' ? 'contain' : fit === 'stretch' ? 'fill' : 'none';
        });
      }
    });
  });

  if (collageGrid) {
    collageGrid.querySelectorAll('.collage-cell-img').forEach((img) => {
      img.style.cursor = 'pointer';
      img.addEventListener('click', () => {
        SoundEngine.playClick();
        const currentSrc = img.getAttribute('src');
        let idx = sampleImages.indexOf(currentSrc);
        if (idx === -1) idx = 0;
        img.src = sampleImages[(idx + 1) % sampleImages.length];
        img.style.transform = 'scale(0.95)';
        setTimeout(() => { img.style.transform = ''; }, 160);
      });
    });
  }

  // ─── 10. PICTURE ADJUSTMENTS (desktop/src/components/wallpaper-tab.tsx) ───
  // Each Adjustments card names the stage it drives. The sliders become three
  // custom properties on that stage; style.css turns them into a filter on the
  // collage, a warm/cool tint and a vignette over each monitor.
  function adjustmentStyle(v) {
    const filters = [];
    if (v.brightness) filters.push(`brightness(${1 + v.brightness / 100})`);
    if (v.contrast) filters.push(`contrast(${1 + v.contrast / 100})`);
    if (v.saturation) filters.push(`saturate(${1 + v.saturation / 100})`);
    if (v.blur) filters.push(`blur(${(v.blur / 100) * 3}px)`);
    const w = v.warmth / 100;
    const tint = w > 0
      ? `rgba(255, 140, 40, ${w * 0.7})`
      : w < 0 ? `rgba(60, 140, 255, ${-w * 0.7})` : 'transparent';
    return {
      filter: filters.length ? filters.join(' ') : 'none',
      tint,
      vignette: String(v.vignette / 100),
    };
  }

  document.querySelectorAll('[data-adjust-for]').forEach((card) => {
    const stage = document.getElementById(card.dataset.adjustFor);
    const ranges = card.querySelectorAll('input[data-adjust]');
    if (!stage || !ranges.length) return;

    function update() {
      const values = {};
      ranges.forEach((r) => {
        values[r.dataset.adjust] = parseInt(r.value, 10) || 0;
        const out = r.closest('.mockup-adjust-row')?.querySelector('output');
        if (out) out.textContent = r.value > 0 && r.min < 0 ? `+${r.value}` : r.value;
      });
      const style = adjustmentStyle(values);
      stage.style.setProperty('--adj-filter', style.filter);
      stage.style.setProperty('--adj-tint', style.tint);
      stage.style.setProperty('--adj-vignette', style.vignette);
    }

    ranges.forEach((r) => {
      r.addEventListener('input', update);
      // Double-clicking the label resets that one slider, as in the app.
      const head = r.closest('.mockup-adjust-row')?.querySelector('.mockup-adjust-head');
      if (head) {
        head.addEventListener('dblclick', (e) => {
          e.preventDefault();
          r.value = 0;
          SoundEngine.playClick();
          update();
        });
      }
    });

    const reset = card.querySelector('.mock-adjust-reset');
    if (reset) {
      reset.addEventListener('click', () => {
        ranges.forEach((r) => { r.value = 0; });
        update();
      });
    }
    update();
  });

  // ─── 11. INSTALLATION TABS ───
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

  // ─── 12. COPY BUTTONS WITH AUDIO FEEDBACK ───
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

  // ─── 13. CLICK SOUNDS FOR INTERACTIVE BUTTONS (NO HOVER SOUNDS) ───
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

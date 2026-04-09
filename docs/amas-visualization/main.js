/* ============================================
   AMAS Algorithm Visualization — JavaScript
   ============================================ */

// ---- Particle Background ----
(function initParticles() {
  const canvas = document.getElementById('particles-canvas');
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  let particles = [];
  const PARTICLE_COUNT = 60;
  const CONNECT_DIST = 120;

  function resize() {
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
  }
  window.addEventListener('resize', resize);
  resize();

  class Particle {
    constructor() {
      this.reset();
    }
    reset() {
      this.x = Math.random() * canvas.width;
      this.y = Math.random() * canvas.height;
      this.vx = (Math.random() - 0.5) * 0.4;
      this.vy = (Math.random() - 0.5) * 0.4;
      this.r = Math.random() * 1.8 + 0.5;
      this.alpha = Math.random() * 0.4 + 0.1;
    }
    update() {
      this.x += this.vx;
      this.y += this.vy;
      if (this.x < 0 || this.x > canvas.width) this.vx *= -1;
      if (this.y < 0 || this.y > canvas.height) this.vy *= -1;
    }
    draw() {
      ctx.beginPath();
      ctx.arc(this.x, this.y, this.r, 0, Math.PI * 2);
      ctx.fillStyle = `rgba(96, 165, 250, ${this.alpha})`;
      ctx.fill();
    }
  }

  for (let i = 0; i < PARTICLE_COUNT; i++) {
    particles.push(new Particle());
  }

  function animate() {
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    for (let i = 0; i < particles.length; i++) {
      particles[i].update();
      particles[i].draw();
      for (let j = i + 1; j < particles.length; j++) {
        const dx = particles[i].x - particles[j].x;
        const dy = particles[i].y - particles[j].y;
        const dist = Math.sqrt(dx * dx + dy * dy);
        if (dist < CONNECT_DIST) {
          ctx.beginPath();
          ctx.moveTo(particles[i].x, particles[i].y);
          ctx.lineTo(particles[j].x, particles[j].y);
          ctx.strokeStyle = `rgba(96, 165, 250, ${0.06 * (1 - dist / CONNECT_DIST)})`;
          ctx.lineWidth = 0.5;
          ctx.stroke();
        }
      }
    }
    requestAnimationFrame(animate);
  }
  animate();
})();

// ---- Scroll Reveal ----
(function initScrollReveal() {
  const observer = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          entry.target.classList.add('visible');
        }
      });
    },
    { threshold: 0.1, rootMargin: '0px 0px -40px 0px' }
  );

  const targets = document.querySelectorAll(
    '.section-header, .feature-card, .algo-card, .dsr-core, ' +
    '.modifier-block, .mastery-section, .selection-path, .mixing-section, ' +
    '.elo-container, .fatigue-recovery, .pipeline-step'
  );
  targets.forEach((el) => observer.observe(el));
})();

// ---- State Ring SVG ----
(function drawStateRings() {
  const svg = document.getElementById('state-ring');
  if (!svg) return;

  const dims = [
    { name: 'Attention', value: 0.72, color: '#60a5fa' },
    { name: 'Fatigue', value: 0.35, color: '#f87171' },
    { name: 'Motivation', value: 0.61, color: '#34d399' },
    { name: 'Confidence', value: 0.68, color: '#a78bfa' },
    { name: 'Cognitive', value: 0.55, color: '#fbbf24' },
    { name: 'Trend', value: 0.50, color: '#fb923c' },
  ];

  const cx = 200, cy = 200;
  const rOuter = 150;
  const ringWidth = 14;
  const gap = 5;

  dims.forEach((dim, i) => {
    const r = rOuter - i * (ringWidth + gap);
    const circumference = 2 * Math.PI * r;
    const dashLength = circumference * dim.value;

    // Background ring
    const bgCircle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    bgCircle.setAttribute('cx', cx);
    bgCircle.setAttribute('cy', cy);
    bgCircle.setAttribute('r', r);
    bgCircle.setAttribute('fill', 'none');
    bgCircle.setAttribute('stroke', 'rgba(255,255,255,0.04)');
    bgCircle.setAttribute('stroke-width', ringWidth);
    svg.appendChild(bgCircle);

    // Value ring
    const circle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    circle.setAttribute('cx', cx);
    circle.setAttribute('cy', cy);
    circle.setAttribute('r', r);
    circle.setAttribute('fill', 'none');
    circle.setAttribute('stroke', dim.color);
    circle.setAttribute('stroke-width', ringWidth);
    circle.setAttribute('stroke-linecap', 'round');
    circle.setAttribute('stroke-dasharray', `${dashLength} ${circumference}`);
    circle.setAttribute('stroke-dashoffset', circumference * 0.25);
    circle.setAttribute('opacity', '0.7');
    circle.style.transform = 'rotate(-90deg)';
    circle.style.transformOrigin = `${cx}px ${cy}px`;
    // Animate
    circle.style.transition = `stroke-dasharray 1.5s ease ${i * 0.15}s`;
    // Start at 0 and animate to full
    circle.setAttribute('stroke-dasharray', `0 ${circumference}`);
    svg.appendChild(circle);

    // Trigger animation
    setTimeout(() => {
      circle.setAttribute('stroke-dasharray', `${dashLength} ${circumference}`);
    }, 300 + i * 150);
  });
})();

// ---- Forgetting Curve ----
(function initForgettingCurve() {
  const canvas = document.getElementById('forgetting-curve');
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  const slider = document.getElementById('stability-slider');
  const valDisplay = document.getElementById('stability-val');

  const W = canvas.width;
  const H = canvas.height;
  const PAD_LEFT = 50;
  const PAD_BOTTOM = 35;
  const PAD_TOP = 15;
  const PAD_RIGHT = 20;

  const config = {
    factor: 19.0 / 81.0,
    decay: -0.5,
    floor: 0.0,
  };

  function recall(t, S) {
    const power = Math.pow(1.0 + config.factor * t / S, config.decay);
    return config.floor + (1.0 - config.floor) * power;
  }

  function draw(stability) {
    ctx.clearRect(0, 0, W, H);
    const plotW = W - PAD_LEFT - PAD_RIGHT;
    const plotH = H - PAD_TOP - PAD_BOTTOM;
    const maxDays = Math.max(stability * 4, 10);

    // Grid
    ctx.strokeStyle = 'rgba(148, 163, 184, 0.08)';
    ctx.lineWidth = 1;
    for (let i = 0; i <= 4; i++) {
      const y = PAD_TOP + plotH * (1 - i * 0.25);
      ctx.beginPath();
      ctx.moveTo(PAD_LEFT, y);
      ctx.lineTo(W - PAD_RIGHT, y);
      ctx.stroke();
    }

    // Axes
    ctx.strokeStyle = 'rgba(148, 163, 184, 0.2)';
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(PAD_LEFT, PAD_TOP);
    ctx.lineTo(PAD_LEFT, H - PAD_BOTTOM);
    ctx.lineTo(W - PAD_RIGHT, H - PAD_BOTTOM);
    ctx.stroke();

    // Axis labels
    ctx.fillStyle = 'rgba(148, 163, 184, 0.5)';
    ctx.font = '11px Inter, sans-serif';
    ctx.textAlign = 'center';
    for (let i = 0; i <= 4; i++) {
      const day = (maxDays * i / 4).toFixed(0);
      const x = PAD_LEFT + plotW * (i / 4);
      ctx.fillText(`${day}d`, x, H - PAD_BOTTOM + 18);
    }
    ctx.textAlign = 'right';
    for (let i = 0; i <= 4; i++) {
      const pct = (i * 25);
      const y = PAD_TOP + plotH * (1 - i / 4);
      ctx.fillText(`${pct}%`, PAD_LEFT - 8, y + 4);
    }

    // 90% recall line
    const y90 = PAD_TOP + plotH * (1 - 0.9);
    ctx.strokeStyle = 'rgba(96, 165, 250, 0.2)';
    ctx.setLineDash([4, 4]);
    ctx.beginPath();
    ctx.moveTo(PAD_LEFT, y90);
    ctx.lineTo(W - PAD_RIGHT, y90);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.fillStyle = 'rgba(96, 165, 250, 0.4)';
    ctx.font = '10px JetBrains Mono, monospace';
    ctx.textAlign = 'left';
    ctx.fillText('R=90%', W - PAD_RIGHT - 44, y90 - 5);

    // Draw multiple stability curves
    const stabilities = [stability * 0.3, stability, stability * 3];
    const colors = ['rgba(248, 113, 113, 0.5)', 'rgba(96, 165, 250, 0.9)', 'rgba(52, 211, 153, 0.5)'];
    const widths = [1.5, 3, 1.5];

    stabilities.forEach((S, idx) => {
      ctx.strokeStyle = colors[idx];
      ctx.lineWidth = widths[idx];
      ctx.beginPath();
      for (let px = 0; px <= plotW; px++) {
        const t = (px / plotW) * maxDays;
        const r = recall(t, S);
        const x = PAD_LEFT + px;
        const y = PAD_TOP + plotH * (1 - r);
        if (px === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();

      // Label
      const labelT = maxDays * 0.75;
      const labelR = recall(labelT, S);
      const labelX = PAD_LEFT + plotW * 0.75;
      const labelY = PAD_TOP + plotH * (1 - labelR);
      ctx.fillStyle = colors[idx];
      ctx.font = '10px JetBrains Mono, monospace';
      ctx.textAlign = 'left';
      ctx.fillText(`S=${S.toFixed(1)}`, labelX + 6, labelY - 4);
    });

    // Stability marker on x-axis
    const sX = PAD_LEFT + (stability / maxDays) * plotW;
    if (sX >= PAD_LEFT && sX <= W - PAD_RIGHT) {
      ctx.strokeStyle = 'rgba(96, 165, 250, 0.3)';
      ctx.setLineDash([3, 3]);
      ctx.beginPath();
      ctx.moveTo(sX, PAD_TOP);
      ctx.lineTo(sX, H - PAD_BOTTOM);
      ctx.stroke();
      ctx.setLineDash([]);
    }
  }

  function update() {
    const val = parseFloat(slider.value);
    valDisplay.textContent = val.toFixed(1);
    draw(val);
  }

  slider.addEventListener('input', update);
  update();
})();

// ---- Pipeline step click to scroll ----
document.querySelectorAll('.pipeline-step[data-target]').forEach((step) => {
  step.addEventListener('click', () => {
    const target = document.getElementById(step.dataset.target);
    if (target) target.scrollIntoView({ behavior: 'smooth', block: 'start' });
  });
});

// ---- Animated state bars ----
(function animateStateBars() {
  const values = [72, 35, 61, 68, 55, 50];
  const fills = document.querySelectorAll('.state-bar-fill');

  // Start at 0, animate to value on scroll
  fills.forEach((fill) => {
    fill.style.width = '0%';
  });

  const observer = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          fills.forEach((fill, i) => {
            setTimeout(() => {
              fill.style.width = `${values[i]}%`;
            }, i * 100 + 200);
          });
          observer.disconnect();
        }
      });
    },
    { threshold: 0.3 }
  );

  const container = document.querySelector('.state-details');
  if (container) observer.observe(container);
})();

// ---- Ensemble weight animation ----
(function animateWeights() {
  const observer = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          // Animate trust score changes
          let t = 0;
          const interval = setInterval(() => {
            t += 0.02;
            if (t > 1) {
              clearInterval(interval);
              return;
            }
            // Slight oscillation to show dynamic nature
            const h = 0.45 + Math.sin(t * Math.PI * 4) * 0.05;
            const ig = 0.30 + Math.sin(t * Math.PI * 4 + 1) * 0.03;
            const s = 1 - h - ig;

            document.getElementById('w-heuristic').textContent = h.toFixed(2);
            document.getElementById('w-ige').textContent = ig.toFixed(2);
            document.getElementById('w-swd').textContent = s.toFixed(2);
          }, 50);
          observer.disconnect();
        }
      });
    },
    { threshold: 0.3 }
  );

  const box = document.querySelector('.merge-box');
  if (box) observer.observe(box);
})();

// ---- ELO Battle Animation ----
(function animateElo() {
  const observer = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          let userElo = 1200;
          let wordElo = 1200;
          let step = 0;
          const results = [1,1,0,1,1,1,0,1,1,1,0,1,1,1,1];

          const interval = setInterval(() => {
            if (step >= results.length) {
              clearInterval(interval);
              return;
            }
            const isCorrect = results[step];
            const expected = 1.0 / (1.0 + Math.pow(10, (wordElo - userElo) / 400));
            const k = 32;
            userElo += k * (isCorrect - expected);
            wordElo += k * (expected - isCorrect);

            document.getElementById('user-elo-display').textContent = Math.round(userElo);
            document.getElementById('word-elo-display').textContent = Math.round(wordElo);
            step++;
          }, 300);

          observer.disconnect();
        }
      });
    },
    { threshold: 0.3 }
  );

  const container = document.querySelector('.elo-battle');
  if (container) observer.observe(container);
})();

// ---- Smooth scroll for hero CTA ----
document.querySelector('.scroll-cta')?.addEventListener('click', (e) => {
  e.preventDefault();
  document.getElementById('pipeline')?.scrollIntoView({ behavior: 'smooth' });
});

/* =========================================================
   Neo Orión · animaciones estilo Marathon / O.R.C.A.
   Se usa en el instalador y en el HUD. Sin dependencias.
   - scramble(): el texto se "decodifica" carácter a carácter
   - glitch(): separación RGB + desplazamiento por franjas, en ráfagas cortas
   - corte(): transición de pantalla con franjas horizontales
   - mantener(): botón que hay que mantener apretado para confirmar
   - acento(): cambia el color de toda la interfaz y lo recuerda
   ========================================================= */
(function (W) {
  const GLIFOS = '█▓▒░<>/\\|#=+*01ABCDEFXZ$%&';
  const reduce = W.matchMedia && W.matchMedia('(prefers-reduced-motion: reduce)').matches;

  function scramble(el, texto, ms = 520) {
    if (!el) return Promise.resolve();
    texto = texto ?? el.textContent;
    if (reduce) { el.textContent = texto; return Promise.resolve(); }
    const t0 = performance.now(), n = texto.length;
    return new Promise(res => {
      (function paso(t) {
        const k = Math.min(1, (t - t0) / ms), fijos = Math.floor(k * n);
        let s = texto.slice(0, fijos);
        for (let i = fijos; i < n; i++) s += texto[i] === ' ' ? ' ' : GLIFOS[(Math.random() * GLIFOS.length) | 0];
        el.textContent = s;
        k < 1 ? requestAnimationFrame(paso) : (el.textContent = texto, res());
      })(t0);
    });
  }

  function glitch(el, ms = 260) {
    if (!el || reduce) return;
    el.classList.add('mx-glitch');
    setTimeout(() => el.classList.remove('mx-glitch'), ms);
  }
  function glitchAzar(sel, cada = 4200) {
    setInterval(() => { if (W.LIVIANO) return; const l = document.querySelectorAll(sel); if (l.length) glitch(l[(Math.random() * l.length) | 0]); }, cada);
  }

  function corte(fn) {
    if (reduce) { fn && fn(); return Promise.resolve(); }
    let c = document.getElementById('mx-corte');
    if (!c) { c = document.createElement('div'); c.id = 'mx-corte'; c.innerHTML = '<i></i>'.repeat(9); document.body.appendChild(c); }
    c.classList.remove('sale'); void c.offsetWidth; c.classList.add('entra');
    return new Promise(res => setTimeout(() => { fn && fn(); c.classList.remove('entra'); c.classList.add('sale'); setTimeout(res, 380); }, 300));
  }

  function mantener(btn, ms, alTerminar) {
    let t0 = 0, raf = 0;
    const barra = document.createElement('i'); barra.className = 'mx-hold'; btn.appendChild(barra);
    const parar = () => { cancelAnimationFrame(raf); barra.style.width = '0'; btn.classList.remove('cargando'); };
    const empezar = e => {
      e.preventDefault(); t0 = performance.now(); btn.classList.add('cargando');
      (function paso(t) { const k = Math.min(1, (t - t0) / ms); barra.style.width = (k * 100) + '%';
        if (k >= 1) { parar(); btn.classList.add('hecho'); alTerminar(); } else raf = requestAnimationFrame(paso); })(t0);
    };
    btn.addEventListener('pointerdown', empezar); ['pointerup', 'pointerleave', 'pointercancel'].forEach(ev => btn.addEventListener(ev, parar));
    btn.addEventListener('keydown', e => { if ((e.key === 'Enter' || e.key === ' ') && !e.repeat) empezar(e); });
    btn.addEventListener('keyup', parar);
  }


  /* ---------------- PIXEL: fuente de bloques 5×5 (+ fila de tilde) ---------------- */
  const F = {
    A: '.###.|#...#|#####|#...#|#...#', B: '####.|#...#|####.|#...#|####.', C: '.####|#....|#....|#....|.####', D: '####.|#...#|#...#|#...#|####.',
    E: '#####|#....|####.|#....|#####', F: '#####|#....|####.|#....|#....', G: '.####|#....|#.###|#...#|.####', H: '#...#|#...#|#####|#...#|#...#',
    I: '#####|..#..|..#..|..#..|#####', J: '..###|....#|....#|#...#|.###.', K: '#...#|#..#.|###..|#..#.|#...#', L: '#....|#....|#....|#....|#####',
    M: '#...#|##.##|#.#.#|#...#|#...#', N: '#...#|##..#|#.#.#|#..##|#...#', O: '.###.|#...#|#...#|#...#|.###.', P: '####.|#...#|####.|#....|#....',
    Q: '.###.|#...#|#.#.#|#..#.|.##.#', R: '####.|#...#|####.|#..#.|#...#', S: '.####|#....|.###.|....#|####.', T: '#####|..#..|..#..|..#..|..#..',
    U: '#...#|#...#|#...#|#...#|.###.', V: '#...#|#...#|#...#|.#.#.|..#..', W: '#...#|#...#|#.#.#|##.##|#...#', X: '#...#|.#.#.|..#..|.#.#.|#...#',
    Y: '#...#|.#.#.|..#..|..#..|..#..', Z: '#####|...#.|..#..|.#...|#####',
    0: '.###.|#..##|#.#.#|##..#|.###.', 1: '..#..|.##..|..#..|..#..|.###.', 2: '####.|....#|.###.|#....|#####', 3: '####.|....#|.###.|....#|####.',
    4: '#...#|#...#|#####|....#|....#', 5: '#####|#....|####.|....#|####.', 6: '.###.|#....|####.|#...#|.###.', 7: '#####|....#|...#.|..#..|..#..',
    8: '.###.|#...#|.###.|#...#|.###.', 9: '.###.|#...#|.####|....#|.###.',
    '-': '.....|.....|.###.|.....|.....', '.': '.....|.....|.....|.....|..#..', ',': '.....|.....|.....|..#..|.#...', ':': '.....|..#..|.....|..#..|.....', '/': '....#|...#.|..#..|.#...|#....',
    '!': '..#..|..#..|..#..|.....|..#..', '?': '.###.|....#|..##.|.....|..#..', '%': '##..#|##.#.|..#..|.#.##|#..##', ' ': '.....|.....|.....|.....|.....',
    '+': '.....|..#..|.###.|..#..|.....', '#': '.#.#.|#####|.#.#.|#####|.#.#.' };
  const TILDE = { 'Á': 'A', 'É': 'E', 'Í': 'I', 'Ó': 'O', 'Ú': 'U', 'Ñ': 'N', 'Ü': 'U' };

  /* iconos de bloques (cada fila igual de ancha) */
  const ICONOS = {
    check: '.......##|......##.|.....##..|##..##...|.####....|..##.....',
    x: '##...##|.##.##.|..###..|.##.##.|##...##',
    alerta: '....#....|...###...|...#.#...|..##.##..|..##.##..|.#######.|.###.###.|#########',
    mic: '..###..|..###..|..###..|#.###.#|#.....#|.#####.|...#...|.#####.',
    robot: '...#...|.#####.|.#.#.#.|.#####.|#######|#.###.#|..#.#..|.##.##.',
    cpu: '.#.#.#.|#######|.#...#.|##.#.##|.#...#.|#######|.#.#.#.',
    disco: '#######|#.....#|#.###.#|#.#.#.#|#.###.#|#.....#|#######',
    ia: '...#...|..###..|#######|.#####.|..###..|.##.##.|##...##',
    candado: '..###..|.#...#.|.#...#.|#######|###.###|###.###|#######',
    mando: '.#######.|##.###.##|#...#.#.#|##.###.##|#########|##.....##',
    radar: '..#####..|.#.....#.|#...#...#|#..###..#|#...#...#|.#.....#.|..#####..',
    onda: '#.......#|#..#.#..#|#.##.##.#|#########|#.##.##.#|#..#.#..#|#.......#',
    globo: '..###..|.#.#.#.|#######|#..#..#|#######|.#.#.#.|..###..',
    mundo: '..###..|.##.##.|#.#.#.#|#######|#.#.#.#|.##.##.|..###..',
    color: '##.##.##|##.##.##|........|##.##.##|##.##.##|........|##.##.##|##.##.##',
    llave: '.###.....|#...#....|#...#####|#...#.#.#|.###.....',
    play: '##.....|####...|######.|#######|######.|####...|##.....',
    usuario: '..###..|.#####.|.#####.|..###..|.......|.#####.|#######',
    carpeta: '###....|#######|#.....#|#.....#|#.....#|#######',
    senal: '#.#.#.#.#|.#.#.#.#.|#.#...#.#|.#.....#.|#.#...#.#|.#.#.#.#.|#.#.#.#.#' };

  function filas(c) {
    c = c.toUpperCase(); const t = TILDE[c]; const g = (F[t || c] || F['?']).split('|');
    const arriba = !t ? '.....' : c === 'Ñ' ? '.#.#.' : '...#.';
    return [arriba, ...g];
  }
  // Dibuja en un canvas: texto de bloques o icono. anim = los bloques llegan en desorden
  function pixel(lineasOIcono, o = {}) {
    const px = o.px || 6, gap = o.gap ?? Math.max(1, Math.round(px / 6)), color = o.color || (() => W.ACC), sombra = o.sombra;
    let mapa;
    if (Array.isArray(lineasOIcono)) mapa = lineasOIcono;            // ícono propio: filas de «#» y «.»
    else if (ICONOS[lineasOIcono]) mapa = ICONOS[lineasOIcono].split('|');
    else {
      const lineas = String(lineasOIcono).split('\n'); mapa = [];
      lineas.forEach((ln, li) => { const fs = [...ln].map(filas); for (let r = 0; r < 6; r++) mapa.push(fs.map(f => f[r]).join('.')); if (li < lineas.length - 1) mapa.push(''); });
    }
    const ancho = Math.max(...mapa.map(m => m.length)), alto = mapa.length, paso = px + gap;
    const cv = o.canvas || document.createElement('canvas');
    cv.width = ancho * paso; cv.height = alto * paso; cv.className = (cv.className || '') + ' mx-px';
    if (o.alto) { cv.style.height = o.alto; cv.style.width = 'auto'; }
    const g = cv.getContext('2d'), puntos = [];
    mapa.forEach((f, y) => [...f].forEach((ch, x) => { if (ch === '#') puntos.push([x, y]); }));
    const pinta = n => {
      g.clearRect(0, 0, cv.width, cv.height);
      for (let i = 0; i < n; i++) { const [x, y] = puntos[i];
        if (sombra) { g.fillStyle = sombra; g.fillRect(x * paso + gap, y * paso + gap, px, px); }
        g.fillStyle = typeof color === 'function' ? color() : color; g.fillRect(x * paso, y * paso, px, px); }
    };
    if (o.anim === false || reduce) pinta(puntos.length);
    else {
      for (let i = puntos.length - 1; i > 0; i--) { const j = (Math.random() * (i + 1)) | 0; [puntos[i], puntos[j]] = [puntos[j], puntos[i]]; }
      const ms = o.ms || 700, t0 = performance.now();
      (function paso(t) { const k = Math.min(1, (t - t0) / ms); pinta(Math.floor(k * puntos.length)); if (k < 1) requestAnimationFrame(paso);
        else { puntos.sort((a, b) => a[1] - b[1] || a[0] - b[0]); pinta(puntos.length); } })(t0);
    }
    cv.repintar = () => pinta(puntos.length);
    W.addEventListener('orion:color', () => { if (!o.color) pinta(puntos.length); });
    return cv;
  }

  // Transición por disolución de bloques (reemplaza a las franjas)
  function disolver(fn, o = {}) {
    if (reduce) { fn && fn(); return Promise.resolve(); }
    const cv = document.createElement('canvas'); cv.id = 'mx-dis';
    const tam = o.bloque || (W.LIVIANO ? 90 : innerWidth < 600 ? 28 : 40), cw = Math.ceil(innerWidth / tam), ch = Math.ceil(innerHeight / tam);
    cv.width = cw; cv.height = ch; document.body.appendChild(cv);
    const g = cv.getContext('2d'), celdas = [];
    for (let y = 0; y < ch; y++) for (let x = 0; x < cw; x++) celdas.push([x, y, Math.random()]);
    const fase = (inverso, ms) => new Promise(res => {
      const orden = celdas.slice().sort((a, b) => (a[2] + (inverso ? 0 : a[0] / cw * .6)) - (b[2] + (inverso ? 0 : b[0] / cw * .6))), t0 = performance.now();
      (function paso(t) { const k = Math.min(1, (t - t0) / ms), n = Math.floor(k * orden.length);
        g.clearRect(0, 0, cw, ch);
        const lista = inverso ? orden.slice(n) : orden.slice(0, n);
        lista.forEach(([x, y], i) => { g.fillStyle = (x + y) % 7 === 0 ? W.ACC : '#0a0b0c'; g.fillRect(x, y, 1, 1); });
        k < 1 ? requestAnimationFrame(paso) : res(); })(t0);
    });
    return fase(false, 260).then(() => { fn && fn(); return fase(true, 300); }).then(() => cv.remove());
  }

  // Alerta roja a pantalla completa tipo "SYS BREACH"
  function alerta(texto, ms = 1500, color) {
    color = color || W.PEL;
    if (reduce) return Promise.resolve();
    const d = document.createElement('div'); d.id = 'mx-alerta';
    d.innerHTML = `<div class="pat"></div><div class="txt"></div><div class="pat"></div>`;
    document.body.appendChild(d);
    const pat = () => { const c = pixel('senal', { px: innerWidth < 600 ? 14 : 22, gap: 4, color, anim: false }); return c; };
    d.querySelectorAll('.pat').forEach(x => x.appendChild(pat()));
    const t = d.querySelector('.txt'); t.style.color = color;
    scramble(t, '▲ ' + texto + ' ▲', 300);
    let v = 0; const parp = setInterval(() => { d.style.opacity = (v++ % 2) ? .55 : 1; }, 140);
    return new Promise(res => setTimeout(() => { clearInterval(parp); d.remove(); res(); }, ms));
  }

  // Columna de caracteres al azar (decoración tipo terminal)
  function columna(el, n = 14) {
    const A = 'ABCDEFGHJKLMNOPRSTUVWXYZ0123456789';
    const tick = () => { if (!document.body.contains(el)) return; el.textContent = Array.from({ length: n }, () => Array.from({ length: 1 + (Math.random() * 3 | 0) }, () => A[(Math.random() * A.length) | 0]).join('')).join('\n'); };
    tick(); setInterval(tick, 900);
  }

  // Color de acento: un solo valor manda en toda la interfaz
  function mezcla(hex, f) {
    const n = parseInt(hex.slice(1), 16), r = n >> 16, g = (n >> 8) & 255, b = n & 255;
    const m = v => Math.round(v * f).toString(16).padStart(2, '0');
    return '#' + m(r) + m(g) + m(b);
  }
  function acento(hex, guardar = true) {
    if (!/^#[0-9a-f]{6}$/i.test(hex || '')) return W.ACC;
    W.ACC = hex.toLowerCase();
    const s = document.documentElement.style;
    s.setProperty('--lime', W.ACC); s.setProperty('--lime-d', mezcla(W.ACC, .62)); s.setProperty('--acc-bg', mezcla(W.ACC, .09));
    // tinta sobre el acento: negra en acentos claros, blanca en oscuros (morado, escarlata profundo)
    const n = parseInt(W.ACC.slice(1), 16), lin = v => { v /= 255; return v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4; };
    const L = .2126 * lin(n >> 16) + .7152 * lin((n >> 8) & 255) + .0722 * lin(n & 255);
    W.INK = L > .18 ? '#111111' : '#ffffff'; s.setProperty('--ink', W.INK);
    // peligro: si el acento ya es rojo, las alertas pasan a amarillo para no confundirse
    const r = n >> 16, g = (n >> 8) & 255, b = n & 255, mx = Math.max(r, g, b), mn = Math.min(r, g, b);
    let h = 0; if (mx !== mn) { const d = mx - mn; h = mx === r ? ((g - b) / d) % 6 : mx === g ? (b - r) / d + 2 : (r - g) / d + 4; h = (h * 60 + 360) % 360; }
    const rojizo = mx - mn > 60 && (h < 25 || h > 330);
    W.PEL = rojizo ? '#ffd400' : '#ff4a2a'; s.setProperty('--red', W.PEL);
    if (guardar) try { localStorage.setItem('orion.color', W.ACC); } catch (e) {}
    W.dispatchEvent(new CustomEvent('orion:color', { detail: W.ACC }));
    return W.ACC;
  }
  let guardado = null; try { guardado = localStorage.getItem('orion.color'); } catch (e) {}
  W.ACC = '#ff2438'; W.INK = '#111111'; W.PEL = '#ffd400';
  // PC patata: poca memoria o pocos núcleos → modo liviano automático (se puede forzar)
  function liviano(on) {
    if (on === undefined) { let g = null; try { g = localStorage.getItem('orion.liviano'); } catch (e) {}
      on = g ? g === '1' : ((navigator.deviceMemory || 8) <= 2 || (navigator.hardwareConcurrency || 4) <= 2); }
    else try { localStorage.setItem('orion.liviano', on ? '1' : '0'); } catch (e) {}
    W.LIVIANO = !!on; document.documentElement.classList.toggle('ligero', !!on); return W.LIVIANO;
  }
  // ?liviano=1 / ?liviano=0 en la dirección (lo pone orion-sesion con «modo patata» del menú de arranque)
  try { const q = new URLSearchParams(location.search); if (q.has('liviano')) liviano(q.get('liviano') === '1'); else liviano(); } catch (e) { liviano(); }
  // Vigía de la pantalla: si los FPS reales caen, baja a liviano solo (sin guardarlo); si se recupera, vuelve.
  let forzado = null; try { forzado = localStorage.getItem('orion.liviano'); } catch (e) {}
  W.AUTO_LIVIANO = false;
  (function vigia() {
    let n = 0, t0 = performance.now(), malos = 0, buenos = 0;
    (function cuadro(t) {
      n++;
      if (t - t0 >= 2000) {
        const fps = n * 1000 / (t - t0); n = 0; t0 = t; W.FPS = Math.round(fps);
        if (!document.hidden && forzado === null) {
          if (fps < 24) { malos++; buenos = 0; } else if (fps > 50) { buenos++; malos = 0; } else { malos = 0; buenos = 0; }
          if (malos >= 3 && !W.LIVIANO) { liviano(true); try { localStorage.removeItem('orion.liviano'); } catch (e) {} W.AUTO_LIVIANO = true;
            W.dispatchEvent(new CustomEvent('orion:rendimiento', { detail: { nivel: 'LIVIANO', fps: W.FPS } })); }
          if (buenos >= 30 && W.AUTO_LIVIANO) { liviano(false); try { localStorage.removeItem('orion.liviano'); } catch (e) {} W.AUTO_LIVIANO = false;
            W.dispatchEvent(new CustomEvent('orion:rendimiento', { detail: { nivel: 'NORMAL', fps: W.FPS } })); }
        }
      }
      requestAnimationFrame(cuadro);
    })(t0);
  })();

  // ---------- EFECTO CRT (tele antigua): 'no' | 'suave' (líneas + viñeta + brillo) | 'curva' (además, la curvatura del vidrio)
  // Se guarda por equipo; en modo liviano la curvatura se apaga sola porque cuesta GPU.
  function mapaBarril(w, h, k) {
    const c = document.createElement('canvas'); c.width = w; c.height = h; const g = c.getContext('2d'), im = g.createImageData(w, h), d = im.data;
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      const nx = x / (w - 1) * 2 - 1, ny = y / (h - 1) * 2 - 1, r2 = nx * nx + ny * ny, i = (y * w + x) * 4;
      d[i] = 128 + 127 * Math.max(-1, Math.min(1, nx * r2 * k)); d[i + 1] = 128 + 127 * Math.max(-1, Math.min(1, ny * r2 * k)); d[i + 2] = 128; d[i + 3] = 255;
    }
    g.putImageData(im, 0, 0); return c.toDataURL();
  }
  function curvaSVG() {
    if (document.getElementById('mx-crt-svg')) return;
    const ns = 'http://www.w3.org/2000/svg', svg = document.createElementNS(ns, 'svg');
    svg.id = 'mx-crt-svg'; svg.setAttribute('width', 0); svg.setAttribute('height', 0); svg.style.position = 'absolute';
    svg.innerHTML = `<filter id="mx-barril" x="0" y="0" width="1" height="1" filterUnits="objectBoundingBox" primitiveUnits="objectBoundingBox" color-interpolation-filters="sRGB">
      <feImage href="${mapaBarril(128, 128, .5)}" x="0" y="0" width="1" height="1" preserveAspectRatio="none" result="m"/>
      <feDisplacementMap in="SourceGraphic" in2="m" scale=".09" xChannelSelector="R" yChannelSelector="G"/></filter>`;
    document.body.appendChild(svg);
  }
  function crt(modo) {
    const OK = ['no', 'suave', 'curva'];
    if (modo === undefined) { try { modo = localStorage.getItem('orion.crt'); } catch (e) {} modo = OK.includes(modo) ? modo : 'no'; }
    else { if (modo === true) modo = 'suave'; if (modo === false) modo = 'no'; if (!OK.includes(modo)) modo = 'no'; try { localStorage.setItem('orion.crt', modo); } catch (e) {} }
    const H = document.documentElement;
    let capa = document.getElementById('mx-crt');
    if (modo !== 'no' && !capa && document.body) { capa = document.createElement('div'); capa.id = 'mx-crt'; document.body.appendChild(capa); }
    if (modo === 'curva' && document.body) curvaSVG();
    H.classList.toggle('crt', modo !== 'no'); H.classList.toggle('crt-curva', modo === 'curva');
    W.CRT = modo; W.dispatchEvent(new CustomEvent('orion:crt', { detail: modo })); return modo;
  }
  acento(guardado || W.ACC, false);

  const PALETA = [['ESCARLATA', '#ff2438'], ['MORADO', '#b14bff'], ['MAGENTA', '#ff3d9a'], ['CIAN', '#3ef0ff'],
    ['ÁMBAR', '#ffb000'], ['NEÓN', '#c6ff00'], ['ÁCIDO', '#cdf564'], ['HIELO', '#e6e8ea']];

  // CSS compartido (se inyecta una vez)
  const css = `
  .mx-glitch{animation:mxg .26s steps(2) 1}
  @keyframes mxg{0%{text-shadow:2px 0 #ff2a6d,-2px 0 #2af5ff;transform:translateX(0);clip-path:inset(0 0 55% 0)}
    25%{text-shadow:-3px 0 #ff2a6d,3px 0 #2af5ff;transform:translateX(3px);clip-path:inset(40% 0 10% 0)}
    50%{text-shadow:2px 0 #ff2a6d,-1px 0 #2af5ff;transform:translateX(-2px);clip-path:inset(10% 0 60% 0)}
    100%{text-shadow:none;transform:none;clip-path:none}}
  #mx-corte{position:fixed;inset:0;z-index:999;pointer-events:none;display:grid;grid-template-rows:repeat(9,1fr)}
  #mx-corte i{display:block;background:var(--lime);transform:scaleX(0);transform-origin:left}
  #mx-corte i:nth-child(odd){background:#0a0b0c}
  #mx-corte.entra i{animation:mxc-in .3s steps(5) forwards}
  #mx-corte.sale i{transform:scaleX(1);transform-origin:right;animation:mxc-out .36s steps(6) forwards}
  ${Array.from({ length: 9 }, (_, i) => `#mx-corte i:nth-child(${i + 1}){animation-delay:${(i * 17) % 90}ms!important}`).join('')}
  @keyframes mxc-in{to{transform:scaleX(1)}} @keyframes mxc-out{to{transform:scaleX(0)}}
  canvas.mx-px,#mx-dis{image-rendering:pixelated;image-rendering:crisp-edges}
  #mx-dis{position:fixed;inset:0;width:100vw;height:100vh;z-index:999;pointer-events:none}
  #mx-alerta{position:fixed;inset:0;z-index:998;background:#1a0705;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:18px;pointer-events:none}
  #mx-alerta .txt{font-family:'Silkscreen',monospace;font-size:clamp(20px,5vw,44px);letter-spacing:.08em;text-shadow:0 0 18px currentColor}
  .mx-cruz{background-image:url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='11' height='11'%3E%3Cpath d='M5 0v11M0 5h11' stroke='%238b8e94' stroke-width='1.4'/%3E%3C/svg%3E"),url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='11' height='11'%3E%3Cpath d='M5 0v11M0 5h11' stroke='%238b8e94' stroke-width='1.4'/%3E%3C/svg%3E"),url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='11' height='11'%3E%3Cpath d='M5 0v11M0 5h11' stroke='%238b8e94' stroke-width='1.4'/%3E%3C/svg%3E"),url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='11' height='11'%3E%3Cpath d='M5 0v11M0 5h11' stroke='%238b8e94' stroke-width='1.4'/%3E%3C/svg%3E");background-repeat:no-repeat;background-position:7px 7px,calc(100% - 7px) 7px,7px calc(100% - 7px),calc(100% - 7px) calc(100% - 7px)}
  .mx-lleno{background:var(--lime)!important;color:#111!important;border-color:var(--lime)!important}
  .mx-lleno *{color:#111!important}
  .mx-hold{position:absolute;left:0;top:0;bottom:0;width:0;background:currentColor;opacity:.35;pointer-events:none}
  .mx-in{animation:mxin .5s steps(6) both}
  @keyframes mxin{0%{clip-path:inset(0 100% 0 0);opacity:.2}40%{opacity:1}100%{clip-path:inset(0 0 0 0)}}
  .mx-flick{animation:mxf .4s steps(4) both}
  @keyframes mxf{0%{opacity:0}25%{opacity:1}50%{opacity:.2}75%{opacity:1}100%{opacity:1}}
  @media (prefers-reduced-motion: reduce){.mx-in,.mx-flick,.mx-glitch{animation:none!important}}
  #mx-crt{position:fixed;inset:0;z-index:997;pointer-events:none;border-radius:2.2vmin;
    background:repeating-linear-gradient(to bottom,rgba(0,0,0,0) 0,rgba(0,0,0,0) 2px,rgba(0,0,0,.28) 3px,rgba(0,0,0,0) 4px),
      radial-gradient(ellipse at center,rgba(0,0,0,0) 58%,rgba(0,0,0,.55) 100%);
    box-shadow:inset 0 0 6vmin rgba(0,0,0,.7),inset 0 0 1.4vmin rgba(0,0,0,.9);animation:mxcrt 6s steps(60) infinite}
  #mx-crt::after{content:'';position:absolute;inset:0;background:linear-gradient(to bottom,rgba(255,255,255,0) 0,rgba(255,255,255,.035) 50%,rgba(255,255,255,0) 100%);
    background-size:100% 18vh;background-repeat:no-repeat;animation:mxbarre 7s linear infinite}
  @keyframes mxcrt{0%,100%{opacity:1}48%{opacity:.94}50%{opacity:1}73%{opacity:.97}}
  @keyframes mxbarre{from{background-position:0 -20vh}to{background-position:0 120vh}}
  html.crt body{text-shadow:.6px 0 rgba(255,40,80,.35),-.6px 0 rgba(40,220,255,.3),0 0 6px color-mix(in srgb,var(--lime) 35%,transparent)}
  html.crt-curva body{filter:url(#mx-barril);min-height:100vh}
  html.crt-curva #mx-crt{border-radius:6vmin/9vmin}
  html.ligero.crt-curva body{filter:none}
  html.ligero #mx-crt,html.ligero #mx-crt::after{animation:none}
  @media (prefers-reduced-motion: reduce){#mx-crt,#mx-crt::after{animation:none}}`;
  const st = document.createElement('style'); st.textContent = css; document.head.appendChild(st);

  if (document.body) crt(); else document.addEventListener('DOMContentLoaded', () => crt());
  try { const q = new URLSearchParams(location.search); if (q.has('crt')) crt(q.get('crt')); } catch (e) {}
  W.MX = { liviano, crt, scramble, glitch, glitchAzar, corte: disolver, franjas: corte, disolver, mantener, acento, PALETA, mezcla, pixel, alerta, columna, ICONOS };
})(window);

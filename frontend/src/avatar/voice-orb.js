// Voice Orb / Ship Notes. Local audio analysis, no dependencies.
(() => {
  if (typeof window === 'undefined' || typeof customElements === 'undefined' || customElements.get('voice-orb')) return;
  const STATES = ['idle', 'listening', 'thinking', 'speaking', 'text'];
  // State-based color palettes (Feature 89): idle warm grey, listening
  // warm amber/brown, thinking vibrant electric purple/magenta, speaking vibrant magenta. The
  // 5th (text) entry is a fallback — real text inherits the STATE palette
  // via the dominant-state tint (see _paint stateTint / uniform textTint).
  const PALETTE = [[.80,.78,.76], [.82,.55,.26], [.82,.15,.96], [1.0,.45,.85], [1.0,.9,.80]];
  const mediaSources = new WeakMap();
  const clamp = (v, lo=0, hi=1) => Math.max(lo, Math.min(hi, Number.isFinite(+v) ? +v : lo));
  const weightsFor = state => STATES.map(s => +(s === state));
  const normalize = w => {
    const a = Array.from({length:5}, (_,i) => clamp(w?.[i]));
    const sum = a.reduce((s,v) => s+v, 0);
    return sum ? a.map(v => v/sum) : [1,0,0,0,0];
  };
  const seeded = i => { const s = Math.sin(i*127.1+311.7)*43758.5453; return s-Math.floor(s); };
  const hash3 = (x, y, z) => {
    const s = Math.sin(x*127.1+y*311.7+z*74.7)*43758.5453;
    return s-Math.floor(s);
  };
  const smooth01 = x => x*x*(3-2*x);
  const mixNum = (a, b, t) => a+(b-a)*t;
  const vnoise = (x, y, z) => {
    const xi=Math.floor(x), yi=Math.floor(y), zi=Math.floor(z);
    const xf=smooth01(x-xi), yf=smooth01(y-yi), zf=smooth01(z-zi);
    const a=hash3(xi,yi,zi), b=hash3(xi+1,yi,zi);
    const c=hash3(xi,yi+1,zi), d=hash3(xi+1,yi+1,zi);
    const e=hash3(xi,yi,zi+1), f=hash3(xi+1,yi,zi+1);
    const g=hash3(xi,yi+1,zi+1), h=hash3(xi+1,yi+1,zi+1);
    return mixNum(mixNum(mixNum(a,b,xf),mixNum(c,d,xf),yf),mixNum(mixNum(e,f,xf),mixNum(g,h,xf),yf),zf)*2.0-1.0;
  };
  const normalize3 = (x, y, z) => { const l = Math.hypot(x,y,z)||1; return [x/l,y/l,z/l]; };
  const cross3 = (ax,ay,az,bx,by,bz) => [ay*bz-az*by, az*bx-ax*bz, ax*by-ay*bx];
  const sphere = count => {
    const data = new Float32Array(count*4);
    const jitter = .55 / Math.sqrt(count);
    for (let i=0; i<count; i++) {
      const y = clamp(1-2*(i+.5)/count+(seeded(i+7)-.5)*jitter, -.99999, .99999);
      const a = i*2.399963229728653+(seeded(i+19)-.5)*jitter*5;
      const r = Math.sqrt(1-y*y);
      data.set([r*Math.cos(a), y, r*Math.sin(a), i/count], i*4);
    }
    return data;
  };
  // Ghost enter: scattered start positions on a wide edge-biased ring —
  // particles burst inward from around the rim, never from dead center.
  const scatterOf = count => {
    const data = new Float32Array(count*3);
    for (let i=0; i<count; i++) {
      const a = seeded(i+53)*6.2831853;
      const r = 1.8 + seeded(i+71)*1.2;
      data.set([r*Math.cos(a), (seeded(i+97)-.5)*2.6, r*Math.sin(a)], i*3);
    }
    return data;
  };
  // ─── Particle-generated text (Feature 88) ─────────────────────────
  // Rasterize the text on an offscreen 2D canvas, sample glyph pixels,
  // and map them into orb space. Returns null when the text renders
  // empty (also the no-canvas graceful path).
  const sampleTextPoints = text => {
    if (typeof document === 'undefined') return null;
    const t = String(text ?? '').trim().slice(0, 16);
    if (!t) return null;
    const W = 360, H = 128;
    const c = document.createElement('canvas');
    c.width = W; c.height = H;
    const ctx = c.getContext('2d', { willReadFrequently: true });
    if (!ctx) return null;
    ctx.fillStyle = '#fff';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    // Fit the text to the canvas instead of a fixed size: at a flat 84px
    // bold, anything past ~5-6 characters overflowed the 360px canvas and
    // got silently clipped at BOTH edges (text is center-anchored), e.g.
    // "Right away." rendered as "ght awa" — only the surviving middle
    // slice. Measure at the reference size, then scale down (never up) so
    // the full phrase — up to the 16-char cap above — stays on-canvas.
    const refSize = 84;
    ctx.font = `bold ${refSize}px "Segoe UI", system-ui, sans-serif`;
    const measured = ctx.measureText(t).width;
    const maxWidth = W * 0.92;
    const fontSize = measured > maxWidth ? Math.max(26, Math.floor(refSize * (maxWidth / measured))) : refSize;
    ctx.font = `bold ${fontSize}px "Segoe UI", system-ui, sans-serif`;
    ctx.fillText(t, W/2, H/2 + 4);
    const img = ctx.getImageData(0, 0, W, H).data;
    const pts = [];
    for (let y=0; y<H; y+=2) {
      for (let x=0; x<W; x+=2) {
        if (img[(y*W+x)*4+3] > 110) pts.push([x/W, 1-y/H]);
      }
    }
    return pts.length ? pts : null;
  };
  // Assign every particle a text target: glyph points get 2-4 particles
  // each (volume), the rest precompute positions on a wide faint ambient
  // halo (flag w >= 0.5 — the shader dims them and never converges them).
  const textOf = (count, pts) => {
    const data = new Float32Array(count*4);
    if (!pts || !pts.length) {
      for (let i=0; i<count; i++) {
        const a = seeded(i+211)*6.2831853;
        const r = 1.42 + seeded(i+233)*0.5;
        data.set([r*Math.cos(a), (seeded(i+251)-.5)*1.4, r*Math.sin(a), 0.5+seeded(i+269)*0.5], i*4);
      }
      return data;
    }
    const nText = Math.round(count * clamp(pts.length*4/count, 0.30, 0.88));
    for (let i=0; i<count; i++) {
      if (i < nText) {
        const p = pts[(i*7919) % pts.length];
        data.set([
          (p[0]-.5)*1.72,
          (p[1]-.5)*0.64+0.02,
          (seeded(i+131)-.5)*0.16,
          seeded(i+173)*0.49,
        ], i*4);
      } else {
        const a = seeded(i+211)*6.2831853;
        const r = 1.42 + seeded(i+233)*0.5;
        data.set([r*Math.cos(a), (seeded(i+251)-.5)*1.4, r*Math.sin(a), 0.5+seeded(i+269)*0.5], i*4);
      }
    }
    return data;
  };

  const VS = `
  precision highp float;
  attribute vec4 seed;
  attribute vec3 scatter;
  attribute vec4 textPos;
  uniform float time, pixels, density, onset, reduced, assemble, tw, textProg, thinkIn;
  uniform mediump float glitch;
  uniform vec4 weights;
  uniform vec3 bands;
  uniform vec3 textTint;
  varying vec3 tint;
  varying float strength, spark;
  varying vec3 sparkTint;
  varying float crisp;
  float hash(vec3 p) { return fract(sin(dot(p,vec3(127.1,311.7,74.7)))*43758.5453); }
  float noise(vec3 p) {
    vec3 i=floor(p), f=fract(p); f=f*f*(3.0-2.0*f);
    float a=mix(hash(i),hash(i+vec3(1,0,0)),f.x);
    float b=mix(hash(i+vec3(0,1,0)),hash(i+vec3(1,1,0)),f.x);
    float c=mix(hash(i+vec3(0,0,1)),hash(i+vec3(1,0,1)),f.x);
    float d=mix(hash(i+vec3(0,1,1)),hash(i+vec3(1,1,1)),f.x);
    return mix(mix(a,b,f.y),mix(c,d,f.y),f.z)*2.0-1.0;
  }
  vec3 turn(vec3 p,float a) { float c=cos(a),s=sin(a);return vec3(c*p.x+s*p.z,p.y,c*p.z-s*p.x); }
  vec3 rotate3D(vec3 p,float yaw,float pitch) {
    float cy=cos(yaw),sy=sin(yaw);
    float cp=cos(pitch),sp=sin(pitch);
    vec3 p1=vec3(cy*p.x+sy*p.z,p.y,cy*p.z-sy*p.x);
    return vec3(p1.x,cp*p1.y-sp*p1.z,cp*p1.z+sp*p1.y);
  }
  void main() {
    float t=time;
    vec3 n=seed.xyz;
    float angle=acos(clamp(n.z,-1.0,1.0));
    float drift=noise(n*2.7+vec3(t*.19,-t*.11,t*.08));
    float bass=noise(n*1.8+vec3(t*.32,0,-t*.2));
    float grain=noise(n*17.0+vec3(-t*1.8,t*.7,t));
    float low=bands.x, mid=bands.y, high=bands.z;
    float idleR=1.0+.018*drift+.008*sin(t*.85);
    float inward=sin(angle*13.0+t*5.4+drift*1.6);
    float outward=sin(angle*12.0-t*6.2+drift*1.6);
    // Listening: 50% of particles pulse radially (STT active); the other
    // half keeps drifting so the sphere never reads fully static. Voice
    // reaction stays VERY subtle (calm, attentive, ready to receive).
    float pid=hash(n*17.3+seed.w*127.1);
    bool pulseOn=pid>0.5;
    float pulse=1.0+0.05*sin(t*2.5+pid*6.2831);
    float listenReact=(.018+.06*mid)*inward+.020*high*grain;
    float listenR=pulseOn?(pulse+listenReact):(1.0+.012*sin(t*1.7+pid*6.2831)+.03*low*bass);
    // Speaking: dense, irregular potato/pebble blob. A lower-frequency,
    // higher-amplitude noise octave dominates the silhouette (fewer,
    // bigger, more irregular/faceted lobes than the old smoother layered
    // bumps — see reference image), with a smaller secondary octave and
    // the existing high-frequency grain for surface sparkle. Replaces the
    // old lat/lon wireframe lattice (too sparse to read as a solid
    // "speaking" presence).
    float lobeNoise=noise(n*0.65+vec3(t*.22,-t*.17,t*.13));
    float lobeNoise2=noise(n*0.85+vec3(-t*.10,t*.14,-t*.09));
    float energy=.35+.65*max(low,mid);
    // Beat-driven mold (sub-phase C2, real TTS-audio onset — see JS
    // _readAudio): a transient visibly lurches the whole irregular
    // silhouette (lobe amplitude itself scales with onset, not just a
    // flat additive bump). Damped by textProg so a beat never jitters
    // glyphs mid-convergence — fades back to full intensity once text
    // dissolves (textProg 0).
    float textDamp=1.0-0.75*textProg;
    float rad=1.0
      +.40*lobeNoise*(1.0+.4*onset)*textDamp
      +.10*lobeNoise2
      +energy*.05*sin(angle*5.0+t*1.4+drift*1.2)
      +high*.022*grain
      +.22*onset*textDamp;
    vec3 speakV=turn(n,t*.24+.04*sin(t*.22))*rad;
    // Thinking: 3D rotating purple starburst with dense white-magenta nucleus
    // and 64 straight radial beaded rays radiating in 3D (reference media_1791105468869.png).
    // Partitioned into STRANDS=64 rays evenly distributed via spherical Fibonacci.
    // Core particles (sAlong < 0.14) form the intense glowing nucleus at center;
    // spoke particles (sAlong >= 0.14) align in 15 concentric beaded steps along
    // laser-straight radial lines with narrow needle beam thickness.
    // Dynamic motion: continuous 3D rotation (yaw t*0.45, pitch t*0.28) and an
    // outward-propagating energy ripple so purple particles are alive and rotating.
    float u=seed.w;
    const float STRANDS=64.0;
    float su=u*STRANDS;
    float strandIdx=floor(su);
    float sAlong=fract(su);
    float sy=1.0-2.0*(strandIdx+0.5)/STRANDS;
    float sr=sqrt(max(0.0,1.0-sy*sy));
    float sang=strandIdx*2.399963229728653;
    vec3 strandDir=vec3(sr*cos(sang),sy,sr*sin(sang));
    vec3 perp1=normalize(cross(strandDir,vec3(0.0,1.0,0.0))+0.0001);
    vec3 perp2=cross(strandDir,perp1);
    float thinkRNorm=0.0;
    vec3 spokePos=vec3(0.0);
    if(sAlong<0.14) {
      float cFrac=sAlong/0.14;
      thinkRNorm=cFrac*0.14;
      float cAngle=hash(n*7.3+vec3(strandIdx,0.0,0.0))*6.2831;
      float cJitter=(0.02+0.04*hash(n*13.1+vec3(0.0,strandIdx,0.0)))*(1.0-cFrac*0.5);
      spokePos=strandDir*(cFrac*0.13)+(perp1*cos(cAngle)+perp2*sin(cAngle))*cJitter;
    } else {
      float tRay=(sAlong-0.14)/0.86;
      float beadIdx=floor(tRay*15.0);
      thinkRNorm=(beadIdx+0.5)/15.0;
      float rReach=0.14+pow(thinkRNorm,1.08)*0.91;
      float pulse=sin(thinkRNorm*18.0-t*3.5);
      rReach+=pulse*0.015;
      float beamWidth=(0.007+0.005*thinkRNorm)*(1.0+0.3*sin(tRay*31.0));
      float bAngle=hash(n*5.7+vec3(strandIdx,beadIdx,0.0))*6.2831;
      vec3 beamOff=(perp1*cos(bAngle)+perp2*sin(bAngle))*beamWidth;
      spokePos=strandDir*rReach+beamOff;
    }
    // Continuous 3D tumbling rotation around yaw and pitch so purple starburst rotates in 3D
    vec3 thought=rotate3D(spokePos,t*0.45,t*0.28)*mix(0.42,1.0,thinkIn);
    // Cloud position in the CURRENT base state (weights renormalized in JS
    // to sum 1 across the four base states; the text weight is separate).
    vec3 cloudPos=turn(n,t*.11)*idleR*weights.x
      +turn(n,t*.16)*listenR*weights.y
      +thought*weights.z
      +speakV*weights.w;
    // Text: glyph particles detach from the cloud, travel with a curved
    // per-particle stagger, and converge into readable letters. Ambient
    // particles (textPos.w >= 0.5) flow outward onto a wide faint halo
    // instead of converging. Dissolve reverses te — particles leave the
    // glyphs with a swirl burst and stream back into the sphere.
    bool isGlyph=textPos.w<0.5;
    float tst=hash(n*3.7+vec3(textPos.w*97.1,0.0,0.0));
    float te=clamp((textProg-tst*0.30)/0.70,0.0,1.0);
    te=te*te*(3.0-2.0*te);
    vec3 glyphT=textPos.xyz+vec3(sin(t*2.8+tst*6.2831),cos(t*2.4+tst*6.2831),0.0)*0.42*(1.0-te);
    vec3 swirlT=vec3(sin(t*3.0+tst*6.2831),cos(t*2.6+tst*6.2831),0.0)*0.38*(1.0-te);
    vec3 pos=mix(cloudPos+swirlT,glyphT,te);
    float active=weights.y+weights.w;
    float ambF=step(0.5,textPos.w)*clamp(tw*1.6,0.0,1.0);
    float rim=pow(max(0.0,1.0-abs(n.z)),2.2);
    float pop=pow(max(0.0,sin(t*8.0+seed.w*149.0)),18.0)*step(.90,seed.w);
    pos*=1.0+active*high*pop*.17;
    // Ghost enter: fly in from scattered rim positions (assemble 0→1).
    // Per-particle stagger staggers arrival; decaying swirl curves the path.
    float stag=hash(n*5.3+vec3(seed.w*91.7,0.0,0.0));
    float ap=clamp((assemble-stag*0.35)/0.65,0.0,1.0);
    float ae=ap*ap*(3.0-2.0*ap);
    vec3 swirl=vec3(sin(t*3.0+stag*6.2831),cos(t*2.6+stag*6.2831),0.0)*0.35*(1.0-ae);
    vec3 flight=mix(scatter+swirl,pos,ae);
    // Glitch transition: a brief (~220ms) burst of large high-frequency
    // jitter when the dominant state changes (glitch 0→1→0, driven from
    // JS _tick). Decorrelated from the calm ambient drift/grain noise —
    // much faster time coefficients — so it reads as a sudden tear, not
    // part of the normal idle motion.
    vec3 glitchJitter=vec3(
      noise(n*35.0+vec3(t*40.0,0.0,0.0)),
      noise(n*31.0+vec3(0.0,t*42.0,0.0)),
      noise(n*29.0+vec3(0.0,0.0,t*38.0))
    )*glitch*0.18;
    flight+=glitchJitter;
    float flow=pow(.5+.5*sin(angle*13.0+(weights.y-weights.w)*t*5.8+drift*2.0),7.0);
    float depth=clamp((flight.z+1.35)/2.7,0.0,1.0);
    float perspective=3.8/(3.8-flight.z*.60);
    gl_Position=vec4(flight.xy*perspective*.61,0,1);
    float point=((2.0+1.8*depth+.85*rim)*mix(1.0,.72,weights.w))*density;
    point*=mix(1.0,.62,ambF);
    point+=active*high*pop*1.8;
    gl_PointSize=max(1.8,point*pixels/720.0);
    float cool=.5+.5*sin(n.y*2.1+n.x*1.6+drift*.65);
    float speakFace=clamp(n.z*.5+.5,0.0,1.0);
    float speakBump=clamp(.5+.5*lobeNoise,0.0,1.0);
    // ─── State palettes (Feature 89) ──────────────────────────────
    // Listening: warm amber/brown (palette-driven). Thinking: vibrant
    // electric purple starburst with blazing white core (reference media_1791105468869.png).
    // Gradient: pure white/hot-pink core -> neon magenta -> electric purple -> deep purple tips.
    // Speaking: vibrant magenta body with white front highlights.
    vec3 ci=vec3(.80,.78,.76);
    vec3 cl=vec3(.82,.55,.26);
    vec3 ctt;
    if(thinkRNorm<0.14) {
      ctt=mix(vec3(1.0,1.0,1.0),vec3(1.0,0.70,0.98),thinkRNorm/0.14);
    } else if(thinkRNorm<0.45) {
      ctt=mix(vec3(1.0,0.70,0.98),vec3(0.90,0.15,0.98),(thinkRNorm-0.14)/0.31);
    } else if(thinkRNorm<0.75) {
      ctt=mix(vec3(0.90,0.15,0.98),vec3(0.72,0.18,0.98),(thinkRNorm-0.45)/0.30);
    } else {
      ctt=mix(vec3(0.72,0.18,0.98),vec3(0.55,0.10,0.88),clamp((thinkRNorm-0.75)/0.25,0.0,1.0));
    }
    vec3 cs=mix(vec3(1.0,.45,.85),vec3(1.0,1.0,1.0),clamp(speakFace*.55+.35*speakBump,0.0,1.0));
    vec3 blendT=ci*weights.x+cl*weights.y+ctt*weights.z+cs*weights.w+textTint*tw;
    // Per-particle color lag: mid-transition some particles still lean
    // toward the old color while others already shifted (gold → gold/purple
    // → purple/blue) — fluid coexistence, never a sudden swap. Settles to
    // the uniform state color when the weight lands.
    float lag=hash(n*9.7+vec3(31.7,0.0,0.0));
    float mw=max(max(weights.x,weights.y),max(weights.z,weights.w));
    vec3 domC=weights.x>=weights.y?(weights.x>=weights.z?(weights.x>=weights.w?ci:cs):(weights.z>=weights.w?ctt:cs)):(weights.y>=weights.z?(weights.y>=weights.w?cl:cs):(weights.z>=weights.w?ctt:cs));
    if(tw>mw)domC=textTint;
    float mixAmt=lag*.55*(1.0-clamp(mw*4.0,0.0,1.0));
    tint=mix(blendT,domC,mixAmt);
    tint*=mix(1.0,.30,ambF);
    crisp=max(weights.w,tw);
    sparkTint=vec3(1.0,1.0,1.0);
    // knotSpark: specular white sparks travelling along each spoke outward from the core
    float knotSpark=pow(max(0.0,sin(thinkRNorm*22.0-t*3.8+strandIdx*0.5)),14.0)*step(0.4,depth);
    float coreBoost=step(sAlong,0.14)*1.4;
    // Energy hierarchy: listening calm/subtle → thinking medium-high →
    // speaking highest. Speaking brightness is synchronized with the
    // radial breathing (expands brighter, contracts dimmer).
    strength=(.22+.45*depth+.70*rim)*(.65+.35*fract(sin(seed.w*912.7+31.4)*43758.5));
    strength+=weights.y*(mid*flow*.40+onset*rim*.45);
    strength+=weights.w*(mid*flow*.95+onset*rim*.9);
    strength+=weights.z*(0.35+0.65*depth+coreBoost+knotSpark*1.8);
    strength+=weights.w*(speakFace*.55+speakBump*.50);
    strength+=tw*(.40+.40*depth);
    strength+=weights.w*.10*sin(t*.9+seed.w*6.2831);
    strength*=mix(1.0,0.70,weights.y);
    strength*=mix(1.0,1.02,weights.z);
    strength*=mix(1.0,1.18,weights.w);
    strength*=mix(1.0,.32,ambF);
    spark=active*(high*pop*.8+onset*rim*.22)+weights.z*(step(sAlong,0.14)*0.85+knotSpark*1.2+step(0.85,depth)*0.15);
  }`;
  const FS = `
  precision mediump float;
  varying vec3 tint;
  varying float strength, spark;
  varying vec3 sparkTint;
  varying float crisp;
  uniform mediump float glitch;
  void main() {
    float r=length(gl_PointCoord-.5)*2.0;
    if(r>1.0)discard;
    float edge=mix(.64,.30,crisp);
    float core=1.0-smoothstep(mix(.18,.08,crisp),edge,r);
    float halo=mix(exp(-r*r*4.0)*.24,exp(-r*r*8.0)*.12,crisp)*(1.0-smoothstep(.75,1.0,r));
    float a=(core+halo)*strength;
    vec3 color=tint*a+sparkTint*spark*core;
    // Glitch transition: per-particle RGB-channel decorrelation flicker
    // (a point-sprite approximation of chromatic-aberration/VHS-tear —
    // true screen-space channel splitting needs a post-process pass,
    // out of reach of this per-point architecture). Paired with the
    // vertex-shader's positional jitter burst (see glitchJitter above).
    if(glitch>0.001) {
      float gr=fract(sin(dot(gl_PointCoord,vec2(12.9898,78.233)))*43758.5453);
      vec3 rgbShift=vec3(
        0.5+0.5*sin(gr*31.0+tint.r*7.0),
        0.5+0.5*sin(gr*37.0+tint.g*7.0+2.1),
        0.5+0.5*sin(gr*41.0+tint.b*7.0+4.2)
      );
      color=mix(color,color*rgbShift*1.6,glitch);
    }
    gl_FragColor=vec4(color,min(1.0,a+spark*core));
  }`;

  class VoiceOrb extends HTMLElement {
    static get observedAttributes() { return ['state','particles','recording']; }
    constructor() {
      super();
      this._weights=[1,0,0,0,0]; this._bands=[0,0,0]; this._time=0; this._last=0;
      this._onset=0; this._bassHistory=0; this._slow=0; this._frame=0;
      this._assemble=1; this._assembleTarget=1; this._assembleFrom=1;
      this._assembleStart=0; this._assembling=false;
      // Particle-text phase machine (converge → hold → dissolve → null).
      this._textPhase=null; this._textProg=0; this._textStart=0;
      this._textHold=1600; this._prevState='idle'; this._textPts=null;
      this._textPending=null; this._textPosArr=null;
      this._thinkStart=0; this._thinkIn=1;
      // Glitch transition: a short burst fired whenever the dominant base
      // state (idle/listening/thinking/speaking) changes (see _tick).
      this._lastDom=null; this._glitchStart=0; this._glitchAmt=0;
      this._tick=this._tick.bind(this);
      this._sync=this._sync.bind(this);
    }
    connectedCallback() {
      if (!this.shadowRoot) {
        const root=this.attachShadow({mode:'open'});
        // Constructed stylesheet (adoptedStyleSheets), not a parsed <style>
        // tag: the stage window's CSP injects a per-load nonce into
        // style-src for Tauri's own IPC init script, which per the CSP
        // spec makes 'unsafe-inline' inert for the WHOLE directive —
        // silently dropping any inline <style> content the same way it
        // drops inline style="" attributes. CSSStyleSheet.replaceSync is a
        // programmatic API, not inline-style text parsing, so it isn't
        // subject to this restriction (live bug, 2026-10-04).
        try {
          const sheet=new CSSStyleSheet();
          sheet.replaceSync(`:host{display:block;position:relative;width:100%;height:100%;aspect-ratio:1;contain:layout paint;pointer-events:none}
canvas{display:block;position:absolute;inset:0;width:100%;height:100%;pointer-events:none}`);
          root.adoptedStyleSheets=[sheet];
        } catch(e) {
          // Older engines without adoptedStyleSheets: fall back to a plain
          // <style> tag (works fine under a permissive/no-nonce CSP).
          const style=document.createElement('style');
          style.textContent=`:host{display:block;position:relative;width:100%;height:100%;aspect-ratio:1;contain:layout paint;pointer-events:none}
canvas{display:block;position:absolute;inset:0;width:100%;height:100%;pointer-events:none}`;
          root.appendChild(style);
        }
        // appendChild, not `innerHTML +=`: the latter re-serializes the
        // whole shadow root (including the fallback <style> tag above)
        // back to a string and re-parses it, hitting the exact same
        // CSP block a second time.
        root.appendChild(document.createElement('canvas')).setAttribute('aria-hidden','true');
        root.appendChild(document.createElement('canvas')).setAttribute('aria-hidden','true');
        [this._halo,this._canvas]=this.shadowRoot.querySelectorAll('canvas');
        this._hctx=this._halo.getContext('2d');
        this._setupRenderer();
        if(!this.hasAttribute('role'))this.setAttribute('role','img');
      }
      this._motion=matchMedia('(prefers-reduced-motion: reduce)');
      this._weights=weightsFor(this.state);
      this._motion.addEventListener('change',this._sync);
      document.addEventListener('visibilitychange',this._sync);
      this._observer=new ResizeObserver(()=>this._resize());
      this._observer.observe(this);
      this._resize();this._sync();this._label();
    }
    disconnectedCallback() {
      cancelAnimationFrame(this._frame);this._frame=0;this._last=0;
      this._observer?.disconnect();
      this._motion?.removeEventListener('change',this._sync);
      document.removeEventListener('visibilitychange',this._sync);
      this.disconnect();
    }
    get state() { return STATES.includes(this.getAttribute('state'))?this.getAttribute('state'):'idle'; }
    set state(value) { this.setAttribute('state',value); }
    get particles() {
      const fixed=Number(this.getAttribute('particles'));
      const count=this.hasAttribute('particles')&&Number.isFinite(fixed)?Math.round(clamp(fixed,200,20000)):this._auto;
      return this._gl ? (count||5000) : Math.min(count||1000,1500);
    }
    get bands() { return this._bands.slice(); }
    get audioContext() { return this._audio?.context || null; }
    get renderer() { return this._gl?'webgl':'canvas2d'; }
    attributeChangedCallback(name) {
      // An external state flip mid-text (e.g. the React prop or the orb
      // leaving speaking) cancels the text phase — the override wins and
      // the weights lerp straight to the new state. setText's own
      // 'text' write passes through (guard !== 'text').
      if(name==='state' && this._textPhase && this.getAttribute('state')!=='text') {
        this._textPhase=null;this._textProg=0;this._textPts=null;this._textPending=null;
      }
      if(!this._hctx)return;
      if(name==='state')this._label();
      if(name==='particles')this._count=0;
      if(name==='recording')this._sync();
      if(this._motion?.matches&&!this.hasAttribute('recording')) {
        this._weights=weightsFor(this.state);this._paint(0,this._weights,[0,0,0],0);
      }
    }
    _label() { this.setAttribute('aria-label',`Voice orb: ${this.state}`); }
    _setupRenderer() {
      const gl=this._canvas.getContext('webgl',{alpha:true,premultipliedAlpha:true,antialias:false,preserveDrawingBuffer:true,powerPreference:'high-performance'});
      try {
        if(!gl)throw new Error('WebGL unavailable');
        const shaders=[gl.VERTEX_SHADER,gl.FRAGMENT_SHADER].map((type,i)=>{
          const shader=gl.createShader(type);gl.shaderSource(shader,i?FS:VS);gl.compileShader(shader);
          if(!gl.getShaderParameter(shader,gl.COMPILE_STATUS))throw new Error(gl.getShaderInfoLog(shader));
          return shader;
        });
        const program=gl.createProgram();shaders.forEach(s=>gl.attachShader(program,s));gl.linkProgram(program);
        if(!gl.getProgramParameter(program,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(program));
        shaders.forEach(s=>gl.deleteShader(s));
        gl.useProgram(program);this._buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this._buffer);
        const attr=gl.getAttribLocation(program,'seed');gl.enableVertexAttribArray(attr);gl.vertexAttribPointer(attr,4,gl.FLOAT,false,0,0);
        this._scatterLoc=gl.getAttribLocation(program,'scatter');
        this._scatterBuf=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this._scatterBuf);
        gl.enableVertexAttribArray(this._scatterLoc);gl.vertexAttribPointer(this._scatterLoc,3,gl.FLOAT,false,0,0);
        this._textLoc=gl.getAttribLocation(program,'textPos');
        this._textBuf=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this._textBuf);
        gl.enableVertexAttribArray(this._textLoc);gl.vertexAttribPointer(this._textLoc,4,gl.FLOAT,false,0,0);
        gl.enable(gl.BLEND);gl.blendFunc(gl.ONE,gl.ONE);gl.disable(gl.DEPTH_TEST);
        this._gl=gl;this._program=program;this._uniforms={};
        for(const n of ['time','pixels','density','weights','bands','onset','assemble','tw','textProg','thinkIn','textTint','glitch'])this._uniforms[n]=gl.getUniformLocation(program,n);
        this._auto=matchMedia('(pointer: coarse)').matches?4000:5000;
        if(!this._lossHandler) {
          this._lossHandler=e=>{e.preventDefault();this._lost=true;this._sync();};
          this._restoreHandler=()=>{this._lost=false;this._count=0;this._setupRenderer();this._resize();this._sync();};
          this._canvas.addEventListener('webglcontextlost',this._lossHandler);
          this._canvas.addEventListener('webglcontextrestored',this._restoreHandler);
        }
      } catch(error) {
        this._gl=null;this._auto=1000;
        // A canvas cannot switch context types after WebGL initialization.
        const replacement=document.createElement('canvas');replacement.setAttribute('aria-hidden','true');
        this._canvas.replaceWith(replacement);this._canvas=replacement;this._ctx=replacement.getContext('2d');
      }
      this._count=0;
    }
    pause() {
      this._paused = true;
      cancelAnimationFrame(this._frame);
      this._frame = 0;
      this._last = 0;
    }
    play() {
      this._paused = false;
      this._resize();
      this._sync();
    }
    // Ghost enter: snap to scattered, then fly in and form the circle (~900ms).
    // Reduced-motion users skip the flight (their rAF loop is parked).
    assemble() {
      if(this._motion?.matches){this._assemble=1;return;}
      this._assemble=0;this._assembleFrom=0;this._assembleTarget=1;
      this._assembleStart=performance.now();this._assembling=true;
      if(!this._frame)this._sync();
    }
    // Ghost exit: scatter back out (~900ms). Not wired by Avatar yet —
    // ghost exit morphs sphere-to-sphere seamlessly instead.
    disperse() {
      this._assembleFrom=this._assemble;this._assembleTarget=0;
      this._assembleStart=performance.now();this._assembling=true;
      if(!this._frame)this._sync();
    }
    // ─── Particle-generated text (Feature 88) ─────────────────────
    // The SAME cloud particles detach, converge into readable glyphs,
    // hold, then dissolve back into the previous state. No overlay text,
    // no crossfade — one continuous particle simulation.
    // Returns false when the text renders empty (no-canvas graceful path).
    setText(text, holdMs = 1600) {
      const pts = sampleTextPoints(text);
      if(!pts)return false;
      this._prevState = this.state==='text' ? (this._prevState||'idle') : this.state;
      this._textHold = Math.max(600, holdMs);
      this._textPts = pts;
      this._textPending = pts;
      if(this._motion?.matches) {
        // Reduced motion: snap the glyphs formed (no travel), hold as a
        // static frame, then revert via a timeout (the rAF loop is parked).
        this._textProg=1;this._textPhase='hold';this._textStart=performance.now();
        this.state='text';
        this._paint(0,this._weights,[0,0,0],0);
        setTimeout(()=>{
          if(this._textPhase==='hold'){
            this._textPhase=null;this._textProg=0;this._textPts=null;this._textPending=null;
            if(this.state==='text'){this.state=this._prevState;this._label();}
            this._sync();
          }
        },this._textHold+2000);
        return true;
      }
      this._textProg=0;this._textStart=performance.now();this._textPhase='converge';
      this.state='text';
      if(!this._frame)this._sync();
      return true;
    }
    _resize() {
      const rect = this.getBoundingClientRect();
      const cssW = this.clientWidth || rect.width || 180;
      const size = Math.max(80, Math.round(cssW * Math.min(devicePixelRatio || 1, 2)));
      if(this._canvas.width!==size||this._canvas.height!==size) {
        this._canvas.width=this._canvas.height=this._halo.width=this._halo.height=size;
      }
      if(this.hasAttribute('recording')&&this._snapshot) {
        const f=this._snapshot;this._paint(f.time,f.weights,f.bands,f.onset);
      } else this._paint(this._motion?.matches?0:this._time,this._weights,this._motion?.matches?[0,0,0]:this._bands,this._onset);
    }
    _sync() {
      cancelAnimationFrame(this._frame);this._frame=0;this._last=0;
      if(!this.isConnected||this.hasAttribute('recording')||this._paused||this._lost)return;
      if(this._motion?.matches) { this._weights=weightsFor(this.state);this._paint(0,this._weights,[0,0,0],0);return; }
      this._frame=requestAnimationFrame(this._tick);
    }
    _tick(now) {
      const elapsed=this._last?(now-this._last)/1000:1/60;
      const dt=Math.min(.08,elapsed);this._last=now;this._time+=dt;
      if(!this.hasAttribute('particles')) {
        this._slow=elapsed>.026?this._slow+1:Math.max(0,this._slow-1);
        if(this._slow>=45&&this._auto>800) {this._auto=Math.max(800,Math.round(this._auto*.7));this._slow=0;}
      }
      const k=1-Math.exp(-dt/.24),target=weightsFor(this.state);
      for(let i=0;i<5;i++)this._weights[i]+=(target[i]-this._weights[i])*k;
      if(this._assembling) {
        const p=Math.min(1,(now-this._assembleStart)/900);
        const e=p<0.5?4*p*p*p:1-Math.pow(-2*p+2,3)/2;
        this._assemble=this._assembleFrom+(this._assembleTarget-this._assembleFrom)*e;
        if(p>=1){this._assembling=false;this._assemble=this._assembleTarget;}
      }
      // Thinking entry: contract-then-expand (sphere pulls inward, then the
      // knot stretches out over ~700ms). Resets when the state leaves.
      if(this.state==='thinking'){if(!this._thinkStart)this._thinkStart=now;}
      else if(this._thinkStart)this._thinkStart=0;
      const tp2=this._thinkStart?Math.min(1,(now-this._thinkStart)/700):1;
      this._thinkIn=tp2*tp2*(3-2*tp2);
      // Glitch transition: fire a short (~220ms) burst whenever the
      // dominant base-state weight flips (naturally lands mid-blend, when
      // the new state's weight crosses the old one — not at the instant
      // the state attribute changes). Suspended during particle-text
      // (weights[0..3] all decay toward 0 there, which would otherwise
      // read as spurious flips) and on the very first tick (no glitch at
      // mount — only on an actual value change).
      if(this.state!=='text') {
        let dom4=0;
        for(let i=1;i<4;i++)if(this._weights[i]>this._weights[dom4])dom4=i;
        if(this._lastDom!==null&&dom4!==this._lastDom)this._glitchStart=now;
        this._lastDom=dom4;
      }
      const gt=this._glitchStart?(now-this._glitchStart)/220:2;
      this._glitchAmt=(gt>=0&&gt<=1)?Math.sin(gt*Math.PI):0;
      // Particle-text phase machine: converge (~700ms) → hold → dissolve
      // (~600ms, state reverts so the sphere reforms while particles
      // stream back with a swirl burst) → cleanup.
      if(this._textPhase==='converge') {
        const p=Math.min(1,(now-this._textStart)/700);
        this._textProg=p<0.5?4*p*p*p:1-Math.pow(-2*p+2,3)/2;
        if(p>=1){this._textPhase='hold';this._textStart=now;}
      } else if(this._textPhase==='hold') {
        this._textProg=1;
        if(now-this._textStart>=this._textHold) {
          this._textPhase='dissolve';this._textStart=now;
          if(this.state==='text'){this.state=this._prevState;this._label();}
        }
      } else if(this._textPhase==='dissolve') {
        const p=Math.min(1,(now-this._textStart)/600);
        this._textProg=1-p;
        if(p>=1){this._textPhase=null;this._textProg=0;this._textPts=null;this._textPending=null;}
      }
      this._readAudio(dt);
      this._paint(this._time,this._weights,this._bands,this._onset);
      this._frame=requestAnimationFrame(this._tick);
    }
    renderAt(time, {weights=weightsFor(this.state),bands=[0,0,0],onset=0}={}) {
      const frame={time:Number.isFinite(+time)?+time:0,weights:normalize(weights),bands:[0,1,2].map(i=>clamp(bands[i])),onset:clamp(onset)};
      this._snapshot=frame;
      if(this._hctx){this._paint(frame.time,frame.weights,frame.bands,frame.onset);this._gl?.flush();}
    }
    _paint(time,weights,bands,onset) {
      if(this._lost)return;
      const size=this._canvas.width,count=this.particles;
      if(count!==this._count||this._textPending) {
        this._count=count;this._seeds=sphere(count);this._scat=scatterOf(count);
        this._textPosArr=textOf(count,this._textPending||this._textPts);
        this._textPending=null;
        if(this._gl){
          const gl=this._gl;
          gl.bindBuffer(gl.ARRAY_BUFFER,this._buffer);gl.bufferData(gl.ARRAY_BUFFER,this._seeds,gl.STATIC_DRAW);
          gl.bindBuffer(gl.ARRAY_BUFFER,this._scatterBuf);gl.bufferData(gl.ARRAY_BUFFER,this._scat,gl.STATIC_DRAW);
          gl.bindBuffer(gl.ARRAY_BUFFER,this._textBuf);gl.bufferData(gl.ARRAY_BUFFER,this._textPosArr,gl.STATIC_DRAW);
        }
      }
      const rgb=[0,1,2].map(c=>Math.round(PALETTE.reduce((sum,p,i)=>sum+p[c]*weights[i],0)*255));
      // Text inherits the STATE palette (Feature 89): the dominant base
      // state's color becomes the textTint — glyphs formed during
      // listening are gold/white, thinking purple/blue, speaking
      // magenta/white. No separate text color exists.
      const w4=weights[4]||0;
      const inv=w4<0.999?1/(1-w4):1;
      const wMain=weights.slice(0,4).map(v=>v*inv);
      let dom=0,domV=-1;
      for(let i=0;i<4;i++){const v=wMain[i];if(v>domV){domV=v;dom=i;}}
      const stateTint=PALETTE[dom];
      const tintRgb=[0,1,2].map(c=>Math.round((PALETTE.reduce((sum,p,i)=>i<4?sum+p[c]*weights[i]:sum,0)*inv+stateTint[c]*w4)*255));
      const h=this._hctx;h.clearRect(0,0,size,size);
      const glow=h.createRadialGradient(size*.5,size*.5,size*.10,size*.5,size*.5,size*.48);
      const energy=(weights[1]+weights[3])*(bands[0]*.025+onset*.035);
      glow.addColorStop(0,`rgba(${tintRgb},.012)`);glow.addColorStop(.58,`rgba(${tintRgb},${.028+energy})`);glow.addColorStop(1,`rgba(${tintRgb},0)`);
      h.fillStyle=glow;h.fillRect(0,0,size,size);
      if(this._gl) {
        const gl=this._gl,u=this._uniforms;gl.viewport(0,0,size,size);gl.clearColor(0,0,0,0);gl.clear(gl.COLOR_BUFFER_BIT);
        gl.useProgram(this._program);gl.uniform1f(u.time,time);gl.uniform1f(u.pixels,size);
        gl.uniform1f(u.density,Math.pow(5000/count,.32));        gl.uniform4fv(u.weights,wMain);gl.uniform3fv(u.bands,bands);gl.uniform1f(u.onset,onset);
        gl.uniform1f(u.assemble,this._assemble);
        gl.uniform1f(u.tw,w4);
        gl.uniform1f(u.textProg,this._textProg||0);
        gl.uniform1f(u.thinkIn,this._thinkIn==null?1:this._thinkIn);
        gl.uniform3fv(u.textTint,stateTint);
        gl.uniform1f(u.glitch,this._glitchAmt||0);
        gl.drawArrays(gl.POINTS,0,count);
      } else this._paint2D(time,wMain,bands,onset,tintRgb,size,w4,this._textProg||0,this._thinkIn==null?1:this._thinkIn,this._textPosArr,this._glitchAmt||0);
    }
    _paint2D(t,w,b,onset,rgb,size,tw=0,textProg=0,thinkIn=1,textArr=null,glitchAmt=0) {
      const ctx=this._ctx;ctx.clearRect(0,0,size,size);ctx.globalCompositeOperation='lighter';
      ctx.fillStyle=`rgb(${rgb})`;
      for(let i=0;i<this._count;i++) {
        const o=i*4,n=this._seeds,x=n[o],y=n[o+1],z=n[o+2],u=n[o+3];
        const a=t*(.11*w[0]+.16*w[1]+.35*w[3]);
        const c=Math.cos(a),sn=Math.sin(a),angle=Math.acos(clamp(z,-1,1));
        const drift=Math.sin(x*3+t*.3)*Math.cos(y*3-t*.2)*Math.sin(z*3+t*.1);
        const wave=Math.sin(angle*13+(w[1]-w[3])*t*5.8+drift*1.6);
        const active=w[1]+w[3];
        let r=1+.018*drift+active*(b[0]*(.08*drift+.03)+b[1]*wave*.15+b[2]*Math.sin(y*25-t*5)*.035);
        let px=(x*c+z*sn)*r,pz=(z*c-x*sn)*r,py=y*r;
        let speakFace=0,speakBump=0;
        if(w[3]>.001) {
          // Dense irregular potato/pebble blob (parity with the GL path):
          // a dominant low-frequency noise octave produces fewer, bigger,
          // more irregular lobes; a smaller secondary octave plus
          // high-frequency grain add surface sparkle.
          const lobeNoise=vnoise(x*0.65+t*.22,y*0.65-t*.17,z*0.65+t*.13);
          const lobeNoise2=vnoise(x*0.85-t*.10,y*0.85+t*.14,z*0.85-t*.09);
          const energy=.35+.65*Math.max(b[0],b[1]);
          const grain2=vnoise(x*17.0-t*1.8,y*17.0+t*.7,z*17.0+t*1.0);
          // Beat-driven mold + text-legibility damping (parity with the
          // GL path).
          const textDamp=1.0-0.75*textProg;
          const rad=1.0
            +.40*lobeNoise*(1.0+.4*onset)*textDamp
            +.10*lobeNoise2
            +energy*.05*Math.sin(angle*5.0+t*1.4+drift*1.2)
            +b[2]*.022*grain2
            +.22*onset*textDamp;
          const rot=t*.24+.04*Math.sin(t*.22);
          const rc=Math.cos(rot),rs=Math.sin(rot);
          const sx=x*rad,sy=y*rad,sz=z*rad;
          const bx=sx*rc+sz*rs,bz=-sx*rs+sz*rc,by=sy;
          speakFace=z;
          speakBump=.5+.5*lobeNoise;
          px=px*(1-w[3])+bx*w[3];
          py=py*(1-w[3])+by*w[3];
          pz=pz*(1-w[3])+bz*w[3];
        }
        if(w[2]>.001) {
          // 3D rotating purple starburst (parity with GL path): 64 rays,
          // dense glowing nucleus, 15 concentric beaded steps, dual-axis 3D rotation.
          const STRANDS=64.0;
          const su=u*STRANDS,strandIdx=Math.floor(su),sAlong=su-strandIdx;
          const sy=1.0-2.0*(strandIdx+0.5)/STRANDS;
          const sr=Math.sqrt(Math.max(0,1.0-sy*sy));
          const sang=strandIdx*2.399963229728653;
          const dx=sr*Math.cos(sang),dy=sy,dz=sr*Math.sin(sang);
          const [p1x,p1y,p1z]=normalize3(...cross3(dx,dy,dz,0,1,0));
          const [p2x,p2y,p2z]=cross3(dx,dy,dz,p1x,p1y,p1z);
          let sx0,sy0,sz0;
          if(sAlong<0.14) {
            const cFrac=sAlong/0.14;
            const cAngle=seeded(i*13+strandIdx*19)*6.2831;
            const cJitter=(0.02+0.04*seeded(i*29+strandIdx*37))*(1.0-cFrac*0.5);
            sx0=dx*(cFrac*0.13)+(p1x*Math.cos(cAngle)+p2x*Math.sin(cAngle))*cJitter;
            sy0=dy*(cFrac*0.13)+(p1y*Math.cos(cAngle)+p2y*Math.sin(cAngle))*cJitter;
            sz0=dz*(cFrac*0.13)+(p1z*Math.cos(cAngle)+p2z*Math.sin(cAngle))*cJitter;
          } else {
            const tRay=(sAlong-0.14)/0.86;
            const beadIdx=Math.floor(tRay*15.0);
            const rNorm=(beadIdx+0.5)/15.0;
            let rReach=0.14+Math.pow(rNorm,1.08)*0.91;
            const pulse=Math.sin(rNorm*18.0-t*3.5);
            rReach+=pulse*0.015;
            const beamWidth=(0.007+0.005*rNorm)*(1.0+0.3*Math.sin(tRay*31.0));
            const bAngle=seeded(i*17+strandIdx*23+beadIdx*41)*6.2831;
            const bx=(p1x*Math.cos(bAngle)+p2x*Math.sin(bAngle))*beamWidth;
            const by=(p1y*Math.cos(bAngle)+p2y*Math.sin(bAngle))*beamWidth;
            const bz=(p1z*Math.cos(bAngle)+p2z*Math.sin(bAngle))*beamWidth;
            sx0=dx*rReach+bx;
            sy0=dy*rReach+by;
            sz0=dz*rReach+bz;
          }
          // Continuous 3D rotation: yaw (t * 0.45) and pitch (t * 0.28)
          const yaw=t*0.45,cy=Math.cos(yaw),sy_rot=Math.sin(yaw);
          const pitch=t*0.28,cp=Math.cos(pitch),sp_rot=Math.sin(pitch);
          const x1=cy*sx0+sy_rot*sz0;
          const y1=sy0;
          const z1=cy*sz0-sy_rot*sx0;
          const tx0=x1;
          const ty0=cp*y1-sp_rot*z1;
          const tz0=cp*z1+sp_rot*y1;
          const scale=mixNum(0.42,1.0,thinkIn);
          const bx=tx0*scale,by=ty0*scale,bz=tz0*scale;
          px=px*(1-w[2])+bx*w[2];
          py=py*(1-w[2])+by*w[2];
          pz=pz*(1-w[2])+bz*w[2];
        }
        let ambF=0;
        if(tw>0.001&&textArr) {
          // Particle text (parity with the GL path): glyph particles
          // converge with a staggered swirl; ambient ones drift to the halo.
          const ti=i*4;
          const isG=textArr[ti+3]<0.5;
          const tst=seeded(i+293);
          const tp0=clamp((textProg-tst*0.30)/0.70);
          const te2=tp0*tp0*(3-2*tp0);
          const swx=Math.sin(t*2.8+tst*6.2831)*0.42*(1-te2);
          const swy=Math.cos(t*2.4+tst*6.2831)*0.42*(1-te2);
          const tx=isG?textArr[ti]+swx:textArr[ti];
          const ty2=isG?textArr[ti+1]+swy:textArr[ti+1];
          const tz2=textArr[ti+2];
          px=px*(1-tw)+tx*tw;
          py=py*(1-tw)+ty2*tw;
          pz=pz*(1-tw)+tz2*tw;
          ambF=(isG?0:1)*clamp(tw*1.6);
        }
        if(this._assemble<1&&this._scat) {
          const stag=seeded(i+143);
          const ap=clamp((this._assemble-stag*0.35)/0.65);
          const ae=ap*ap*(3-2*ap);
          const swx=Math.sin(t*3+stag*6.2831)*0.35*(1-ae);
          const swy=Math.cos(t*2.6+stag*6.2831)*0.35*(1-ae);
          px=px*ae+(this._scat[i*3]+swx)*(1-ae);
          py=py*ae+(this._scat[i*3+1]+swy)*(1-ae);
          pz=pz*ae+this._scat[i*3+2]*(1-ae);
        }
        if(glitchAmt>0.001) {
          // Glitch transition (parity with the GL path): fast-changing
          // per-particle jitter burst, quantized to ~90Hz so it reads as
          // flickery digital noise rather than smooth drift.
          const gq=Math.floor(t*90);
          px+=(hash3(i,gq,1)-.5)*glitchAmt*0.18;
          py+=(hash3(i,gq,2)-.5)*glitchAmt*0.18;
          pz+=(hash3(i,gq,3)-.5)*glitchAmt*0.18;
        }
        const perspective=3.8/(3.8-pz*.6),rim=Math.pow(1-Math.abs(z),2);
        const dot=size/720*(.7+.3*(pz+1)+rim*.6)*(1-.28*w[3])*mixNum(1.0,.62,ambF);
        ctx.globalAlpha=clamp((.20+.24*(pz+1)+rim*.3+w[2]*.4+w[3]*(speakFace*.22+speakBump*.15)+tw*(.30+.25*(pz+1)))*mixNum(1.0,.32,ambF));
        ctx.beginPath();ctx.arc(size*(.5+px*perspective*.305),size*(.5-py*perspective*.305),dot,0,Math.PI*2);ctx.fill();
      }
      ctx.globalAlpha=1;ctx.globalCompositeOperation='source-over';
    }

    async connect(source) {
      const isStream=typeof MediaStream!=='undefined'&&source instanceof MediaStream;
      const isMedia=typeof HTMLMediaElement!=='undefined'&&source instanceof HTMLMediaElement;
      const isNode=source&&typeof source.connect==='function'&&source.context&&typeof source.context.createAnalyser==='function';
      if(!isStream&&!isMedia&&!isNode)throw new TypeError('connect() needs a MediaStream, HTMLMediaElement or AudioNode.');
      const AC=window.AudioContext||window.webkitAudioContext;
      if(!AC)throw new Error('Web Audio is unavailable in this browser.');
      this.disconnect();
      let context,node,owned=false;
      if(isNode) {context=source.context;node=source;}
      else if(isMedia) {
        let entry=mediaSources.get(source);
        if(!entry) {
          context=new AC();
          try{node=context.createMediaElementSource(source);}catch(e){await context.close();throw e;}
          // Keep normal playback when the analysis tap is detached or the element is removed.
          node.connect(context.destination);entry={context,node};mediaSources.set(source,entry);
        }
        ({context,node}=entry);
      } else {context=new AC();owned=true;node=context.createMediaStreamSource(source);}
      const analyser=context.createAnalyser();analyser.fftSize=2048;analyser.smoothingTimeConstant=0;
      analyser.minDecibels=-85;analyser.maxDecibels=-10;
      // A silent output keeps stream analysis running without monitoring the microphone.
      const mute=context.createGain();mute.gain.value=0;analyser.connect(mute);mute.connect(context.destination);
      node.connect(analyser);
      const audio={context,node,analyser,mute,owned,data:new Float32Array(analyser.frequencyBinCount)};
      this._audio=audio;
      try {if(context.state!=='running')await context.resume();}
      catch(e){if(this._audio===audio)this.disconnect();throw e;}
      return this;
    }
    disconnect() {
      const a=this._audio;this._audio=null;
      if(a) {
        try{a.node.disconnect(a.analyser);}catch(_){}
        a.analyser.disconnect();a.mute.disconnect();
        if(a.owned&&a.context.state!=='closed')a.context.close().catch(()=>{});
      }
      this._bands=[0,0,0];this._onset=0;this._bassHistory=0;
    }
    setLevel(vol) {
      const v = clamp(vol);
      this._manualRaw = [v * 1.0, v * 0.85, v * 0.70];
    }
    _readAudio(dt) {
      const raw=[0,0,0],a=this._audio;
      if(a?.context.state==='running') {
        a.analyser.getFloatFrequencyData(a.data);
        const limits=[[45,250],[250,2400],[2400,12000]],step=a.context.sampleRate/a.analyser.fftSize;
        for(let k=0;k<3;k++) {
          const lo=Math.max(1,Math.ceil(limits[k][0]/step)),hi=Math.min(a.data.length-1,Math.floor(limits[k][1]/step));
          let power=0;
          for(let j=lo;j<=hi;j++)power+=Math.pow(10,a.data[j]/10);
          // Sum power so a narrow bass note is not diluted by unused FFT bins.
          const amplitude=Math.sqrt(power);
          raw[k]=clamp((Math.sqrt(amplitude)*[2.0,2.15,2.5][k]-.025)/.975);
        }
      } else if(this._manualRaw) {
        raw[0]=this._manualRaw[0];
        raw[1]=this._manualRaw[1];
        raw[2]=this._manualRaw[2];
        this._manualRaw=null;
      }
      // A fast baseline cancels a rolling 808 line, so the kick on top of it still reads as a hit.
      const flux=Math.max(0,raw[0]-this._bassHistory-.04)*6;
      this._bassHistory+=(raw[0]-this._bassHistory)*(1-Math.exp(-dt/.06));
      this._onset=Math.max(clamp(flux),this._onset*Math.exp(-dt/.18));
      const attack=[.012,.028,.006],release=[.20,.15,.085];
      for(let k=0;k<3;k++)this._bands[k]+=(raw[k]-this._bands[k])*(1-Math.exp(-dt/(raw[k]>this._bands[k]?attack[k]:release[k])));
    }
  }
  if (typeof customElements !== 'undefined') {
    customElements.define('voice-orb',VoiceOrb);
  }
})();

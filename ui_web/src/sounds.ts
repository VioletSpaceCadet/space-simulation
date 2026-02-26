let audioCtx: AudioContext | null = null;

function ctx(): AudioContext | null {
  if (typeof AudioContext === 'undefined') {return null;}
  if (!audioCtx) {audioCtx = new AudioContext();}
  return audioCtx;
}

function playBeep(frequency: number, durationMs: number, volume = 0.3, type: OscillatorType = 'square') {
  const ac = ctx();
  if (!ac) {return;}
  if (ac.state === 'suspended') {ac.resume();}
  const t = ac.currentTime;
  const dur = durationMs / 1000;
  const osc = ac.createOscillator();
  const gain = ac.createGain();
  osc.type = type;
  osc.frequency.value = frequency;
  // Hold volume, then quick fade at the end to avoid click
  gain.gain.setValueAtTime(volume, t);
  gain.gain.setValueAtTime(volume, t + dur * 0.8);
  gain.gain.linearRampToValueAtTime(0, t + dur);
  osc.connect(gain);
  gain.connect(ac.destination);
  osc.start(t);
  osc.stop(t + dur);
}

export function playPause() {
  // Short percussive click — noise burst
  const ac = ctx();
  if (!ac) {return;}
  if (ac.state === 'suspended') {ac.resume();}
  const t = ac.currentTime;
  const bufferSize = ac.sampleRate * 0.03;
  const buffer = ac.createBuffer(1, bufferSize, ac.sampleRate);
  const data = buffer.getChannelData(0);
  for (let i = 0; i < bufferSize; i++) {data[i] = (Math.random() * 2 - 1) * (1 - i / bufferSize);}
  const source = ac.createBufferSource();
  source.buffer = buffer;
  const gain = ac.createGain();
  gain.gain.setValueAtTime(0.15, t);
  const filter = ac.createBiquadFilter();
  filter.type = 'bandpass';
  filter.frequency.value = 800;
  filter.Q.value = 1;
  source.connect(filter);
  filter.connect(gain);
  gain.connect(ac.destination);
  source.start(t);
}

export function playResume() {
  playPause();
}

export function playSave() {
  playBeep(880, 100, 0.15, 'sine');
  setTimeout(() => playBeep(1320, 150, 0.15, 'sine'), 100);
}

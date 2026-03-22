import { STAR_TILE_SIZE } from './types';

/** Generate a 1024x1024 star tile as a data URL. Deterministic (seeded RNG). */
export function generateStarTile(): string {
  const canvas = document.createElement('canvas');
  const dpr = window.devicePixelRatio || 1;
  canvas.width = STAR_TILE_SIZE * dpr;
  canvas.height = STAR_TILE_SIZE * dpr;
  const ctx = canvas.getContext('2d');
  if (!ctx) { return ''; }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

  // Deterministic seeded random
  let seed = 48271;
  function sr(): number {
    seed = (seed * 16807) % 2147483647;
    return seed / 2147483647;
  }

  // Layer 1: dim background stars (many, tiny)
  for (let i = 0; i < 300; i++) {
    ctx.beginPath();
    ctx.arc(
      sr() * STAR_TILE_SIZE,
      sr() * STAR_TILE_SIZE,
      sr() * 0.6 + 0.2,
      0,
      Math.PI * 2,
    );
    ctx.fillStyle = `rgba(180,190,210,${sr() * 0.12 + 0.03})`;
    ctx.fill();
  }

  // Layer 2: medium stars
  for (let i = 0; i < 60; i++) {
    ctx.beginPath();
    ctx.arc(
      sr() * STAR_TILE_SIZE,
      sr() * STAR_TILE_SIZE,
      sr() * 0.8 + 0.4,
      0,
      Math.PI * 2,
    );
    ctx.fillStyle = `rgba(200,210,230,${sr() * 0.18 + 0.08})`;
    ctx.fill();
  }

  // Layer 3: bright accent stars (few, larger, warm/cool tinted)
  for (let i = 0; i < 12; i++) {
    const x = sr() * STAR_TILE_SIZE;
    const y = sr() * STAR_TILE_SIZE;
    const r = sr() * 0.6 + 0.8;
    const brightness = sr() * 0.15 + 0.2;
    const warm = sr() > 0.5;
    ctx.beginPath();
    ctx.arc(x, y, r, 0, Math.PI * 2);
    ctx.fillStyle = warm
      ? `rgba(230,215,190,${brightness})`
      : `rgba(190,210,240,${brightness})`;
    ctx.fill();
  }

  return canvas.toDataURL();
}

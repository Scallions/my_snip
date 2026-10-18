export function themedPixels(input: Uint8ClampedArray, width: number, height: number, dark: boolean): Uint8ClampedArray {
  const samples: number[] = [];
  const luminance = (i: number) => input[i] * 0.2126 + input[i + 1] * 0.7152 + input[i + 2] * 0.0722;
  const step = Math.max(1, Math.floor(Math.max(width, height) / 60));
  for (let x = 0; x < width; x += step) { samples.push(luminance(x * 4), luminance(((height - 1) * width + x) * 4)); }
  for (let y = 0; y < height; y += step) { samples.push(luminance(y * width * 4), luminance((y * width + width - 1) * 4)); }
  samples.sort((a, b) => a - b);
  const background = samples[Math.floor(samples.length / 2)] ?? 255;
  const sourceIsLight = background >= 128;
  const bg = dark ? [33, 37, 41] : [249, 250, 251];
  const fg = dark ? [220, 225, 229] : [42, 48, 54];
  const out = new Uint8ClampedArray(input.length);
  for (let i = 0; i < input.length; i += 4) {
    const value = luminance(i);
    const ink = Math.max(0, Math.min(1, sourceIsLight ? (background - value) / Math.max(background, 1) : (value - background) / Math.max(255 - background, 1))) * input[i + 3] / 255;
    out[i] = bg[0] + (fg[0] - bg[0]) * ink;
    out[i + 1] = bg[1] + (fg[1] - bg[1]) * ink;
    out[i + 2] = bg[2] + (fg[2] - bg[2]) * ink;
    out[i + 3] = 255;
  }
  return out;
}

import { describe, expect, it } from 'vitest';
import { themedPixels } from './imageTheme';

describe('screenshot display theme', () => {
  it('maps both light and already-dark source images to a dark preview without changing input', () => {
    for (const inverted of [false, true]) {
      const input = new Uint8ClampedArray(3 * 3 * 4);
      for (let i = 0; i < input.length; i += 4) input.set([inverted ? 0 : 255, inverted ? 0 : 255, inverted ? 0 : 255, 255], i);
      input.set([inverted ? 255 : 0, inverted ? 255 : 0, inverted ? 255 : 0, 255], 16);
      const before = input.slice();
      const output = themedPixels(input, 3, 3, true);
      expect(output.slice(0, 3)).toEqual(new Uint8ClampedArray([33, 37, 41]));
      expect(output.slice(16, 19)).toEqual(new Uint8ClampedArray([220, 225, 229]));
      expect(input).toEqual(before);
    }
  });
});

<script lang="ts">
  import { onMount } from 'svelte';
  import { IconPalette } from '@tabler/icons-svelte';
  import { themedPixels } from '../lib/imageTheme';
  let { source }: { source: string } = $props();
  let canvas: HTMLCanvasElement;
  let original = $state(false);
  let failed = $state(false);
  let dark = $state(false);
  let image: HTMLImageElement | null = null;
  let revision = 0;

  function draw() {
    if (!canvas || !image?.complete || !image.naturalWidth) return;
    const ratio = Math.min(1, 1800 / Math.max(image.naturalWidth, image.naturalHeight));
    canvas.width = Math.max(1, Math.round(image.naturalWidth * ratio));
    canvas.height = Math.max(1, Math.round(image.naturalHeight * ratio));
    const context = canvas.getContext('2d', { willReadFrequently: true });
    if (!context) return;
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    if (!original) {
      try {
        const data = context.getImageData(0, 0, canvas.width, canvas.height);
        const transformed = themedPixels(data.data, canvas.width, canvas.height, dark);
        data.data.set(transformed); context.putImageData(data, 0, 0);
      } catch { /* Keep the unchanged image visible if a browser restricts canvas reads. */ }
    }
  }
  onMount(() => {
    const media = matchMedia('(prefers-color-scheme: dark)');
    const change = () => { dark = document.documentElement.dataset.theme ? document.documentElement.dataset.theme === 'dark' : media.matches; draw(); };
    change(); media.addEventListener('change', change);
    return () => { media.removeEventListener('change', change); if (image) image.src = ''; if (canvas) { canvas.width = 0; canvas.height = 0; } };
  });
  $effect(() => {
    const url = source; const current = ++revision;
    failed = false;
    const next = new Image(); next.crossOrigin = 'anonymous';
    next.onload = () => { if (current === revision) { image = next; draw(); } };
    next.onerror = () => { if (current === revision) failed = true; };
    next.src = url;
    return () => { next.onload = null; next.onerror = null; next.src = ''; };
  });
  $effect(() => { original; dark; draw(); });
</script>

<div class="screenshot-preview" class:original>
  {#if failed}<p class="muted">截图预览暂不可用</p>{/if}
  <canvas bind:this={canvas} aria-label="公式截图预览">公式截图预览</canvas>
  <button class="icon-button original-toggle" title={original ? '使用主题预览' : '查看截图原色'} aria-label={original ? '使用主题预览' : '查看截图原色'} aria-pressed={original} onclick={() => original = !original}>
    <IconPalette size={16} stroke={1.6}/>
  </button>
</div>

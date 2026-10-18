<script lang="ts">
  import { onMount } from 'svelte';
  import katex from 'katex';
  let { latex, small = false }: { latex: string; small?: boolean } = $props();
  let viewport = $state<HTMLDivElement>();
  let content = $state<HTMLDivElement>();
  let scale = $state(1);
  let frame = 0;

  function scheduleFit() {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (!viewport || !content) return;
      const style = getComputedStyle(viewport);
      const width = viewport.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
      const height = viewport.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
      // Measure the unscaled layout, so each edit/resize can grow the preview
      // back to its normal size as well as shrink it to fit both dimensions.
      const naturalWidth = Math.max(content.offsetWidth, content.scrollWidth);
      const naturalHeight = Math.max(content.offsetHeight, content.scrollHeight);
      if (width > 0 && height > 0 && naturalWidth > 0 && naturalHeight > 0) {
        scale = Math.min(1, width / naturalWidth, height / naturalHeight);
      }
    });
  }

  $effect(() => {
    const host = viewport;
    const formula = content;
    rendering.html;
    if (!host || !formula) return;
    const observer = new ResizeObserver(scheduleFit);
    observer.observe(host);
    observer.observe(formula);
    scheduleFit();
    return () => observer.disconnect();
  });

  onMount(() => {
    let disposed = false;
    const fontsLoaded = () => { if (!disposed) scheduleFit(); };
    document.fonts.ready.then(fontsLoaded);
    document.fonts.addEventListener('loadingdone', fontsLoaded);
    return () => {
      disposed = true;
      cancelAnimationFrame(frame);
      document.fonts.removeEventListener('loadingdone', fontsLoaded);
    };
  });
  let rendering = $derived.by(() => {
    if (!latex.trim()) return { html: '', error: '' };
    if (latex.length > 16000) return { html: '', error: '公式过长，请缩小范围。' };
    try {
      return { html: katex.renderToString(latex, { displayMode: true, throwOnError: true, trust: false, strict: 'warn', maxExpand: 500, maxSize: 30 }), error: '' };
    } catch { return { html: '', error: '公式暂时无法排版，请检查 LaTeX。' }; }
  });
</script>

<div class:small class="equation" bind:this={viewport} aria-label={latex || '等待公式'}>
  {#if rendering.html}
    <div class="equation-content" bind:this={content} style:transform={`translate(-50%, -50%) scale(${scale})`}>
      {@html rendering.html}
    </div>
  {:else if rendering.error}
    <p class="render-error">{rendering.error}</p>
  {:else}
    <span class="empty-equation">等待识别结果</span>
  {/if}
</div>

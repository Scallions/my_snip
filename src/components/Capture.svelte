<script lang="ts">
  import { onMount } from 'svelte';
  import { IconX, IconLoader2 } from '@tabler/icons-svelte';
  import { invoke, imageUrl } from '../lib/bridge';
  import { appError, type AppError, type ViewState } from '../lib/types';
  let { view }: { view: ViewState } = $props();
  let start = $state<{ x: number; y: number } | null>(null);
  let end = $state<{ x: number; y: number } | null>(null);
  let submitting = $state(false);
  let error = $state<AppError | null>(null);
  let surface: HTMLDivElement;
  let rect = $derived(start && end ? { x: Math.min(start.x, end.x), y: Math.min(start.y, end.y), width: Math.abs(end.x - start.x), height: Math.abs(end.y - start.y) } : null);
  function down(e: PointerEvent) {
    if (e.button !== 0 || submitting || (e.target as HTMLElement).closest('button')) return;
    e.preventDefault(); error = null; start = { x: e.clientX, y: e.clientY }; end = { ...start };
    surface.setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (!start || submitting) return;
    end = { x: Math.max(0, Math.min(innerWidth, e.clientX)), y: Math.max(0, Math.min(innerHeight, e.clientY)) };
  }
  async function up(e: PointerEvent) {
    if (!start || !rect || submitting || !view.capture) return;
    surface.releasePointerCapture(e.pointerId);
    if (rect.width < 4 || rect.height < 4) { start = null; end = null; return; }
    submitting = true;
    try { await invoke('commit_selection', { generation: view.capture.generation, selection: { ...rect, viewportWidth: innerWidth, viewportHeight: innerHeight } }); }
    catch (e) { error = appError(e); submitting = false; start = null; end = null; }
  }
  onMount(() => {
    const keys = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.preventDefault(); void invoke('cancel_session'); } };
    window.addEventListener('keydown', keys);
    return () => window.removeEventListener('keydown', keys);
  });
</script>

<div class="capture-surface" bind:this={surface} role="application" aria-label="拖动框选公式，Esc 取消" tabindex="-1" onpointerdown={down} onpointermove={move} onpointerup={up} oncontextmenu={e => { e.preventDefault(); start = null; end = null; }}>
  {#if view.capture}
    <img class="frozen-screen" src={imageUrl(view.capture.imageToken)} alt="当前屏幕截图" draggable="false"
      onload={() => invoke('window_ready')}
      onerror={() => { error = { code: 'preview', message: '无法显示截图，请按 Esc 退出后重试。', retryable: true }; void invoke('window_ready'); }}/>
  {/if}
  {#if rect}
    <div class="selection-rect" style:left={`${rect.x}px`} style:top={`${rect.y}px`} style:width={`${rect.width}px`} style:height={`${rect.height}px`}>
      <span class="selection-size">{Math.round(rect.width)} × {Math.round(rect.height)}</span>
    </div>
  {:else}<div class="capture-shade"></div>{/if}
  <div class="capture-instructions">
    {#if submitting}<IconLoader2 size={17} class="spin"/>准备识别…{:else}<span>拖动选择公式</span><span class="instruction-separator"></span><kbd>Esc</kbd><span>取消</span>{/if}
    <button class="icon-button" aria-label="取消截图" onclick={() => invoke('cancel_session')}><IconX size={18}/></button>
  </div>
  {#if error}<div class="capture-error" role="alert">{error.message}</div>{/if}
</div>

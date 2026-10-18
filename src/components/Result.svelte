<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { IconCheck, IconChevronDown, IconLoader2, IconRefresh, IconCamera } from '@tabler/icons-svelte';
  import { appError, formats, type AppError, type OutputFormat, type ViewState } from '../lib/types';
  import { imageUrl, invoke } from '../lib/bridge';
  import Chrome from './Chrome.svelte';
  import Equation from './Equation.svelte';
  import ScreenshotPreview from './ScreenshotPreview.svelte';

  let { view }: { view: ViewState } = $props();
  let latex = $state('');
  let format = $state<OutputFormat>('latex');
  let menuOpen = $state(false);
  let copying = $state(false);
  let localError = $state<AppError | null>(null);
  let feedback = $state('');
  let editor: HTMLTextAreaElement;
  let previewSection: HTMLElement;
  let formatButton: HTMLButtonElement;
  let menu = $state<HTMLDivElement>();
  let menuPosition = $state({ left: 0, top: 0, maxHeight: 320 });
  let previousSession = '';
  let previousServerLatex = '';
  let previousServerFormat: OutputFormat = 'latex';
  let error = $derived(localError ?? view.error);
  let recognizing = $derived(view.phase === 'recognizing');
  let selected = $derived(formats.find(f => f.value === format)!);

  $effect(() => {
    const session = view.session;
    if (!session) return;
    if (session.id !== previousSession) {
      previousSession = session.id; latex = session.latex; format = session.format;
      previousServerLatex = session.latex; previousServerFormat = session.format;
    } else {
      if (session.latex !== previousServerLatex) { latex = session.latex; previousServerLatex = session.latex; }
      if (session.format !== previousServerFormat) { format = session.format; previousServerFormat = session.format; }
    }
  });

  async function copy() {
    if (!view.session || !latex.trim() || recognizing || copying) return;
    copying = true; localError = null;
    try {
      await invoke('copy_result', { sessionId: view.session.id, generation: view.session.generation, latex, format });
    } catch (e) { localError = appError(e); }
    finally { copying = false; }
  }
  async function retry() {
    if (!view.session) return;
    localError = null;
    try { await invoke('retry_recognition', { sessionId: view.session.id }); } catch (e) { localError = appError(e); }
  }
  async function settings() {
    try { await invoke('open_settings', { latex, format }); } catch (e) { localError = appError(e); }
  }
  async function capture() {
    localError = null;
    try { await invoke('begin_capture'); } catch (e) { localError = appError(e); }
  }
  async function toggleMenu() {
    menuOpen = !menuOpen;
    if (menuOpen) {
      await tick();
      const button = formatButton.getBoundingClientRect();
      const frame = formatButton.closest('.app-window')!.getBoundingClientRect();
      const height = Math.min(menu!.offsetHeight, frame.height - 16);
      menuPosition = {
        left: Math.max(frame.left + 8, Math.min(button.right - 296, frame.right - 304)),
        top: Math.max(frame.top + 8, Math.min(button.top - height - 8, frame.bottom - height - 8)),
        maxHeight: frame.height - 16,
      };
      menu?.querySelector<HTMLButtonElement>('[aria-checked="true"]')?.focus();
    }
  }
  function choose(value: OutputFormat) { format = value; menuOpen = false; feedback = ''; localError = null; formatButton?.focus(); }
  async function setDefault() {
    try {
      await invoke('set_default_format', { format }); feedback = '已设为默认格式'; menuOpen = false; formatButton?.focus();
    } catch (e) { localError = appError(e); }
  }
  function menuKeys(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); menuOpen = false; formatButton.focus(); return; }
    if (!['ArrowDown','ArrowUp','Home','End'].includes(e.key)) return;
    e.preventDefault();
    const items = Array.from(menu!.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"], .set-default'));
    let i = items.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === 'Home') i = 0;
    else if (e.key === 'End') i = items.length - 1;
    else i = (i + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
    items[i]?.focus();
  }
  onMount(() => {
    previewSection?.focus();
    const keys = (e: KeyboardEvent) => {
      if (e.isComposing || e.keyCode === 229) return;
      if (e.key === 'Escape') {
        if (menuOpen) { menuOpen = false; formatButton?.focus(); }
        else void invoke('cancel_session').catch(err => localError = appError(err));
        e.preventDefault();
      } else if (e.key === 'Tab' && e.target === previewSection) {
        e.preventDefault(); editor?.focus();
      } else if (e.key === 'Enter' && !menuOpen) {
        const editing = e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLInputElement;
        if ((!editing && !(e.target instanceof HTMLButtonElement)) || (editing && (e.ctrlKey || e.metaKey))) { e.preventDefault(); void copy(); }
      }
    };
    const outside = (e: PointerEvent) => { if (menuOpen && !menu?.contains(e.target as Node) && !formatButton?.contains(e.target as Node)) menuOpen = false; };
    const resized = () => { menuOpen = false; };
    window.addEventListener('keydown', keys); document.addEventListener('pointerdown', outside);
    window.addEventListener('resize', resized);
    return () => { window.removeEventListener('keydown', keys); document.removeEventListener('pointerdown', outside); window.removeEventListener('resize', resized); };
  });
</script>

<div class="app-window result-window" data-testid="result-window">
  <Chrome {settings}/>
  <main class="result-body">
    <section class="comparison" tabindex="-1" bind:this={previewSection} aria-label="检查截图与识别结果">
      <div class="source-column">
        <div class="section-label">截图预览</div>
        {#if view.session}<ScreenshotPreview source={imageUrl(view.session.imageToken)}/>{/if}
      </div>
      <div class="result-column">
        <div class="result-heading"><span class="section-label">识别结果</span>
          <button class="text-button recapture" onclick={capture}><IconCamera size={15} stroke={1.6}/>重新截图</button>
        </div>
        {#if recognizing && !latex}
          <div class="recognizing" role="status"><IconLoader2 size={23} stroke={1.5} class="spin"/><span>正在识别公式…</span><button class="text-button" onclick={() => invoke('cancel_session')}>取消</button></div>
        {:else}
          <Equation {latex}/>
        {/if}
      </div>
    </section>
    {#if error}
      <div class="error-banner" role="alert"><span>{error.message}</span>
        {#if error.retryable && view.session}<button class="text-button" onclick={retry}><IconRefresh size={15}/>重试</button>{/if}
      </div>
    {/if}
    <div class="code-section">
      <label class="section-label" for="latex-editor">LaTeX</label>
      <div class="code-row">
        <textarea id="latex-editor" aria-label="编辑 LaTeX 公式" spellcheck="false" bind:this={editor} bind:value={latex}
          rows={latex.includes('\n') ? Math.min(4, latex.split('\n').length) : 1}
          placeholder={recognizing ? '识别完成后可在此修改' : '识别结果，也可以手动输入 LaTeX'}
          oninput={() => { localError = null; feedback = ''; }}></textarea>
        <div class="format-control">
          <button class="format-toggle" bind:this={formatButton} aria-haspopup="menu" aria-expanded={menuOpen} onclick={toggleMenu}>
            <span>{selected.label}</span><IconChevronDown size={16} stroke={1.5}/>
          </button>
          {#if menuOpen}
            <div class="format-menu" style:left={`${menuPosition.left}px`} style:top={`${menuPosition.top}px`} style:max-height={`${menuPosition.maxHeight}px`} role="menu" aria-label="复制格式" tabindex="-1" bind:this={menu} onkeydown={menuKeys}>
              {#each formats as item}
                <button role="menuitemradio" aria-checked={format === item.value} class:selected={format === item.value} onclick={() => choose(item.value)}>
                  <span class="menu-check">{#if format === item.value}<IconCheck size={17} stroke={1.8}/>{/if}</span>
                  <span class="menu-label">{item.label}</span>
                  {#if view.settings.defaultFormat === item.value}<span class="default-label">默认</span>{/if}
                </button>
              {/each}
              <div class="menu-divider"></div>
              <button class="set-default" role="menuitem" onclick={setDefault}>设为默认<span>临时切换不会改变默认</span></button>
            </div>
          {/if}
        </div>
      </div>
      {#if format === 'wordLatex'}<p class="field-help">在 Word 中进入公式框，并选择 LaTeX 输入方式后粘贴。</p>{/if}
    </div>
  </main>
  <footer class="result-footer">
    <div class="copy-group">
      <button class="button primary copy-button" disabled={!latex.trim() || recognizing || copying} onclick={copy}>
        {#if copying}<IconLoader2 size={16} class="spin"/>复制中{:else}复制{/if}<kbd>Enter</kbd>
      </button>
      <span class="feedback" role="status">{feedback}</span>
    </div>
    <div class="keyboard-hints"><span><kbd>Tab</kbd>编辑</span><button class="hint-button" onclick={() => invoke('cancel_session')}><kbd>Esc</kbd>关闭</button></div>
  </footer>
</div>

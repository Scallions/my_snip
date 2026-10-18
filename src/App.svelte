<script lang="ts">
  import { onMount } from 'svelte';
  import { IconLoader2, IconX } from '@tabler/icons-svelte';
  import { desktop, invoke, preview, subscribe } from './lib/bridge';
  import { appError, type AppError, type ViewState } from './lib/types';
  import Result from './components/Result.svelte';
  import Settings from './components/Settings.svelte';
  import Capture from './components/Capture.svelte';

  let view = $state<ViewState | null>(null);
  let error = $state<AppError | null>(null);
  const theme = new URLSearchParams(location.search).get('theme');
  if (preview && (theme === 'dark' || theme === 'light')) document.documentElement.dataset.theme = theme;
  function accept(next: ViewState) { if (!view || next.windowGeneration >= view.windowGeneration) view = next; }
  onMount(() => {
    let disposed = false; let unlisten: (() => void) | undefined;
    (async () => {
      try {
        unlisten = await subscribe(next => { if (!disposed) accept(next); });
        const initial = await invoke<ViewState>('initial_state');
        if (disposed) { unlisten(); return; }
        accept(initial);
        if (initial.page !== 'capture') requestAnimationFrame(() => { void invoke('window_ready').catch(() => {}); });
      } catch (e) { error = appError(e); if (desktop) void invoke('window_ready').catch(() => {}); }
    })();
    return () => { disposed = true; unlisten?.(); };
  });
</script>

<div class:browser-preview={!desktop} class:capturing={view?.page === 'capture'} class="application">
  {#if view?.page === 'capture'}
    <Capture {view}/>
  {:else if view?.page === 'settings'}
    <Settings {view}/>
  {:else if view}
    <Result {view}/>
  {:else}
    <div class="startup"><IconLoader2 size={22} class="spin"/><span>{error?.message ?? '正在打开 My Snip…'}</span>{#if error}<button class="icon-button" aria-label="关闭" onclick={() => invoke('cancel_session')}><IconX size={20}/></button>{/if}</div>
  {/if}
  {#if preview && view?.notice && view.page === 'result'}<p class="preview-notice" role="status">{view.notice}</p>{/if}
</div>

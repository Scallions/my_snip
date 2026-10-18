<script lang="ts">
  import { IconX, IconSettings, IconCamera } from '@tabler/icons-svelte';
  import { invoke } from '../lib/bridge';
  let { settings, showCapture = false }: { settings?: () => void; showCapture?: boolean } = $props();
</script>

<header class="window-chrome">
  <div class="window-title" data-tauri-drag-region>My Snip</div>
  <nav class="window-actions" aria-label="窗口操作">
    {#if showCapture}
      <button class="icon-button" aria-label="截图识别公式" title="截图识别公式" onclick={() => void invoke('begin_capture').catch(() => {})}><IconCamera size={19} stroke={1.6}/></button>
    {/if}
    {#if settings}
      <button class="icon-button" aria-label="打开设置" title="设置" onclick={settings}><IconSettings size={18} stroke={1.6}/></button>
    {/if}
    <button class="icon-button close-button" aria-label="关闭并返回原应用" title="关闭 · Esc" onclick={() => void invoke('cancel_session').catch(() => {})}><IconX size={21} stroke={1.5}/></button>
  </nav>
</header>

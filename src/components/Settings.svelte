<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { IconArrowLeft, IconEye, IconEyeOff, IconLoader2, IconCheck, IconCamera } from '@tabler/icons-svelte';
  import { appError, formats, shortcutLabel, type AppError, type OutputFormat, type Recognition, type SettingsInput, type ViewState } from '../lib/types';
  import { invoke, sampleUrl, testConnection } from '../lib/bridge';
  import Chrome from './Chrome.svelte';
  import Equation from './Equation.svelte';
  let { view }: { view: ViewState } = $props();
  let baseUrl = $state(untrack(() => view.settings.baseUrl));
  let model = $state(untrack(() => view.settings.model));
  let apiKey = $state('');
  let shortcut = $state(untrack(() => view.settings.shortcut));
  let defaultFormat = $state<OutputFormat>(untrack(() => view.settings.defaultFormat));
  let reveal = $state(false);
  let saving = $state(false);
  let testing = $state(false);
  let error = $state<AppError | null>(null);
  let saved = $state('');
  let tested = $state<Recognition | null>(null);
  let elapsed = $state(0);
  let lastRevision = untrack(() => view.settings.revision);
  let baseInput: HTMLInputElement;
  let finalEndpoint = $derived.by(() => {
    try {
      const url = new URL(baseUrl.trim());
      return url.toString().replace(/\/+$/, '').replace(/\/chat\/completions$/, '') + '/chat/completions';
    } catch { return ''; }
  });
  $effect(() => {
    if (view.settings.revision !== lastRevision) { lastRevision = view.settings.revision; defaultFormat = view.settings.defaultFormat; }
  });
  function input(): SettingsInput { return { baseUrl, model, apiKey: apiKey.trim() || null, shortcut, defaultFormat }; }
  function edited() { saved = ''; tested = null; error = null; }
  async function save(e: SubmitEvent) {
    e.preventDefault(); saving = true; error = null; saved = '';
    try {
      await invoke('save_settings', { input: input() }); apiKey = ''; reveal = false;
      saved = `已保存。按 ${shortcutLabel(shortcut)} 开始截图。`;
    } catch (e) { error = appError(e); }
    finally { saving = false; }
  }
  async function test() {
    if (testing) return;
    testing = true; error = null; tested = null; elapsed = 0;
    const started = Date.now(); const timer = setInterval(() => elapsed = Math.floor((Date.now() - started) / 1000), 1000);
    try { tested = await testConnection(input()); }
    catch (e) { error = appError(e); }
    finally { testing = false; clearInterval(timer); }
  }
  async function back() { try { await invoke('back_to_result'); } catch (e) { error = appError(e); } }
  onMount(() => {
    if (!baseUrl) baseInput?.focus();
    const keys = (e: KeyboardEvent) => { if (e.key === 'Escape' && !e.isComposing) { e.preventDefault(); void back(); } };
    window.addEventListener('keydown', keys);
    return () => { window.removeEventListener('keydown', keys); apiKey = ''; };
  });
</script>

<div class="app-window settings-window" data-testid="settings-window">
  <Chrome showCapture={view.settings.hasApiKey}/>
  <main class="settings-body">
    <div class="settings-heading">
      <button class="back-button" onclick={back}><IconArrowLeft size={18} stroke={1.6}/>返回</button>
      <div><h1>模型设置</h1><p class="muted">使用在线视觉模型识别公式</p></div>
    </div>
    {#if view.error || view.settings.warning || view.notice}
      <p class="settings-notice" role="status">{view.error?.message ?? view.settings.warning ?? view.notice}</p>
    {/if}
    <form onsubmit={save}>
      <div class="field">
        <label for="base-url">接口地址</label>
        <input id="base-url" bind:this={baseInput} type="url" placeholder="https://api.example.com/v1" bind:value={baseUrl} oninput={edited} required autocomplete="url" spellcheck="false"/>
        <p class="field-help" title={finalEndpoint}>{finalEndpoint || 'OpenAI 兼容接口，支持自定义地址'}</p>
      </div>
      <div class="field">
        <label for="api-key">API Key</label>
        <div class="secret-field"><input id="api-key" type={reveal ? 'text' : 'password'} placeholder={view.settings.hasApiKey ? '密钥已保存，留空继续使用' : '填写该接口的 API Key'} bind:value={apiKey} oninput={edited} autocomplete="off" spellcheck="false"/>
          <button class="icon-button" type="button" aria-label={reveal ? '隐藏新输入的密钥' : '显示新输入的密钥'} title={reveal ? '隐藏密钥' : '显示密钥'} onclick={() => reveal = !reveal}>{#if reveal}<IconEyeOff size={18} stroke={1.6}/>{:else}<IconEye size={18} stroke={1.6}/>{/if}</button>
        </div>
        <p class="field-help">密钥保存在此设备的系统凭据存储中</p>
      </div>
      <div class="field">
        <label for="model">模型名称</label>
        <input id="model" placeholder="填写支持图片输入的模型 ID" bind:value={model} oninput={edited} required spellcheck="false" autocomplete="off"/>
      </div>
      <div class="preferences-row">
        <div class="field"><label for="shortcut">截图快捷键</label><input id="shortcut" bind:value={shortcut} oninput={edited} placeholder={view.platform === 'macos' ? 'Command+Shift+M' : 'Ctrl+Shift+M'} required spellcheck="false"/></div>
        <div class="field"><label for="default-format">默认复制格式</label><select id="default-format" bind:value={defaultFormat} onchange={edited}>{#each formats as f}<option value={f.value}>{f.label}</option>{/each}</select></div>
      </div>
      {#if error}<div class="error-banner" role="alert">{error.message}</div>{/if}
      {#if tested}
        <div class="connection-result" role="status">
          <div class="connection-status"><IconCheck size={17}/>图片识别已返回<span>{(tested.elapsedMs / 1000).toFixed(1)} 秒</span></div>
          <div class="connection-comparison"><img src={sampleUrl} alt="测试公式 E 等于 mc 平方"/><Equation latex={tested.latex} small/></div>
          <p class="field-help">请核对示例公式；本次测试不会修改已保存的设置。</p>
        </div>
      {/if}
      <footer class="settings-footer">
        <div class="test-group"><button class="button" type="button" disabled={testing || saving || !baseUrl || !model || (!apiKey && !view.settings.hasApiKey)} onclick={test}>
          {#if testing}<IconLoader2 size={16} class="spin"/>测试中 {elapsed}s{:else}测试连接{/if}
        </button><span class="field-help">使用示例公式验证图片识别</span></div>
        <button class="button primary" type="submit" disabled={saving || testing}>{#if saving}<IconLoader2 size={16} class="spin"/>保存中{:else}保存{/if}</button>
      </footer>
      {#if saved}<div class="saved-status" role="status"><IconCheck size={16}/><span>{saved}</span><button type="button" class="text-button" onclick={() => invoke('begin_capture')}><IconCamera size={15}/>开始截图</button></div>{/if}
    </form>
  </main>
</div>

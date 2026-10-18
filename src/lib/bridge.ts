import { convertFileSrc, invoke as desktopInvoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { OutputFormat, Recognition, SettingsInput, Settings, ViewState } from './types';
import sample from '../assets/connection-test.png';

export const desktop = isTauri();
export const preview = !desktop && import.meta.env.DEV;
const query = new URLSearchParams(location.search);
const listeners = new Set<(state: ViewState) => void>();
const defaults: Settings = { schemaVersion: 1, baseUrl: '', model: '', defaultFormat: 'latex', shortcut: 'Ctrl+Shift+M', revision: 0, hasApiKey: false, warning: null };
let state: ViewState = {
  windowGeneration: 1, page: query.get('preview') === 'settings' ? 'settings' : 'result', phase: 'ready',
  session: { id: 'browser-preview', generation: 1, imageToken: 'sample', width: 500, height: 173, latex: 'E = mc^2', format: 'latex', elapsedMs: 850 },
  capture: null, settings: defaults, error: null, notice: null, platform: 'windows',
};

function notify() { for (const listener of listeners) listener(structuredClone(state)); }

// Used only by the Vite browser preview. Desktop builds always use real IPC.
async function previewInvoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if (!preview) throw new Error('This tool runs as a desktop application.');
  switch (command) {
    case 'initial_state': return structuredClone(state) as T;
    case 'window_ready': return undefined as T;
    case 'open_settings':
      if (state.session) { state.session.latex = String(args.latex ?? state.session.latex); state.session.format = (args.format ?? state.session.format) as OutputFormat; }
      state.page = 'settings'; notify(); break;
    case 'back_to_result': state.page = 'result'; notify(); break;
    case 'begin_capture':
      state.page = 'result'; state.phase = 'recognizing'; state.error = null; notify();
      setTimeout(() => { state.phase = 'ready'; state.session!.latex = 'E = mc^2'; notify(); }, 800); break;
    case 'retry_recognition':
      state.phase = 'recognizing'; state.error = null; notify();
      setTimeout(() => { state.phase = 'ready'; notify(); }, 800); break;
    case 'cancel_session': state.notice = '已收起。桌面版会销毁窗口并返回原应用。'; notify(); break;
    case 'copy_result':
      {
        const source = String(args.latex ?? '').trim();
        const format = args.format as OutputFormat;
        const output = format === 'mdInline' ? `$${source}$` : format === 'mdBlock' ? `$$\n${source}\n$$` : source;
        await navigator.clipboard.writeText(output);
        state.notice = '已复制。桌面版会自动收起。'; notify(); break;
      }
    case 'set_default_format': state.settings.defaultFormat = args.format as OutputFormat; notify(); break;
    case 'get_settings': return structuredClone(state.settings) as T;
    case 'save_settings':
      {
        const input = args.input as SettingsInput;
        if (!input.baseUrl || !input.model || (!input.apiKey && !state.settings.hasApiKey)) {
          throw { code: 'validation', message: '请填写接口地址、模型名称和 API Key。', retryable: false };
        }
        state.settings = { ...state.settings, baseUrl: input.baseUrl, model: input.model, shortcut: input.shortcut,
          defaultFormat: input.defaultFormat, hasApiKey: true, revision: state.settings.revision + 1 };
        state.notice = '设置已保存。'; notify(); return structuredClone(state.settings) as T;
      }
    case 'test_connection':
      throw { code: 'desktop_only', message: '真实接口连接测试请在桌面版中执行。浏览器预览不会发送密钥。', retryable: false };
    default: throw { code: 'desktop_only', message: '请在桌面版中执行这个操作。', retryable: false };
  }
  return undefined as T;
}

export function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  return desktop ? desktopInvoke<T>(command, args) : previewInvoke<T>(command, args);
}
export async function subscribe(callback: (state: ViewState) => void): Promise<() => void> {
  if (desktop) return listen<ViewState>('snip-state', event => callback(event.payload));
  listeners.add(callback); return () => listeners.delete(callback);
}
export function imageUrl(token: string): string { return desktop ? convertFileSrc(token, 'snip-image') : sample; }
export const sampleUrl = sample;
export function testConnection(input: SettingsInput) { return invoke<Recognition>('test_connection', { input }); }

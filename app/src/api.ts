// Команды приложения: вход, связь по Wi-Fi и облако — общие, из kit; свои команды — ниже.
// В браузере (`npm run dev`) вместо них — демо в памяти (demo.ts).
import { cloudCommands, lockCommands, wifiCommands } from '@primus-fixer/kit/ui';
import { demo } from './demo';

export { LOCKED, type LockStatus } from '@primus-fixer/kit/ui';

/** Запущено внутри приложения (Tauri), а не в браузере. */
export const inApp = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/** О приложении — для экрана настроек. */
export type About = { version: string; key_method: string; device: string; records: number };

/** Почтовая служба ящика. */
export type Provider = 'yandex' | 'gmail' | 'mailru' | 'other';

/** Подключённый ящик (без пароля). */
export type AccountView = { id: string; email: string; name: string; provider: Provider; title: string; imap: string };

/** Новый ящик: для «другой» почты — ещё серверы IMAP и SMTP. */
export type NewAccount = { email: string; password: string; name: string; provider: Provider; imap: string; smtp: string };

const call = async <T,>(cmd: string, args?: Record<string, unknown>) => (await import('@tauri-apps/api/core')).invoke<T>(cmd, args);

const real = {
  // Вход по PIN-коду, отпечатку и Windows Hello — команды kit-tauri.
  ...lockCommands(),
  // Связь по Wi-Fi и облако — команды kit-tauri.
  ...wifiCommands(),
  ...cloudCommands(),
  about: () => call<About>('about'),
  // Ящики.
  accounts: () => call<AccountView[]>('accounts'),
  accountAdd: (input: NewAccount) => call<{ account: AccountView; folders: number }>('account_add', { input }),
  accountRemove: (id: string) => call<void>('account_remove', { id }),
  mailPage: (provider: Provider, page: 'password' | 'settings') => call<void>('mail_page', { provider, page }),
};

export type Api = typeof real;

export const api: Api = inApp ? real : demo;

/** Телефон: в приложении на Android. */
export const isMobileApp = inApp && /android|iphone|ipad/i.test(navigator.userAgent);

/** QR-код связи по Wi-Fi — камерой телефона. */
export async function scanQr(): Promise<string> {
  const scanner = await import('@tauri-apps/plugin-barcode-scanner');
  let perm = await scanner.checkPermissions();
  if (perm !== 'granted') perm = await scanner.requestPermissions();
  if (perm !== 'granted') throw new Error('Нет доступа к камере. Разрешите его в настройках телефона');
  const res = await scanner.scan({ windowed: false, formats: [scanner.Format.QRCode] });
  return res.content;
}

export function message(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

// Демо в браузере (`npm run dev`): те же команды, что у приложения, но в памяти — для разработки экранов
// без Tauri. Связи по Wi-Fi и облака здесь нет: разделы показывают, как они выглядят.
import type { AccountView, Api, LockStatus } from './api';
import { providerOf } from './providers';
import type { CloudStatus, LanView } from '@primus-fixer/kit/ui';

let pin: string | null = null;
let locked = false;
let recovering = false;

const lockView = (error: string | null = null): LockStatus => ({
  pin: !!pin,
  locked,
  bio: false,
  bio_ready: false,
  bio_kind: '',
  recover: '',
  wait: 0,
  left: 5,
  recovery: false,
  key: 'demo',
  notice: null,
  error,
});

const LAN: LanView = { name: 'Демо', hosts: ['192.168.1.20'], code: null, note: null, peers: [], nearby: [] };
let lan: LanView = { ...LAN };

const CLOUD_OFF: CloudStatus = {
  desktop: true,
  providers: [
    { code: 'yadisk', title: 'Яндекс Диск', kind: 'password' },
    { code: 'webdav', title: 'Другой сервер WebDAV', kind: 'password' },
  ],
  state: 'off',
  provider: null,
  title: null,
  login: null,
  dav: null,
  cloud_exists: false,
  backup_password: false,
  every: 'auto',
  running: false,
  progress: null,
  last: null,
  error: null,
  key_pending: false,
  notice: null,
};
let cloud: CloudStatus = { ...CLOUD_OFF };

// Ящики: в демо вход «проверяется» без сети; пароль «wrong» — как будто сервер не пустил.
let accounts: AccountView[] = [];
const TITLES = { yandex: 'Яндекс', gmail: 'Gmail', mailru: 'Mail.ru', other: 'Другая почта' } as const;

const now = () => Math.floor(Date.now() / 1000);
const noSync = () => Promise.reject(new Error('В демо в браузере связи с другими устройствами нет'));
const quiet = async () => () => undefined;

export const demo: Api = {
  lockStatus: async () => lockView(),
  pinUnlock: async (code) => {
    if (code !== pin) return lockView('Неверный PIN-код');
    locked = false;
    return lockView();
  },
  pinSet: async (code, current) => {
    if (pin && !recovering && current !== pin) return lockView('Неверный PIN-код');
    pin = code;
    recovering = false;
    return lockView();
  },
  pinOff: async (current) => {
    if (current !== pin) return lockView('Неверный PIN-код');
    pin = null;
    locked = false;
    return lockView();
  },
  bioUnlock: async () => lockView('В демо вход по отпечатку недоступен'),
  bioSet: async () => lockView('В демо вход по отпечатку недоступен'),
  pinRecover: async () => {
    locked = false;
    recovering = true;
    return { ...lockView(), recovery: true };
  },
  lockNow: async () => {
    if (pin) locked = true;
    return lockView();
  },
  appVisible: async () => lockView(),
  onLocked: quiet,
  lanStatus: async () => ({ ...lan }),
  lanScan: async () => ({ ...lan }),
  lanCodeStart: async () => {
    lan = { ...lan, code: { code: 'DEMO2026', secs: 300, left: 5, wrong: 0, qr_svg: '', hosts: lan.hosts, port: 0 } };
    return { ...lan };
  },
  lanCodeStop: async () => {
    lan = { ...lan, code: null };
    return { ...lan };
  },
  lanPair: noSync,
  lanPairQr: noSync,
  lanSyncNow: noSync,
  lanUnpair: async () => ({ ...lan }),
  onLan: quiet,
  cloudStatus: async () => ({ ...cloud }),
  cloudDavLogin: async (kind, url, user) => {
    const host = url ? new URL(url).host : 'webdav.yandex.ru';
    cloud = { ...cloud, state: 'password', provider: kind, title: kind === 'yadisk' ? 'Яндекс Диск' : 'Сервер WebDAV', dav: { url: url ?? 'https://webdav.yandex.ru', host, user: user.trim() }, error: null };
    return { ...cloud };
  },
  cloudAppPasswords: async () => undefined,
  cloudPassword: async () => {
    const last = { at: now(), received: 0, sent: 0, files_received: 0, files_sent: 0, files_waiting: 0, whole_received: 0, whole_sent: 0, whole_waiting: 0, problems: [] };
    cloud = { ...cloud, state: 'ready', cloud_exists: true, backup_password: true, last };
    return { ...cloud };
  },
  cloudSyncNow: async () => {
    cloud = { ...cloud, last: cloud.last ? { ...cloud.last, at: now() } : null };
    return { ...cloud };
  },
  cloudEvery: async (every) => {
    cloud = { ...cloud, every };
    return { ...cloud };
  },
  cloudDisconnect: async () => {
    cloud = { ...CLOUD_OFF };
    return { ...cloud };
  },
  onCloud: quiet,
  about: async () => ({ version: __APP_VERSION__, key_method: 'demo', device: 'демо', records: 0 }),
  accounts: async () => [...accounts],
  accountAdd: async (input) => {
    const email = input.email.trim();
    const id = email.toLowerCase();
    if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email)) throw 'Похоже, в адресе ошибка: нужен вид имя@домен.ru';
    if (accounts.some((a) => a.id === id)) throw `${email} уже подключён`;
    if (input.password.trim() === 'wrong') throw 'Сервер не пустил: неверный пароль приложения (демо).';
    const provider = input.provider ?? providerOf(email);
    const imap = { yandex: 'imap.yandex.ru', gmail: 'imap.gmail.com', mailru: 'imap.mail.ru', other: input.imap.trim() || 'imap.example.ru' }[provider];
    const account: AccountView = { id, email, name: input.name.trim(), provider, title: TITLES[provider], imap };
    accounts = [...accounts, account];
    return { account, folders: 7 };
  },
  accountRemove: async (id) => {
    accounts = accounts.filter((a) => a.id !== id);
  },
  mailPage: async () => undefined,
};

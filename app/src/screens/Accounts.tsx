import { useEffect, useState } from 'react';
import { api, message, type AccountView, type Provider } from '../api';
import { PASSWORD_HINT, PROVIDERS, providerOf } from '../providers';
import { Cut, Seg } from '../ui';

type Props = {
  /** Ящики изменились: экраны папок перечитывают список. */
  onChange: () => void;
  toast: (text: string) => void;
  /** Пришли записи с другого устройства — перечитать. */
  synced: number;
};

const folders = (n: number) => `${n} ${n % 10 === 1 && n % 100 !== 11 ? 'папка' : [2, 3, 4].includes(n % 10) && ![12, 13, 14].includes(n % 100) ? 'папки' : 'папок'}`;

/** Настройки → «Ящики»: подключённые ящики и подключение нового — по адресу и паролю приложения. */
export default function AccountsSection({ onChange, toast, synced }: Props) {
  const [list, setList] = useState<AccountView[] | null>(null);
  const [adding, setAdding] = useState(false);
  const [removing, setRemoving] = useState<AccountView | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () => api.accounts().then(setList, (e) => toast(message(e)));
  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [synced]);

  const remove = (a: AccountView) => {
    setBusy(true);
    api
      .accountRemove(a.id)
      .then(() => {
        setRemoving(null);
        toast(`${a.email} отключён`);
        onChange();
        return load();
      })
      .catch((e) => toast(message(e)))
      .finally(() => setBusy(false));
  };

  const count = list?.length ?? 0;
  return (
    <Cut id="accounts" title="Ящики" hint={list === null ? '' : count ? String(count) : 'пока нет'}>
      {count === 0 && !adding && (
        <p className="small muted">Яндекс (в том числе рабочая почта на Яндекс 360), Gmail, Mail.ru или любой ящик с IMAP — по адресу и паролю приложения. Основной пароль от почты «Ящик» не спрашивает.</p>
      )}
      {list?.map((a) => (
        <div key={a.id} className="row">
          <span className="lines">
            <span className="title">{a.email}</span>
            <span className="sub">{[a.title, a.name].filter(Boolean).join(' · ')}</span>
          </span>
          <button type="button" className="link-btn" disabled={busy} onClick={() => setRemoving(a)}>
            Отключить
          </button>
        </div>
      ))}
      {removing && (
        <div className="paircard stack-s" role="group" aria-label="Отключить ящик">
          <p className="small">
            Отключить {removing.email}? Ящик пропадёт со всех ваших устройств; письма на сервере останутся, подключить его можно снова.
          </p>
          <div className="key-actions">
            <button type="button" className="btn primary small" disabled={busy} onClick={() => remove(removing)}>
              Отключить
            </button>
            <button type="button" className="link-btn" disabled={busy} onClick={() => setRemoving(null)}>
              Отмена
            </button>
          </div>
        </div>
      )}
      {adding ? (
        <AddForm
          onDone={(a, n) => {
            setAdding(false);
            toast(`${a.email} подключён: ${folders(n)}`);
            onChange();
            load();
          }}
          onCancel={() => setAdding(false)}
        />
      ) : (
        <div className="key-actions">
          <button type="button" className="btn primary small" onClick={() => setAdding(true)}>
            Подключить ящик
          </button>
        </div>
      )}
      {count > 0 && <p className="small muted3">Подключённый ящик сам появится на других ваших устройствах — через обмен по Wi‑Fi или облако; пароль вводится один раз.</p>}
    </Cut>
  );
}

function AddForm({ onDone, onCancel }: { onDone: (a: AccountView, folders: number) => void; onCancel: () => void }) {
  const [email, setEmail] = useState('');
  // Служба выбирается по адресу, пока её не выбрали вручную.
  const [picked, setPicked] = useState<Provider | null>(null);
  const [password, setPassword] = useState('');
  const [name, setName] = useState('');
  const [imap, setImap] = useState('');
  const [smtp, setSmtp] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const provider = picked ?? (email.includes('@') ? providerOf(email) : 'yandex');
  const other = provider === 'other';

  const submit = () => {
    setBusy(true);
    setError(null);
    api
      .accountAdd({ email, password, name, provider, imap, smtp })
      .then((r) => onDone(r.account, r.folders))
      .catch((e) => setError(message(e)))
      .finally(() => setBusy(false));
  };

  const page = (which: 'password' | 'settings') => api.mailPage(provider, which).catch((e) => setError(message(e)));

  return (
    <form
      className="key-form"
      aria-label="Подключить ящик"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <p className="cloud-form-title">Новый ящик</p>
      <label className="field">
        <span>Адрес</span>
        <input type="email" inputMode="email" autoComplete="username" autoCapitalize="none" spellCheck={false} placeholder="имя@yandex.ru" value={email} onChange={(e) => setEmail(e.target.value)} disabled={busy} />
      </label>
      <Seg label="Почта" value={provider} options={PROVIDERS} onChange={(v) => setPicked(v)} small />
      <p className="small muted">{PASSWORD_HINT[provider]}</p>
      {!other && (
        <div className="key-actions">
          <button type="button" className="btn line small" disabled={busy} onClick={() => page('password')}>
            Создать пароль приложения
          </button>
          {provider === 'yandex' && (
            <button type="button" className="btn line small" disabled={busy} onClick={() => page('settings')}>
              Почтовые программы
            </button>
          )}
        </div>
      )}
      {other && (
        <>
          <label className="field">
            <span>Сервер IMAP</span>
            <input inputMode="url" autoCapitalize="none" spellCheck={false} placeholder="imap.example.ru" value={imap} onChange={(e) => setImap(e.target.value)} disabled={busy} />
          </label>
          <label className="field">
            <span>Сервер SMTP</span>
            <input inputMode="url" autoCapitalize="none" spellCheck={false} placeholder="smtp.example.ru" value={smtp} onChange={(e) => setSmtp(e.target.value)} disabled={busy} />
          </label>
        </>
      )}
      <label className="field">
        <span>{other ? 'Пароль' : 'Пароль приложения'}</span>
        <input type="password" autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} disabled={busy} />
      </label>
      <label className="field">
        <span>Имя в письмах — необязательно</span>
        <input autoComplete="name" placeholder="Иван Петров" value={name} onChange={(e) => setName(e.target.value)} disabled={busy} />
      </label>
      {error && <p className="small warn-text">{error}</p>}
      <div className="key-actions">
        <button type="submit" className="btn primary small" disabled={busy || !email.trim() || !password || (other && (!imap.trim() || !smtp.trim()))}>
          {busy ? 'Проверяю вход…' : 'Подключить'}
        </button>
        <button type="button" className="link-btn" disabled={busy} onClick={onCancel}>
          Отмена
        </button>
      </div>
    </form>
  );
}

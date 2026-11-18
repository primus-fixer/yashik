import { useEffect, useRef, useState } from 'react';
import { api, message, type MailStatus, type MessageRow } from '../api';
import { folder, type FolderId } from '../folders';
import { Header, Icon } from '../ui';

/** Сколько писем в одной странице списка (столько же отдаёт команда `messages`). */
const PAGE = 50;

const connected = (n: number) => (n === 1 ? 'Ящик подключён' : `${n} ${[2, 3, 4].includes(n % 10) && ![12, 13, 14].includes(n % 100) ? 'ящика' : 'ящиков'} подключено`);

/** Когда: сегодня — время, в этом году — день и месяц, раньше — дата. */
function when(ts: number): string {
  const d = new Date(ts * 1000);
  const now = new Date();
  if (d.toDateString() === now.toDateString()) return d.toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' });
  if (d.getFullYear() === now.getFullYear()) return d.toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' }).replace('.', '');
  return d.toLocaleDateString('ru-RU');
}

type Props = {
  id: FolderId;
  /** Пришли записи с другого устройства. */
  synced: number;
  /** Сколько ящиков подключено. */
  accounts: number;
  /** Загрузка писем; mailRev меняется с каждым её событием — список перечитывается. */
  mail: MailStatus | null;
  mailRev: number;
  onConnect: () => void;
  /** Папка открыта из списка «Папки» на телефоне: сверху «Назад». */
  onBack?: () => void;
  toast: (text: string) => void;
};

/** Папка: заголовок, под ним поиск по почте, ниже письма всех ящиков, новые сверху. */
export default function Folder({ id, synced, accounts, mail, mailRev, onConnect, onBack, toast }: Props) {
  const f = folder(id);
  const [query, setQuery] = useState('');
  const [q, setQ] = useState('');
  const [rows, setRows] = useState<MessageRow[] | null>(null);
  const [switched, setSwitched] = useState<string | null>(null);
  const [more, setMore] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const shown = useRef(0);
  shown.current = rows?.length ?? 0;

  // Поиск — когда перестали печатать.
  useEffect(() => {
    const t = window.setTimeout(() => setQ(query.trim()), 250);
    return () => window.clearTimeout(t);
  }, [query]);

  const place = { kind: 'role' as const, role: f.role };
  useEffect(() => {
    if (!accounts) {
      setRows([]);
      return;
    }
    // Пока идёт загрузка, первая страница перечитывается с каждой пачкой; если листали дальше — не сбиваем.
    if (shown.current > PAGE) return;
    let live = true;
    api.messages(place, q, null).then(
      (p) => {
        if (!live) return;
        setRows(p.rows);
        setSwitched(p.switched);
        setMore(p.rows.length === PAGE);
      },
      (e) => toast(message(e)),
    );
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id, q, accounts, synced, mailRev]);

  const loadMore = () => {
    const last = rows?.[rows.length - 1];
    if (!last) return;
    api.messages(place, q, [last.date, last.id]).then(
      (p) => {
        setRows([...(rows ?? []), ...p.rows]);
        setMore(p.rows.length === PAGE);
      },
      (e) => toast(message(e)),
    );
  };

  const searching = q !== '';
  const progress = mail?.running ? mail.progress : null;
  const checked = mail?.last ? `Проверено в ${new Date(mail.last * 1000).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })}` : '';
  return (
    <div className="stack" data-synced={synced}>
      {onBack ? <Header title={f.label} onBack={onBack} /> : <h1 className="screen-title">{f.label}</h1>}
      <div className="stack-s">
        <label className="search-field">
          <Icon name="search" />
          <input
            ref={input}
            type="search"
            enterKeyHint="search"
            placeholder="Поиск по почте"
            aria-label="Поиск по почте"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === 'Escape' && setQuery('')}
          />
          {query && (
            <button
              type="button"
              className="clear"
              aria-label="Очистить поиск"
              onClick={() => {
                setQuery('');
                input.current?.focus();
              }}
            >
              <Icon name="x" size={18} />
            </button>
          )}
        </label>
        {accounts > 0 && (
          <p className="small muted mail-status" aria-live="polite">
            <span>{mail?.running ? (progress ? `Загружаю письма: ${progress.folder} — ${progress.done} из ${progress.total}` : 'Проверяю почту…') : checked}</span>
            {!mail?.running && (
              <button type="button" className="link-btn" onClick={() => api.mailSyncNow().catch((e) => toast(message(e)))}>
                Проверить
              </button>
            )}
          </p>
        )}
        {mail?.errors.map(([email, text]) => (
          <p key={email} className="small warn-text">
            {email}: {text}
          </p>
        ))}
      </div>

      {rows && rows.length > 0 ? (
        <>
          {switched && <p className="small muted">По «{q}» ничего нет — показаны письма по «{switched}»: похоже, запрос набран в другой раскладке.</p>}
          <div className="mail-list">
            {rows.map((r) => (
              <button key={r.id} type="button" className={`mail-row${r.flags.seen ? '' : ' unread'}`} onClick={() => toast('Письмо откроется в следующей версии «Ящика»')}>
                <span className="mail-dot" aria-label={r.flags.seen ? undefined : 'не прочитано'} />
                <span className="lines">
                  <span className="title">{r.who || 'без адреса'}</span>
                  <span className="sub">{r.subject || '(без темы)'}</span>
                </span>
                <span className="mail-meta">
                  <span>{when(r.date)}</span>
                  <span className="mail-icons">
                    {r.flags.flagged && <Icon name="star" size={14} />}
                    {r.attachments && <Icon name="clip" size={14} />}
                  </span>
                </span>
              </button>
            ))}
          </div>
          {more && (
            <div className="key-actions">
              <button type="button" className="btn line small" onClick={loadMore}>
                Показать ещё
              </button>
            </div>
          )}
        </>
      ) : (
        <div className="empty">
          <div className="empty-icon">
            <Icon name={searching ? 'search' : f.icon} size={32} />
          </div>
          <p className="muted">
            {searching
              ? `${accounts ? 'Ничего не нашлось.' : 'Искать пока негде — подключите ящик.'} Поиск понимает словоформы, транслит и другую раскладку: «отчёт» найдёт «отчёты» и «otchet», «ghbdtn» — «привет».`
              : !accounts
                ? f.empty
                : mail?.running
                  ? 'Загружаю письма…'
                  : rows === null
                    ? ''
                    : mail?.last
                      ? f.nothing
                      : `${connected(accounts)} — письма появятся после первой проверки почты.`}
          </p>
          {!accounts && (
            <button type="button" className="btn primary" onClick={onConnect}>
              Подключить ящик
            </button>
          )}
        </div>
      )}
    </div>
  );
}

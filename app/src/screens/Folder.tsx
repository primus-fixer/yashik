import { useRef, useState } from 'react';
import { folder, type FolderId } from '../folders';
import { Header, Icon } from '../ui';

const connected = (n: number) => (n === 1 ? 'Ящик подключён' : `${n} ${[2, 3, 4].includes(n % 10) && ![12, 13, 14].includes(n % 100) ? 'ящика' : 'ящиков'} подключено`);

/** Папка: заголовок, под ним поиск по почте, ниже письма. Пока ящиков нет — что здесь будет и кнопка
 * «Подключить ящик». `onBack` — папка открыта из списка «Папки» на телефоне: сверху «Назад».
 * `synced` меняется, когда пришло новое с другого устройства, — тогда экран перечитывает данные. */
export default function Folder({ id, synced, accounts, onConnect, onBack }: { id: FolderId; synced: number; accounts: number; onConnect: () => void; onBack?: () => void }) {
  const f = folder(id);
  const [query, setQuery] = useState('');
  const input = useRef<HTMLInputElement>(null);
  const searching = query.trim() !== '';
  return (
    <div className="stack" data-synced={synced}>
      {onBack ? <Header title={f.label} onBack={onBack} /> : <h1 className="screen-title">{f.label}</h1>}
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
      <div className="empty">
        <div className="empty-icon">
          <Icon name={searching ? 'search' : f.icon} size={32} />
        </div>
        <p className="muted">
          {searching
            ? `${accounts ? 'Писем на устройстве пока нет: загрузка писем — в следующей версии.' : 'Искать пока негде — подключите ящик.'} Поиск понимает словоформы, транслит и другую раскладку: «отчёт» найдёт «отчёты» и «otchet», «ghbdtn» — «привет».`
            : accounts
              ? `${connected(accounts)}. Загрузка писем появится в следующей версии «Ящика».`
              : f.empty}
        </p>
        {!accounts && (
          <button type="button" className="btn primary" onClick={onConnect}>
            Подключить ящик
          </button>
        )}
      </div>
    </div>
  );
}
